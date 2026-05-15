# Storage and Model Routing

## Общая идея

AI Gateway теперь хранит конфигурацию как tenant-scoped данные. Один процесс может обслуживать несколько Factum-инсталляций, но `smart-default`, provider secrets, machine tokens и request history не являются глобальными.

Публичный `POST /api/v1/generate` требует `Authorization: Bearer <gateway-client-token>`. В БД хранится только SHA-256 hash token, по нему gateway находит `gateway_clients.tenant_id`.

## Основные таблицы

- `users` - admin users с Argon2 password hash.
- `tenants` - отдельные клиенты/инсталляции/организации.
- `tenant_members` - роль пользователя внутри tenant: `tenant_admin` или `viewer`.
- `admin_sessions` - opaque admin session tokens, в БД только hash.
- `gateway_clients` - machine clients для Factum backend, в БД только token hash и prefix.
- `providers` - tenant-local provider config: `code`, `kind`, `base_url`, enabled flag, timeout.
- `provider_secrets` - encrypted-at-rest provider API key.
- `model_routes` - tenant-local alias route: `alias`, `provider_code`, `external_model`.
- `requests` - tenant/client-scoped история запросов.

## Provider secrets

Plaintext provider API key не хранится и не возвращается через API.

Для MVP используется AES-256-GCM:

- env: `GATEWAY_MASTER_KEY=<base64-32-byte-key>`
- DB: `ciphertext`, `nonce`, `algorithm`, `key_version`

Decrypted key существует только внутри generate flow перед вызовом provider adapter. Его нельзя логировать, возвращать в API или писать в request history.

## Routing flow

1. Factum вызывает `POST /api/v1/generate` с gateway client token.
2. Gateway hash-ит token и ищет enabled `gateway_clients`.
3. Из `gateway_clients.tenant_id` определяется tenant.
4. `model_routes` ищется по `(tenant_id, alias)`.
5. Provider ищется по `(tenant_id, provider_code)`.
6. Provider secret берётся из `provider_secrets` и расшифровывается через `GATEWAY_MASTER_KEY`.
7. Adapter вызывает внешний AI API.
8. `requests` получает `tenant_id` и `gateway_client_id`.

## Seed-логика

После migrations приложение делает upsert default provider/model route в tenant `default`. Это сохраняет быстрый старт, но provider API key всё равно нужно сохранить через admin UI, иначе generation вернёт `gateway_misconfigured`.

## Как добавить provider

Через admin UI:

1. Выбрать tenant.
2. Создать provider с `code`, `kind`, `base_url`, `timeout_ms`.
3. Сохранить API key в provider secret.
4. Создать/обновить model route на этот `provider_code`.

Новый provider kind в коде по-прежнему добавляется через отдельный adapter в `src/providers/` и регистрацию в `ProviderRegistry`.
