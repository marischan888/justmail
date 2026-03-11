use claims::assert_err;
use crate::helpers::spawn_app;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;
    let response = app.post_subscriptions(body.into()).await;

    assert_eq!(200, response.status().as_u16());
}

#[tokio::test]
async fn subscribe_persist_new_subscribers() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch saved subscription.");
    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "pending_confirmation");
}

#[tokio::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    let app = spawn_app().await;
    let test_cases = vec![
        ("name=le%20guin", "missing the email"),
        ("email=ursula_le_guin%40gmail.com", "missing the name"),
        ("", "missing both name and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        let response = app.post_subscriptions(invalid_body.into()).await;

        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 Bad Request when the payload was {}.",
            error_message
        )
    }
}

#[tokio::test]
async fn subscribe_returns_a_400_when_fields_are_present_but_invalid() {
    // Arrange
    let app = spawn_app().await;
    let test_cases = vec![
        ("name=&email=ursula_le_guin%40gmail.com", "empty name"),
        ("name=Ursula&email=", "empty email"),
        ("name=Ursula&email=definitely-not-an-email", "invalid email"),
    ];
    for (body, description) in test_cases {
        // Act
        let response = app.post_subscriptions(body.into()).await;
        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not return a 400 Bad Request when the payload was {}.",
            description
        );
    }
}

#[tokio::test]
async fn subscribe_sends_a_confirmation_email_for_valid_data() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;
}

#[tokio::test]
async fn subscriber_sends_a_confirmation_email_with_a_link() {
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

    let raw_confirmation_link = app.get_confirmation_links(&received_request);
    assert_eq!(raw_confirmation_link.html_link, raw_confirmation_link.plain_text);
}

/*
1. pending user will receive a new email while the old ones will be invalid
2. confirmed user get a friendly reminder
*/

#[tokio::test]
async fn pending_subscriber_will_receive_new_email_if_they_subscribe_twice_with_same_email() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;
    // Act
    app.post_subscriptions(body.into()).await;
    app.post_subscriptions(body.into()).await;
    // Arrange
    let received_request = &app.email_server.received_requests().await.unwrap();
    assert_eq!(received_request.len(), 2);
    let first_request_token = &app.get_confirmation_link_token(&received_request[0]);
    let second_request_token = &app.get_confirmation_link_token(&received_request[1]);
    assert_ne!(first_request_token, second_request_token);
    let query_first_token = sqlx::query!(
        "SELECT subscription_token FROM subscription_tokens WHERE subscription_token = $1",
        first_request_token
    )
        .fetch_one(&app.db_pool)
        .await;
    assert_err!(query_first_token);
    let query_second_token = sqlx::query!(
        "SELECT subscription_token FROM subscription_tokens WHERE subscription_token = $1",
        second_request_token
    )
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch subscription_token");
    assert_eq!(query_second_token.subscription_token.as_str(), second_request_token);
    let saved = sqlx::query!(
        "SELECT COUNT(*) FROM subscription_tokens",
    )
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch subscription_tokens");
    assert_eq!(saved.count, Some(1));
}

#[tokio::test]
async fn confirmed_subscriber_will_not_receive_new_email_if_they_subscribe_twice_with_same_email() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;
    // Act
    app.post_subscriptions(body.into()).await;
    let received_request = &app.email_server.received_requests().await.unwrap();
    let confirmation_link = app.get_confirmation_links(&received_request[0]);
    reqwest::get(confirmation_link.html_link)
        .await
        .unwrap();
    let second_response = app.post_subscriptions(body.into()).await;
    let requests = &app.email_server.received_requests().await.unwrap();
    // Arrange
    assert_eq!(second_response.status().as_u16(), 200);
    assert_eq!(requests.len(), 1);
    let saved = sqlx::query!(
        "SELECT COUNT(*) FROM subscription_tokens",
    )
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch subscription_tokens");
    assert_eq!(saved.count, Some(1));

}