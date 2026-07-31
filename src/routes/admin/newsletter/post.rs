use actix_web::{HttpResponse, web};
use actix_web_flash_messages::FlashMessage;
use sqlx::PgPool;
use askama::Template;
use crate::authentication::UserId;
use crate::idempotency::{IdempotencyKey, NextAction, save_response, try_processing};
use crate::utils::{e500, see_other, e400};
use crate::email_client::EmailClient;
use crate::domain::SubscriberEmail;

#[derive(Template)]
#[template(path = "newsletter.html")]
struct NewsletterTemplate<'a> {
    content_html: &'a str,
}

#[derive(serde::Deserialize)]
pub struct FormData {
    title: String,
    content: String,
    idempotency_key: String,
}

fn success_message() -> FlashMessage {
    FlashMessage::info("You has issued newsletter to all your subscribers.")
}


#[tracing::instrument
(
    name = "Send email to the subscriber"
    skip(form, pool, email_client),
    fields (
        user_name=tracing::field::Empty,
        user_id=tracing::field::Empty,
    ),
)
]
pub async fn issue_newsletters(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error>{
    let FormData {
        title,
        content,
        idempotency_key,
    } = form.0;
    let user_id = user_id.into_inner();
    let idempotency_key: IdempotencyKey = idempotency_key.try_into().map_err(e400)?;
    let transaction = match try_processing(&pool, &idempotency_key, *user_id)
        .await
        .map_err(e500)?
    {
        NextAction::StartProcessing(t) => t,
        NextAction::ReturnSavedResponse(saved_response) => {
            success_message().send();
            return Ok(saved_response)
        }
    };
    // format html
    let formatted_html = text_to_simple_html(content.clone());
    let html_template = NewsletterTemplate {
        content_html: &formatted_html,
    };
    let html_body = html_template.render().map_err(e500)?;
    // email client send request
    let confirmed_subscribers = get_confirmed_subscriber(&pool)
        .await
        .map_err(e500)?;
    if confirmed_subscribers.is_empty() {
        FlashMessage::error("no confirmed subscriber").send();
        return Ok(see_other("/admin/newsletters"))
    }
    for subscriber in confirmed_subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email, 
                        &title,
                        &html_body,
                        &content,
                    )
                    .await
                    .map_err(e500)?;
            }
            Err(error) => {
                tracing::warn!(
                    error.cause_chain = ?error,
                    "Skipping a confirmed subscriber. \n Their stored emails are invalid."
                );
            }
        }
    }
    let response = see_other("/admin/newsletters");
    let response = save_response(transaction, &idempotency_key, *user_id, response)
        .await
        .map_err(e500)?;
    success_message().send();
    Ok(response)
}

fn text_to_simple_html(text: String) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    let paragraphs: Vec<String> = escaped
        .split("\n\n")
        .map(|para| format!("<p>{}</p>", para.replace('\n', "<br/>")))
        .collect();

    paragraphs.join("")
}

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument
(
    name = "Get confirmed subscriber from database",
    skip(pool)
)
]
async fn get_confirmed_subscriber(
    pool: &PgPool
)
    -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, anyhow::Error> {
    let confirmed_subscriber = sqlx::query!(
        r#"SELECT email FROM subscriptions WHERE status = 'confirmed'"#,
    )
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            match SubscriberEmail::parse(row.email)
            {
                Ok(email) => Ok(ConfirmedSubscriber { email }),
                Err(error) => Err(anyhow::anyhow!(error)), // empty email will also be here
            }
        })
        .collect();
    Ok(confirmed_subscriber)
}
