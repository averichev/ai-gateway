# AI Gateway

`ai-gateway` - внутренний HTTP gateway для LLM-вызовов. Его задача - дать приложениям один стабильный URL и один внутренний контракт, а детали внешних AI-провайдеров оставить внутри gateway.

Клиентское приложение отправляет запрос в gateway по alias модели, например `smart-default`. Gateway сам находит реальный provider, внешнюю модель, endpoint провайдера, API key из env, выполняет upstream-вызов и возвращает нормализованный ответ.

Важно: текущий MVP не является прозрачной заменой OpenAI API, Anthropic API или Claude API. Сейчас публичный контракт gateway - собственный `POST /api/v1/generate`. Если цель - чтобы клиент мог указать URL gateway и слать запросы ровно как в OpenAI/Claude SDK, нужно отдельно добавить vendor-compatible endpoints вроде `/v1/chat/completions` и/или provider-specific compatibility layer.

## Зачем нужен

- не размазывать API keys и provider-specific настройки по приложениям;
- переключать реальные модели через alias без изменений в клиентах;
- хранить историю LLM-вызовов в одном месте;
- иметь базовый admin UI для наблюдения, проверки маршрутов и диагностики;
- постепенно добавлять новых providers через адаптеры, не ломая клиентский контракт.

## Как работает

Базовый поток:

1. Клиент вызывает `POST /api/v1/generate`.
2. В запросе указывает внутренний `model` alias.
3. Gateway ищет alias в `model_routes`.
4. Через `provider_code` получает provider-конфиг из `providers`.
5. По `provider.kind` выбирает adapter.
6. Adapter вызывает внешний AI API.
7. Gateway сохраняет запись в `requests`.
8. Клиент получает нормализованный response.

Сейчас реализован один provider adapter:

- `openai-compatible` - вызывает `POST {base_url}/chat/completions`.

Это покрывает OpenAI и сервисы, которые совместимы с OpenAI Chat Completions API. Anthropic Claude напрямую этим адаптером не покрывается, если только он не доступен через совместимый proxy.

## Что есть в MVP

- Rust backend на `axum`;
- PostgreSQL-хранилище для providers, model routes и истории запросов;
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

- показывать последние запросы;
- открывать детали запроса;
- показывать providers;
- показывать model routes;
- выполнять ручной smoke-test генерации.

Редактирование providers/routes через UI пока не реализовано.

## Конфигурация

Основные env-переменные:

- `APP_HOST`
- `APP_PORT`
- `DATABASE_URL`
- `DEFAULT_PROVIDER_CODE`
- `DEFAULT_PROVIDER_KIND`
- `DEFAULT_PROVIDER_BASE_URL`
- `DEFAULT_PROVIDER_API_KEY_ENV`
- `DEFAULT_PROVIDER_TIMEOUT_MS`
- `DEFAULT_MODEL_ALIAS`
- `DEFAULT_EXTERNAL_MODEL`
- `OPENAI_API_KEY`

Пример значений находится в [.env.example](.env.example).

При старте приложение применяет migrations и делает upsert default provider/model route из env. Это позволяет поднять рабочий gateway без ручного заполнения БД.

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

- `providers` - provider-конфиги без секретов;
- `model_routes` - mapping внутреннего alias на provider и external model;
- `requests` - история вызовов и ошибок.

Секреты не сохраняются в БД. В `providers.api_key_env` хранится только имя env-переменной, из которой gateway берет ключ во время запроса.

Подробнее:

- [Storage and Model Routing](docs/storage-and-routing.md)
- [Architecture](architecture.md)

## Граница текущей версии

Сознательно не реализовано:

- streaming;
- embeddings;
- tools / function calling;
- image generation;
- Anthropic-native adapter;
- retries и fallback orchestration;
- auth, roles, quotas и billing;
- полноценный audit trail изменения маршрутов;
- редактирование конфигурации через UI;
- vendor-compatible endpoints для прямого подключения OpenAI/Claude SDK.

Эти ограничения важны. Если обещать пользователю "укажи URL gateway вместо OpenAI/Claude и все заработает", текущая реализация этого не выдержит. Ближайший архитектурно честный следующий шаг - решить, gateway должен оставаться с собственным стабильным контрактом или дополнительно поддерживать совместимые внешние контракты провайдеров.

## Документы

- [AI Gateway MVP](docs/ai-gateway-mvp.md)
- [REST API](docs/rest-api.md)
- [Storage and Model Routing](docs/storage-and-routing.md)
- [Deploy on VPS](docs/vps-deploy.md)
- [Server CTL](docs/server-ctl.md)
- [Architecture](architecture.md)
