use std::{env, num::ParseIntError};

use thiserror::Error;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server_host: String,
    pub server_port: u16,
    pub database_url: String,
    pub gateway_master_key: String,
    pub default_provider_code: String,
    pub default_provider_kind: String,
    pub default_provider_base_url: String,
    pub default_provider_timeout_ms: i32,
    pub default_model_alias: String,
    pub default_external_model: String,
    pub request_preview_chars: usize,
    pub response_preview_chars: usize,
    pub admin_dist_dir: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable `{0}`")]
    Missing(String),
    #[error("invalid number in environment variable `{name}`: {source}")]
    InvalidNumber {
        name: String,
        #[source]
        source: ParseIntError,
    },
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            server_host: optional("APP_HOST", "0.0.0.0"),
            server_port: optional_parse("APP_PORT", 8080)?,
            database_url: required("DATABASE_URL")?,
            gateway_master_key: required("GATEWAY_MASTER_KEY")?,
            default_provider_code: optional("DEFAULT_PROVIDER_CODE", "openai"),
            default_provider_kind: optional("DEFAULT_PROVIDER_KIND", "openai-compatible"),
            default_provider_base_url: optional(
                "DEFAULT_PROVIDER_BASE_URL",
                "https://api.openai.com/v1",
            ),
            default_provider_timeout_ms: optional_parse("DEFAULT_PROVIDER_TIMEOUT_MS", 60_000)?,
            default_model_alias: optional("DEFAULT_MODEL_ALIAS", "smart-default"),
            default_external_model: optional("DEFAULT_EXTERNAL_MODEL", "gpt-4.1-mini"),
            request_preview_chars: optional_parse("REQUEST_PREVIEW_CHARS", 1_500)?,
            response_preview_chars: optional_parse("RESPONSE_PREVIEW_CHARS", 2_000)?,
            admin_dist_dir: optional("ADMIN_DIST_DIR", "admin/dist"),
        })
    }
}

fn required(name: &str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::Missing(name.to_owned()))
}

fn optional(name: &str, default_value: &str) -> String {
    env::var(name).unwrap_or_else(|_| default_value.to_owned())
}

fn optional_parse<T>(name: &str, default_value: T) -> Result<T, ConfigError>
where
    T: std::str::FromStr<Err = ParseIntError> + Copy,
{
    match env::var(name) {
        Ok(value) => value.parse().map_err(|source| ConfigError::InvalidNumber {
            name: name.to_owned(),
            source,
        }),
        Err(_) => Ok(default_value),
    }
}
