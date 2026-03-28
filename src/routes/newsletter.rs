use std::fmt::{Debug, Formatter};
use actix_web::{web, HttpRequest, HttpResponse, ResponseError};
use actix_web::http::{header, StatusCode};
use actix_web::http::header::{HeaderMap, HeaderValue};
use anyhow::Context;
use secrecy::{SecretString};
use serde::Deserialize;
use sqlx::{PgPool};
use base64::{engine::general_purpose::STANDARD, engine::Engine as _};
use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::routes::error_chain_fmt;

#[derive(Deserialize)]
pub struct BodyData {
    title: String,
    content: Content,
}

#[derive(Deserialize)]
pub struct Content {
    html: String,
    plain: String,
}

#[non_exhaustive]
#[derive(thiserror::Error)]
pub enum PublishNewsletterError {
    #[error("Authentication failed.")]
    AuthError(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl Debug for PublishNewsletterError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for PublishNewsletterError {
    fn error_response(&self) -> HttpResponse {
        match self {
            PublishNewsletterError::UnexpectedError(_) => {
                HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
            }
            PublishNewsletterError::AuthError(_) => {
                let mut response = HttpResponse::new(StatusCode::UNAUTHORIZED);
                let header_value = HeaderValue::from_str(r#"Basic realm="publish""#)
                    .unwrap();
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, header_value);
                response
            }
        }
    }
}
#[tracing::instrument
(
    name = "Send news letter email to the subscriber.",
    skip(pool, body, email_client),
)
]
pub async fn publish_newsletter(
    pool: web::Data<PgPool>,
    body: web::Json<BodyData>,
    email_client: web::Data<EmailClient>,
    request: HttpRequest,
)
    -> Result<HttpResponse, PublishNewsletterError> {
    let _credentials = extract_credentials(request.headers())
        .map_err(PublishNewsletterError::AuthError)?;
    let confirmed_subscribers = get_confirmed_subscriber(&pool)
        .await
        .context("Failed to retrieve confirmed subscribers from the database.")?; // sqlx error
    for subscriber in confirmed_subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &body.title,
                        &body.content.html,
                        &body.content.plain,
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to send newsletter issue to {}.",
                            &subscriber.email
                        )
                    })?;
            }
            Err(error) => {
                tracing::warn!(
                    error.cause_chain = ?error,
                    "Skipping a confirmed subscriber. \n Their stored emails are invalid."
                );
            }
        }
    }
    Ok(HttpResponse::Ok().finish())
}

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument
(
    name = "Get confirmed subscriber from database",
    skip(pool)
)
]
async fn get_confirmed_subscriber(
    pool: &PgPool
)
    -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, anyhow::Error> {
    let confirmed_subscriber = sqlx::query!(
        r#"SELECT email FROM subscriptions WHERE status = 'confirmed'"#,
    )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            match SubscriberEmail::parse(row.email)
            {
                Ok(email) => Ok(ConfirmedSubscriber { email }),
                Err(error) => Err(anyhow::anyhow!(error)), // empty email will also be here
            }
        })
        .collect();
    Ok(confirmed_subscriber)
}

struct Credentials {
    username: String,
    password: SecretString,
}
fn extract_credentials(headers: &HeaderMap) -> Result<Credentials, anyhow::Error> {
    let header_value = headers
        .get("Authorization")
        .context("The 'Authorization' header was missing.")?
        .to_str()
        .context("The 'Authorization' header was not a valid string.")?;
    let base64encoded_segment  = header_value
        .strip_prefix("Basic ")
        .context("The authorization header was not 'Basic'.")?;
    let decoded_bytes = STANDARD.decode(base64encoded_segment.as_bytes())?;
    let decoded_string = String::from_utf8(decoded_bytes.to_vec())?;

    // Split into segments
    let mut credentials = decoded_string.splitn(2, ':');
    let username = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A username must be provided in the authorization header"))?
        .to_string();
    let password = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A password must be provided in the authorization header"))?
        .to_string()
        .into_boxed_str();
    Ok(
        Credentials {
            username,
            password: SecretString::new(password),
        }
    )
}