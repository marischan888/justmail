use crate::routes::{health_check, subscribe};
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
        let server = run(listener, connection_pool, email_client)?;
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

pub fn run(listener: TcpListener,
           db_pool: PgPool,
           email_client: EmailClient
) -> Result<Server, std::io::Error> {
    let db_pool  = Data::new(db_pool);
    let email_client = Data::new(email_client);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .app_data(db_pool.clone()) // db connection registration
            .app_data(email_client.clone()) // http client registration
    })
    .listen(listener)?
    .run();
    Ok(server)
}
