use crate::helpers::{spawn_app, ConfirmationLinks, TestApp};
use wiremock::matchers::{method, any, path};
use wiremock::{Mock, ResponseTemplate};

async fn create_unconfirmed_subscriber(app: &TestApp) -> ConfirmationLinks {
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    let _mock_guard = Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(201))
        .named("Create unconfirmed subscriber")
        .expect(1)
        .mount_as_scoped(&app.email_server)
        .await;
    app.post_subscriptions(body.to_string())
        .await
        .error_for_status()
        .unwrap();

    let request = &app.email_server.received_requests().await.unwrap()[0];
    app.get_confirmation_links(request)
}

async fn create_confirmed_subscriber(app: &TestApp) {
    let link = create_unconfirmed_subscriber(app).await.html_link;
    reqwest::get(link)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

#[tokio::test]
async fn newsletters_are_not_delivered_to_unconfirmed_subscribers() {
    let app = spawn_app().await;
    create_unconfirmed_subscriber(&app).await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&app.email_server)
        .await;
    // Act
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "content": {
            "html": "<p>Newsletter body as HTML</p>",
            "plain": "Newsletter body as plain text",
        }
    });
    let response = app.post_newsletter(newsletter_request_body).await;
    // Arrange
    assert_eq!(response.status().as_u16(), 200)
}

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers() {
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Act
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "content": {
            "html": "<p>Newsletter body as HTML</p>",
            "plain": "Newsletter body as plain text",
        }
    });
    let response = app.post_newsletter(newsletter_request_body).await;
    // Arrange
    assert_eq!(response.status().as_u16(), 200)
}

#[tokio::test]
async fn newsletter_returns_400_for_invalid_data() {
    let app = spawn_app().await;
    // Act
    let invalid_requests = vec![
        (
            serde_json::json!(
            {
                "content":
                {
                    "html": "<p>Newsletter body as HTML</p>",
                    "plain": "Newsletter body as plain text",
                }
            }),
            "missing title",
        ),
        (
            serde_json::json!({"title": "Newsletter!"}),
            "missing content",
        ),
    ];

    for (invalid_body, error_message) in invalid_requests {
        let response = app.post_newsletter(invalid_body).await;

        // Arrange
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not fail with 400 when the payload was {}.)",
            error_message
        )
    }
}

#[tokio::test]
async fn requests_mising_authorization_are_rejected(){
    let app = spawn_app().await;

    let response = reqwest::Client::new()
        .post(&format!("{}/newsletter", app.address))
        .json(&serde_json::json!({
            "title": "Newsletter title",
            "content": {
                "html": "<p>Newsletter body as HTML</p>",
                "plain": "Newsletter body as plain text",
            }
        }))
        .send()
        .await
        .expect("Failed to execute request.");

    assert_eq!(response.status().as_u16(), 401);
    assert_eq!(r#"Basic realm="publish""#, response.headers()["WWW-Authenticate"]);
}