-- Миграция для создания таблицы пользователей

CREATE TABLE users (
  -- Уникальный идентификатор пользователя (UUID)
  id UUID PRIMARY KEY,
  -- Имя пользователя
  name VARCHAR NOT NULL,
  -- Email пользователя, уникальный
  email VARCHAR NOT NULL UNIQUE,
  -- Хешированный пароль
  password VARCHAR NOT NULL,
  -- Возраст пользователя
  age INTEGER NOT NULL
);