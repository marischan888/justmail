use uuid::Uuid;

use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn you_must_login_to_see_the_change_password_form() {
    let app = spawn_app().await;
    let response = app.get_change_passworrd().await;
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn you_must_login_to_change_the_password() {
    let app = spawn_app().await;
    let new_password = Uuid::new_v4().to_string();
    let body = serde_json::json!({
        "current_password": &Uuid::new_v4().to_string(),
        "new_password": &new_password,
        "check_password": &new_password,
    });
    let response = app.post_change_password(&body).await;

    assert_is_redirect_to(&response, "/login")
}

#[tokio::test]
async fn new_password_must_match() {
    let app = spawn_app().await;
    let login_body = serde_json::json!(
        {
            "username": &app.test_user.username,
            "password": &app.test_user.password,
        }
    );
    let new_password = Uuid::new_v4().to_string();
    let check_password = Uuid::new_v4().to_string();
    let reset_body = serde_json::json!(
        {
            "current_password": &app.test_user.password,
            "new_password": &new_password,
            "check_password": &check_password
        }
    );
    // Act 1: login
    app.post_login(&login_body).await;
    // Act 2: reset password but failed with the distinct new password
    let response = app.post_change_password(&reset_body).await;
    assert_is_redirect_to(&response, "/admin/password");
    // Act 3: Follow the redirection
    let html_page = app.get_change_passworrd().await.text().await.unwrap();
    assert!(html_page.contains(
        "<p><i>You enter two different new password - this field value must be the same</i></p>"
    ));
}

#[tokio::test]
async fn the_current_password_is_invalid() {
    let app = spawn_app().await;
    let login_body = serde_json::json!(
        {
            "username": &app.test_user.username,
            "password": &app.test_user.password,
        }
    );
    let new_password = Uuid::new_v4().to_string();
    let reset_body = serde_json::json!(
        {
            "current_password": Uuid::new_v4().to_string(),
            "new_password": &new_password,
            "check_password": &new_password,
        }
    );
    // Act 1: login
    app.post_login(&login_body).await;
    // Act 2: reset password failed for the distinct current password
    let response = app.post_change_password(&reset_body).await;
    assert_is_redirect_to(&response, "/admin/password");
    // Act 3: flash message info
    let html_page = app.get_change_passworrd().await.text().await.unwrap();
    assert!(html_page.contains("<p><i>Wrong current password</i></p>"));
}

#[tokio::test]
async fn the_new_password_is_too_short() {
    let app = spawn_app().await;
    let login_body = serde_json::json!(
        {
            "username": &app.test_user.username,
            "password": &app.test_user.password,
        }
    );
    let new_password = String::from("short");
    let reset_body = serde_json::json!(
        {
            "current_password": &app.test_user.password,
            "new_password": &new_password,
            "check_password": &new_password,
        }
    );
    // Act 1: login
    app.post_login(&login_body).await;
    // Act 2: reset password failed for the distinct current password
    let response = app.post_change_password(&reset_body).await;
    assert_is_redirect_to(&response, "/admin/password");
    // Act 3: flash message info
    let html_page = app.get_change_passworrd().await.text().await.unwrap();
    assert!(html_page.contains(
        "<p><i>Password should be longer than 12 characters but shorter than 128 characters.</i></p>"
    ));
}

#[tokio::test]
async fn login_using_new_password() {
    let app = spawn_app().await;
    let login_body = serde_json::json!(
        {
            "username": &app.test_user.username,
            "password": &app.test_user.password,
        }
    );
    let new_password = Uuid::new_v4().to_string();
    let reset_body = serde_json::json!(
        {
            "current_password": &app.test_user.password,
            "new_password": &new_password,
            "check_password": &new_password,
        }
    );
    // Act1: log in
    let response = app.post_login(&login_body).await;
    assert_is_redirect_to(&response, "/admin/dashboard");
    // Act2: into the dashboard
    let html = app.get_admin_dashboard().await.text().await.unwrap();
    assert!(html.contains(&format!("Welcome {}", app.test_user.username)));
    // Act3: change password
    let response = app.post_change_password(&reset_body).await;
    let html = app.get_change_passworrd().await.text().await.unwrap();
    assert_is_redirect_to(&response, "/admin/password");
    assert!(html.contains(r#"<p><i>Your password has been changed.</i></p>"#));
    // Act4: log out
    let response = app.post_logout().await;
    assert_is_redirect_to(&response, "/login");
    // Act5: logout message
    let html = app.get_login_html().await;
    assert!(html.contains(r#"<p><i>You have successfully log out.</i></p>"#));
    // Act6: login with new password
    let login_body = serde_json::json!(
        {
            "username": &app.test_user.username,
            "password": &new_password,
        }
    );
    let response = app.post_login(&login_body).await;
    assert_is_redirect_to(&response, "/admin/dashboard");
}
