use std::sync::Arc;

use crate::{
    domain::{
        ReadinessCheckDto, ReadinessCheckStatus, ReadinessStatus, ResolvedRouteRecord,
        RouteReadinessDto,
    },
    providers::ProviderRegistry,
    repositories::{RepositoryError, RoutesRepository},
    security::SecretCrypto,
};
use uuid::Uuid;

#[derive(Clone)]
pub struct ReadinessService {
    routes: Arc<RoutesRepository>,
    providers: ProviderRegistry,
    crypto: SecretCrypto,
}

impl ReadinessService {
    pub fn new(
        routes: Arc<RoutesRepository>,
        providers: ProviderRegistry,
        crypto: SecretCrypto,
    ) -> Self {
        Self {
            routes,
            providers,
            crypto,
        }
    }

    /// Configuration only: no adapter.generate, network probes or history writes.
    pub async fn check(
        &self,
        tenant_id: Uuid,
        alias: &str,
    ) -> Result<RouteReadinessDto, RepositoryError> {
        let mut result = RouteReadinessDto {
            model_alias: alias.to_owned(),
            provider_code: None,
            status: ReadinessStatus::Ready,
            checks: vec![],
        };
        let Some(route) = self
            .routes
            .get_model_route_by_alias(tenant_id, alias)
            .await?
        else {
            result.check(
                "model_route",
                false,
                ReadinessStatus::Incomplete,
                "Создайте маршрут с этим alias в выбранном tenant",
            );
            return Ok(result);
        };
        result.provider_code = Some(route.provider_code.clone());
        result.check("model_route", true, ReadinessStatus::Incomplete, "");
        result.check(
            "model_route_enabled",
            route.is_enabled,
            ReadinessStatus::Unavailable,
            "Включите маршрут модели",
        );
        result.check(
            "external_model",
            !route.external_model.trim().is_empty(),
            ReadinessStatus::Incomplete,
            "Укажите внешнее имя модели в маршруте",
        );
        let Some(provider) = self
            .routes
            .get_provider_by_code(tenant_id, &route.provider_code)
            .await?
        else {
            result.check(
                "provider",
                false,
                ReadinessStatus::Incomplete,
                "Создайте связанного провайдера в этом tenant или исправьте маршрут",
            );
            return Ok(result);
        };
        result.check("provider", true, ReadinessStatus::Incomplete, "");
        result.check(
            "provider_enabled",
            provider.is_enabled,
            ReadinessStatus::Unavailable,
            "Включите провайдера",
        );
        result.check(
            "provider_timeout",
            provider.timeout_ms > 0,
            ReadinessStatus::Incomplete,
            "Укажите положительный timeout_ms в настройках провайдера",
        );
        let adapter = self.providers.get(&provider.kind);
        result.check(
            "provider_adapter",
            adapter.is_some(),
            ReadinessStatus::Unavailable,
            "Выберите kind, поддерживаемый зарегистрированным adapter",
        );
        if let Some(adapter) = adapter {
            let resolved = ResolvedRouteRecord {
                provider_id: provider.id,
                provider_code: provider.code,
                provider_kind: provider.kind,
                provider_base_url: provider.base_url,
                provider_timeout_ms: provider.timeout_ms,
                external_model: route.external_model,
            };
            let validation = adapter.validate_config(&resolved);
            result.check(
                "provider_parameters",
                validation.is_ok(),
                ReadinessStatus::Incomplete,
                validation.err().unwrap_or_default(),
            );
        } else {
            result.checks.push(ReadinessCheckDto {
                code: "provider_parameters".to_owned(),
                status: ReadinessCheckStatus::Skipped,
                recommendation: Some("Сначала настройте поддерживаемый adapter".to_owned()),
            });
        }
        let secret = self
            .routes
            .get_provider_secret(tenant_id, provider.id)
            .await?;
        let credentials_ok = secret
            .as_ref()
            .is_some_and(|secret| self.crypto.provider_api_key(secret).is_ok());
        result.check("provider_credentials", credentials_ok,
            if secret.is_some() { ReadinessStatus::Unavailable } else { ReadinessStatus::Incomplete },
            "Сохраните API key провайдера. При ошибке расшифровки проверьте прежний мастер-ключ в GATEWAY_MASTER_KEY или ai-gateway.key");
        Ok(result)
    }
}

impl RouteReadinessDto {
    fn check(&mut self, code: &str, ok: bool, failure: ReadinessStatus, recommendation: &str) {
        if !ok && self.status != ReadinessStatus::Unavailable {
            self.status = failure;
        }
        self.checks.push(ReadinessCheckDto {
            code: code.to_owned(),
            status: if ok {
                ReadinessCheckStatus::Ok
            } else {
                ReadinessCheckStatus::Error
            },
            recommendation: if ok {
                None
            } else {
                Some(recommendation.to_owned())
            },
        });
    }
}
