use actix_web::{HttpResponse, web};
use chrono::Utc;
use sqlx::{PgPool};
use sqlx::types::Uuid;

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

pub async fn subscribe(
    from: web::Form<FormData>,
    pool: web::Data<PgPool>,
) -> HttpResponse {
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
        .await {
            Ok(_) => HttpResponse::Ok().finish(),
            Err(e) => {
                eprintln!("Failed to execute query: {:?}", e);
                HttpResponse::InternalServerError().finish()
            }
        }
}

