use actix_web::HttpResponse;
use actix_web::http::header::ContentType;
use actix_web_flash_messages::IncomingFlashMessages;
use std::fmt::Write;

#[tracing::instrument(skip(flash_messages))]

pub async fn subscribe_form(flash_messages: IncomingFlashMessages) -> HttpResponse {
    let mut error_html = String::new();
    // display all level flash message
    for msg in flash_messages.iter() {
        writeln!(error_html, "<p><i>{}</i></p>", msg.content()).unwrap();
    }

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"<!DOCTYPE html>
            <html lang="en">
            <head>
            <meta http-equiv="content-type" content="text/html; charset=utf-8">
            <title>Subscribe me!</title>
            </head>
            <body>
            <div>
            <p>
            The unsubscribe is inavalible (too lazy to implement).
            Every update deserve a read. Thx for support!
            </p>
            {}
            </div>
            <form action="/subscription" method="post">
            <label>Name
            <input
            type="text"
            placeholder="Enter name"
            name="name"
            >
            </label>
            <label>Email
            <input
            type="text"
            placeholder="Enter email"
            name="email"
            >
            </label>
            <button type="submit">Subscribe</button>
            </form>
            </body>
            </html>"#,
            error_html
        ))
}
