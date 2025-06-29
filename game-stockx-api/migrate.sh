#!/bin/bash


# Ожидаем, пока база данных будет доступна
echo "Waiting for PostgreSQL to become available..."
until pg_isready -h postgres -p 5432; do
  sleep 2
done

# Выполнение миграций
echo "Running database migrations..."
diesel migration run --database-url "$DATABASE_URL"

# Запуск приложения
echo "Starting the backend application..."
exec "$@"
