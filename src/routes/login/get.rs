use std::fmt::Write;
use actix_web::http::header::ContentType;
use actix_web::{HttpResponse};
use actix_web_flash_messages::{IncomingFlashMessages};

#[tracing::instrument
(
    skip(flash_messages),
)]

pub async fn login_form(flash_messages: IncomingFlashMessages) -> HttpResponse {
    let mut error_html = String::new();
    // display all level flash message
    for msg in flash_messages.iter()
    {
        writeln!(error_html, "<p><i>{}</i></p>", msg.content()).unwrap();
    }

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"<!DOCTYPE html>
            <html lang="en">
            <head>
            <meta http-equiv="content-type" content="text/html; charset=utf-8">
            <title>Login</title>
            </head>
            <body>
            {}
            <form action="/login" method="post">
            <label>Username
            <input
            type="text"
            placeholder="Enter Username"
            name="username"
            >
            </label>
            <label>Password
            <input
            type="password"
            placeholder="Enter Password"
            name="password"
            >
            </label>
            <button type="submit">Login</button>
            </form>
            </body>
            </html>"#,
            error_html)
        )
}
