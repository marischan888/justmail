use crate::routes::{health_check, subscribe, subscription_confirm};
use actix_web::dev::Server;
use actix_web::{web, web::Data, App, HttpServer};
use sqlx::{PgPool};
use std::net::TcpListener;
use sqlx::postgres::PgPoolOptions;
use tracing_actix_web::TracingLogger;
use crate::email_client::EmailClient;
use crate::configuration::{DatabaseSettings, Settings};

pub struct Application {
    port: u16,
    server: Server,
}

// To retrieve the URL in the 'subscribe" handler
// Retrieval from the context, in actix-web, is type-based
pub struct ApplicationBaseUrl(pub String);

impl Application {
    // why build need to be async?
    pub async fn build(configuration: Settings) -> Result<Self, std::io::Error> {
        // pgpool
        let connection_pool = get_connection_pool(&configuration.database);
        // http email client config
        let sender_email = configuration
            .email_client.sender()
            .expect("Invalid sender address.");
        let timeout = configuration
            .email_client
            .timeout();
        let email_client = EmailClient::new(
            configuration.email_client.base_url,
            sender_email,
            configuration.email_client.auth_token,
            timeout,
        );
        let address = format!(
            "{}:{}",
            configuration.application.host,
            configuration.application.port
        );
        let listener = TcpListener::bind(address)?;
        let port = listener.local_addr()?.port();

        let server = run
            (
                listener,
                connection_pool,
                email_client,
                configuration.application.base_url,
            )?;

        Ok(Self {port, server})
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        // the server should await
        self.server.await
    }
}

pub fn get_connection_pool(
    database: &DatabaseSettings
) -> PgPool {
    PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy_with(database.with_db())
}

pub fn run
(
    listener: TcpListener,
    db_pool: PgPool,
    email_client: EmailClient,
    base_url: String
) -> Result<Server, std::io::Error> {
    let db_pool  = Data::new(db_pool);
    let email_client = Data::new(email_client);
    let base_url = Data::new(ApplicationBaseUrl(base_url));
    // acrix-web spin up workers based on your cpu
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .route("/subscriptions/confirm", web::get().to(subscription_confirm))
            .app_data(db_pool.clone()) // db connection registration
            .app_data(email_client.clone()) // http client registration
            .app_data(base_url.clone()) // base url for app
    })
    .listen(listener)?
    .run();
    Ok(server)
}
