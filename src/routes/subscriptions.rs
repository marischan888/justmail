use actix_web::{HttpResponse, web};
use chrono::Utc;
use sqlx::{PgPool};
use sqlx::types::Uuid;

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

#[tracing::instrument(
name = "Adding a new subscriber",
skip(from, pool),
fields(
subscriber_email = %from.email,
subscriber_name = %from.name
)
)]
pub async fn subscribe(
    from: web::Form<FormData>,
    pool: web::Data<PgPool>,
) -> HttpResponse {
    match insert_subscriber(&from, &pool).await {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[tracing::instrument(
name = "Saving new subscriber details in the database.",
skip(from, pool)
)]
pub async fn insert_subscriber(
    from: &FormData,
    pool: &PgPool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        from.email,
        from.name,
        Utc::now(),
    )
        .execute(pool)
        .await
        .map_err(|err| {
            tracing::error!("Failed to execute query: {:?}", err);
            err
        })?;
    Ok(())
}
