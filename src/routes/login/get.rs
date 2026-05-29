use std::fmt::Write;
use actix_web::http::header::ContentType;
use actix_web::{HttpRequest, HttpResponse};
use actix_web::cookie::Cookie;
use actix_web::cookie::time::Duration;
use actix_web_flash_messages::{IncomingFlashMessages, Level};

pub async fn login_form(flash_message: IncomingFlashMessages) -> HttpResponse {
    let mut error_html = String::new();
    for msg in flash_message.iter()
        .filter(|msg| { msg.level() == Level::Error })
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
