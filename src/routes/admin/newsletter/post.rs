use actix_web::{HttpResponse, web};
use actix_web_flash_messages::FlashMessage;
use secrecy::SecretString;
use sqlx::PgPool;
use askama::Template;
use crate::authentication::{AuthError, Credentials, UserId, validate_credentials};
use crate::routes::admin::dashboard::get_username;
use crate::utils::{e500, see_other};
use crate::email_client::EmailClient;
use crate::domain::SubscriberEmail;

#[derive(Template)]
#[template(path = "newsletter.html")]
struct NewsletterTemplate<'a> {
    content_html: &'a str,
}

#[derive(serde::Deserialize)]
pub struct FormData {
    current_password: SecretString,
    title: String,
    content: String,
}

#[tracing::instrument
(
    name = "Send email to the subscriber"
    skip(form, pool, email_client, user_id),
    fields (
        user_name=tracing::field::Empty,
        user_id=tracing::field::Empty,
    ),
)
]
pub async fn issue_newsletter(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error>{
    let user_id = user_id.into_inner();
    let user_name = get_username(*user_id, &pool).await.map_err(e500)?;
    // record user info
    tracing::Span::current().record(
        "user_name",
        tracing::field::display(&user_name),
    );
    tracing::Span::current().record(
        "user_id",
        tracing::field::display(*user_id),
    );
    let credential = Credentials {
        username: user_name,
        password: form.0.current_password,
    };
    if let Err(e) = validate_credentials(credential, &pool).await {
        return match e {
            AuthError::InvalidCredentials(_) => {
                FlashMessage::error("Wrong current password").send();
                Ok(see_other("/admin/newsletter"))
            }
            AuthError::UnexpectedError(_) => Err(e500(e).into()),
        }
    }
    // format html
    let plain_text = form.0.content;
    let formatted_content = text_to_simple_html(plain_text.clone());
    let html_template = NewsletterTemplate {
        content_html: &formatted_content,
    };
    let html_body = html_template.render().map_err(e500)?;
    // email client send request
    let confirmed_subscribers = get_confirmed_subscriber(&pool)
        .await
        .map_err(e500)?;
    if confirmed_subscribers.is_empty() {
        FlashMessage::error("no confirmed subscriber").send();
        return Ok(see_other("/admin/newsletter"))
    }
    for subscriber in confirmed_subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email, 
                        &form.0.title,
                        &html_body,
                        &plain_text,
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
    FlashMessage::error("You has issued newsletter to all your subscribers.").send();
    Ok(see_other("/admin/newsletter"))
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
