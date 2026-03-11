use actix_web::{HttpResponse, web};
use chrono::Utc;
use sqlx::{Executor, PgPool, Postgres};
use sqlx::types::Uuid;
use rand::distr::Alphanumeric;
use rand::{rng, RngExt};
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

pub struct ExistingSubscriber {
    pub subscriber_id: Uuid,
    pub status: String,
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

    // impl Executor for Transaction
    let mut transaction = match pool.begin().await {
        Ok(transaction) => transaction,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    // temp handle of transaction
    let existing_subscriber = match insert_subscriber(&new_subscriber, &mut *transaction).await {
        Ok(insertion_result) => insertion_result,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    // check status
    if existing_subscriber.status == "confirmed" {
        if transaction.commit().await.is_err() {
            return HttpResponse::InternalServerError().finish();
        }
        return HttpResponse::Ok().body("You are already subscribed! No further action needed.");
    }

    if delete_old_token(
        &mut *transaction,
        existing_subscriber.subscriber_id
    )
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    let subscription_token = generate_subscription_token();
    if store_token
        (
            &mut *transaction,
            existing_subscriber.subscriber_id,
            &subscription_token
        )
        .await
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    if transaction.commit().await.is_err() {
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
    name = "Removing previous token under the same subscriber",
    skip(executor, subscriber_id)
)
]
pub async fn delete_old_token(
    executor: impl Executor<'_, Database = Postgres>,
    subscriber_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM subscription_tokens WHERE subscriber_id = $1",
        subscriber_id
    )
        .execute(executor)
        .await
        .map_err(|e| {
            tracing::error!("Failed to execute query: {:?}", e);
            e
        })?;
    Ok(())
}

#[tracing::instrument
(
    name = "Store subscription token and subscriber ID",
    skip(executor, subscriber_id, subscription_token),
)
]
pub async fn store_token(
    executor: impl Executor<'_, Database=Postgres>,
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
        .execute(executor)
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
        base_url, // application settings
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
    skip(new_subscriber, executor)
)
]
pub async fn insert_subscriber(
    new_subscriber: &NewSubscriber,
    executor: impl Executor<'_, Database=Postgres>,
) -> Result<ExistingSubscriber, sqlx::Error>
{
    let subscriber_id = Uuid::new_v4();
    let insertion_result = sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        ON CONFLICT (email) DO UPDATE SET name=EXCLUDED.name
        RETURNING id, status
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(), // read-only value
        Utc::now(),
    )
        .fetch_one(executor)
        .await
        .map_err(|err|
            {
                tracing::error!("Failed to execute query: {:?}", err);
                err
            }
        )?;
    Ok(
        ExistingSubscriber {
            subscriber_id: insertion_result.id,
            status: insertion_result.status,
        }
    )
}
