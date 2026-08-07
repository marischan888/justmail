use chrono::{TimeDelta, Utc};
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};
use crate::helpers::spawn_app;

#[tokio::test]
async fn confirmation_without_token_are_rejected_with_a_link() {
    let app = spawn_app().await;
    let response = reqwest::get(&format!("{}/subscriptions/confirm", app.address))
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn confirmation_failed_if_there_is_a_fatal_database_error() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    app.post_subscriptions(body.into()).await;
    let received_request = &app.email_server
        .received_requests()
        .await
        .unwrap()[0];
    let confirmation_link = app.get_confirmation_links(&received_request);
    // Act
    sqlx::query!("ALTER TABLE subscription_tokens DROP COLUMN subscriber_id;")
        .execute(&app.db_pool)
        .await
        .unwrap();
    let response = reqwest::get(confirmation_link.html_link)
        .await
        .unwrap();
    // Arrange
    assert_eq!(response.status().as_u16(), 500);
}

// not a database fatal error
#[tokio::test]
async fn confirmation_failed_given_a_unknown_token() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    app.post_subscriptions(body.into()).await;
    let received_request = &app.email_server
        .received_requests()
        .await
        .unwrap()[0];
    let mut confirmation_link = app.get_confirmation_links(&received_request).html_link;
    confirmation_link.set_query(Some("subscription_token=haha"));
    // Act
    let response = reqwest::get(confirmation_link)
        .await
        .unwrap();
    // Arrange
    assert_eq!(response.status().as_u16(), 401);
}

#[tokio::test]
async fn the_link_returned_by_subscribe_returns_a_200_if_called() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    let received_request = &app.email_server
        .received_requests()
        .await
        .unwrap()[0];
    let confirmation_link = app.get_confirmation_links(&received_request);
    // Act
    let response = reqwest::get(confirmation_link.html_link)
        .await
        .unwrap();
    // Assert
    assert_eq!(response.status().as_u16(), 200);
}

#[tokio::test]
async fn clicking_on_confirmation_link_confirms_a_subscriber() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    let received_request = &app.email_server
        .received_requests()
        .await
        .unwrap()[0];
    let confirmation_link = app.get_confirmation_links(&received_request);
    reqwest::get(confirmation_link.html_link)
        .await
        .unwrap();
    // Act
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch saved subscription");
    // Assert
    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "confirmed");
}

#[tokio::test]
async fn confirmed_subscriber_click_twice() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;
    let received_request = &app.email_server
        .received_requests()
        .await
        .unwrap()[0];
    let confirmation_link = app.get_confirmation_links(&received_request);
    // Act1: click first for confirming subscription
    let response = reqwest::get(confirmation_link.html_link.clone())
        .await
        .unwrap();
    let html_content = response.text().await.unwrap();
    assert!(html_content.contains("<h1>Subscribe Successfully.</h1>"));
    let response = reqwest::get(confirmation_link.html_link)
        .await
        .unwrap();
    let html_content = response.text().await.unwrap();
    assert!(html_content.contains("<h1>You have already subscribed.</h1>"))
}

#[tokio::test]
async fn confirming_link_should_be_expired_after_two_days() {
    let app = spawn_app().await;
    let token = "expiredaaaaaaaaaaaaaaaaaa";
    let subscriber_id = Uuid::new_v4();

    // Insert subscriber
    sqlx::query!(
        r#"INSERT INTO subscriptions (id, email, name, status, subscribed_at) 
        VALUES ($1, $2, $3, $4, $5)"#,
        subscriber_id,
        "user@example.com",
        "Test User",
        "pending_confirmation",
        Utc::now()
    )
    .execute(&app.db_pool)
    .await
    .unwrap();

    // Insert an expired token
    let expired_timestamp = Utc::now() - TimeDelta::days(6);
    sqlx::query!(
        r#"
        INSERT INTO subscription_tokens (subscription_token, subscriber_id, created_at)
        VALUES ($1, $2, $3)
        "#,
        token,
        subscriber_id,
        expired_timestamp
    )
    .execute(&app.db_pool)
    .await
    .unwrap();

    // Act: Call the confirmation endpoint
    let response = reqwest::Client::new()
        .get(&format!("{}/subscriptions/confirm?subscription_token={}", app.address, token))
        .send()
        .await
        .expect("Failed to execute request.");
    assert_eq!(response.status().as_u16(), 200);
    let body = response.text().await.unwrap();
    assert!(body.contains("This confirmation link has been expired"));
    // token has been removed
    let saved = sqlx::query!(
        r#"SELECT * FROM subscription_tokens WHERE subscription_token = $1"#,
        token
    )
        .fetch_optional(&app.db_pool)
        .await
        .unwrap();
    assert!(saved.is_none());
}

