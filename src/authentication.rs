use anyhow::Context;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use secrecy::{ExposeSecret, SecretString};
use sqlx::PgPool;
use crate::telemetry::spawn_blocking_with_tracing;

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials.")]
    InvalidCredentials(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

pub struct Credentials {
    pub username: String,
    pub password: SecretString,
}

#[tracing::instrument
(
    name = "Validate Credentials",
    skip(credentials, pool)
)
]
pub async fn validate_credentials(
    credentials: Credentials,
    pool: &PgPool,
) -> Result<uuid::Uuid, AuthError> {
    // generate an non-existing user as default value
    let mut user_id = None;
    let mut expected_password_hash = SecretString::new(
        "$argon2id$v=19$m=15000,t=2,p=1$\
        gZiV/M1gPc22ElAH/Jh1Hw$\
        CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
            .to_string()
            .into_boxed_str()
    );
    if let Some((stored_user_id, stored_password_hash)) = get_stored_credentials(
        &credentials.username,
        pool
    )
        .await?
    {
        user_id = Some(stored_user_id);
        expected_password_hash = stored_password_hash;
    }

    spawn_blocking_with_tracing(move ||
        {
            verify_password_hash(
                expected_password_hash,
                credentials.password,
            )
        }
    )
        .await
        .context("Failed to spawn blocking task")??;

    user_id
        .ok_or_else(|| anyhow::anyhow!("Unknown user name."))
        .map_err(AuthError::InvalidCredentials)
}

#[tracing::instrument
(
    name = "Get credentials from users.",
    skip(username, pool)
)
]
async fn get_stored_credentials (
    username: &str,
    pool: &PgPool,
) -> Result<Option<(uuid::Uuid, SecretString)>, anyhow::Error> {
    let record = sqlx::query!(
        r#"
        SELECT user_id, password_hash
        FROM users
        WHERE username = $1"#,
        username,
    )
        .fetch_optional(pool)
        .await
        .context("Failed to fetch credentials from the database.")?
        .map(|record| {
            (record.user_id, SecretString::from(record.password_hash))
        });
    Ok(record)
}

#[tracing::instrument
(
    name = "Verify password hash",
    skip(expected_password_hash, password_hash)
)
]
fn verify_password_hash(
    expected_password_hash: SecretString,
    password_hash: SecretString,
) -> Result<(), AuthError> {
    let expected_parsed_hash = PasswordHash::new(
        expected_password_hash.expose_secret()
    )
        .context("Failed to parse hash in PHC string format.")?;

    Argon2::default()
        .verify_password
        (
            password_hash.expose_secret().as_bytes(),
            &expected_parsed_hash,
        )
        .context("Invalid user password.")
        .map_err(AuthError::InvalidCredentials)
}
