use config::{Config, ConfigError, };
use secrecy::{ExposeSecret, SecretBox};

#[derive(serde::Deserialize)]
pub struct Settings {
    pub database: DatabaseSettings,
    pub application_port: u16,
}

#[derive(serde::Deserialize)]
pub struct DatabaseSettings {
    pub username: String,
    pub password:SecretBox<String>,
    pub host: String,
    pub port: u16,
    pub database_name: String,
}

pub fn get_configuration() -> Result<Settings, ConfigError> {
    let settings = Config::builder()
        .add_source(config::File::with_name("config"))
        .build()?;
    settings.try_deserialize::<Settings>()
}

impl DatabaseSettings {
    pub fn connection_string(&self) -> SecretBox<String> {
        SecretBox::new(
            Box::new(
                format!(
                    "postgres://{}:{}@{}:{}/{}",
                    self.username,
                    self.password.expose_secret(),
                    self.host,
                    self.port,
                    self.database_name
                )
            )
        )
    }

    pub fn connection_string_without_db(&self) -> SecretBox<String> {
        SecretBox::new(
            Box::new(
                format!(
                    "postgres://{}:{}@{}:{}",
                    self.username,
                    self.password.expose_secret(),
                    self.host,
                    self.port
                )
            )
        )
    }
}
