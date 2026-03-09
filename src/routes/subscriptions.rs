use actix_web::{HttpResponse, web};
use chrono::Utc;
use sqlx::{PgPool};
use sqlx::types::Uuid;
use rand::distr::Alphanumeric;
use rand::{Rng, rng, RngExt};
use crate::domain::{NewSubscriber, SubscriberEmail, SubscriberName};
use crate::email_client::EmailClient;
use crate::startup::ApplicationBaseUrl;

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
) -> HttpResponse {
    // try_into is a mirror of tru_from, directly take self do bot need to write A::try_from
    let new_subscriber = match from.0.try_into() {
        Ok(new_subscriber) => new_subscriber,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };

    let subscriber_id = match insert_subscriber(&new_subscriber, &pool).await {
        Ok(subscriber_id) => subscriber_id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let subscription_token = generate_subscription_token();
    if store_token
        (
            &pool,
            subscriber_id,
            &subscription_token
        )
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    // take ownership of subscriber since it is the end point
    if send_confirmation_email
        (
            &email_client,
            new_subscriber,
            &base_url.0,
            &subscription_token,
        )
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    HttpResponse::Ok().finish()
}

#[tracing::instrument
(
    name = "Store subscription token and subscriber ID",
    skip(pool, subscriber_id, subscription_token),
)
]
pub async fn store_token(
    pool: &PgPool,
    subscriber_id: Uuid,
    subscription_token: &String,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)
        "#,
        subscription_token,
        subscriber_id,
    )
        .execute(pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to store subscription token: {:?}", e);
            e
        })?;
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
    receiver: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    // a static confirm link with token
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url,
        subscription_token);

    email_client.send_email(
        receiver.email,
        "Welcome!",
        &format!(
            "Welcome to Maris Park!<br />\
            Click <a href=\"{}\">here</a> to confirm your subscription!",
            confirmation_link
        ),
        &format!(
            "Welcome to Maris Park!\nVisit {} to confirm your subscription!",
            confirmation_link
        ),
    )
        .await
        .map(|err|
            (
                tracing::error!("Error sending confirmation email: {:?}", err),
                err
            )
        )?;
    Ok(())
}

#[tracing::instrument
(
    name = "Saving new subscriber details in the database.",
    skip(new_subscriber, pool)
)
]
pub async fn insert_subscriber(
    new_subscriber: &NewSubscriber,
    pool: &PgPool,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(), // read-only value
        Utc::now(),
    )
        .execute(pool)
        .await
        .map_err(|err|
            {
                tracing::error!("Failed to execute query: {:?}", err);
                err
            }
        )?;
    Ok(subscriber_id)
}
