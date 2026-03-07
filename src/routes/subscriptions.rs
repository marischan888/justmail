use actix_web::{HttpResponse, web};
use chrono::Utc;
use sqlx::{PgPool};
use sqlx::types::Uuid;
use crate::domain::{NewSubscriber, SubscriberEmail, SubscriberName};

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
    // try_into is a mirror of tru_from, directly take self dont need to write A::try_from
    let new_subscriber = match from.0.try_into() {
        Ok(new_subscriber) => new_subscriber,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };
    match insert_subscriber(&new_subscriber, &pool).await {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[tracing::instrument(
name = "Saving new subscriber details in the database.",
skip(new_subscriber, pool)
)]
pub async fn insert_subscriber(
    new_subscriber: &NewSubscriber,
    pool: &PgPool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'confirmed')
        "#,
        Uuid::new_v4(),
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(), // read-only value
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
