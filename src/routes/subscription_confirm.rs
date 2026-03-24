use std::fmt::{Debug};
use actix_web::{HttpResponse, web, ResponseError};
use actix_web::http::StatusCode;
use anyhow::Context;
use serde::Deserialize;
use sqlx::{Executor, PgPool, Postgres};
use uuid::Uuid;
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

    let subscriber_id = get_subscriber_id_from_token
        (
            &mut *transaction,
            &parameters.subscription_token,
        )
        .await
        .context("Failed to get subscriber id from the database.")?
        .ok_or(ConfirmError::UnknownToken)?;

    mark_subscriber_confirmed(&mut *transaction, subscriber_id)
        .await
        .context("Failed to mark subscriber as confirmed.")?;

    consume_tokens(&mut *transaction, &parameters.subscription_token)
        .await
        .context("Failed to consume tokens from subscription token.")?;

    transaction
        .commit()
        .await
        .context("Failed to commit transaction for confirmation subscription.")?;

    Ok(HttpResponse::Ok().finish())
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

#[tracing::instrument
(
    name = "Get subscriber_id from token",
    skip(executor, subscription_token),
)
]
pub async fn get_subscriber_id_from_token(
    executor: impl Executor<'_, Database=Postgres>,
    subscription_token: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    // result: Record{subscriber_id}
    let result = sqlx::query!(
        r#"
        SELECT subscriber_id
        FROM subscription_tokens
        WHERE subscription_token = $1
        AND created_at >= now() - INTERVAL '1 day'
        "#,
        subscription_token
    )
        .fetch_optional(executor)
        .await?;

    Ok(result.map(|r| r.subscriber_id))
}

#[tracing::instrument
(
    name = "Mark subscriber as confirmed",
    skip(executor, subscriber_id),
)]
pub async fn mark_subscriber_confirmed(
    executor: impl Executor<'_, Database=Postgres>,
    subscriber_id: Uuid
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE subscriptions SET status = 'confirmed' WHERE id = $1 AND status != 'confirmed'"#,
        subscriber_id,
    )
    .execute(executor)
    .await?;
    Ok(())
}