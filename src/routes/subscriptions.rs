use std::fmt::{Debug,  Formatter};
use actix_web::{HttpResponse, web, ResponseError};
use actix_web::http::StatusCode;
use anyhow::Context;
use chrono::Utc;
use sqlx::{Executor, PgPool, Postgres};
use sqlx::types::Uuid;
use rand::distr::Alphanumeric;
use rand::{rng, RngExt};
use crate::domain::{NewSubscriber, SubscriberEmail, SubscriberName};
use crate::email_client::EmailClient;
use crate::startup::ApplicationBaseUrl;

// TODO: How to clean up the Abandon records?
// Generate a random 25-char-long case-sensitive token
fn generate_subscription_token() -> String {
    let mut rng = rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

// deal with conversion allowed error without consuming input
// Type Conversion
impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(form: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(form.name)?;
        let email = SubscriberEmail::parse(form.email)?;
        Ok(NewSubscriber {name, email})
    }
}

#[non_exhaustive]
#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl Debug for SubscribeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}


impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            SubscribeError::UnexpectedError(_)=> StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by: {}", cause)?;
        current = cause.source();
    }
    Ok(())
}

#[tracing::instrument
(
    name = "Adding a new subscriber",
    skip(from, pool, email_client, base_url),
    fields
    (
        subscriber_email = %from.email,
        subscriber_name = %from.name
    )
)
]
pub async fn subscribe(
    from: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>
) -> Result<HttpResponse, SubscribeError> {
    // try_into is a mirror of tru_from, directly take self do bot need to write A::try_from
    let new_subscriber = from.0
        .try_into()
        .map_err(SubscribeError::ValidationError)?;

    let mut transaction = pool
        .begin()
        .await
        .context("Failed to start the transaction for subscription.")?;

    let subscriber_status = insert_subscriber
        (
            &new_subscriber,
            &mut *transaction,
        )
        .await
        .context("Failed to insert new subscriber into the database.")?;

    //if subscriber_status.status == "confirmed" {return Ok(HttpResponse::Ok().finish())}

    let subscription_token = generate_subscription_token();
    store_new_token
        (
         &mut *transaction,
         subscriber_status.subscriber_id,
         &subscription_token,
        )
        .await
        .context("Failed to store new subscriber token in the database.")?;

    transaction
        .commit()
        .await
        .context("Failed to commit the current transaction for subscription.")?;

    send_confirmation_email
        (
            &email_client,
            &new_subscriber,
            &base_url.0,
            &subscription_token,
        )
        .await
        .context("Failed to send a confirmation email to the subscriber.")?;

    Ok(HttpResponse::Created().finish())
}


#[tracing::instrument
(
    name = "Store subscription token and subscriber ID",
    skip(executor, subscriber_id, subscription_token),
)
]
pub async fn store_new_token(
    executor: impl Executor<'_, Database=Postgres>,
    subscriber_id: Uuid,
    subscription_token: &String,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO subscription_tokens (subscription_token, subscriber_id, created_at)
        VALUES ($1, $2, $3)
        "#,
        subscription_token,
        subscriber_id,
        Utc::now(),
    )
        .execute(executor)
        .await?;
    Ok(())
}

#[tracing::instrument
(
    name = "Sending confirmation email to the subscriber",
    skip(email_client, receiver, base_url, subscription_token),
)
]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    receiver: &NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    // a static confirm link with token
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, // application settings
        subscription_token);
    let html_body = format!(
        "Welcome to our newsletter!<br />\
         Click <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link
    );
    let plain_body = format!(
        "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
        confirmation_link
    );

    email_client.send_email(
        &receiver.email,
        "Welcome!",
        &html_body,
        &plain_body,
    )
        .await
}

pub struct SubscriberStatus {
    pub subscriber_id: Uuid,
    pub status: String,
}

#[tracing::instrument
(
    name = "Saving new subscriber details in the database.",
    skip(new_subscriber, executor)
)
]
pub async fn insert_subscriber(
    new_subscriber: &NewSubscriber,
    executor: impl Executor<'_, Database=Postgres>,
) -> Result<SubscriberStatus, sqlx::Error>
{
    let subscriber_id = Uuid::new_v4();
    let result = sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        ON CONFLICT (email) DO UPDATE SET name=EXCLUDED.name
        RETURNING id, status
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(), // read-only value
        Utc::now(),
    )
        .fetch_one(executor)
        .await?;
    Ok(
        SubscriberStatus {
            subscriber_id: result.id,
            status: result.status,
        }
    )
}