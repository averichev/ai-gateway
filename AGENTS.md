# AGENTS.md

## Общие правила

- Не поддакивай.
- Критикуй решения, если они ухудшают изменяемость, поддержку или общую логику приложения.
- Выбирай решения по приоритетам: лёгкость внесения изменений, дешевизна поддержки, согласованность с архитектурой проекта.
- Учитывай общую логику приложения, а не только локальную задачу.
- Задавай уточняющие вопросы, если без них решение будет рискованным или двусмысленным.

## Архитектура AI Gateway

- Проект должен оставаться provider-agnostic gateway: клиенты используют внутренний контракт `POST /api/v1/generate`, а различия внешних AI API скрываются за `ProviderAdapter`.
- Ближайший фокус — довести `openai-compatible`, но нельзя превращать core проекта в OpenAI wrapper.
- OpenAI-specific request/response, путь `/chat/completions`, parsing `choices`, `message.content`, `usage` и mapping upstream errors должны жить в `src/providers/openai_compatible.rs`.
- Не протаскивай OpenAI DTO и OpenAI naming в `src/domain/mod.rs`, `src/usecases/generate.rs`, repositories, schema и admin UI как основной контракт.
- Новые providers добавляются через новый adapter в `src/providers/`, регистрацию в provider registry и новый `providers.kind`.
- DeepSeek, локальные модели и внешние proxy сначала подключай через `openai-compatible`, если они реально совместимы с OpenAI Chat Completions. Новый kind добавляй только когда поведения существующего adapter недостаточно.
- Anthropic/Claude не надо встраивать хаками в `openai_compatible.rs`; для него нужен отдельный adapter, когда дойдём до реализации.
- Если появятся входящие OpenAI-compatible endpoints вроде `/v1/chat/completions`, они должны быть тонким compatibility layer: vendor request -> `GenerateRequestDto` -> `GenerateService` -> vendor response. Они не должны заменять внутренний core API.
- API keys не хранятся в PostgreSQL. В БД хранится только имя env-переменной `api_key_env`.
- Подробная стратегия расширения providers описана в `docs/provider-strategy.md`.

## UI

- Для построения и изменения интерфейсов обязательно используй шаблон PrimeVue Sakai Vue как основной источник UI-паттернов: `.references/sakai-vue`.
- Если `.references/sakai-vue` отсутствует, перед UI-работой скачай его командой: `mkdir -p .references && git clone --depth 1 https://github.com/primefaces/sakai-vue.git .references/sakai-vue`.
- Перед любой существенной UI-работой сначала изучи релевантные файлы Sakai: layout, меню, страницы, компоненты, стили, CSS-переменные и использование PrimeVue.
- Интерфейсы в `admin/` должны повторять подход Sakai: структура layout, навигация, таблицы, формы, карточки, отступы, состояния, PrimeVue-компоненты и визуальный язык.
- Не изобретай отдельную дизайн-систему, кастомные primitives или несовместимый визуальный стиль, если задачу можно решить средствами и паттернами Sakai.
- Отклоняйся от Sakai только если шаблон явно не покрывает нужный сценарий или если пользователь явно требует другое решение. Причину отклонения фиксируй в ответе.
- `.references/` не является частью продукта и не должна попадать в git; это локальный справочник для LLM и разработчика.
