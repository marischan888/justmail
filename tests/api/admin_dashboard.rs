use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn you_must_be_logged_in_to_access_the_admin_dashboard() {
    let app = spawn_app().await;
    let response = app.get_admin_dashboard().await;
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn logout_clear_session_state() {
    let app = spawn_app().await;
    let login_body = serde_json::json!(
        {
            "username": &app.test_user.username,
            "password": &app.test_user.password,
        }
    );
    // act1: login
    let response = app.post_login(&login_body).await;
    assert_is_redirect_to(&response, "/admin/dashboard");
    let html_page = app.get_admin_dashboard().await.text().await.unwrap();
    assert!(html_page.contains(&format!("Welcome {}", app.test_user.username)));
    // act2: logout
    let response = app.post_logout().await;
    assert_is_redirect_to(&response, "/login");
    let html_page = app.get_login_html().await;
    assert!(html_page.contains(r#"<p><i>You have successfully log out.</i></p>"#));
    // act3: attempt to get dashboard after logout
    let response  = app.get_admin_dashboard().await;
    assert_is_redirect_to(&response, "/login");
}
