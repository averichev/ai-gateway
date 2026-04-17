# Storage and Model Routing

## Общая идея

В MVP конфигурация маршрутов и providers живёт в PostgreSQL, потому что это:

- достаточно просто для первого этапа;
- уже даёт admin API поверх реальных данных;
- не мешает позже добавить UI-редактирование;
- не требует прятать секреты в БД.

Секреты провайдера в таблицах не хранятся. Вместо этого provider хранит имя env-переменной, например `OPENAI_API_KEY`.

## Таблица `providers`

Назначение: описывает, как gateway должен ходить к конкретному AI-провайдеру.

Поля:

- `code` — внутренний код провайдера, например `openai`
- `kind` — тип adapter, например `openai-compatible`
- `base_url` — upstream base URL
- `api_key_env` — имя env-переменной с ключом
- `is_enabled` — включён ли provider
- `timeout_ms` — timeout на upstream call

Пример строки:

```text
code=openai
kind=openai-compatible
base_url=https://api.openai.com/v1
api_key_env=OPENAI_API_KEY
```

## Таблица `model_routes`

Назначение: мапит внутренний alias на конкретный provider и внешнюю модель.

Поля:

- `alias`
- `provider_code`
- `external_model`
- `is_enabled`

Пример:

```text
alias=smart-default
provider_code=openai
external_model=gpt-4.1-mini
```

## Таблица `requests`

Назначение: хранит историю вызовов gateway.

Поля:

- `id`
- `created_at`
- `model_alias`
- `provider_code`
- `external_model`
- `status`
- `latency_ms`
- `error_message`
- `input_tokens`
- `output_tokens`
- `prompt_preview`
- `response_preview`

Таблица хранит не полный transcript, а безопасный preview. Это осознанный компромисс MVP.

## Почему нет `request_events`

Для первой версии отдельная таблица `request_events` не введена специально.

Причина простая: пока нет retry orchestration, fallback chains, streaming states и сложной асинхронной обработки, одной таблицы `requests` достаточно.

Когда появятся:

- retry;
- fallback между providers;
- промежуточные состояния;
- streaming lifecycle;

тогда `request_events` станет оправданной.

## Seed-логика

После применения migrations приложение делает upsert базовой конфигурации:

- provider из env `DEFAULT_PROVIDER_*`
- alias route из env `DEFAULT_MODEL_ALIAS` и `DEFAULT_EXTERNAL_MODEL`

Это даёт рабочий старт с нуля без ручного наполнения БД.

## Как добавить нового provider

1. Добавить adapter в код.
2. Зарегистрировать новый `kind` в provider registry.
3. Вставить строку в `providers`.
4. Убедиться, что env из `api_key_env` задан в runtime.

Пример SQL:

```sql
INSERT INTO providers (code, kind, base_url, api_key_env, is_enabled, timeout_ms)
VALUES ('my-provider', 'openai-compatible', 'https://relay.example.com/v1', 'MY_PROVIDER_API_KEY', TRUE, 60000);
```

## Как добавить новый alias

Пример:

```sql
INSERT INTO model_routes (alias, provider_code, external_model, is_enabled)
VALUES ('cheap-default', 'openai', 'gpt-4.1-mini', TRUE);
```

После этого клиент может вызывать:

```json
{
  "model": "cheap-default",
  "messages": [
    { "role": "user", "content": "..." }
  ]
}
```

## Текущие ограничения схемы

- нет versioning для provider-конфигов;
- нет audit trail изменений routes;
- нет отдельного хранения полного prompt/response;
- нет метрик по retry/fallback;
- нет нормализованной billing-аналитики.

Для MVP это сознательно упрощено.
