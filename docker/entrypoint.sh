#!/usr/bin/env bash
set -euo pipefail

export POSTGRES_USER="${POSTGRES_USER:-ai_gateway}"
export POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-ai_gateway}"
export POSTGRES_DB="${POSTGRES_DB:-ai_gateway}"
export APP_HOST="${APP_HOST:-0.0.0.0}"
export APP_PORT="${APP_PORT:-8080}"
export ADMIN_DIST_DIR="${ADMIN_DIST_DIR:-/app/admin/dist}"
export DATABASE_URL="${DATABASE_URL:-postgres://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:5432/${POSTGRES_DB}}"

/usr/local/bin/docker-entrypoint.sh postgres -c config_file=/etc/postgresql/postgresql.conf &
POSTGRES_PID=$!

cleanup() {
  kill -TERM "${APP_PID:-0}" "${POSTGRES_PID:-0}" 2>/dev/null || true
  wait "${APP_PID:-0}" 2>/dev/null || true
  wait "${POSTGRES_PID:-0}" 2>/dev/null || true
}

trap cleanup TERM INT

until pg_isready -h 127.0.0.1 -p 5432 -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" >/dev/null 2>&1; do
  if ! kill -0 "${POSTGRES_PID}" 2>/dev/null; then
    echo "postgres exited before becoming ready" >&2
    exit 1
  fi

  sleep 1
done

/app/ai-gateway &
APP_PID=$!

wait -n "${APP_PID}" "${POSTGRES_PID}"
STATUS=$?

cleanup
exit "${STATUS}"
