# Server CTL

## Что такое `agctl`

`agctl` — это production-oriented CLI для `ai-gateway`, который запускается прямо на VPS и управляет:

- Docker на сервере;
- operational-конфигом `ctl.toml`;
- runtime `.env`;
- образом приложения;
- apply/restart/status/logs.

Он не предназначен для локальной разработки. Для локальной работы остаётся `Makefile`.

## Почему отдельный `ctl`

Вместо того чтобы каждый раз вручную собирать длинные `docker pull`, `docker rm`, `docker run` команды, `agctl` даёт один внутренний интерфейс под production-операции.

Это особенно полезно, когда нужно:

- быстро проверить готовность сервера;
- не забыть проверить `.env`;
- единообразно обновлять image;
- аккуратно перезапускать контейнер и ждать healthcheck.

## Где лежит конфиг

По умолчанию:

- `ctl.toml` — `/opt/ai-gateway/ctl.toml`
- `.env` — `/opt/ai-gateway/.env`

Путь можно переопределить:

```bash
agctl --config /custom/path/ctl.toml status
```

## Формат `ctl.toml`

Пример:

```toml
app_dir = "/opt/ai-gateway"
env_file = "/opt/ai-gateway/.env"
image = "ghcr.io/your-org/ai-gateway:latest"
container_name = "ai-gateway"
published_port = 8080
volume_name = "ai-gateway-pgdata"
```

Назначение полей:

- `app_dir` — рабочая директория сервера для конфигов;
- `env_file` — путь к runtime env;
- `image` — Docker image, который будет запускаться;
- `container_name` — имя production-контейнера;
- `published_port` — внешний HTTP-порт;
- `volume_name` — volume для встроенного PostgreSQL.

## Команды

### `agctl doctor`

Проверяет:

- Linux platform;
- Docker CLI;
- Docker daemon;
- наличие `ctl.toml`;
- наличие `.env`;
- обязательные env-переменные;
- доступность image.

### `agctl docker ensure`

Проверяет Docker и при необходимости пытается установить его на Debian/Ubuntu.

### `agctl init`

Создаёт:

- `ctl.toml`
- `.env`

Пример:

```bash
agctl init
```

С кастомным app dir:

```bash
agctl --config /srv/ai-gateway/ctl.toml init --app-dir /srv/ai-gateway
```

### `agctl env show`

Показывает текущий `.env` в безопасном виде. Секреты маскируются.

### `agctl env set KEY VALUE`

Обновляет или добавляет env-переменную.

Пример:

```bash
agctl env set GATEWAY_MASTER_KEY '<base64-32-byte-key>'
```

### `agctl env validate`

Проверяет обязательные переменные и не пропускает placeholder-значения вроде `replace-me`.

### `agctl image show`

Показывает текущий image и связанные параметры контейнера.

### `agctl image set <image[:tag]>`

Меняет image в `ctl.toml`.

Пример:

```bash
agctl image set ghcr.io/<username>/ai-gateway:v0.1.1
```

### `agctl apply`

Основная команда применения текущей конфигурации приложения:

1. проверяет env;
2. делает `docker pull`;
3. останавливает и удаляет старый контейнер;
4. запускает новый контейнер;
5. ждёт успешный `/health`.

### `agctl restart`

Перезапускает контейнер и тоже ждёт успешный healthcheck.

### `agctl status`

Показывает:

- текущий config path;
- app dir;
- env path;
- image;
- имя контейнера;
- порт;
- volume;
- статус контейнера;
- healthcheck status.

### `agctl logs`

Показывает логи контейнера.

Примеры:

```bash
agctl logs
agctl logs --tail 500
agctl logs --follow
```

## Установка

Если репозиторий доступен на VPS:

```bash
make ctl-release
sudo make ctl-install
```

После этого бинарь будет установлен в `/usr/local/bin/agctl`.

## Ограничения текущей версии

- нет SSH-оркестрации с локальной машины;
- нет rollback/release history;
- нет registry auth management;
- нет управления Nginx/Caddy/systemd;
- установка Docker автоматизирована только для Debian/Ubuntu.

## Рекомендуемый сценарий

```bash
agctl doctor
agctl docker ensure
agctl init
agctl image set ghcr.io/<username>/ai-gateway:v0.1.0
agctl env set GATEWAY_MASTER_KEY '<base64-32-byte-key>'
agctl env validate
agctl apply
agctl status
```
