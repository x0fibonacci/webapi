-- Миграция для создания таблицы пользователей
-- Версия: 2.0
-- Дата: 2025-07-01

-- Создаем перечисление для ролей пользователей
CREATE TYPE user_role AS ENUM ('User', 'Admin', 'Moderator');

-- Создаем таблицу пользователей
CREATE TABLE users (
  -- Уникальный идентификатор пользователя (UUID)
  id UUID PRIMARY KEY,
  
  -- Имя пользователя (от 2 до 100 символов)
  name VARCHAR(100) NOT NULL CHECK (LENGTH(name) >= 2),
  
  -- Email пользователя, уникальный
  email VARCHAR(255) NOT NULL UNIQUE CHECK (email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'),
  
  -- Хешированный пароль (Argon2id)
  password_hash VARCHAR(255) NOT NULL,
  
  -- Возраст пользователя (положительное число)
  age SMALLINT NOT NULL CHECK (age >= 13 AND age <= 120),
  
  -- Роль пользователя
  role user_role NOT NULL DEFAULT 'User',
  
  -- Дата создания записи
  created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
  
  -- Дата последнего обновления записи
  updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
  
  -- Активен ли аккаунт пользователя
  is_active BOOLEAN NOT NULL DEFAULT TRUE
);

-- Создаем индекс для быстрого поиска по email
CREATE INDEX idx_users_email ON users(email);

-- Создаем индекс для фильтрации по активным пользователям и роли
CREATE INDEX idx_users_active_role ON users(is_active, role);

-- Создаем триггерную функцию для автоматического обновления поля updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Создаем триггер, который будет вызывать функцию при обновлении записи
CREATE TRIGGER update_users_updated_at
BEFORE UPDATE ON users
FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column();

-- Добавляем комментарии к таблице и столбцам для документации
COMMENT ON TABLE users IS 'Таблица пользователей системы';
COMMENT ON COLUMN users.id IS 'Уникальный идентификатор пользователя (UUID)';
COMMENT ON COLUMN users.name IS 'Имя пользователя';
COMMENT ON COLUMN users.email IS 'Email пользователя (используется для входа)';
COMMENT ON COLUMN users.password_hash IS 'Хешированный пароль пользователя (Argon2id)';
COMMENT ON COLUMN users.age IS 'Возраст пользователя (от 13 до 120 лет)';
COMMENT ON COLUMN users.role IS 'Роль пользователя в системе';
COMMENT ON COLUMN users.created_at IS 'Дата и время создания учетной записи';
COMMENT ON COLUMN users.updated_at IS 'Дата и время последнего обновления учетной записи';
COMMENT ON COLUMN users.is_active IS 'Флаг активности учетной записи';