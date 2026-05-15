pub mod openai_compatible;

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{GenerateRequestDto, ProviderGenerateResult, ResolvedRouteRecord};

pub use openai_compatible::OpenAiCompatibleProvider;

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn kind(&self) -> &'static str;

    async fn generate(
        &self,
        route: &ResolvedRouteRecord,
        request: &GenerateRequestDto,
        api_key: &str,
    ) -> Result<ProviderGenerateResult, ProviderError>;
}

#[derive(Clone)]
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn ProviderAdapter>>,
}

impl ProviderRegistry {
    pub fn new(adapters: Vec<Arc<dyn ProviderAdapter>>) -> Self {
        let providers = adapters
            .into_iter()
            .map(|adapter| (adapter.kind().to_owned(), adapter))
            .collect();

        Self { providers }
    }

    pub fn get(&self, kind: &str) -> Option<Arc<dyn ProviderAdapter>> {
        self.providers.get(kind).cloned()
    }
}

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider timeout: {0}")]
    Timeout(String),
    #[error("provider upstream error: {message}")]
    Upstream {
        status: Option<u16>,
        message: String,
    },
    #[error("invalid provider response: {0}")]
    InvalidResponse(String),
    #[error("missing encrypted provider secret for `{0}`")]
    MissingProviderSecret(String),
    #[error("provider secret cannot be decrypted for `{0}`")]
    ProviderSecretUnavailable(String),
    #[error("unsupported provider kind `{0}`")]
    UnsupportedKind(String),
    #[error("provider transport error: {0}")]
    Transport(String),
}

impl ProviderError {
    pub fn status_label(&self) -> &'static str {
        match self {
            Self::Timeout(_) => "provider_timeout",
            Self::Upstream { .. } => "provider_error",
            Self::InvalidResponse(_) => "provider_invalid_response",
            Self::MissingProviderSecret(_)
            | Self::ProviderSecretUnavailable(_)
            | Self::UnsupportedKind(_) => "gateway_misconfigured",
            Self::Transport(_) => "provider_transport_error",
        }
    }

    pub fn client_code(&self) -> &'static str {
        match self {
            Self::Timeout(_) => "provider_timeout",
            Self::Upstream { .. } => "provider_error",
            Self::InvalidResponse(_) => "provider_invalid_response",
            Self::MissingProviderSecret(_)
            | Self::ProviderSecretUnavailable(_)
            | Self::UnsupportedKind(_) => "gateway_misconfigured",
            Self::Transport(_) => "provider_transport_error",
        }
    }

    pub fn client_message(&self) -> String {
        match self {
            Self::Timeout(_) => "Провайдер не ответил вовремя".to_owned(),
            Self::Upstream { message, .. } => format!("Провайдер вернул ошибку: {message}"),
            Self::InvalidResponse(_) => "Провайдер вернул некорректный ответ".to_owned(),
            Self::MissingProviderSecret(provider_code) => {
                format!("Gateway не настроен: отсутствует provider secret для `{provider_code}`")
            }
            Self::ProviderSecretUnavailable(provider_code) => {
                format!("Gateway не смог прочитать provider secret для `{provider_code}`")
            }
            Self::UnsupportedKind(kind) => {
                format!("Gateway не умеет работать с provider kind `{kind}`")
            }
            Self::Transport(_) => "Не удалось связаться с AI-провайдером".to_owned(),
        }
    }

    fn status(&self) -> String {
        match self.status_label() {
            "provider_timeout" => "provider_timeout".to_owned(),
            "provider_error" => "provider_error".to_owned(),
            "provider_invalid_response" => "provider_invalid_response".to_owned(),
            "gateway_misconfigured" => "gateway_misconfigured".to_owned(),
            _ => "provider_transport_error".to_owned(),
        }
    }

    pub fn storage_status(&self) -> String {
        self.status()
    }
}
