# AI Gateway

`ai-gateway` - внутренний HTTP gateway для LLM-вызовов. Его задача - дать приложениям один стабильный URL и один внутренний контракт, а детали внешних AI-провайдеров оставить внутри gateway.

Клиентское приложение отправляет запрос в gateway по alias модели, например `smart-default`, и передаёт `Authorization: Bearer <gateway-client-token>`. Gateway определяет tenant по hash machine token, находит tenant-local route/provider, расшифровывает provider secret и возвращает нормализованный ответ.

Важно: текущий MVP не является прозрачной заменой OpenAI API, Anthropic API или Claude API. Сейчас публичный контракт gateway - собственный `POST /api/v1/generate`. Если цель - чтобы клиент мог указать URL gateway и слать запросы ровно как в OpenAI/Claude SDK, нужно отдельно добавить vendor-compatible endpoints вроде `/v1/chat/completions` и/или provider-specific compatibility layer.

## Зачем нужен

- не размазывать API keys и provider-specific настройки по приложениям;
- изолировать tenants: provider keys, model routes, machine tokens и request history;
- переключать реальные модели через alias без изменений в клиентах;
- хранить историю LLM-вызовов в одном месте;
- иметь базовый admin UI для наблюдения, проверки маршрутов и диагностики;
- постепенно добавлять новых providers через адаптеры, не ломая клиентский контракт.

## Как работает

Базовый поток:

1. Клиент вызывает `POST /api/v1/generate`.
2. В запросе указывает внутренний `model` alias.
3. Gateway ищет alias в `model_routes`.
4. Gateway определяет tenant по hash machine token.
5. Через tenant-local `provider_code` получает provider-конфиг из `providers`.
6. По `provider.kind` выбирает adapter.
7. Gateway расшифровывает provider secret из `provider_secrets`.
8. Adapter вызывает внешний AI API.
9. Gateway сохраняет tenant/client-scoped запись в `requests`.
10. Клиент получает нормализованный response.

Сейчас реализован один provider adapter:

- `openai-compatible` - вызывает `POST {base_url}/chat/completions`.

Это покрывает OpenAI и сервисы, которые совместимы с OpenAI Chat Completions API. Anthropic Claude напрямую этим адаптером не покрывается, если только он не доступен через совместимый proxy.

## Что есть в MVP

- Rust backend на `axum`;
- PostgreSQL-хранилище для providers, model routes и истории запросов;
- users/tenants/tenant_members и session-based admin auth;
- gateway clients с machine tokens для Factum backend;
- encrypted-at-rest provider secrets через `GATEWAY_MASTER_KEY`;
- `POST /api/v1/generate` для text generation;
- admin API;
- Swagger UI;
- PrimeVue admin UI;
- smoke-test генерации из админки;
- Docker image, где runtime-контейнер запускает приложение и PostgreSQL;
- `agctl` для управления деплоем на VPS.

## API

Основной endpoint:

```http
POST /api/v1/generate
Authorization: Bearer <gateway-client-token>
```

Пример запроса:

```json
{
  "model": "smart-default",
  "messages": [
    { "role": "system", "content": "Ты помогаешь CMS." },
    { "role": "user", "content": "Коротко опиши структуру меню." }
  ],
  "options": {
    "temperature": 0.2,
    "max_tokens": 1000
  }
}
```

Пример ответа:

```json
{
  "id": "req_4d4d6f4df77f4c1e8260f60f052f63cc",
  "model": "smart-default",
  "provider": "openai",
  "output_text": "Структура меню такая: ...",
  "finish_reason": "stop",
  "usage": {
    "input_tokens": 120,
    "output_tokens": 80
  }
}
```

Полное описание API:

- [REST API](docs/rest-api.md)
- Swagger UI: `GET /swagger-ui/`
- OpenAPI JSON: `GET /api-docs/openapi.json`

## Admin UI

Admin UI доступен на корне сервиса, если собран `admin/dist`:

```text
http://localhost:8080/
```

В текущей версии UI умеет:

- показывать setup registration при пустой таблице пользователей;
- выполнять login;
- создавать users через owner-only admin UI;
- создавать tenants;
- создавать gateway client tokens;
- показывать последние запросы;
- открывать детали запроса;
- создавать и обновлять providers, включая encrypted API key;
- создавать и обновлять model routes;
- выполнять ручной smoke-test генерации.

## Конфигурация

Основные env-переменные:

- `APP_HOST`
- `APP_PORT`
- `DATABASE_URL`
- `GATEWAY_MASTER_KEY` - base64-encoded 32-byte key для AES-256-GCM provider secrets
- `DEFAULT_PROVIDER_CODE`
- `DEFAULT_PROVIDER_KIND`
- `DEFAULT_PROVIDER_BASE_URL`
- `DEFAULT_PROVIDER_TIMEOUT_MS`
- `DEFAULT_MODEL_ALIAS`
- `DEFAULT_EXTERNAL_MODEL`

Пример значений находится в [.env.example](.env.example).

При старте приложение применяет migrations и делает upsert default provider/model route в tenant `default`. Provider API key задаётся через admin UI и хранится только encrypted-at-rest.

## Запуск

Через Makefile:

```bash
make run
```

Или вручную:

```bash
docker build -t ai-gateway .
docker run --rm -p 8080:8080 --env-file .env ai-gateway
```

После запуска:

- API: `http://localhost:8080`
- Admin UI: `http://localhost:8080/`
- Swagger UI: `http://localhost:8080/swagger-ui/`

## Хранилище и маршрутизация

Схема PostgreSQL состоит из трех основных таблиц:

- `users`, `tenants`, `tenant_members` - admin auth и tenant access;
- `gateway_clients` - machine clients, в БД только token hash;
- `providers` - tenant-local provider-конфиги без plaintext secrets;
- `provider_secrets` - encrypted-at-rest provider API keys;
- `model_routes` - tenant-local mapping внутреннего alias на provider и external model;
- `requests` - tenant/client-scoped история вызовов и ошибок.

Подробнее:

- [Storage and Model Routing](docs/storage-and-routing.md)
- [Provider Strategy](docs/provider-strategy.md)
- [Architecture](architecture.md)

## Граница текущей версии

Сознательно не реализовано:

- streaming;
- embeddings;
- tools / function calling;
- image generation;
- Anthropic-native adapter;
- retries и fallback orchestration;
- quotas и billing;
- полноценный audit trail изменения маршрутов;
- редактирование конфигурации через UI;
- vendor-compatible endpoints для прямого подключения OpenAI/Claude SDK.

Эти ограничения важны. Если обещать пользователю "укажи URL gateway вместо OpenAI/Claude и все заработает", текущая реализация этого не выдержит. Ближайший архитектурно честный следующий шаг - решить, gateway должен оставаться с собственным стабильным контрактом или дополнительно поддерживать совместимые внешние контракты провайдеров.

## Документы

- [AI Gateway MVP](docs/ai-gateway-mvp.md)
- [REST API](docs/rest-api.md)
- [Storage and Model Routing](docs/storage-and-routing.md)
- [Provider Strategy](docs/provider-strategy.md)
- [Deploy on VPS](docs/vps-deploy.md)
- [Server CTL](docs/server-ctl.md)
- [Architecture](architecture.md)
