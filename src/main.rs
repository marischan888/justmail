use justmail::configuration::get_configuration;
use justmail::startup::{Application};
use justmail::telemetry::{get_subscriber, init_subscriber};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Register subscriber of Tracing log
    let subscriber = get_subscriber("justmail".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    // Should be panic when configuration fail
    let configuration = get_configuration().expect("Failed to read configuration.");
    let application = Application::build(configuration).await?;
    application.run_until_stopped().await?;
    Ok(())
}
