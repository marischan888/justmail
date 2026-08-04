use actix_web::{HttpResponse, web};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context;
use sqlx::{PgPool, Postgres, Executor};
use askama::Template;
use uuid::Uuid;
use crate::authentication::UserId;
use crate::idempotency::{IdempotencyKey, NextAction, save_response, try_processing};
use crate::utils::{e500, see_other, e400};

#[derive(Template)]
#[template(path = "newsletter.html")]
struct NewsletterTemplate<'a> {
    content_html: &'a str,
}

#[derive(serde::Deserialize)]
pub struct FormData {
    title: String,
    content: String,
    idempotency_key: String,
}

fn success_message() -> FlashMessage {
    FlashMessage::info("You has issued newsletter to all your subscribers.")
}


#[tracing::instrument
(
    name = "Send email to the subscriber"
    skip(form, pool),
    fields (
        user_name=tracing::field::Empty,
        user_id=tracing::field::Empty,
    ),
)
]
pub async fn issue_newsletters(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error>{
    let FormData {
        title,
        content,
        idempotency_key,
    } = form.0;
    let user_id = user_id.into_inner();
    let idempotency_key: IdempotencyKey = idempotency_key.try_into().map_err(e400)?;
    let mut transaction = match try_processing(&pool, &idempotency_key, *user_id)
        .await
        .map_err(e500)?
    {
        NextAction::StartProcessing(t) => t,
        NextAction::ReturnSavedResponse(saved_response) => {
            success_message().send();
            return Ok(saved_response)
        }
    };
    // format html
    let formatted_html = text_to_simple_html(content.clone());
    let html_template = NewsletterTemplate {
        content_html: &formatted_html,
    };
    let html_body = html_template.render().map_err(e500)?;
    // insert into the issue table and enqueue the task for background worker to send email
    let issue_id = insert_newsletter_issue(
        &mut *transaction, 
        &title,
        &content,
        html_body.as_ref()
    )
        .await
        .context("Failed to store newsletter issue details")
        .map_err(e500)?;
    enqueue_delivery_tasks(&mut *transaction, issue_id)
        .await
        .context("Failed to enqueue delivery tasks")
        .map_err(e500)?;
    let response = see_other("/admin/newsletters");
    // save response take the ownership and commit the transaction
    let response = save_response(transaction, &idempotency_key, *user_id, response)
        .await
        .map_err(e500)?;
    success_message().send();
    Ok(response)
}

fn text_to_simple_html(text: String) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    let paragraphs: Vec<String> = escaped
        .split("\n\n")
        .map(|para| format!("<p>{}</p>", para.replace('\n', "<br/>")))
        .collect();

    paragraphs.join("")
}

#[tracing::instrument(skip_all)]
async fn insert_newsletter_issue(
    executor: impl Executor<'_, Database=Postgres>,
    title: &str,
    text_content: &str,
    html_content: &str,
) -> Result<Uuid, sqlx::Error> {
    let newsletter_issue_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO newsletter_issues (
            newsletter_issue_id,
            title,
            text_content,
            html_content,
            published_at
        )
        VALUES ($1, $2, $3, $4, now())
        "#,
        newsletter_issue_id,
        title,
        text_content,
        html_content
    )
        .execute(executor)
        .await?;
    Ok(newsletter_issue_id)
}

#[tracing::instrument(skip_all)]
async fn enqueue_delivery_tasks(
    executor: impl Executor<'_, Database=Postgres>,
    newsletter_issue_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO issue_delivery_queue (
            newsletter_issue_id,
            subscriber_email
        )
        SELECT $1, email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
        newsletter_issue_id,
    )
        .execute(executor)
        .await?;
    Ok(())
}
