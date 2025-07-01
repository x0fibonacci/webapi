#!/bin/bash

# Скрипт для запуска PostgreSQL с помощью Docker Compose

# Запускаем контейнер PostgreSQL в фоновом режиме
echo "Запускаем PostgreSQL через Docker Compose..."
docker-compose up -d

# Проверяем, готов ли PostgreSQL к подключению
echo "Ожидаем готовности PostgreSQL..."
until docker-compose exec postgres pg_isready; do
  echo "PostgreSQL еще не готов, ждем 1 секунду..."
  sleep 1
done

# Сообщаем об успешном запуске
echo "PostgreSQL успешно запущен!"

# Примечание: Убедитесь, что скрипт исполняемый
# Выполните: chmod +x start_postgres.sh