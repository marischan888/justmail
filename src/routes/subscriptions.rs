use actix_web::{HttpResponse, web};
use chrono::Utc;
use sqlx::{PgPool};
use sqlx::types::Uuid;
use tracing::Instrument;

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

pub async fn subscribe(
    from: web::Form<FormData>,
    pool: web::Data<PgPool>,
) -> HttpResponse {
    let request_id = Uuid::new_v4();
    let request_span = tracing::info_span!(
        "Adding a new subscriber.",
        %request_id,
        subscriber_email = %from.email,
        subscriber_name = %from.name,
    );
    let _request_span_guard = request_span.enter();
    let query_span = tracing::info_span!(
        "Saving new subscriber details in the database.",
    );
    match sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        from.email,
        from.name,
        Utc::now(),
    )
        .execute(pool.as_ref())
        .instrument(query_span)
        .await {
            Ok(_) => {
                tracing::info!(
                    "request_id {} - New subscriber details have been saved",
                    request_id
                );
                HttpResponse::Ok().finish()
            },
            Err(e) => {
                tracing::error!(
                    "request_id {} - Failed to execute query: {:?}",
                    request_id,
                    e
                );
                HttpResponse::InternalServerError().finish()
            }
        }
}

