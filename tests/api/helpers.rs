use argon2::{Argon2, PasswordHasher, Algorithm, Params, Version};
use argon2::password_hash::phc::SaltString;
use linkify::{LinkFinder, LinkKind};
use justmail::configuration::{get_configuration, DatabaseSettings};
use justmail::startup::{get_connection_pool, Application};
use justmail::telemetry::{get_subscriber, init_subscriber};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use once_cell::sync::Lazy;
use reqwest::{Response};
use serde_json::Value;
use wiremock::MockServer;

static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    };
});


pub struct TestApp {
    pub db_pool: PgPool,
    pub address: String,
    pub email_server: MockServer,
    // port for test in dev
    pub port: u16,
    pub test_user: TestUser,
    // reqwest api client
    pub api_client: reqwest::Client,
}

pub struct ConfirmationLinks {
    pub html_link: reqwest::Url,
    pub plain_text: reqwest::Url,
    pub token: String,
}
 pub struct TestUser {
     pub user_id: Uuid,
     pub username: String,
     pub password: String,
 }

impl TestUser {
    pub fn generate() -> Self {
        Self {
            user_id: Uuid::new_v4(),
            username: Uuid::new_v4().to_string(),
            password: Uuid::new_v4().to_string(),
        }
    }

    async fn store(&self, pool: &PgPool) {
        let salt = SaltString::generate();
        let argon2 = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(15000, 2, 1, None).unwrap(),
        );
        let password_hash = argon2
            .hash_password_with_salt(&salt.as_bytes(), &self.password.as_bytes())
            .unwrap()
            .to_string();

        sqlx::query!(
            "INSERT INTO users (user_id, username, password_hash)
            VALUES ($1, $2, $3)",
            self.user_id,
            self.username,
            password_hash
        )
            .execute(pool)
            .await
            .expect("Failed to store test user.");
    }
}
impl TestApp {
    pub async fn get_login_html(&self) -> String {
        self.api_client
            .get(&format!("{}/login", &self.address))
            .send()
            .await
            .expect("Failed to get login html")
            .text()
            .await
            .unwrap()
    }

    pub async fn post_login<Body>(&self, body: &Body) -> Response
        where
            Body: serde::Serialize
    {
        self.api_client
            .post(&format!("{}/login", self.address))
            .form(body)
            .send()
            .await
            .expect("Failed to send request")
    }
    pub async fn post_newsletter(&self, json_body: Value) -> Response {
        self.api_client
            .post(&format!("{}/newsletter", &self.address))
            .basic_auth(&self.test_user.username, Some(&self.test_user.password))
            .json(&json_body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub async fn post_subscriptions(&self, body: String) -> Response {
        self.api_client
            .post(&format!("{}/subscriptions", &self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    pub fn get_confirmation_links(&self, request: &wiremock::Request) -> ConfirmationLinks {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();

        // Extract the link from one of the request field
        let get_link = |s: &str| {
            let links: Vec<_> = LinkFinder::new()
                .links(s)
                .filter(|x| {*x.kind() == LinkKind::Url})
                .collect();
            assert_eq!(links.len(), 1);
            let raw_link = links[0].as_str().to_owned();
            let mut confirmation_link = reqwest::Url::parse(&raw_link).unwrap();
            // Let's make sure we don't call random APIs on the web
            assert_eq!(confirmation_link.host_str().unwrap(), "127.0.0.1");
            confirmation_link.set_port(Some(self.port)).unwrap();
            confirmation_link
        };

        let html_link = get_link(&body["HtmlBody"].as_str().unwrap());
        let plain_text = get_link(&body["TextBody"].as_str().unwrap());
        let token = html_link
            .query_pairs()
            .find(|(key, _)| key == "subscription_token")
            .map(|(_, value)| value.into_owned())
            .expect("Subscription token not found in the URL");

        ConfirmationLinks { html_link, plain_text, token }
    }
}

pub fn assert_is_redirect_to(response: &reqwest::Response, location: &str) {
    assert_eq!(response.status().as_u16(), 303);
    assert_eq!(response.headers().get("Location").unwrap(), location);
}

pub async fn spawn_app() -> TestApp {
    Lazy::force(&TRACING);

    let email_server = MockServer::start().await;

    let configurations = {
        let mut config = get_configuration().expect("Failed to read configuration.");
        config.database.database_name = Uuid::new_v4().to_string();
        config.application.port = 0;
        config.email_client.base_url = email_server.uri();
        config
    };

    configure_db(&configurations.database).await;

    let application = Application::build(configurations.clone())
        .await
        .expect("Failed to build application.");
    let application_port = application.port();
    let address = format!("http://127.0.0.1:{}", application.port());
    let _ = tokio::spawn(application.run_until_stopped());

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    let test_app = TestApp {
        db_pool: get_connection_pool(&configurations.database),
        address,
        email_server,
        port: application_port,
        test_user: TestUser::generate(),
        api_client: client,
    };
    test_app.test_user.store(&test_app.db_pool).await;
    test_app
}

async fn configure_db(config: &DatabaseSettings) -> PgPool {
    let mut connection = PgConnection::connect_with(&config.without_db())
        .await
        .expect("Failed to connect to Postgres");
    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database");

    let connection_pool = PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to Postgres");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to run migrations");

    connection_pool
}
