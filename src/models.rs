use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use validator::Validate;  // Удален неиспользуемый импорт ValidateArgs

// Добавьте в начало файла models.rs
#[derive(Debug)]
pub struct AppConfig {
    pub database_url: String,
    pub server_host: String,
    pub server_port: u16,
    pub jwt_secret: String,
    pub jwt_expiration: u64,
    pub cors_origins: String,
}

// Структура для пользователя в базе данных
#[derive(Debug, Serialize, FromRow)]
pub struct User {
    pub id: uuid::Uuid,                 // Уникальный идентификатор пользователя
    pub name: String,             // Имя пользователя
    pub email: String,            // Логин (почтовый адрес)
    #[serde(skip_serializing)]    // Не включаем пароль при сериализации
    pub password_hash: String,    // Хешированный пароль (Argon2id)
    pub age: i32,                 // Возраст пользователя (изменен тип с u32 на i32)
    pub role: UserRole,           // Роль пользователя
    pub created_at: DateTime<Utc>, // Время создания аккаунта
    pub updated_at: DateTime<Utc>, // Время последнего обновления
    pub is_active: bool,          // Активен ли аккаунт
}

// Перечисление для ролей пользователя
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
pub enum UserRole {
    User,
    Admin,
    Moderator,
}

// Структура для запроса на создание пользователя
#[derive(Debug, Deserialize, Validate)]
pub struct UserRequest {
    #[validate(length(min = 2, max = 100, message = "Имя должно содержать от 2 до 100 символов"))]
    pub name: String,             // Имя пользователя
    
    #[validate(email(message = "Некорректный формат email"))]
    pub email: String,            // Логин (почтовый адрес)
    
    #[validate(length(min = 8, message = "Пароль должен быть не менее 8 символов"))]
    #[validate(regex(path = "PASSWORD_REGEX", message = "Пароль должен содержать цифры, строчные и заглавные буквы"))]
    pub password: String,         // Пароль (нехешированный, для создания)
    
    #[validate(range(min = 13, max = 120, message = "Возраст должен быть от 13 до 120 лет"))]
    pub age: i32,                 // Возраст пользователя (изменен тип с u16 на i32)
}

// Регулярное выражение для проверки сложности пароля
lazy_static::lazy_static! {
    static ref PASSWORD_REGEX: regex::Regex = regex::Regex::new(
        r"^(?=.*[a-z])(?=.*[A-Z])(?=.*\d).{8,}$"
    ).unwrap();
}

// Структура для запроса на авторизацию
#[derive(Debug, Deserialize, Validate, Clone)]  // Добавлен Clone
pub struct LoginRequest {
    #[validate(email(message = "Некорректный формат email"))]
    pub email: String,            // Логин (почтовый адрес)
    
    #[validate(length(min = 1, message = "Пароль не может быть пустым"))]
    pub password: String,         // Пароль (нехешированный, для проверки)
}

// Структура для запроса на обновление пользователя
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateUserRequest {
    #[validate(length(min = 2, max = 100, message = "Имя должно содержать от 2 до 100 символов"))]
    pub name: Option<String>,     // Новое имя (опционально)
    
    #[validate(range(min = 13, max = 120, message = "Возраст должен быть от 13 до 120 лет"))]
    pub age: Option<i32>,         // Новый возраст (изменен тип с u16 на i32)
}

// Структура для запроса на смену пароля
#[derive(Debug, Deserialize, Validate, Clone)]
pub struct ChangePasswordRequest {
    #[validate(length(min = 1, message = "Текущий пароль не может быть пустым"))]
    pub current_password: String,
    
    #[validate(regex(path = "PASSWORD_REGEX", message = "Новый пароль должен содержать минимум 8 символов, включая цифры, строчные и заглавные буквы"))]
    pub new_password: String,
    
    #[validate(must_match(other = "new_password", message = "Пароли должны совпадать"))]
    pub confirm_password: String,
}

// Структура для ответа с токеном
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,            // JWT токен
    pub user: UserResponse,       // Информация о пользователе
}

// Структура для ответа с данными пользователя (без чувствительных полей)
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub age: i32,                 // Изменен тип с u16 на i32
    pub role: UserRole,
    pub created_at: DateTime<Utc>,
}

// Структура для JWT claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,              // Идентификатор пользователя (UUID)
    pub exp: i64,                 // Время истечения токена (Unix timestamp)
    pub iat: i64,                 // Время выдачи токена (Unix timestamp)
    pub role: UserRole,           // Роль пользователя
    pub email: String,            // Email пользователя
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            name: user.name.clone(),
            email: user.email.clone(),
            age: user.age,
            role: user.role,
            created_at: user.created_at,
        }
    }
}
