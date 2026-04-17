# AI Gateway MVP

## Что это

Первая версия `ai-gateway` — это внутренний сервис для твоих проектов, который даёт один HTTP endpoint для LLM-вызовов.

Клиентские приложения не знают:

- какой внешний провайдер используется;
- как называется реальная внешняя модель;
- по какому URL ходит upstream API.

Они знают только:

- endpoint gateway;
- alias модели, например `smart-default`.

## Что реализовано в MVP

- `POST /api/v1/generate` для text generation;
- alias routing через таблицу `model_routes`;
- provider-конфиг через таблицу `providers`;
- один adapter `openai-compatible`;
- хранение истории вызовов в таблице `requests`;
- базовый admin API;
- минимальный PrimeVue admin UI;
- один Docker-контейнер, внутри которого запускаются PostgreSQL и приложение.

## Что не делается в этой версии

Сознательно не реализовано:

- streaming;
- embeddings;
- tools / function calling;
- workflows / agents;
- domain-specific AI endpoints;
- роли, access control, квоты и биллинг;
- сложный fallback между несколькими providers;
- редактор providers/routes в UI.

Это не забытые задачи, а осознанная граница MVP.

## Базовый сценарий

Приложение отправляет:

```json
{
  "model": "smart-default",
  "messages": [
    { "role": "system", "content": "Ты помогаешь CMS." },
    { "role": "user", "content": "Разбери twig-фрагмент и опиши структуру меню." }
  ],
  "options": {
    "temperature": 0.2,
    "max_tokens": 2000
  }
}
```

Gateway:

1. ищет `smart-default` в `model_routes`;
2. получает связанный provider из `providers`;
3. читает API key из env по `api_key_env`;
4. вызывает upstream API;
5. сохраняет запись в `requests`;
6. возвращает нормализованный ответ.

## Как запустить

### Через Docker

Собрать образ:

```bash
docker build -t ai-gateway .
```

Запустить:

```bash
docker run --rm -p 8080:8080 -e OPENAI_API_KEY=your-key ai-gateway
```

После старта:

- API доступно на `http://localhost:8080`
- Admin UI доступен на `http://localhost:8080/`

### Важные env-переменные

- `OPENAI_API_KEY`
- `DATABASE_URL`
- `DEFAULT_PROVIDER_CODE`
- `DEFAULT_PROVIDER_KIND`
- `DEFAULT_PROVIDER_BASE_URL`
- `DEFAULT_PROVIDER_API_KEY_ENV`
- `DEFAULT_MODEL_ALIAS`
- `DEFAULT_EXTERNAL_MODEL`

Пример значений есть в `.env.example`.

## Как добавить нового provider

На уровне кода:

1. добавить adapter в `src/providers/`;
2. зарегистрировать его в `src/main.rs`.

На уровне данных:

1. добавить запись в `providers`;
2. указать `kind`, который понимает registry;
3. указать `api_key_env`, из которого gateway возьмёт секрет.

## Как добавить новый alias

Достаточно добавить запись в `model_routes`.

Пример:

```sql
INSERT INTO model_routes (alias, provider_code, external_model, is_enabled)
VALUES ('reasoning-default', 'openai', 'o4-mini', TRUE);
```

## Ограничения текущей версии

- adapter сейчас рассчитан на OpenAI-compatible `chat/completions`;
- история хранит только preview, а не полный prompt/response;
- route resolution пока работает только для enabled-конфигураций;
- retries и fallback отсутствуют;
- UI предназначен для наблюдения, а не администрирования настроек.
