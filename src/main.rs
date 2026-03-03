use justmail::configuration::get_configuration;
use std::net::TcpListener;
use sqlx::{postgres::PgPoolOptions};
use justmail::email_client::EmailClient;
use justmail::startup::run;
use justmail::telemetry::{get_subscriber, init_subscriber};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Register subscriber of Tracing
    let subscriber = get_subscriber("justmail".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    // db pool config
    let configuration = get_configuration().expect("Failed to read configuration.");
    let connection_pool = PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy_with(configuration.database.with_db());
    // http email client config
    // TODO
    let sender_email = configuration.email_client.sender()
        .expect("Invalid sender address.");
    let email_client = EmailClient::new(
        configuration.email_client.base_url,
        sender_email
    );
    let address = format!(
        "{}:{}",
        configuration.application.host,
        configuration.application.port
    );
    let listener = TcpListener::bind(address)?;

    run(listener, connection_pool, email_client)?.await
}
