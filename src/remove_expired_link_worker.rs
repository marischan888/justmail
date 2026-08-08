use std::time::Duration;

use sqlx::PgPool;

use crate::{configuration::Settings, startup::get_connection_pool};

pub async fn run_token_worker_until_stopped(
    configuration: Settings
) -> Result<(), anyhow::Error>{
    let connection_pool = get_connection_pool(&configuration.database);
    token_worker_loop(connection_pool).await
}

//TODO: Not sure if i need the retry logic
async fn token_worker_loop(
    pool: PgPool,
) -> Result<(), anyhow::Error> {
    loop {
        match try_clear_expired_token(&pool).await {
            Ok(ClearTokenOutcome::EmptyQueue) => {
                tokio::time::sleep(Duration::from_hours(24)).await;
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
            Ok(ClearTokenOutcome::TaskCompleted) => {}
        }
    }
}

pub enum ClearTokenOutcome {
    TaskCompleted,
    EmptyQueue,
}

#[tracing::instrument(
    level = "trace",
    skip_all,
    err,
)]
pub async fn try_clear_expired_token(
    pool: &PgPool
) -> Result<ClearTokenOutcome, anyhow::Error>{
    // single query is atomic by default in pgpool, so do not need a transaction
    let n_rows_affected = sqlx::query!(
        r#"
        DELETE FROM subscription_tokens
        WHERE created_at < NOW() - INTERVAL '2 days'
        "#,
    )
        .execute(pool)
        .await?
        .rows_affected();
    if n_rows_affected != 0 {
        Ok(ClearTokenOutcome::TaskCompleted)
    } else {
        Ok(ClearTokenOutcome::EmptyQueue)
    }
}
