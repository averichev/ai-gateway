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
