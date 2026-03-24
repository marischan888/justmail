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
async fn confirmation_failed_given_a_unknow_token() {
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
async fn click_same_confirmation_link_twice_results_401_for_confirmed_subscribers() {
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
    reqwest::get(confirmation_link.html_link).await.unwrap();
    let response  = reqwest::get(confirmation_link.plain_text).await.unwrap();
    let query_result = sqlx::query!(
        r#"SELECT status FROM subscriptions WHERE email = 'ursula_le_guin@gmail.com'"#
    ).fetch_one(&app.db_pool).await.expect("Failed to fetch saved subscription");
    // Arrange
    assert_eq!(response.status().as_u16(), 401);
    assert_eq!(query_result.status.as_str(), "confirmed");
}

#[tokio::test]
async fn click_expire_link_results_401() {
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
    let token = &app.get_confirmation_links(&received_request).token;
    // Act
    sqlx::query!(
        r#"UPDATE subscription_tokens
        SET created_at = now() - INTERVAL '25 hours'
        WHERE subscription_token = $1
        "#,
        token
    )
        .execute(&app.db_pool)
        .await
        .expect("Failed to update subscription_tokens");

    let confirmation_link = app.get_confirmation_links(&received_request);
    let response= reqwest::get(confirmation_link.html_link).await.unwrap();
    let query_result = sqlx::query!(
        r#"SELECT status FROM subscriptions WHERE email = 'ursula_le_guin@gmail.com'"#
    ).fetch_one(&app.db_pool).await.expect("Failed to fetch saved subscription");
    // Arrange
    assert_eq!(response.status().as_u16(), 401);
    assert_eq!(query_result.status.as_str(), "pending_confirmation");
}

/*
TODO: what happen if subscription token is well-formatted but not existence? "invalid request,
 please confirm again"
1. Click the previous link (invalid token)
2. Somebody else fake the token
*/
