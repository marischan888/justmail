use std::fmt::{Debug};
use actix_web::http::header::ContentType;
use actix_web::{HttpResponse, web, ResponseError};
use actix_web::http::StatusCode;
use anyhow::Context;
use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;
use sqlx::{Executor, PgPool, Postgres};
use uuid::Uuid;
use askama::Template;
use crate::routes::error_chain_fmt;

#[derive(Deserialize)]
pub struct Parameters {
    subscription_token: String,
}

#[non_exhaustive]
#[derive(thiserror::Error)]
pub enum ConfirmError {
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
    #[error("There is no subscriber associated with this token")]
    UnknownToken,
}

impl Debug for ConfirmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(&self, f)
    }
}

impl ResponseError for ConfirmError {
    fn status_code(&self) -> StatusCode {
        match self {
            ConfirmError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            ConfirmError::UnknownToken => StatusCode::UNAUTHORIZED,
        }
    }
}


#[derive(Template)]
#[template(path = "confirmation.html")]
struct ConfirmationTemplate<'a> {
    pub title: &'a str,
    pub message: &'a str,
}

#[tracing::instrument
(
    name = "Confirming a pending subscriber",
    skip(parameters, pool)
)
]
pub async fn subscription_confirm(
    parameters: web::Query<Parameters>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, ConfirmError> {
    let mut transaction = pool
        .begin()
        .await
        .context("Failed to start transaction for confirmation subscription.")?;

    let current_time = Utc::now();
    let token_record = get_record_from_token
        (
            &mut *transaction,
            &parameters.subscription_token,
        )
        .await
        .context("Failed to get subscriber id from the database.")?
        .ok_or(ConfirmError::UnknownToken)?;

    let duration = current_time - token_record.token_created_at;
    let next_action =  if duration > TimeDelta::days(2) {
        remove_expired_token_record(&mut *transaction, &parameters.subscription_token)
            .await
            .context("can not delete token record")?
    } else {
        mark_subscriber_confirmed(&mut *transaction, token_record.subscriber_id)
            .await
            .context("Failed to mark subscriber as confirmed.")?
    };
    transaction
        .commit()
        .await
        .context("Failed to commit transaction for confirmation subscription.")?;

    let html_template = match next_action {
        Action::SendSuccess => {
            ConfirmationTemplate {
                title: "Subscribe Successfully.",
                message: "Thank you for your support."
            }
        }
        Action::SendAlreadyConfirm => {
            ConfirmationTemplate {
                title: "You have already subscribed.",
                message: "Thank you."
            }
        }
        Action::LinkExpired => {
            ConfirmationTemplate {
                title: "This confirmation link has been expired",
                message: "Please subscribe again"
            }
        }
    };
    let html_body = html_template.render().context("can not generate html")?;
    Ok(
        HttpResponse::Ok()
            .content_type(ContentType::html())
            .body(html_body)
    )
}

#[tracing::instrument
(
    name = "Consume invalid tokens",
    skip(executor, subscription_token)
)
]
pub async fn consume_tokens(
    executor: impl Executor<'_, Database=Postgres>,
    subscription_token: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM subscription_tokens WHERE subscription_token = $1",
        subscription_token
    )
        .execute(executor)
        .await?;
    Ok(())
}

pub struct TokenRecord {
    subscriber_id: Uuid,
    token_created_at: DateTime<Utc>,
}

#[tracing::instrument
(
    name = "Get subscriber_id from token",
    skip(executor, subscription_token),
)
]
pub async fn get_record_from_token(
    executor: impl Executor<'_, Database=Postgres>,
    subscription_token: &str,
) -> Result<Option<TokenRecord>, sqlx::Error> {
    // result: Record{subscriber_id}
    let result = sqlx::query!(
        r#"
        SELECT subscriber_id, created_at
        FROM subscription_tokens
        WHERE subscription_token = $1
        "#,
        subscription_token
    )
        .fetch_optional(executor)
        .await?;

    Ok(result.map(|r| TokenRecord{
        subscriber_id: r.subscriber_id,
        token_created_at: r.created_at,
    }))
}

pub enum Action {
    SendSuccess,
    SendAlreadyConfirm,
    LinkExpired,
}

pub async fn remove_expired_token_record(
    executor: impl Executor<'_, Database=Postgres>,
    subscription_token: &str,
) -> Result<Action, sqlx::Error> {
    sqlx::query!(
        r#"
        DELETE FROM subscription_tokens
        WHERE subscription_token = $1
        "#,
        subscription_token,
    )
        .execute(executor)
        .await?;
    Ok(Action::LinkExpired)
}

#[tracing::instrument
(
    name = "Mark subscriber as confirmed",
    skip(executor, subscriber_id),
)]
pub async fn mark_subscriber_confirmed(
    executor: impl Executor<'_, Database=Postgres>,
    subscriber_id: Uuid
) -> Result<Action, sqlx::Error> {
    let n_rows_affected =  sqlx::query!(
        r#"
        UPDATE subscriptions 
        SET status = 'confirmed' 
        WHERE id = $1 AND status != 'confirmed'
        "#,
        subscriber_id,
    )
    .execute(executor)
    .await?
    .rows_affected();

    if n_rows_affected > 0 {
        Ok(Action::SendSuccess)
    } else {
        Ok(Action::SendAlreadyConfirm)
    }
}
