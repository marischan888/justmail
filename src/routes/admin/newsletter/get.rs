use crate::session_state::TypedSession;
use crate::utils::{e500, see_other};
use actix_web::{HttpResponse, http::header::ContentType};
use actix_web_flash_messages::IncomingFlashMessages;
use std::fmt::Write;

#[tracing::instrument(skip(session, flash_messages))]
pub async fn issue_newsletters_form(
    session: TypedSession,
    flash_messages: IncomingFlashMessages,
) -> Result<HttpResponse, actix_web::Error> {
    if session.get_user_id().map_err(e500)?.is_none() {
        return Ok(see_other("/login"));
    };
    let mut msg_html = String::new();
    for msg in flash_messages.iter() {
        writeln!(msg_html, "<p><i>{}</i></p>", msg.content()).unwrap();
    }
    let idempotency_key = uuid::Uuid::new_v4();

    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"<!DOCTYPE html>
            <html lang="en">
            <head>
            <meta http-equiv="content-type" content="text/html; charset=utf-8">
            <title>Issue Newsletter</title>
            </head>
            <body>
            {}
            <form action="/admin/newsletters" method="post">
            <label>Title
            <input
            type="text"
            placeholder="Enter publish title"
            name="title"
            >
            </label>
            <br>
            <label>Content
            <input
            type="text"
            placeholder="Enter publish content"
            name="content"
            >
            </label>
            <br>
            <label>Idempotency Key
            <input
            hidden
            type="text"
            name="idempotency_key"
            value="{idempotency_key}"
            >
            </label>
            <br>
            <button type="submit">Publish</button>
            </form>
            <p><a href="/admin/dashboard">Back</a></p>
            </body>
            </html>"#,
            msg_html
        )))
}
