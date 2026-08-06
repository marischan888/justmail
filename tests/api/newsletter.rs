use crate::helpers::{ConfirmationLinks, TestApp, assert_is_redirect_to, spawn_app};
use fake::Fake;
use fake::faker::internet::en::SafeEmail;
use fake::faker::name::en::Name;
use justmail::domain::{NewSubscriber, SubscriberEmail, SubscriberName};
use wiremock::matchers::{method, any, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn you_must_login_to_see_the_issue_newsletter_form() {
    let app = spawn_app().await;
    let response = app.get_newsletters().await;
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn you_must_login_to_issue_newsletter() {
    let app = spawn_app().await;
    let body = serde_json::json!({
        "title": "Welcome",
        "content": "This is the notice for subscriber.",
        "idempotency_key": uuid::Uuid::new_v4().to_string(),
    });
    let response = app.post_newsletters(&body).await;
    assert_is_redirect_to(&response, "/login");

}

struct PendingSubscriber {
    subscriber: NewSubscriber,
    link: ConfirmationLinks
}

async fn create_unconfirmed_subscriber(app: &TestApp) -> PendingSubscriber {
    //let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let name: String = Name().fake();
    let email: String = SafeEmail().fake();
    let body = serde_urlencoded::to_string(&serde_json::json!({
        "name": name,
        "email": email,
    }))
        .unwrap();

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

    let email_request = &app
        .email_server
        .received_requests()
        .await
        .unwrap()
        .pop()
        .unwrap();
    let link = app.get_confirmation_links(email_request);
    let subscriber = NewSubscriber {
        name: SubscriberName::parse(name).unwrap(),
        email: SubscriberEmail::parse(email).unwrap(),
    };
    PendingSubscriber {
        subscriber,
        link,
    }
}

async fn create_confirmed_subscriber(app: &TestApp) -> NewSubscriber {
    let pending = create_unconfirmed_subscriber(app).await;
    reqwest::get(pending.link.html_link)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    pending.subscriber
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
    let pednding = create_unconfirmed_subscriber(&app).await;
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions")
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch saved subscription");
    assert_eq!(saved.email, pednding.subscriber.email.as_ref());
    assert_eq!(saved.name, pednding.subscriber.name.as_ref());
    assert_eq!(saved.status, "pending_confirmation");
    // no request fired in the postmark
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&app.email_server)
        .await;
    // Act: issue to unconfirmed subscriber
    let request = serde_json::json!({
        "title": "Newsletter title",
        "content": "hey hey",
        "idempotency_key": uuid::Uuid::new_v4().to_string(),
    });
    let response = app.post_newsletters(&request).await;
    assert_is_redirect_to(&response, "/admin/newsletters");
    let html_page = app.get_newsletters().await.text().await.unwrap();
    // TODO: Set up the current subscribers
    assert!(html_page.contains(
        "<p><i>You has issued newsletter to all your subscribers.</i></p>"
    ));
    app.dispatch_all_pending_emails().await;
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
    let confirmed_subscriber = create_confirmed_subscriber(&app).await;
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
    assert_eq!(saved.email, confirmed_subscriber.email.as_ref());
    assert_eq!(saved.name, confirmed_subscriber.name.as_ref());
    assert_eq!(saved.status, "confirmed");
    // Act: issue to confirmed subscriber
    let request = serde_json::json!({
        "title": "Newsletter title",
        "content": "hey hey",
        "idempotency_key": uuid::Uuid::new_v4().to_string(),
    });
    let response = app.post_newsletters(&request).await;
    assert_is_redirect_to(&response, "/admin/newsletters");
    let html_page = app.get_newsletters().await.text().await.unwrap();
    assert!(html_page.contains(
        "<p><i>You has issued newsletter to all your subscribers.</i></p>"
    ));
    app.dispatch_all_pending_emails().await;
}

#[tokio::test]
async fn newsletter_creation_is_idempotent () {
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;
    let login_body = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    });
    let response = app.post_login(&login_body).await;
    assert_is_redirect_to(&response, "/admin/dashboard");

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;
    // Act: submit newsletter form
    let request = serde_json::json!({
        "title": "Newsletter title",
        "content": "this is the plain text",
        "idempotency_key": uuid::Uuid::new_v4().to_string(),
    });
    let response = app.post_newsletters(&request).await;
    assert_is_redirect_to(&response, "/admin/newsletters");
    let html_page = app.get_newsletters().await.text().await.unwrap();
    assert!(html_page.contains(
        "<p><i>You has issued newsletter to all your subscribers.</i></p>"
    ));
    // Act: submit newsletter again
    let response = app.post_newsletters(&request).await;
    assert_is_redirect_to(&response, "/admin/newsletters");
    let html_page = app.get_newsletters().await.text().await.unwrap();
    assert!(html_page.contains(
        "<p><i>You has issued newsletter to all your subscribers.</i></p>"
    ));
    app.dispatch_all_pending_emails().await;
}

#[tokio::test]
async fn concurrent_form_submission_is_handled_gracefully () {
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;
    let login_body = serde_json::json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    });
    let response = app.post_login(&login_body).await;
    assert_is_redirect_to(&response, "/admin/dashboard");

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;
    // Act: submit newsletter form
    let request = serde_json::json!({
        "title": "Newsletter title",
        "content": "this is the plain text",
        "idempotency_key": uuid::Uuid::new_v4().to_string(),
    });
    let response_1 = app.post_newsletters(&request);
    let response_2 = app.post_newsletters(&request);
    let (response_1, response_2) = tokio::join!(response_1, response_2);
    assert_eq!(response_1.status(), response_2.status());
    assert_eq!(response_1.text().await.unwrap(), response_2.text().await.unwrap());
    app.dispatch_all_pending_emails().await;
}
