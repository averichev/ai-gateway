use std::{env, net::SocketAddr, num::ParseIntError};

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
    pub store_content_previews: bool,
    pub request_preview_chars: usize,
    pub response_preview_chars: usize,
    pub admin_dist_dir: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid APP_HOST or APP_PORT")]
    InvalidListenAddress,
    #[error("missing required environment variable `{0}`")]
    Missing(String),
    #[error("environment variable `{0}` must be true or false")]
    InvalidBoolean(String),
    #[error("invalid number in environment variable `{name}`: {source}")]
    InvalidNumber {
        name: String,
        #[source]
        source: ParseIntError,
    },
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let (server_host, server_port) = listen_settings()?;
        Ok(Self {
            server_host,
            server_port,
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
            store_content_previews: parse_store_content_previews(
                env::var("STORE_CONTENT_PREVIEWS").ok().as_deref(),
            )?,
            request_preview_chars: optional_parse("REQUEST_PREVIEW_CHARS", 1_500)?,
            response_preview_chars: optional_parse("RESPONSE_PREVIEW_CHARS", 2_000)?,
            admin_dist_dir: optional("ADMIN_DIST_DIR", "admin/dist"),
        })
    }
}

fn listen_settings() -> Result<(String, u16), ConfigError> {
    Ok((
        optional("APP_HOST", "0.0.0.0"),
        optional_parse("APP_PORT", 8080)?,
    ))
}

pub(crate) fn listen_address() -> Result<SocketAddr, ConfigError> {
    let (host, port) = listen_settings()?;
    format!("{host}:{port}")
        .parse()
        .map_err(|_| ConfigError::InvalidListenAddress)
}

fn parse_store_content_previews(value: Option<&str>) -> Result<bool, ConfigError> {
    match value {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        _ => Err(ConfigError::InvalidBoolean(
            "STORE_CONTENT_PREVIEWS".to_owned(),
        )),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_previews_require_explicit_true() {
        assert!(!parse_store_content_previews(None).unwrap());
        assert!(!parse_store_content_previews(Some("false")).unwrap());
        assert!(parse_store_content_previews(Some("true")).unwrap());
        for value in ["", "TRUE", "1", "secret-value"] {
            let error = parse_store_content_previews(Some(value)).unwrap_err();
            assert!(!error.to_string().contains(value) || value.is_empty());
        }
    }
}
