use crate::authentication::{validate_credentials, AuthError, Credentials};
use crate::routes::error_chain_fmt;
use actix_web::error::InternalError;
use actix_web::http::header::LOCATION;
use actix_web::{web, HttpResponse};
use actix_web::cookie::Cookie;
use actix_web_flash_messages::FlashMessage;
use secrecy::{SecretString};
use serde::Deserialize;
use sqlx::PgPool;

#[derive(Deserialize)]
pub struct FromData {
    username: String,
    password: SecretString
}

#[derive(thiserror::Error)]
pub enum LoginError {
    #[error("Authentication failed")]
    AuthError(#[source] anyhow::Error),
    #[error("Something went wrong")]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

#[tracing::instrument
(
    skip(form,pool),
    fields(
        username=tracing::field::Empty,
        user_id=tracing::field::Empty
    ),
)
]
pub async fn login(
    form: web::Form<FromData>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, InternalError<LoginError>> {
    let credentials = Credentials {
        username: form.0.username,
        password: form.0.password
    };

    match validate_credentials(credentials, &pool).await {
        Ok(user_id) => {
            tracing::Span::current()
                .record("userid", &tracing::field::display(&user_id));
            Ok(
                HttpResponse::SeeOther()
                    .insert_header((LOCATION, "/"))
                    .finish()
            )
        }
        Err(error) => {
            let error = match error {
                AuthError::InvalidCredentials(_) => LoginError::AuthError(error.into()),
                AuthError::UnexpectedError(_) => LoginError::UnexpectedError(error.into()),
            };
            // Send one-time notification
            FlashMessage::error(error.to_string()).send();
            let response = HttpResponse::SeeOther()
                .insert_header((LOCATION, "/login",))
                .finish();
            Err(InternalError::from_response(error, response))
        }
    }
}