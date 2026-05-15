# REST API

## OpenAPI / Swagger UI

- OpenAPI JSON: `GET /api-docs/openapi.json`
- Swagger UI: `GET /swagger-ui/`

## Общие принципы

- Все клиентские приложения ходят только в gateway.
- Внешние форматы OpenAI/Anthropic/Gemini не являются публичным контрактом gateway.
- Основной продуктовый endpoint MVP только один: `POST /api/v1/generate`.
- Клиентский endpoint требует `Authorization: Bearer <gateway-client-token>`.
- Admin API требует admin session token из `POST /api/auth/login`.

## POST `/api/v1/generate`

### Назначение

Выполнить text generation через внутренний alias модели.

Tenant определяется по machine token. Alias ищется только внутри этого tenant.

### Request body

```json
{
  "model": "smart-default",
  "messages": [
    { "role": "system", "content": "Ты помогаешь CMS." },
    { "role": "user", "content": "Разбери этот twig-фрагмент и опиши структуру меню." }
  ],
  "options": {
    "temperature": 0.2,
    "max_tokens": 2000
  }
}
```

### Поля

- `model` — внутренний alias маршрута.
- `messages` — список сообщений.
- `messages[].role` — `system`, `user`, `assistant`.
- `messages[].content` — текст сообщения.
- `options.temperature` — optional.
- `options.max_tokens` — optional.

### Success response

```json
{
  "id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
  "model": "smart-default",
  "provider": "openai",
  "output_text": "Структура меню такая: ...",
  "finish_reason": "stop",
  "usage": {
    "input_tokens": 1200,
    "output_tokens": 350
  }
}
```

### Ошибки

Gateway возвращает понятную внутреннюю ошибку:

```json
{
  "request_id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
  "error": {
    "code": "provider_timeout",
    "message": "Провайдер не ответил вовремя"
  }
}
```

### Основные error codes

- `unauthorized`
- `invalid_request`
- `route_not_found`
- `provider_timeout`
- `provider_error`
- `provider_invalid_response`
- `provider_transport_error`
- `gateway_misconfigured`
- `storage_error`

### HTTP status mapping

- `400` — `invalid_request`
- `401` — отсутствующий или неизвестный bearer token
- `404` — `route_not_found`
- `502` — provider error / invalid response / transport error
- `504` — provider timeout
- `500` — gateway misconfiguration / storage error

## Admin API

### Auth

- `GET /api/auth/bootstrap` - возвращает `{ "has_users": boolean }`.
- `POST /api/auth/register` - работает только пока нет пользователей; создаёт первого owner.
- `POST /api/auth/login` - возвращает admin session token.
- `GET /api/auth/me` - возвращает текущего пользователя и доступные tenants.

### Tenant-scoped endpoints

Все endpoints ниже требуют `Authorization: Bearer <admin-session-token>`.

- `GET /api/admin/tenants`
- `POST /api/admin/tenants`
- `GET /api/admin/users`
- `POST /api/admin/users`
- `GET /api/admin/tenants/:tenant_id/requests`
- `GET /api/admin/tenants/:tenant_id/requests/:id`
- `GET /api/admin/tenants/:tenant_id/providers`
- `POST /api/admin/tenants/:tenant_id/providers`
- `POST /api/admin/tenants/:tenant_id/providers/:provider_id/secret`
- `GET /api/admin/tenants/:tenant_id/model-routes`
- `POST /api/admin/tenants/:tenant_id/model-routes`
- `GET /api/admin/tenants/:tenant_id/gateway-clients`
- `POST /api/admin/tenants/:tenant_id/gateway-clients`
- `POST /api/admin/tenants/:tenant_id/generate`

### GET `/api/admin/tenants/:tenant_id/requests`

Список последних запросов.

Query params:

- `limit` — optional, default `100`, max `200`

Пример ответа:

```json
[
  {
    "id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
    "tenant_id": "8f6b4a40-4d10-4e6e-8f03-53b8d2c0e6aa",
    "gateway_client_id": "f840c62d-a8d0-4cfd-93a2-4f33f3ce94cb",
    "gateway_client_name": "factum-prod",
    "created_at": "2026-04-17T18:20:00Z",
    "model_alias": "smart-default",
    "provider_code": "openai",
    "external_model": "gpt-4.1-mini",
    "status": "success",
    "latency_ms": 1824,
    "error_message": null,
    "input_tokens": 1200,
    "output_tokens": 350
  }
]
```

### Provider secrets

Provider API key передаётся только в write endpoints:

- optional `api_key` в `POST /api/admin/tenants/:tenant_id/providers`;
- обязательный `api_key` в `POST /api/admin/tenants/:tenant_id/providers/:provider_id/secret`.

API responses возвращают только `api_key_configured: true|false`.

### Gateway clients

`POST /api/admin/tenants/:tenant_id/gateway-clients` возвращает plaintext token один раз. В БД хранится только hash и prefix.

## GET `/health`

Простейший health endpoint:

```json
{
  "status": "ok"
}
```

В MVP он проверяет только доступность самого HTTP-сервиса, а не полноценный deep health upstream-провайдера.
