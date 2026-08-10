use std::time::Duration;

use sqlx::{PgPool, Postgres, Transaction};
use tracing::{Span, field::display};
use uuid::Uuid;

use crate::{
    configuration::Settings, domain::SubscriberEmail, email_client::EmailClient,
    startup::get_connection_pool,
};

pub async fn run_worker_until_stopped(configuration: Settings) -> Result<(), anyhow::Error> {
    let connection_pool = get_connection_pool(&configuration.database);
    let email_client = configuration.email_client.client();
    worker_loop(connection_pool, email_client).await
}

pub enum ExecutionOutcome {
    TaskCompleted,
    EmptyQueue,
}

async fn worker_loop(pool: PgPool, email_client: EmailClient) -> Result<(), anyhow::Error> {
    loop {
        match try_execute_task(&pool, &email_client).await {
            Ok(ExecutionOutcome::EmptyQueue) => {
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Ok(ExecutionOutcome::TaskCompleted) => {}
        }
    }
}

#[tracing::instrument(
    level = "trace",
    skip_all,
    fields(
        newsletter_issue_id=tracing::field::Empty,
        subscriber_email=tracing::field::Empty
    ),
    err
)]
pub async fn try_execute_task(
    pool: &PgPool,
    email_client: &EmailClient,
) -> Result<ExecutionOutcome, anyhow::Error> {
    let task = dequeue_task(pool).await?;
    if task.is_none() {
        return Ok(ExecutionOutcome::EmptyQueue);
    }
    let (transaction, issue_id, email, attempts) = task.unwrap();
    let max_retries = 5;
    Span::current()
        .record("newsletter_issue_id", display(issue_id))
        .record("subscriber_email", display(&email));
    match SubscriberEmail::parse(email.clone()) {
        Ok(email) => {
            let issue = get_issue(pool, issue_id).await?;
            if let Err(e) = email_client
                .send_email(
                    &email,
                    &issue.title,
                    &issue.html_content,
                    &issue.text_content,
                )
                .await
            {
                tracing::error!(
                    error.cause_chain = ?e,
                    error.message = %e,
                    "Failed to deliver issue to confirmed subscriber. \
                    Skipping.",
                );
                if attempts >= max_retries {
                    tracing::warn!(
                        "Max retries ({}) reached for {}. Dropping task.",
                        max_retries,
                        email
                    );
                    // delete the failed task
                    delete_task(transaction, issue_id, email.as_ref()).await?;
                } else {
                    tracing::info!(
                        "Recording failure for {}. Attempt {} of {}",
                        email,
                        attempts + 1,
                        max_retries
                    );
                    record_failure(transaction, issue_id, email.as_ref()).await?;
                }
                return Err(anyhow::anyhow!(e));
            }
        }
        Err(e) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "Skipping a confirmed subscriber. \
                Their stored contact details are invalid.",
            );
        }
    }
    // delete the completed task
    delete_task(transaction, issue_id, &email).await?;
    Ok(ExecutionOutcome::TaskCompleted)
}

#[tracing::instrument(level = "trace", skip_all)]
async fn record_failure(
    mut transaction: PgTransaction,
    issue_id: Uuid,
    email: &str,
) -> Result<(), anyhow::Error> {
    sqlx::query!(
        r#"
        UPDATE issue_delivery_queue
        SET attempts = attempts + 1
        WHERE newsletter_issue_id = $1 AND subscriber_email = $2
        "#,
        issue_id,
        email
    )
    .execute(&mut *transaction)
    .await?;
    // to prevent to call the rollback function
    transaction.commit().await?;
    Ok(())
}

struct NewsletterIssue {
    title: String,
    text_content: String,
    html_content: String,
}

#[tracing::instrument(level = "trace", skip_all)]
async fn get_issue(pool: &PgPool, issue_id: Uuid) -> Result<NewsletterIssue, anyhow::Error> {
    let issue = sqlx::query_as!(
        NewsletterIssue,
        r#"
        SELECT title, text_content, html_content
        FROM newsletter_issues
        WHERE
            newsletter_issue_id = $1
        "#,
        issue_id
    )
    .fetch_one(pool)
    .await?;
    Ok(issue)
}

type PgTransaction = Transaction<'static, Postgres>;

#[tracing::instrument(level = "trace", skip_all)]
async fn dequeue_task(
    pool: &PgPool,
) -> Result<Option<(PgTransaction, Uuid, String, i32)>, anyhow::Error> {
    let mut transaction = pool.begin().await?;
    let record = sqlx::query!(
        r#"
        SELECT newsletter_issue_id, subscriber_email, attempts
        FROM issue_delivery_queue
        FOR UPDATE
        SKIP LOCKED
        LIMIT 1
        "#,
    )
    .fetch_optional(&mut *transaction)
    .await?;
    if let Some(record) = record {
        Ok(Some((
            transaction,
            record.newsletter_issue_id,
            record.subscriber_email,
            record.attempts,
        )))
    } else {
        Ok(None)
    }
}

#[tracing::instrument(level = "trace", skip_all)]
async fn delete_task(
    mut transaction: PgTransaction,
    issue_id: Uuid,
    email: &str,
) -> Result<(), anyhow::Error> {
    sqlx::query!(
        r#"
        DELETE FROM issue_delivery_queue
        WHERE
            newsletter_issue_id = $1 AND subscriber_email = $2
        "#,
        issue_id,
        email
    )
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}
