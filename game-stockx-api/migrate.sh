#!/bin/bash
set -euo pipefail

: "${DATABASE_URL:?DATABASE_URL must be set}"

# Ожидаем, пока база данных будет доступна
echo "Waiting for PostgreSQL to become available..."
until pg_isready -h postgres -p 5432; do
  sleep 2
done

# Выполнение миграций
echo "Running database migrations..."
if ! diesel migration run --database-url "$DATABASE_URL"; then
  echo "Database migrations failed; backend startup aborted." >&2
  exit 1
fi

# Запуск приложения
echo "Starting the backend application..."
exec "$@"
