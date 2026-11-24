# Deploy on VPS

## Целевая схема

В текущем MVP production разворачивается одним Docker-контейнером:

- внутри контейнера живёт `ai-gateway`;
- внутри того же контейнера поднимается локальный PostgreSQL;
- наружу публикуется только HTTP-порт приложения.

Для локальной работы остаётся `Makefile`, а для production-сервера основным инструментом становится `agctl`, который запускается прямо на VPS.

## Что нужно заранее

- Linux VPS;
- публичный Docker image;
- `GATEWAY_MASTER_KEY` для encrypted provider secrets;
- желательно домен и reverse proxy перед сервисом.

Важно:

- `5432` наружу не публикуем;
- image предполагается публичным, поэтому отдельный registry login не нужен;
- `agctl` управляет только приложением и Docker, но не настраивает Nginx/Caddy.

## 1. Собрать и опубликовать образ

Эта часть делается локально или в CI.

Пример:

```bash
make docker-build IMAGE=ghcr.io/<username>/ai-gateway TAG=v0.1.0
make docker-push IMAGE=ghcr.io/<username>/ai-gateway TAG=v0.1.0
```

Или:

```bash
make docker-release IMAGE=ghcr.io/<username>/ai-gateway TAG=v0.1.0
```

## 2. Установить `agctl` на сервер

Если репозиторий лежит на VPS:

```bash
make ctl-release
sudo make ctl-install
```

После этого бинарь окажется в `/usr/local/bin/agctl`.

Проверка:

```bash
agctl --help
```

Если Cargo на сервере нет, можно собрать `agctl` заранее и доставить бинарь отдельно, но для MVP проще держать repo на сервере и собирать его там.

## 3. Подготовить сервер

Проверка окружения:

```bash
agctl doctor
```

Если Docker ещё не установлен:

```bash
agctl docker ensure
```

В первой версии автоустановка Docker рассчитана на Debian/Ubuntu.

## 4. Инициализировать production-конфиг

```bash
agctl init
```

По умолчанию будут созданы:

- `/opt/ai-gateway/ctl.toml`
- `/opt/ai-gateway/.env`

`ctl.toml` хранит operational-конфиг `agctl`, а `.env` — runtime-конфиг самого приложения.

## 5. Настроить image

Указать production image:

```bash
agctl image set ghcr.io/<username>/ai-gateway:v0.1.0
```

Проверить:

```bash
agctl image show
```

## 6. Настроить `.env`

Базовый шаблон уже создаётся через `agctl init`.

Минимум нужно заменить placeholder в `GATEWAY_MASTER_KEY`. Provider API keys после первого входа сохраняются через admin UI в encrypted-at-rest storage.

Можно отредактировать файл вручную:

```bash
nano /opt/ai-gateway/.env
```

Или менять значения точечно:

```bash
agctl env set GATEWAY_MASTER_KEY '<base64-32-byte-key>'
agctl env set DEFAULT_PROVIDER_BASE_URL https://api.openai.com/v1
```

Если используется relay/proxy, укажи его в `DEFAULT_PROVIDER_BASE_URL`.

Проверить конфиг:

```bash
agctl env validate
```

`agctl` считает placeholder-значения вроде `replace-me` невалидными для production.

## 7. Развернуть приложение

```bash
agctl apply
```

Что делает команда:

1. проверяет Docker;
2. проверяет `.env`;
3. делает `docker pull` выбранного image;
4. удаляет старый контейнер, если он был;
5. запускает новый контейнер;
6. ждёт успешный `/health`.

После запуска:

- API будет доступно на `http://<vps-ip>:8080/api/v1/generate`
- Admin UI будет доступно на `http://<vps-ip>:8080/`

## 8. Дальнейшее управление

Проверка статуса:

```bash
agctl status
```

Логи:

```bash
agctl logs
agctl logs --follow
```

Перезапуск:

```bash
agctl restart
```

Применение новой версии после смены image:

```bash
agctl image set ghcr.io/<username>/ai-gateway:v0.1.1
agctl apply
```

Docker volume `ai-gateway-pgdata` сохраняет встроенную PostgreSQL базу между перезапусками контейнера.

## 9. Проверка после запуска

Healthcheck:

```bash
curl http://127.0.0.1:8080/health
```

Ожидаемый ответ:

```json
{"status":"ok"}
```

Тестовый generate-запрос:

```bash
curl -X POST http://127.0.0.1:8080/api/v1/generate \
  -H "Content-Type: application/json" \
  -d '{
    "model": "smart-default",
    "messages": [
      { "role": "system", "content": "Ты помогаешь CMS." },
      { "role": "user", "content": "Кратко опиши назначение AI Gateway." }
    ],
    "options": {
      "temperature": 0.2,
      "max_tokens": 200
    }
  }'
```

## Практические замечания

- Для внешнего доступа лучше ставить перед сервисом Nginx или Caddy с HTTPS.
- Не публикуй `5432:5432` наружу.
- Храни `.env` только на VPS и не коммить его в репозиторий.
- Один контейнер с приложением и PostgreSQL подходит для MVP, но позже лучше разнести сервис и базу.
- `agctl` не делает rollback orchestration и не управляет reverse proxy.

## Безопасная история и очистка previews

Перед работой с документами Factum установите в `.env` (при отсутствии переменной поведение такое же):

```dotenv
STORE_CONTENT_PREVIEWS=false
REQUEST_PREVIEW_CHARS=1500
RESPONSE_PREVIEW_CHARS=2000
```

После изменения env пересоздайте контейнер обычной командой деплоя (`agctl apply`), чтобы новое значение попало в процесс. `agctl restart` только перезапускает существующий контейнер и изменения `.env` не применяет. `true` разрешает ограниченные previews обычного текста для временной диагностики. Запросы с инструментами и их историей исключены всегда. После диагностики верните `false` и очистите накопленные previews отдельно. Лимиты сами по себе не защищают конфиденциальные документы.

Отключение previews не удаляет старые данные. Автоматической очистки при старте нет. Утилита `purge_previews` использует тот же `DATABASE_URL`, что приложение, и доступ к нему должен быть только у администратора. Сначала создайте backup и проверьте его восстановление в изолированном окружении. Для встроенного PostgreSQL пример backup:

```bash
# Backup также содержит прежние конфиденциальные данные — храните с ограниченным доступом.
umask 077
docker exec ai-gateway sh -c 'pg_dump -U "$POSTGRES_USER" -d "$POSTGRES_DB" -Fc' > ai-gateway-before-preview-cleanup.dump

# Только количество записей с хотя бы одним не-NULL preview, без их содержимого:
docker exec ai-gateway /app/purge_previews
docker exec ai-gateway /app/purge_previews --tenant-id <tenant-uuid>

# Явная очистка выбранного tenant после backup:
docker exec ai-gateway /app/purge_previews --tenant-id <tenant-uuid> --apply
# Для всех tenants:
docker exec ai-gateway /app/purge_previews --apply
```

В локальном checkout эквивалент: `cargo run --bin purge_previews -- [--tenant-id <uuid>] [--apply]`. Убедитесь, что `DATABASE_URL` указывает на нужную БД. Без `--apply` записи не меняются. С `--apply` выполняется атомарный UPDATE только `prompt_preview`/`response_preview` в NULL; usage, status, latency, время, tenant/client IDs и routing metadata остаются. Утилита выводит только количество найденных/изменённых записей; ошибки имеют техническую категорию. Повторный запуск безопасен. Число между dry run и UPDATE может измениться при параллельном трафике.

Операция необратима без backup. Для больших историй запланируйте окно обслуживания: UPDATE создаёт WAL и удерживает блокировки изменяемых строк. Это логическая очистка: старые backups, WAL, физические копии страниц, прежние application/DB logs она не стирает. PostgreSQL и reverse proxy также не должны журналировать тела запросов и Authorization; application logging ограничен безопасными событиями даже при debug.

## Проверка готовности перед демонстрацией

1. `/health` подтверждает только работу HTTP-сервиса, как раньше.
2. Выполните в UI **Маршруты моделей → Проверить готовность** либо административный GET `/api/admin/tenants/{tenant_id}/model-routes/{alias}/readiness`. Нужны admin session token и доступ к tenant. Проверка читает конфигурацию/credentials локально, не обращается к LLM и не расходует её токены. `ready` не гарантирует доступность внешней модели.
3. С действующим machine token выбранного tenant явно выполните реальный тест (Python 3.10+ на машине администратора):

```bash
read -r -s GATEWAY_MACHINE_TOKEN
export GATEWAY_MACHINE_TOKEN
python3 scripts/smoke_llm.py --url https://gateway.example --model smart-default --timeout 90
unset GATEWAY_MACHINE_TOKEN
```

Скрипт проверит текст, структурированный вызов `demo_add_numbers` и финальный ответ после локального сложения. Успех — три строки `success=true` и exit code 0. Отсутствие Tool Calling — ошибка. Полный текст и секреты не печатаются; provider может тарифицировать запросы. CLI выполняется снаружи Docker: Python в production image не требуется. Ручная «Проверка генерации» в UI остаётся реальным текстовым запросом.

[REST API](rest-api.md#диагностика-маршрута-модели) описывает статусы, рекомендации и категории ошибок. Сопоставляйте request ID и alias в UI/журналах. При ошибках upstream автоматических повторов нет; после исправления причины запуск повторяет администратор.
