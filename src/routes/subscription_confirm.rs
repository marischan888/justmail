use actix_web::{HttpResponse, web};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct Parameters {
    subscription_token: String,
}

// web::Query make sure the existence of Parameters or return 400
#[tracing::instrument
(
    name = "Confirming a pending subscriber",
    skip(parameters, pool)
)
]
pub async fn subscription_confirm(
    parameters: web::Query<Parameters>,
    pool: web::Data<PgPool>,
) -> HttpResponse {
    let subscriber_id = match get_subscriber_id_from_token(
        &pool,
        &parameters.subscription_token,
    ).await
    {
        Ok(subscriber_id) => subscriber_id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    match subscriber_id {
        None => HttpResponse::Unauthorized().finish(),
        Some(subscriber_id) => {
            if mark_subscriber_confirmed(&pool, subscriber_id).await.is_err() {
                return HttpResponse::InternalServerError().finish();
            }
            HttpResponse::Ok().finish()
        }
    }
}


#[tracing::instrument
(
    name = "Get subscirber_id from token",
    skip(pool, subscription_token),
)]
pub async fn get_subscriber_id_from_token(
    pool: &PgPool,
    subscription_token: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    // result: Record{subscriber_id}
    let result = sqlx::query!(
        r#"SELECT subscriber_id FROM subscription_tokens WHERE subscription_token = $1"#,
        subscription_token
    )
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch subscriber_id: {:?}", e);
            e
        })?;
    Ok(result.map(|r| r.subscriber_id))
}

#[tracing::instrument
(
    name = "Mark subscriber as confirmed",
    skip(pool, subscriber_id),
)]
pub async fn mark_subscriber_confirmed(
    pool: &PgPool,
    subscriber_id: Uuid
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE subscriptions SET status = 'confirmed' WHERE id = $1"#,
        subscriber_id,
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to mark subscriber as confirmed: {:?}", e);
        e
    })?;
    Ok(())
}