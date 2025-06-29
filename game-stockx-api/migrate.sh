#!/bin/bash

# Выполнение миграций
echo "Running database migrations..."
diesel migration run --database-url $DATABASE_URL

# Запуск приложения
echo "Starting the backend application..."
exec "$@"
