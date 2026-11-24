# G02 — итоговый отчёт

## Изменения

По умолчанию `STORE_CONTENT_PREVIEWS=false`, включая отсутствие переменной. Прежние поля `prompt_preview` и `response_preview` остаются в таблице и API, но новые запросы получают NULL. Явное `true` разрешает ограниченные previews обычного текста (1500/2000 Unicode-символов, прежний суффикс `...`). Любые tools/tool_choice/tool calls/results исключены всегда. При нулевом лимите соответствующий preview отключён.

История сохраняет request/tenant/client IDs, alias, provider/external model, status, latency, usage, техническую ошибку и время. Ошибки чтения конфигурации теперь также пытаются записать безопасную историю. Ошибка её сохранения не отменяет состоявшийся LLM-вызов. Миграций схемы нет; `/api/v1/generate` сохраняет JSON-контракт G01.

Ошибки provider и PostgreSQL не форматируются как произвольный текст в журналах/истории. Сохраняются категория, HTTP status, request ID и routing metadata. JSON rejection обработан безопасно во всех write/auth endpoints; HTTP trace использует шаблон маршрута без URI/query/headers/body. SQL statement logging отключён, внешние dependency logs фильтруются даже при RUST_LOG=trace.

Добавлен `purge_previews`: по умолчанию считает записи с не-NULL preview; `--apply` явно очищает только preview-поля, optional `--tenant-id` ограничивает tenant. Утилита включена в Docker image. Прежде нужен backup, восстановление preview возможно только из него. Очистка не меняет остальные поля, не удаляет историю, не выполняется автоматически и не стирает прежние backups/WAL/logs. Массовая очистка пользовательской БД в этой задаче не запускалась — операции проверялись на изолированных тестовых данных.

Readiness реализован отдельным use case и зарегистрированным adapter configuration check. Административный endpoint сначала проверяет session/tenant access. Он проверяет существование/включённость route/provider, внешнюю модель, kind, URL, timeout и локальную доступность credentials; сетевых вызовов, записей истории и платных запросов не делает. Общие статусы: ready/incomplete/unavailable; статусы проверок: ok/error/skipped. Отсутствующий alias — HTTP 404 с model_route=error; чужой tenant — 403 без конфигурации. Секреты и URL не возвращаются. `/health` не изменён.

Admin UI использует PrimeVue Button/Dialog/DataTable/Message/Tag и паттерны Sakai из `.references/sakai-vue`; отклонений от шаблона не потребовалось. В таблице/меню маршрутов доступно «Проверить готовность», проблемы показаны первыми с рекомендациями. Ручная генерация явно объясняет реальный вызов и расход токенов. Найден и исправлен прежний дефект admin DTO: `serde(flatten)` с arbitrary_precision отклонял temperature; прозрачная обёртка сохраняет внешний JSON.

Добавлен Python smoke-test реального `POST /api/v1/generate`: текст, структурированный `demo_add_numbers(12500, 7500)`, локальное сложение разрешённых аргументов и финальный ответ модели. Произвольный код не выполняется; секреты, prompts, tool results и ответы не печатаются. Неподдерживаемый/проигнорированный Tool Calling завершает тест ошибкой. Автоматических повторов и fallback нет. Удалён существовавший повтор max_tokens → max_completion_tokens; отключены также транспортные retries и upstream redirects.

## Пример readiness

GET `/api/admin/tenants/{tenant_id}/model-routes/smart-default/readiness`, HTTP 200:

```json
{
  "model_alias": "smart-default",
  "provider_code": "openai",
  "status": "ready",
  "checks": [
    {"code":"model_route","status":"ok"},
    {"code":"model_route_enabled","status":"ok"},
    {"code":"external_model","status":"ok"},
    {"code":"provider","status":"ok"},
    {"code":"provider_enabled","status":"ok"},
    {"code":"provider_timeout","status":"ok"},
    {"code":"provider_adapter","status":"ok"},
    {"code":"provider_parameters","status":"ok"},
    {"code":"provider_credentials","status":"ok"}
  ]
}
```

## Проверки

Выполнены 24 ноября 2026 года:

| Проверка | Результат |
| --- | --- |
| `cargo fmt --all -- --check` | PASS |
| `cargo test` | PASS: 7 unit + 18 integration = 25, 0 failed |
| `cargo clippy --all-targets -- -D warnings` | PASS |
| `npm ci --prefix admin` | PASS, существующий dependency audit сообщил 10 high |
| `npm run build --prefix admin` на Node 22.22 | PASS; предупреждение о bundle >500 kB |
| `git diff --check` | PASS |
| UI: readiness без credentials и ready | PASS на временном PostgreSQL и mock upstream |
| UI: существующая генерация с temperature | PASS на mock upstream; response/request ID/usage показаны |

Интеграционные тесты используют отдельный PostgreSQL 17 с CREATE DATABASE, sqlx создаёт БД для каждого теста. Проверены default/opt-in previews, ограничения длины, запрет previews в tool-истории, сохранность metadata, CLI dry run/apply/tenant scope, санитизация ошибок/диагностических журналов, invalid JSON/schema/tool calls, upstream/timeout/transport/storage failures, readiness и tenant isolation, прежние текстовые клиенты/admin smoke-test, полный G01 round-trip. Сам Python smoke-test проверен через настоящий Axum gateway с mock upstream на успешном цикле и отказе/игнорировании Tool Calling. Оплачиваемый API для тестов не нужен.

**Настоящая внешняя LLM не вызывалась.** Credentials для такого запуска не предоставлены. Ни mock upstream, ни ready не считаются доказательством работы реальной модели.

## Запуск с настоящей моделью

1. Настройте provider/route/credentials выбранного tenant и проверьте readiness через admin UI.
2. Передайте действующий machine token через env (ниже ввод для bash) и явно запустите скрипт:

```bash
read -r -s GATEWAY_MACHINE_TOKEN
export GATEWAY_MACHINE_TOKEN
python3 scripts/smoke_llm.py --url https://gateway.example --model smart-default --timeout 90
unset GATEWAY_MACHINE_TOKEN
```

Альтернатива аргументам: GATEWAY_URL, GATEWAY_MODEL_ALIAS, GATEWAY_TIMEOUT. Тест может тарифицироваться provider. Успех — text/tool_call/tool_result success=true и exit code 0; ошибки сценария — 1, неверная конфигурация — 2. Отдельный текстовый запрос доступен через прежнюю форму Admin UI.

## Ограничения и выявленные расхождения

- Readiness подтверждает локальную конфигурацию, не валидность ключа/модели на upstream или поддержку Tool Calling. Возможности реальной модели остаются непроверенными до явного smoke-test.
- Существующее хранение credentials в AES-256-GCM `provider_secrets` расходится с правилом AGENTS.md об env-only ключах. Оно уже было в исходном коде; G02 не мигрирует credential-модель и не добавляет новый способ хранения секретов.
- Старые previews остаются до явной административной очистки; прежние backup/WAL/logs и другие поля команда не переписывает. Для большого UPDATE может понадобиться окно обслуживания.
- Модель, отклоняющая `options.max_tokens`, теперь возвращает исходную техническую ошибку без скрытого повторного вызова. Можно опустить этот параметр и повторить вручную; smoke-test его не задаёт.
- В frontend lockfile остаются 10 high из npm audit и большой bundle (~945 kB до gzip); зависимости/разбиение bundle в G02 не обновлялись.
- Полная Docker-сборка образа и production deployment не выполнялись; backend checks и production-сборка Admin UI выполнены отдельно.

## Все изменённые и добавленные файлы

- `.env.example`
- `Dockerfile`
- `README.md`
- `admin/src/api.ts`
- `admin/src/views/GenerateTestView.vue`
- `admin/src/views/ModelRoutesView.vue`
- `docs/ai-gateway-mvp.md`
- `docs/g02-report.md`
- `docs/rest-api.md`
- `docs/storage-and-routing.md`
- `docs/vps-deploy.md`
- `scripts/smoke_llm.py`
- `src/app.rs`
- `src/bin/purge_previews.rs`
- `src/config/mod.rs`
- `src/domain/mod.rs`
- `src/http/admin.rs`
- `src/http/auth.rs`
- `src/http/generate.rs`
- `src/openapi.rs`
- `src/providers/mod.rs`
- `src/providers/openai_compatible.rs`
- `src/repositories/mod.rs`
- `src/repositories/postgres_requests.rs`
- `src/repositories/postgres_routes.rs`
- `src/security.rs`
- `src/server.rs`
- `src/usecases/generate.rs`
- `src/usecases/mod.rs`
- `src/usecases/readiness.rs`
- `tests/tool_calling.rs`
