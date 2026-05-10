# REST API

## Общие принципы

- Все клиентские приложения ходят только в gateway.
- Внешние форматы OpenAI/Anthropic/Gemini не являются публичным контрактом gateway.
- Основной продуктовый endpoint MVP только один: `POST /api/v1/generate`.

## POST `/api/v1/generate`

### Назначение

Выполнить text generation через внутренний alias модели.

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
- `404` — `route_not_found`
- `502` — provider error / invalid response / transport error
- `504` — provider timeout
- `500` — gateway misconfiguration / storage error

## Admin API

### GET `/api/admin/requests`

Список последних запросов.

Query params:

- `limit` — optional, default `100`, max `200`

Пример ответа:

```json
[
  {
    "id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
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

### GET `/api/admin/requests/:id`

Детали одного запроса, включая `prompt_preview` и `response_preview`.

### GET `/api/admin/providers`

Список provider-конфигов, которые сейчас лежат в БД.

### GET `/api/admin/model-routes`

Список alias routes, через которые gateway резолвит модель.

### POST `/api/admin/generate`

Admin-only smoke-test генерации. Формат совпадает с `POST /api/v1/generate`, но дополнительно можно передать `api_key` для разового вызова upstream-провайдера:

```json
{
  "model": "smart-default",
  "api_key": "sk-...",
  "messages": [
    { "role": "user", "content": "Проверь маршрут" }
  ]
}
```

Если `api_key` не передан, gateway берёт ключ из env по `providers.api_key_env`. Переданный ключ не сохраняется в таблицу `requests`; в истории остаются только обычные preview запроса и ответа.

## GET `/health`

Простейший health endpoint:

```json
{
  "status": "ok"
}
```

В MVP он проверяет только доступность самого HTTP-сервиса, а не полноценный deep health upstream-провайдера.
