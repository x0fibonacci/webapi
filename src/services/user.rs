use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use chrono::Utc;
use sqlx::PgPool;
use tokio::task;
use uuid::Uuid;
use crate::models::ChangePasswordRequest;
use crate::repositories;
use crate::errors::AppError;

use crate::models::{
    AuthResponse, Claims, LoginRequest, UpdateUserRequest, User, UserRequest, UserResponse, UserRole,
};
use crate::repositories::user::{
    create_user as create_user_repo, find_user_by_email, update_user as update_user_repo,
};
use jsonwebtoken::{encode, EncodingKey, Header};
use std::env;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use validator::Validate;

// Константы для токенов
const TOKEN_EXPIRY_SECONDS: i64 = 3600; // 1 час

// Хеширует пароль с использованием Argon2id
async fn hash_password(password: String) -> Result<String, AppError> {
    task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        
        argon2.hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Ошибка хеширования пароля: {}", e)))
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Ошибка в задаче хеширования: {}", e)))?
}

// Проверяет соответствие пароля хешу
async fn verify_password(password: String, hash: String) -> Result<bool, AppError> {
    task::spawn_blocking(move || {
        let parsed_hash = match PasswordHash::new(&hash) {
            Ok(h) => h,
            Err(e) => return Err(AppError::Internal(anyhow::anyhow!("Ошибка парсинга хеша: {}", e))),
        };
        
        Ok(Argon2::default().verify_password(password.as_bytes(), &parsed_hash).is_ok())
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("Ошибка в задаче проверки: {}", e)))?
}

// Создаёт токен JWT
fn generate_token(user_id: &Uuid, email: &str, role: UserRole) -> Result<String, AppError> {
    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET должен быть задан в .env");
    
    // Текущее время в секундах
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as i64;
    
    let claims = Claims {
        sub: user_id.to_string(),
        exp: now + TOKEN_EXPIRY_SECONDS,
        iat: now,
        role,
        email: email.to_string(),
    };
    
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|_e| {
        AppError::Internal(anyhow::anyhow!("Ошибка создания токена"))
    })
}

// Создаёт нового пользователя с безопасно хешированным паролем
pub async fn create_user_service(user_request: UserRequest, pool: &PgPool) -> Result<User, AppError> {
    log::info!("Запрос на создание пользователя с email: {}", user_request.email);
    
    // Валидируем данные
    user_request.validate()
        .map_err(|e| {
            log::warn!("Ошибки валидации при создании пользователя: {:?}", e);
            AppError::from(e)
        })?;
    
    // Проверяем, что пользователь с таким email не существует
    if let Ok(_) = find_user_by_email(&user_request.email, pool).await {
        log::warn!("Попытка создать пользователя с существующим email: {}", user_request.email);
        return Err(AppError::Conflict(
            format!("Пользователь с email '{}' уже существует", user_request.email)
        ));
    }

    // Хешируем пароль безопасным алгоритмом Argon2id
    let hashed_password = hash_password(user_request.password).await?;
    
    // Текущее время для создания/обновления
    let now = Utc::now();

    // Создаём пользователя через репозиторий
    let user = User {
        id: Uuid::new_v4(),
        name: user_request.name,
        email: user_request.email,
        password_hash: hashed_password,
        age: user_request.age,
        role: UserRole::User, // По умолчанию обычная роль
        created_at: now,
        updated_at: now,
        is_active: true,
    };
    
    let created_user = create_user_repo(&user, pool).await?;
    log::info!("Пользователь успешно создан с ID: {}", created_user.id);
    
    Ok(created_user)
}

// Аутентифицирует пользователя и возвращает JWT-токен и данные
pub async fn login_service(login_request: LoginRequest, pool: &PgPool) -> Result<AuthResponse, AppError> {
    log::info!("Попытка входа пользователя с email: {}", login_request.email);
    
    // Валидируем данные
    login_request.validate()
        .map_err(|e| {
            log::warn!("Ошибки валидации при входе: {:?}", e);
            AppError::from(e)
        })?;
    
    // Находим пользователя по email
    let user = find_user_by_email(&login_request.email, pool)
        .await
        .map_err(|e| {
            log::warn!("Неудачный вход: пользователь с email {} не найден", login_request.email);
            // Не раскрываем, существует ли пользователь
            AppError::Unauthorized
        })?;

    // Проверяем пароль
    let is_valid = verify_password(login_request.password, user.password_hash.clone()).await?;
    
    if !is_valid {
        log::warn!("Неудачный вход: неверный пароль для пользователя {}", user.email);
        return Err(AppError::Unauthorized);
    }

    // Проверяем, что аккаунт активен
    if !user.is_active {
        log::warn!("Попытка входа в неактивный аккаунт: {}", user.email);
        return Err(AppError::Forbidden("Аккаунт деактивирован".to_string()));
    }

    // Генерируем JWT-токен
    let token = generate_token(&user.id, &user.email, user.role)?;
    
    log::info!("Успешный вход пользователя: {} (ID: {})", user.email, user.id);
    
    // Создаем безопасный ответ (без пароля)
    let user_response = UserResponse::from(&user);
    
    Ok(AuthResponse {
        token,
        user: user_response,
    })
}

// Обновляет данные пользователя
pub async fn update_user_service(
    user_id: Uuid,
    update_request: UpdateUserRequest,
    pool: &PgPool,
) -> Result<User, AppError> {
    log::info!("Запрос на обновление пользователя с ID: {}", user_id);
    
    // Валидируем данные
    update_request.validate()
        .map_err(|e| {
            log::warn!("Ошибки валидации при обновлении пользователя: {:?}", e);
            AppError::from(e)
        })?;
    
    // Обновляем данные через репозиторий
    let updated_user = update_user_repo(user_id, update_request, pool).await?;
    log::info!("Пользователь с ID {} успешно обновлен", user_id);
    
    Ok(updated_user)
}

// Сменить пароль пользователя
pub async fn change_password_service(
    user_id: Uuid,
    request: &ChangePasswordRequest,
    pool: &PgPool,
) -> Result<(), AppError> {
    // Получаем пользователя из базы данных
    let user = repositories::user::find_user_by_id(user_id, pool).await?;
    
    // Проверяем текущий пароль - клонируем строки
    let is_current_password_valid = verify_password(request.current_password.clone(), user.password_hash.clone()).await?;
    
    if !is_current_password_valid {
        return Err(AppError::Forbidden("Текущий пароль указан неверно".to_string()));
    }
    
    // Хешируем новый пароль - клонируем строку
    let new_password_hash = hash_password(request.new_password.clone()).await?;
    
    // Обновляем пароль в базе данных
    repositories::user::update_user_password(user_id, &new_password_hash, pool).await?;
    
    Ok(())
}