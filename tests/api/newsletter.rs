use actix_web_lab::respond;
use uuid::Uuid;
use crate::helpers::{ConfirmationLinks, TestApp, assert_is_redirect_to, spawn_app};
use crate::newsletter;
use wiremock::matchers::{method, any, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn you_must_login_to_see_the_issue_newsletter_form() {
    let app = spawn_app().await;
    let response = app.get_newsletter().await;
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn you_must_login_to_issue_newsletter() {
    let app = spawn_app().await;
    let body = serde_json::json!({
        "title": "Welcome",
        "content": "This is the notice for subscriber."
    });
    let response = app.post_newsletter(&body).await;
    assert_is_redirect_to(&response, "/login");

}

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
async fn newsletter_are_not_delivered_to_unconfirmed_subscribers() {
    let app = spawn_app().await;
    let login_body = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    });
    let response = app.post_login(&login_body).await;
    assert_is_redirect_to(&response, "/admin/dashboard");
    // Act: create unconfirmed subscriber
    create_unconfirmed_subscriber(&app).await;
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch saved subscription");
    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "pending_confirmation");
    // no request fired in the postmark
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&app.email_server)
        .await;
    // Act: issue to unconfirmed subscriber
    let request = serde_json::json!({
        "current_password": &app.test_user.password,
        "title": "Newsletter title",
        "content": "hey hey"
    });
    let response = app.post_newsletter(&request).await;
    assert_is_redirect_to(&response, "/admin/newsletter");
    let html_page = app.get_newsletter().await.text().await.unwrap();
    assert!(html_page.contains(
        "<p><i>no confirmed subscriber</i></p>"
    ));
}

#[tokio::test]
async fn newsletter_are_delivered_to_confirmed_subscribers(){
    let app = spawn_app().await;
    let login_body = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    });
    let response = app.post_login(&login_body).await;
    assert_is_redirect_to(&response, "/admin/dashboard");
    // Act: create confirmed subscriber
    create_confirmed_subscriber(&app).await;
    // fake postmark keep firing the send email after post newsletter
    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch saved subscription");
    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "confirmed");
    // Act: issue to confirmed subscriber
    let request = serde_json::json!({
        "current_password": &app.test_user.password,
        "title": "Newsletter title",
        "content": "hey hey"
    });
    let response = app.post_newsletter(&request).await;
    assert_is_redirect_to(&response, "/admin/newsletter");
    let html_page = app.get_newsletter().await.text().await.unwrap();
    assert!(html_page.contains(
        "<p><i>You has issued newsletter to all your subscribers.</i></p>"
    ));
}

//#[tokio::test]
//async fn newsletter_returns_400_for_invalid_data() {
//    let app = spawn_app().await;
//    // Act
//    let invalid_requests = vec![
//        (
//            serde_json::json!(
//            {
//                "content":
//                {
//                    "html": "<p>Newsletter body as HTML</p>",
//                    "plain": "Newsletter body as plain text",
//                }
//            }),
//            "missing title",
//        ),
//        (
//            serde_json::json!({"title": "Newsletter!"}),
//            "missing content",
//        ),
//    ];
//
//    for (invalid_body, error_message) in invalid_requests {
//        let response = app.post_newsletter(invalid_body).await;
//
//        // Arrange
//        assert_eq!(
//            400,
//            response.status().as_u16(),
//            "The API did not fail with 400 when the payload was {}.)",
//            error_message
//        )
//    }
//}
//
//#[tokio::test]
//async fn requests_mising_authorization_are_rejected(){
//    let app = spawn_app().await;
//
//    let response = reqwest::Client::new()
//        .post(&format!("{}/newsletter", app.address))
//        .json(&serde_json::json!({
//            "title": "Newsletter title",
//            "content": {
//                "html": "<p>Newsletter body as HTML</p>",
//                "html": "<p>Newsletter body as HTML</p>",
//                "plain": "Newsletter body as plain text",
//            }
//        }))
//        .send()
//        .await
//        .expect("Failed to execute request.");
//
//    assert_eq!(response.status().as_u16(), 401);
//    assert_eq!(r#"Basic realm="publish""#, response.headers()["WWW-Authenticate"]);
//}
//
//#[tokio::test]
//async fn non_existing_user_is_rejected(){
//    let app = spawn_app().await;
//    let username = Uuid::new_v4().to_string();
//    let password = Uuid::new_v4().to_string();
//
//    let response = reqwest::Client::new()
//        .post(&format!("{}/newsletter", &app.address))
//        .basic_auth(username, Some(password))
//        .json(&serde_json::json!({
//            "title": "Newsletter title",
//            "content": {
//                "html": "<p>Newsletter body as HTML</p>",
//                "plain": "Newsletter body as plain text",
//            }
//        }))
//        .send()
//        .await
//        .expect("Failed to execute request.");
//
//    assert_eq!(response.status().as_u16(), 401);
//    assert_eq!(r#"Basic realm="publish""#, response.headers()["WWW-Authenticate"]);
//}
//
//#[tokio::test]
//async fn invalid_password_is_rejected(){
//    // Arrange
//    let app = spawn_app().await;
//    let username = &app.test_user.username;
//    // Random password
//    let password = Uuid::new_v4().to_string();
//    assert_ne!(app.test_user.password, password);
//    let response = reqwest::Client::new()
//        .post(&format!("{}/newsletter", &app.address))
//        .basic_auth(username, Some(password))
//        .json(&serde_json::json!({
//            "title": "Newsletter title!",
//            "content": {
//                "html": "<p>Newsletter body as HTML</p>",
//                "plain": "Newsletter body as plain text",
//            }
//        }))
//        .send()
//        .await
//        .expect("Failed to execute request.");
//    // Assert
//    assert_eq!(401, response.status().as_u16());
//    assert_eq!(
//        r#"Basic realm="publish""#,
//        response.headers()["WWW-Authenticate"]
//    );
//}
