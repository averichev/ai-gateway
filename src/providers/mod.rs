pub mod openai_compatible;

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use thiserror::Error;

use crate::domain::{
    GenerateRequestDto, ProviderGenerateResult, ResolvedRouteRecord, TokenUsageDto,
};

pub use openai_compatible::OpenAiCompatibleProvider;

#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn kind(&self) -> &'static str;

    /// Pure configuration validation; must never contact upstream.
    fn validate_config(&self, route: &ResolvedRouteRecord) -> Result<(), &'static str>;

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
    #[error("invalid provider response: {message}")]
    InvalidResponse {
        message: String,
        usage: Option<TokenUsageDto>,
    },
    #[error("model does not support the requested tool calling capability")]
    ToolCallingNotSupported,
    #[error("missing encrypted provider secret for `{0}`")]
    MissingProviderSecret(String),
    #[error("provider secret cannot be decrypted for `{0}`")]
    ProviderSecretUnavailable(String),
    #[error("unsupported provider kind `{0}`")]
    UnsupportedKind(String),
    #[error("invalid provider configuration")]
    InvalidConfiguration,
    #[error("provider transport error: {0}")]
    Transport(String),
}

impl ProviderError {
    pub fn status_label(&self) -> &'static str {
        match self {
            Self::Timeout(_) => "provider_timeout",
            Self::Upstream { .. } => "provider_error",
            Self::InvalidResponse { .. } => "provider_invalid_response",
            Self::ToolCallingNotSupported => "tool_calling_not_supported",
            Self::MissingProviderSecret(_)
            | Self::ProviderSecretUnavailable(_)
            | Self::UnsupportedKind(_)
            | Self::InvalidConfiguration => "gateway_misconfigured",
            Self::Transport(_) => "provider_transport_error",
        }
    }

    pub fn client_code(&self) -> &'static str {
        self.status_label()
    }

    pub fn client_message(&self) -> String {
        match self {
            Self::Timeout(_) => "Провайдер не ответил вовремя".to_owned(),
            Self::Upstream {
                status: Some(status),
                ..
            } => {
                format!("Провайдер вернул ошибку: HTTP {status} from upstream")
            }
            Self::Upstream { status: None, .. } => "Провайдер вернул ошибку".to_owned(),
            Self::InvalidResponse { .. } => "Провайдер вернул некорректный ответ".to_owned(),
            Self::ToolCallingNotSupported => {
                "Выбранная модель или провайдер не поддерживает запрошенный Tool Calling".to_owned()
            }
            Self::MissingProviderSecret(provider_code) => {
                format!("Gateway не настроен: отсутствует provider secret для `{provider_code}`")
            }
            Self::ProviderSecretUnavailable(provider_code) => {
                format!("Gateway не смог прочитать provider secret для `{provider_code}`")
            }
            Self::UnsupportedKind(kind) => {
                format!("Gateway не умеет работать с provider kind `{kind}`")
            }
            Self::InvalidConfiguration => "Некорректные параметры provider или маршрута".to_owned(),
            Self::Transport(_) => "Не удалось связаться с AI-провайдером".to_owned(),
        }
    }

    pub fn upstream_status(&self) -> Option<u16> {
        match self {
            Self::Upstream { status, .. } => *status,
            _ => None,
        }
    }

    /// Never persist Display/Debug or free-form adapter error messages.
    pub fn safe_diagnostic(&self) -> String {
        match self.upstream_status() {
            Some(status) => format!("{}: upstream HTTP {status}", self.client_code()),
            None => self.client_code().to_owned(),
        }
    }

    pub fn usage(&self) -> Option<&TokenUsageDto> {
        match self {
            Self::InvalidResponse { usage, .. } => usage.as_ref(),
            _ => None,
        }
    }

    pub fn storage_status(&self) -> String {
        self.status_label().to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_form_adapter_errors_never_reach_history_or_clients() {
        for error in [
            ProviderError::Timeout("private-url?key=secret".to_owned()),
            ProviderError::Transport("Bearer private-credential".to_owned()),
            ProviderError::InvalidResponse {
                message: "private-tool-arguments".to_owned(),
                usage: None,
            },
            ProviderError::Upstream {
                status: Some(503),
                message: "private-document".to_owned(),
            },
        ] {
            assert!(!error.safe_diagnostic().contains("private"));
            assert!(!error.client_message().contains("private"));
        }
        let error = ProviderError::Upstream {
            status: Some(503),
            message: "private-document".to_owned(),
        };
        assert!(error.safe_diagnostic().contains("503"));
        assert_eq!(error.client_code(), "provider_error");
    }
}
