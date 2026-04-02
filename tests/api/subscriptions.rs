use crate::helpers::spawn_app;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(201))
        .expect(1)
        .mount(&app.email_server)
        .await;
    let response = app.post_subscriptions(body.into()).await;

    assert_eq!(201, response.status().as_u16());
}

#[tokio::test]
async fn subscribe_persist_new_subscribers() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(201))
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
        .respond_with(ResponseTemplate::new(201))
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
        .respond_with(ResponseTemplate::new(201))
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

// test log
#[tokio::test]
async fn subscribe_fails_if_there_is_a_fatal_database_error() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    // Act
    sqlx::query!("ALTER TABLE subscriptions DROP COLUMN email;")
        .execute(&app.db_pool)
        .await
        .unwrap();
    let response = app.post_subscriptions(body.into()).await;
    // Arrange
    assert_eq!(response.status().as_u16(), 500);
}

#[tokio::test]
async fn subscriber_will_receive_two_email_when_subscribe_twice_with_same_email() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&app.email_server)
        .await;

    // Act
    app.post_subscriptions(body.into()).await;
    app.post_subscriptions(body.into()).await;
    // Arrange
    let received_request = &app.email_server.received_requests().await.unwrap();
    assert_eq!(received_request.len(), 2);

    let request_one = &received_request[0];
    let raw_confirmation_link_one = app.get_confirmation_links(&request_one);
    assert_eq!(raw_confirmation_link_one.html_link, raw_confirmation_link_one.plain_text);

    let request_two = &received_request[1];
    let raw_confirmation_link_two = app.get_confirmation_links(&request_two);
    assert_eq!(raw_confirmation_link_two.html_link, raw_confirmation_link_two.plain_text);

    assert_ne!(raw_confirmation_link_one.token, raw_confirmation_link_two.token);
}

#[tokio::test]
async fn subscribe_twice_with_same_email_will_get_distinct_token_under_the_same_subscriber_id() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&app.email_server)
        .await;

    // Act
    app.post_subscriptions(body.into()).await;
    app.post_subscriptions(body.into()).await;
    let received_request = &app.email_server.received_requests().await.unwrap();
    let token_one = app.get_confirmation_links(&received_request[0]).token;
    let token_two = app.get_confirmation_links(&received_request[1]).token;

    let id_one = sqlx::query!("SELECT subscriber_id FROM subscription_tokens WHERE \
    subscription_token = $1",
        token_one).fetch_one(&app.db_pool).await.unwrap().subscriber_id;
    let id_two = sqlx::query!("SELECT subscriber_id FROM subscription_tokens WHERE \
    subscription_token = $1",
        token_two).fetch_one(&app.db_pool).await.unwrap().subscriber_id;
    let subscriber_cnt = sqlx::query!(
        r#"
        SELECT COUNT(*) FROM subscriptions
        WHERE email = 'ursula_le_guin@gmail.com'
        "#
    )
        .fetch_one(&app.db_pool)
        .await.unwrap().count.unwrap();

    // Arrange
    assert_ne!(token_one, token_two);
    assert_eq!(id_one, id_two);
    assert_eq!(subscriber_cnt, 1);
}

#[tokio::test]
async fn subscribe_twice_with_distinct_name_and_same_email_will_update_name() {
    let app = spawn_app().await;
    let body_one = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let body_two = "name=Canary&email=ursula_le_guin%40gmail.com";

    // Act
    app.post_subscriptions(body_one.into()).await;
    let name_before = sqlx::query!(
        "SELECT name FROM subscriptions WHERE email = 'ursula_le_guin@gmail.com'"
    )
        .fetch_one(&app.db_pool)
        .await
        .unwrap().name;

    app.post_subscriptions(body_two.into()).await;
    let name_update = sqlx::query!(
        "SELECT name FROM subscriptions WHERE email = 'ursula_le_guin@gmail.com'"
    )
        .fetch_one(&app.db_pool)
        .await
        .unwrap().name;
    // Arrange
    assert_eq!(name_update.as_str(), "Canary");
    assert_eq!(name_before.as_str(), "le guin");
    assert_ne!(name_before, name_update);
}

//#[tokio::test]
//async fn confirmed_subscriber_subscribe_with_same_email_results_200() {
//    let app = spawn_app().await;
//    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
//
//    Mock::given(path("/email"))
//        .and(method("POST"))
//        .respond_with(ResponseTemplate::new(200))
//        .mount(&app.email_server)
//        .await;
//    // Act
//    app.post_subscriptions(body.into()).await;
//    let received_request = &app.email_server.received_requests().await.unwrap();
//    let confirmation_link = app.get_confirmation_links(&received_request[0]).html_link;
//    let response = reqwest::get(confirmation_link).await.unwrap();
//    assert_eq!(response.status().as_u16(), 200);
//
//    let post_response = app.post_subscriptions(body.into()).await;
//    assert_eq!(post_response.status(), 200);
//}

#[tokio::test]
async fn confirmed_subscriber_receive_new_link_using_distinct_email() {
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let body_two = "name=le%20guin&email=canary%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&app.email_server)
        .await;
    // Act
    app.post_subscriptions(body.into()).await;
    let received_request = &app.email_server.received_requests().await.unwrap();
    let confirmation_link = app.get_confirmation_links(&received_request[0]);
    let token_one = confirmation_link.token;
    reqwest::get(confirmation_link.html_link).await.unwrap();
    app.post_subscriptions(body_two.into()).await;
    let second_request = &app.email_server.received_requests().await.unwrap()[1];
    let token_two = app.get_confirmation_links(&second_request).token;

    let query_result = sqlx::query!(
        r#"
        SELECT COUNT(*) FROM subscriptions WHERE name = 'le guin'
        "#).fetch_one(&app.db_pool).await.unwrap();
    // Arrange
    assert_ne!(token_one, token_two);
    assert_eq!(query_result.count.unwrap(), 2);
}
