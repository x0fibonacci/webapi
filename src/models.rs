use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// Структура для пользователя в базе данных
#[derive(Debug, Serialize, FromRow)]
pub struct User {
    pub id: Uuid,         // Уникальный идентификатор пользователя
    pub name: String,     // Имя пользователя
    pub email: String,    // Логин (почтовый адрес)
    pub password: String, // Хешированный пароль (SHA-256)
    pub age: i32,         // Возраст пользователя
}

// Структура для запроса на создание пользователя
#[derive(Debug, Deserialize)]
pub struct UserRequest {
    pub name: String,     // Имя пользователя
    pub email: String,    // Логин (почтовый адрес)
    pub password: String, // Пароль (нехешированный, для создания)
    pub age: i32,         // Возраст пользователя
}

// Структура для запроса на авторизацию
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,    // Логин (почтовый адрес)
    pub password: String, // Пароль (нехешированный, для проверки)
}

// Структура для запроса на обновление пользователя
#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub name: Option<String>, // Новое имя (опционально)
    pub age: Option<i32>,     // Новый возраст (опционально)
}

// Структура для JWT claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // Идентификатор пользователя (UUID)
    pub exp: i64,    // Время истечения токена (Unix timestamp)
}
