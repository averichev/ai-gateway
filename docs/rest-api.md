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

Выполнить генерацию текста и/или структурированных вызовов инструментов через внутренний alias модели.

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
- `messages[].role` — `system`, `user`, `assistant`, `tool`.
- `messages[].content` — строка; у assistant может отсутствовать или быть `null`, если есть `tool_calls`.
- `messages[].tool_calls` — только у assistant: массив `{id, name, arguments}`.
- `messages[].tool_call_id` — обязательный идентификатор вызова для сообщения `tool`; `content` содержит результат строкой, включая JSON, сериализованный клиентом.
- `tools` — необязательный массив `{name, description, parameters}`. `parameters` — JSON Schema с корневым `"type": "object"`, без OpenAI-обёртки `function`.
- `tool_choice` — необязательное `"auto"`, `"none"`, `"required"` или `{"name": "weather"}`. Именованный выбор должен ссылаться на объявленный инструмент; `auto`/`required` требуют непустого `tools`. При отсутствии поля используется поведение провайдера.
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

`output_text` всегда остаётся строкой. При ответе только с инструментами это `""`, а `tool_calls` содержит структурированные вызовы. Для текстовых ответов пустой `tool_calls` опускается: старый формат ответа сохраняется.

Нормализованные `finish_reason`: `stop`, `length`, `tool_calls`, `content_filter`, `unknown`. Отсутствующий reason определяется по наличию вызовов; неизвестное значение становится `unknown`. Неполный completion с вызовами (`length`/`content_filter`) отклоняется, чтобы клиент не исполнил обрезанный вызов.

### Полный цикл Tool Calling

Все запросы идут в `POST /api/v1/generate` с тем же machine token. Gateway делает один completion за запрос. Историей, выбором бизнес-инструментов, их исполнением и следующими запросами управляет Factum.

1. **Tool Definition** — приложение объявляет доступный инструмент:

```json
{
  "model": "smart-default",
  "messages": [{"role": "user", "content": "Какая погода в Москве?"}],
  "tools": [{
    "name": "weather",
    "description": "Получить текущую погоду по названию города",
    "parameters": {
      "type": "object",
      "properties": {"city": {"type": "string"}},
      "required": ["city"],
      "additionalProperties": false
    }
  }],
  "tool_choice": "auto"
}
```

2. **LLM Tool Call** — gateway возвращает ответ модели без выполнения инструмента:

```json
{
  "id": "req_first",
  "model": "smart-default",
  "provider": "openai",
  "output_text": "",
  "tool_calls": [{"id": "call_weather_1", "name": "weather", "arguments": {"city": "Москва"}}],
  "finish_reason": "tool_calls",
  "usage": {"input_tokens": 80, "output_tokens": 20}
}
```

`arguments` — JSON object, а не строка с JSON. Vendor-specific строковая сериализация аргументов выполняется только внутри adapter. Числа сохраняются без промежуточного преобразования в `f64`.

3. **Tool Result** — Factum исполняет вызов и отправляет полную историю с теми же идентификатором и аргументами. В этом примере новые вызовы запрещены через `none`, поэтому повторять definitions не требуется:

```json
{
  "model": "smart-default",
  "messages": [
    {"role": "user", "content": "Какая погода в Москве?"},
    {"role": "assistant", "tool_calls": [
      {"id": "call_weather_1", "name": "weather", "arguments": {"city": "Москва"}}
    ]},
    {"role": "tool", "tool_call_id": "call_weather_1", "content": "{\"temperature_c\": 12, \"condition\": \"солнечно\"}"}
  ],
  "tool_choice": "none"
}
```

Если нужно разрешить следующие вызовы, приложение снова передаёт `tools` и подходящий `tool_choice`. Assistant может возвращать несколько вызовов; нужны результаты каждого вызова перед следующим сообщением другой роли. Порядок результатов может отличаться от порядка вызовов. Сиротские, повторные и пропущенные результаты отклоняются. Definitions из прошлых ходов не обязательны для передачи истории.

4. **Final Answer** — очередной обычный текстовый ответ:

```json
{
  "id": "req_second",
  "model": "smart-default",
  "provider": "openai",
  "output_text": "В Москве солнечно, +12 °C.",
  "finish_reason": "stop",
  "usage": {"input_tokens": 120, "output_tokens": 15}
}
```

### Валидация и лимиты

- Не более 2 MiB JSON на запрос, 256 сообщений, 128 definitions и 128 вызовов в одном assistant message / completion.
- Не более 64 KiB сериализованного JSON на каждую схему и каждый объект аргументов; upstream body ограничен 4 MiB.
- Уникальные имена инструментов: 1–64 ASCII-символа `[a-zA-Z0-9_-]`. Идентификаторы вызовов: 1–256 печатных ASCII-символов без пробелов, уникальные в истории.
- Структура схемы проверяется библиотекой `jsonschema`; поддерживаются стандартные drafts библиотеки (по умолчанию 2020-12). Внешние HTTP/file references и пользовательские meta schemas, требующие загрузки, недоступны. Локальные `$ref` разрешены.
- Аргументы новых вызовов должны быть JSON object и соответствовать объявленной схеме. Неизвестный инструмент, неверный JSON, повторный ID и нарушение `tool_choice` — `provider_invalid_response`, без фиктивного текстового ответа.
- Поддержка tools и отдельных политик выбора зависит от модели и upstream. Явный отказ upstream нормализуется в `400 tool_calling_not_supported`; статического списка моделей и автоматического fallback на текст нет. Прочие upstream errors остаются `502 provider_error`.
- Для запросов с инструментами prompt/response preview не сохраняются. Аргументы, результаты и исходные ошибки upstream не попадают в диагностические сообщения; сохраняются tenant/client, routing, latency, status и usage. Usage учитывается и для некорректных вызовов, если удалось разобрать envelope с usage.

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
- `tool_calling_not_supported`
- `provider_transport_error`
- `gateway_misconfigured`
- `storage_error`

### HTTP status mapping

- `400` — `invalid_request` / `tool_calling_not_supported`
- `401` — отсутствующий или неизвестный bearer token
- `403` — отключённый machine token
- `413` — тело запроса превышает лимит 2 MiB (`invalid_request`)
- `404` — `route_not_found`
- `502` — provider error / invalid response / transport error
- `504` — provider timeout
- `500` — gateway misconfiguration / storage error

Ошибки JSON и лимита HTTP body отклоняются до use case и не записываются в историю запросов.

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
