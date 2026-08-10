use std::fmt::{Debug, Display};

use justmail::configuration::get_configuration;
use justmail::issue_delivery_worker::run_worker_until_stopped;
use justmail::remove_expired_link_worker::run_token_worker_until_stopped;
use justmail::startup::Application;
use justmail::telemetry::{get_subscriber, init_subscriber};
use tokio::task::JoinError;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Register subscriber of Tracing log
    let subscriber = get_subscriber("justmail".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    let configuration = get_configuration().expect("Failed to read configuration.");
    // run application and spawn task into another thread
    let application = Application::build(configuration.clone()).await?;
    let application_task = tokio::spawn(application.run_until_stopped());
    // let worker keep issuing email to subscriber
    let worker_task = tokio::spawn(run_worker_until_stopped(configuration.clone()));
    // let clear token worker keep clear the expired link token per day
    let token_worker_task = tokio::spawn(run_token_worker_until_stopped(configuration));

    // waits on multiple future (tasks) on multiple threads
    // black box: not sure which task complete first
    tokio::select! {
        outcome = application_task => report_exit("API", outcome),
        outcome = worker_task => report_exit("Send email background work", outcome),
        outcome = token_worker_task => report_exit("Clear roken background work", outcome)
    };

    Ok(())
}

fn report_exit(task_name: &str, outcome: Result<Result<(), impl Debug + Display>, JoinError>) {
    match outcome {
        Ok(Ok(())) => {
            tracing::info!("{} has exited", task_name)
        }
        Ok(Err(e)) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "{} failed",
                task_name
            )
        }
        Err(e) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "{} task failed to complete",
                task_name
            )
        }
    }
}
