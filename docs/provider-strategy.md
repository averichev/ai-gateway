# Provider Strategy

## Главная идея

`ai-gateway` должен развиваться как provider-agnostic gateway: клиенты используют один внутренний контракт gateway, а различия OpenAI, Anthropic, DeepSeek, локальных моделей и кастомных серверов скрываются за provider adapters.

На ближайшем этапе фокус практичный: довести `openai-compatible` до надежного состояния. Но архитектурно проект не должен становиться OpenAI-centric.

Правильная граница:

```text
HTTP API gateway
  -> внутренний GenerateRequest / GenerateResponse
    -> ProviderAdapter
      -> конкретный внешний protocol
```

Неправильная граница:

```text
Весь gateway = OpenAI API wrapper
```

Если OpenAI-форматы расползутся по domain DTO, use cases, storage и admin UI, добавление Claude или локального сервера станет дорогим. Поэтому OpenAI-specific детали должны оставаться внутри adapter layer.

## Что значит "универсальный"

Gateway не может быть универсальным "для любой модели" сам по себе. Модель доступна через конкретный protocol:

- OpenAI Chat Completions;
- Anthropic Messages API;
- DeepSeek OpenAI-compatible API;
- локальный OpenAI-compatible endpoint, например vLLM, LM Studio, Ollama-compatible proxy;
- кастомный HTTP endpoint.

Поэтому расширяемость строится не вокруг названий моделей, а вокруг `provider.kind`.

Примеры будущих `kind`:

```text
openai-compatible
anthropic
custom-http
```

Отдельный `deepseek` или `local-openai-compatible` нужен только если поведения `openai-compatible` недостаточно. Если provider реально совместим с OpenAI Chat Completions, дешевле использовать тот же adapter с другим `base_url`.

## Текущий фокус

Сейчас нужно оттачивать `openai-compatible`:

- корректная работа с разными `base_url`;
- понятные ошибки upstream;
- timeout handling;
- нормализация usage;
- устойчивость к неожиданным response shapes;
- тесты на success, upstream error, timeout, invalid response.

При этом нельзя превращать внутренний контракт gateway в OpenAI request/response.

Основной продуктовый endpoint остается:

```http
POST /api/v1/generate
```

Он принимает внутренний request:

```json
{
  "model": "smart-default",
  "messages": [
    { "role": "user", "content": "..." }
  ],
  "options": {
    "temperature": 0.2,
    "max_tokens": 1000
  }
}
```

И возвращает внутренний response:

```json
{
  "id": "req_...",
  "model": "smart-default",
  "provider": "openai",
  "output_text": "...",
  "finish_reason": "stop",
  "usage": {
    "input_tokens": 120,
    "output_tokens": 80
  }
}
```

## Где живет OpenAI-specific код

OpenAI-compatible детали должны жить в:

```text
src/providers/openai_compatible.rs
```

Там допустимы:

- путь `/chat/completions`;
- OpenAI-compatible request payload;
- OpenAI-compatible response payload;
- parsing `choices`, `message.content`, `usage`;
- mapping upstream error body.

OpenAI-specific детали не должны попадать в:

- `src/domain/mod.rs`;
- `src/usecases/generate.rs`;
- `src/repositories/*`;
- PostgreSQL schema, кроме generic provider config;
- admin UI как обязательная модель данных.

Исключение: отдельный compatibility endpoint, если он будет добавлен явно.

## ProviderAdapter contract

`ProviderAdapter` - граница между общей логикой gateway и конкретным внешним provider protocol.

Adapter получает:

- resolved route: external model, base URL, timeout, provider metadata;
- внутренний `GenerateRequestDto`;
- API key, уже выбранный gateway.

Adapter возвращает:

- нормализованный `ProviderGenerateResult`;
- либо `ProviderError`, который use case преобразует в HTTP error и запись истории.

Use case не должен знать, как устроен OpenAI, Claude или локальный сервер. Он должен знать только `provider.kind` и результат adapter.

## Как добавлять новых providers

Новый provider добавляется так:

1. Создать adapter в `src/providers/`.
2. Реализовать `ProviderAdapter`.
3. Зарегистрировать adapter в provider registry.
4. Добавить `kind` в таблицу `providers`.
5. Добавить alias в `model_routes`.

Пример:

```sql
INSERT INTO providers (code, kind, base_url, api_key_env, is_enabled, timeout_ms)
VALUES ('anthropic', 'anthropic', 'https://api.anthropic.com', 'ANTHROPIC_API_KEY', TRUE, 60000);

INSERT INTO model_routes (alias, provider_code, external_model, is_enabled)
VALUES ('claude-default', 'anthropic', 'claude-sonnet', TRUE);
```

Клиент при этом продолжает вызывать gateway одинаково:

```json
{
  "model": "claude-default",
  "messages": [
    { "role": "user", "content": "..." }
  ]
}
```

## OpenAI SDK compatibility

Сейчас gateway не является drop-in заменой OpenAI API. Нельзя просто поменять `baseURL` в OpenAI SDK и ожидать полной совместимости.

Если такая совместимость понадобится, ее нужно добавлять отдельным входящим слоем:

```http
POST /v1/chat/completions
```

Этот слой должен:

1. принять OpenAI-compatible request;
2. перевести его во внутренний `GenerateRequestDto`;
3. вызвать общий `GenerateService`;
4. перевести `GenerateResponseDto` обратно в OpenAI-compatible response.

Он не должен становиться новой внутренней архитектурой. Это только adapter на входе, такой же внешний слой, как provider adapters на выходе.

## Что не делать

- Не тащить OpenAI request/response в core domain как основной контракт.
- Не добавлять Claude через хаки в `openai_compatible.rs`.
- Не создавать преждевременно "универсальный AI runtime" для streaming, tools, embeddings и multimodal, пока эти сценарии не реализуются.
- Не хранить API keys в PostgreSQL.
- Не делать provider-specific поля обязательными для всех providers.

Расширять нужно capability-first:

1. text/chat generation;
2. streaming;
3. tools/function calling;
4. embeddings;
5. multimodal.

Каждая новая capability должна расширять внутренний контракт осознанно, а не через случайное копирование формата одного vendor.
