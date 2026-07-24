use actix_web::{HttpResponse, http::header::ContentType};
use actix_web_flash_messages::{IncomingFlashMessages};
use std::fmt::Write;
use crate::session_state::TypedSession;
use crate::utils::{see_other, e500};

#[tracing::instrument
(
    skip(session, flash_messages),
)]
pub async fn issue_newsletter_form(
    session: TypedSession,
    flash_messages: IncomingFlashMessages,
) -> Result<HttpResponse, actix_web::Error> {
    if session.get_user_id().map_err(e500)?.is_none() {
        return Ok(see_other("/login"));
    } ;
    let mut msg_html = String::new();
    for msg in flash_messages.iter()
    {
        writeln!(msg_html, "<p><i>{}</i></p>", msg.content()).unwrap();
    }

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
            <form action="/admin/newsletter" method="post">
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
            </form>
            <p><a href="/admin/dashboard">Back</a></p>
            </body>
            </html>"#, msg_html)
        )
    )
}
