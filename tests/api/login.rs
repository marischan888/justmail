use std::collections::HashSet;
use reqwest::header::HeaderValue;
use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    let app = spawn_app().await;
    let login_body = serde_json::json!({
        "username": "username",
        "password": "password",
    });
    // Act 1: try to log in
    let response = app.post_login(&login_body).await;
    assert_is_redirect_to(&response, "/login");
    // Act 2: Follow the redirect
    let html_page = app.get_login_html().await;
    assert!(html_page.contains("<p><i>Authentication failed</i></p>"));
    // Act 3: Reload the login page, should not show error message when reload the page
    assert!(!app.get_login_html().await.contains("<p><i>Authentication failed</i></p>"));
}
