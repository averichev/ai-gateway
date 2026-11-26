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
- `POST /api/v1/generate` для текста и структурированного Tool Calling;
- admin API;
- Swagger UI;
- PrimeVue admin UI;
- smoke-test генерации из админки;
- Docker image, где runtime-контейнер запускает приложение и PostgreSQL;
- `agctl` для управления деплоем на VPS.

Глобальному владельцу доступна страница **Конфигурация → Обновления**. В `.env.example` настроен официальный канал AI Gateway. По команде владельца шлюз проверяет манифест и подпись выпуска встроенным ключом. В Docker образ устанавливается через `agctl`. Отдельная Linux-служба поддерживает установку и откат бинарника со встроенной админкой. [Настройка подписанных обновлений](docs/standalone-updates.md)

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

## Tool Calling

Gateway передаёт definitions и результаты инструментов модели, возвращает один или несколько структурированных вызовов. Factum исполняет их и отправляет следующий запрос. Gateway не подключает MCP, не запускает инструменты и не ведёт agent loop.

Пример полного цикла (каждый запрос — `POST /api/v1/generate` с machine token):

1. Объявить инструмент:

```json
{
  "model": "smart-default",
  "messages": [{"role": "user", "content": "Погода в Москве?"}],
  "tools": [{"name": "weather", "description": "Текущая погода", "parameters": {
    "type": "object", "properties": {"city": {"type": "string"}}, "required": ["city"]
  }}],
  "tool_choice": "auto"
}
```

2. Получить вызов:

```json
{
  "id": "req_first", "model": "smart-default", "provider": "openai",
  "output_text": "", "finish_reason": "tool_calls",
  "tool_calls": [{"id": "call_1", "name": "weather", "arguments": {"city": "Москва"}}],
  "usage": {"input_tokens": 80, "output_tokens": 20}
}
```

3. После исполнения в приложении передать результат:

```json
{
  "model": "smart-default",
  "messages": [
    {"role": "user", "content": "Погода в Москве?"},
    {"role": "assistant", "tool_calls": [{"id": "call_1", "name": "weather", "arguments": {"city": "Москва"}}]},
    {"role": "tool", "tool_call_id": "call_1", "content": "+12 °C, солнечно"}
  ],
  "tool_choice": "none"
}
```

4. Получить финальный ответ:

```json
{
  "id": "req_second", "model": "smart-default", "provider": "openai",
  "output_text": "В Москве солнечно, +12 °C.", "finish_reason": "stop",
  "usage": {"input_tokens": 120, "output_tokens": 15}
}
```

Старые текстовые запросы и строковый `output_text` сохранены. `tool_choice` поддерживает `auto`, `none`, `required`, `{"name":"weather"}`. Поддержка конкретной модели определяется ответом upstream; явный отказ возвращается как `400 tool_calling_not_supported`.

Лимиты: 2 MiB на запрос, 256 сообщений, 128 инструментов / вызовов за completion, 64 KiB на схему / объект аргументов. Аргументы проверяются по JSON Schema без сетевого и файлового доступа. Preview запросов с инструментами не сохраняется. Подробности и ошибки: [REST API](docs/rest-api.md#полный-цикл-tool-calling).

## Проверки backend

Интеграционные тесты поднимают контролируемый HTTP upstream и реальный Axum gateway. Нужен отдельный PostgreSQL с правом `CREATE DATABASE`: `sqlx::test` создаёт и удаляет отдельную БД для каждого теста. Не используйте production instance.

```bash
export DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5432/postgres
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Тесты проверяют реальные HTTP payloads, полный цикл с одним/несколькими вызовами, `tool_choice`, сохранность аргументов, ошибки и лимиты, usage, machine auth, tenant-local routing и историю.

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
- `PORT` — HTTP-порт шлюза. При отсутствии `PORT` используется `APP_PORT`, затем значение по умолчанию `8080`.
- `DATABASE_URL`
- `GATEWAY_MASTER_KEY` - base64-encoded 32-byte key для AES-256-GCM provider secrets
- `DEFAULT_PROVIDER_CODE`
- `DEFAULT_PROVIDER_KIND`
- `DEFAULT_PROVIDER_BASE_URL`
- `DEFAULT_PROVIDER_TIMEOUT_MS`
- `DEFAULT_MODEL_ALIAS`
- `DEFAULT_EXTERNAL_MODEL`
- `STORE_CONTENT_PREVIEWS` — `false` по умолчанию; только явное `true` включает previews обычного текста
- `REQUEST_PREVIEW_CHARS`, `RESPONSE_PREVIEW_CHARS` — лимиты previews (1500/2000)

Пример значений находится в [.env.example](.env.example).

При старте приложение применяет migrations и делает upsert default provider/model route в tenant `default`. Provider API key задаётся через admin UI и хранится только encrypted-at-rest.

## Запуск

Для запуска из исходников на порту `9090` задайте `PORT` в окружении процесса или в корневом `.env`.

```bash
PORT=9090 cargo run --bin ai-gateway
```

Для запуска в Docker укажите `PORT=9090` в `.env` и выполните команду ниже. Makefile опубликует тот же порт на хосте. Если нужен другой внешний порт, задайте его при запуске командой `make run PORT=3000`.

```bash
make run
```

Для ручного запуска Docker задайте порт сервера и пробросьте его на хост.

```bash
docker build -t ai-gateway .
docker run --rm --env-file .env -e PORT=9090 -p 9090:9090 ai-gateway
```

При внешнем порте `9090` API и админка доступны на `http://localhost:9090/`, а Swagger UI — на `http://localhost:9090/swagger-ui/`.

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
- выполнение инструментов и agent orchestration;
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

## Безопасная история и диагностика маршрутов (G02)

`STORE_CONTENT_PREVIEWS=false` — безопасное значение по умолчанию, в том числе при отсутствии переменной. В `requests` остаются request/tenant/client IDs, alias, provider, external model, status, latency, usage, техническая ошибка и время. `prompt_preview` и `response_preview` равны `null`; контракт истории и схема таблицы сохранены.

Явное `STORE_CONTENT_PREVIEWS=true` разрешает временную диагностику обычного текста с лимитами `REQUEST_PREVIEW_CHARS=1500` и `RESPONSE_PREVIEW_CHARS=2000`. Эти записи могут содержать конфиденциальные данные. Запросы с `tools`, `tool_choice`, assistant tool calls или tool results исключены всегда. Настройка применяется после перезапуска и не очищает старые записи.

Очистка старых previews — отдельная операция администратора. Сначала сделайте резервную копию БД. Без `--apply` команда только показывает число записей, не выводя содержимое:

```bash
cargo run --bin purge_previews --
cargo run --bin purge_previews -- --tenant-id <tenant-uuid>
# После проверки количества и резервного копирования:
cargo run --bin purge_previews -- --tenant-id <tenant-uuid> --apply
```

Без `--tenant-id` выбираются все tenants. Меняются только два preview-поля, история и metadata остаются. Восстановление возможно из backup; прежние backups и WAL эта команда не очищает. В Docker образ включён `/app/purge_previews`. Порядок backup и очистки: [деплой](docs/vps-deploy.md#безопасная-история-и-очистка-previews).

`GET /api/admin/tenants/{tenant_id}/model-routes/{alias}/readiness` требует admin session и доступ к tenant. Проверяет включённость route/provider, adapter, внешнее имя модели, параметры URL, timeout и возможность прочитать credentials. Возвращает `ready`, `incomplete` или `unavailable` с машинными проверками и рекомендациями; не выполняет сетевых запросов. В Admin UI: **Маршруты моделей → Проверить готовность**. `/health` по-прежнему проверяет только HTTP-сервис.

Для реальной проверки текстовой генерации и полного Tool Calling round-trip используйте Python 3.10+ и действующий machine token выбранного tenant:

```bash
# В bash: ввод скрыт и не попадает в историю команд.
read -r -s GATEWAY_MACHINE_TOKEN
export GATEWAY_MACHINE_TOKEN
python3 scripts/smoke_llm.py --url http://127.0.0.1:8080 --model smart-default --timeout 90
unset GATEWAY_MACHINE_TOKEN
```

Все параметры можно передать через `GATEWAY_URL`, `GATEWAY_MACHINE_TOKEN`, `GATEWAY_MODEL_ALIAS`, `GATEWAY_TIMEOUT`; есть также `--token` (предпочтительнее env, чтобы не раскрыть token в process list). Скрипт делает реальные `POST /api/v1/generate`: текст, вызов `demo_add_numbers(12500, 7500)`, затем ответ с локально вычисленной суммой. Gateway инструмент не выполняет. Печатаются только JSON-строки с success, HTTP status, request ID, alias, latency, usage и категорией ошибки. Код завершения: `0` — всё успешно, `1` — сценарий не прошёл, `2` — неверная конфигурация скрипта. Неподдерживаемый или проигнорированный Tool Calling — ошибка, а не успех.

Эти вызовы могут тарифицироваться провайдером и запускаются только явно. Readiness не подтверждает доступность модели, действительность ключа у провайдера или поддержку Tool Calling. Ручная «Проверка генерации» в Admin UI также выполняет реальный вызов, проверяет только текст и сохраняет существующий контракт. Автоматических повторов, fallback и переходов по upstream redirects нет; в том числе удалён прежний повтор с заменой `max_tokens`. Если модель отклоняет этот параметр, исключите `options.max_tokens` и повторите вручную.

CI и `cargo test` используют только mock upstream и отдельный PostgreSQL, без оплачиваемого API. Интеграционные тесты дополнительно запускают сам Python-скрипт через Gateway с mock upstream, проверяют admin smoke-test, CLI-очистку и отсутствие секретов в диагностике. Сборка Admin UI: `cd admin && npm ci && npm run build` (Node.js 22.12+).

Подробные статусы readiness и диагностика ошибок: [REST API](docs/rest-api.md#диагностика-маршрута-модели).
