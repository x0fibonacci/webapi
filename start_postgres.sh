#!/bin/bash

# Скрипт для запуска PostgreSQL с помощью Docker Compose

# Установка переменных
MAX_RETRIES=30
SLEEP_TIME=2
COMPOSE_FILE="docker-compose.yml"

# Функция для очистки при выходе
cleanup() {
  echo "Скрипт прерван. Выполняется очистка..."
  exit 1
}

# Перехват сигналов прерывания
trap cleanup SIGINT SIGTERM

# Проверяем наличие docker
if ! command -v docker &> /dev/null; then
  echo "Ошибка: Docker не установлен. Установите Docker и попробуйте снова."
  exit 1
fi

# Проверяем наличие docker-compose
if ! command -v docker-compose &> /dev/null; then
  echo "Ошибка: Docker Compose не установлен. Установите Docker Compose и попробуйте снова."
  exit 1
fi

# Проверяем наличие файла docker-compose.yml
if [ ! -f "$COMPOSE_FILE" ]; then
  echo "Ошибка: Файл $COMPOSE_FILE не найден в текущей директории."
  echo "Убедитесь, что вы находитесь в корне проекта или укажите правильный путь к файлу."
  exit 1
fi

# Проверяем, запущен ли уже контейнер postgres
if docker-compose ps | grep -q "postgres.*Up"; then
  echo "PostgreSQL уже запущен."
  exit 0
fi

# Запускаем контейнер PostgreSQL в фоновом режиме
echo "Запускаем PostgreSQL через Docker Compose..."
if ! docker-compose up -d; then
  echo "Ошибка: Не удалось запустить контейнеры через Docker Compose."
  exit 1
fi

# Проверяем, готов ли PostgreSQL к подключению с таймаутом
echo "Ожидаем готовности PostgreSQL (максимум ${MAX_RETRIES} попыток)..."
retries=0
while [ $retries -lt $MAX_RETRIES ]; do
  if docker-compose exec postgres pg_isready &> /dev/null; then
    echo "PostgreSQL успешно запущен и готов к работе!"
    
    # Дополнительная информация о подключении
    DB_NAME=$(grep POSTGRES_DB $COMPOSE_FILE | cut -d: -f2 | tr -d ' ' || echo "postgres")
    DB_USER=$(grep POSTGRES_USER $COMPOSE_FILE | cut -d: -f2 | tr -d ' ' || echo "postgres")
    DB_PORT=$(grep '5432' $COMPOSE_FILE | grep -oP '\d+(?=:5432)' || echo "5432")
    
    echo "-------------------------------------"
    echo "Информация для подключения:"
    echo "  Хост:     localhost"
    echo "  Порт:     $DB_PORT"
    echo "  База:     $DB_NAME"
    echo "  Пользователь: $DB_USER"
    echo "-------------------------------------"
    echo "Для подключения к psql выполните: docker-compose exec postgres psql -U $DB_USER -d $DB_NAME"
    exit 0
  fi
  
  retries=$((retries+1))
  echo "PostgreSQL еще не готов (попытка $retries из $MAX_RETRIES), ждем $SLEEP_TIME секунды..."
  sleep $SLEEP_TIME
done

echo "Ошибка: PostgreSQL не стал доступен после $MAX_RETRIES попыток."
echo "Проверьте логи: docker-compose logs postgres"
echo "Останавливаем контейнеры..."
docker-compose down
exit 1