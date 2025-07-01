use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::task;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::{LoginRequest, UpdateUserRequest, User, UserRequest};
use crate::repositories::user::{create_user as create_user_repo, find_user_by_email, update_user as update_user_repo};
use jsonwebtoken::{encode, EncodingKey, Header};
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

// Создаёт нового пользователя с хешированным паролем
pub async fn create_user_service(user_request: UserRequest, pool: &PgPool) -> Result<User, AppError> {
    // Проверяем, что пользователь с таким email не существует
    if find_user_by_email(&user_request.email, pool).await.is_ok() {
        return Err(AppError::BadRequest("Пользователь с таким email уже существует".to_string()));
    }

    // Хешируем пароль асинхронно
    let password = user_request.password.clone();
    let hashed_password = task::spawn_blocking(move || {
        let mut hasher = Sha256::new();
        hasher.update(password);
        hex::encode(hasher.finalize())
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    // Создаём пользователя через репозиторий
    let user = User {
        id: Uuid::new_v4(),
        name: user_request.name,
        email: user_request.email,
        password: hashed_password,
        age: user_request.age,
    };
    create_user_repo(&user, pool).await
}

// Аутентифицирует пользователя и возвращает JWT-токен
pub async fn login_service(login_request: LoginRequest, pool: &PgPool) -> Result<String, AppError> {
    // Находим пользователя по email
    let user = find_user_by_email(&login_request.email, pool)
        .await
        .map_err(|_| AppError::Unauthorized)?;

    // Проверяем пароль асинхронно
    let input_password = login_request.password.clone();
    let stored_password = user.password.clone();
    let is_valid = task::spawn_blocking(move || {
        let mut hasher = Sha256::new();
        hasher.update(input_password);
        hex::encode(hasher.finalize()) == stored_password
    })
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    if !is_valid {
        return Err(AppError::Unauthorized);
    }

    // Генерируем JWT-токен
    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET должен быть задан в .env");
    let claims = crate::models::Claims {
        sub: user.id.to_string(),
        exp: (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 3600) as i64,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )?;
    Ok(token)
}

// Обновляет данные пользователя
pub async fn update_user_service(
    user_id: Uuid,
    update_request: UpdateUserRequest,
    pool: &PgPool,
) -> Result<User, AppError> {
    // Обновляем данные через репозиторий
    update_user_repo(user_id, update_request, pool).await
}