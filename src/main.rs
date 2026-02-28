use justmail::configuration::get_configuration;
use std::net::TcpListener;
use sqlx::{postgres::PgPoolOptions};
use justmail::startup::run;
use justmail::telemetry::{get_subscriber, init_subscriber};
use secrecy::ExposeSecret;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Register subscriber of Tracing
    let subscriber = get_subscriber("justmail".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    let configuration = get_configuration().expect("Failed to read configuration.");
    let connection_pool = PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy(&configuration.database.connection_string().expose_secret())
        .expect("Failed to connect to database.");
    let address = format!(
        "{}:{}",
        configuration.application.host,
        configuration.application.port
    );
    let listener = TcpListener::bind(address)?;

    run(listener, connection_pool)?.await
}
