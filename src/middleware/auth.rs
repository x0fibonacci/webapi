use hyper::{Body, Request, Response};
use jsonwebtoken::{decode, DecodingKey, Validation};
use sqlx::PgPool;
use std::env;

use crate::errors::AppError;
use crate::models::Claims;

// Middleware для проверки JWT-токена
pub async fn auth_middleware<F, Fut>(
    mut req: Request<Body>,
    pool: PgPool,
    handler: F,
) -> Result<Response<Body>, hyper::Error>
where
    F: Fn(Request<Body>, PgPool) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Response<Body>, hyper::Error>> + Send,
{
    // Извлекаем заголовок X-User-Access-Token
    let token = match req.headers().get("X-User-Access-Token") {
        Some(header) => match header.to_str() {
            Ok(token) => token,
            Err(_) => return Ok(AppError::Unauthorized.into_response()),
        },
        None => return Ok(AppError::Unauthorized.into_response()),
    };

    // Загружаем секрет JWT
    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET должен быть задан в .env");

    // Проверяем JWT-токен
    let claims = match decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    ) {
        Ok(token_data) => token_data.claims,
        Err(_) => return Ok(AppError::InvalidToken.into_response()),
    };

    // Проверяем срок действия токена
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    if claims.exp < now {
        return Ok(AppError::InvalidToken.into_response());
    }

    // Извлекаем user_id из claims
    let user_id = match uuid::Uuid::parse_str(&claims.sub) {
        Ok(uuid) => uuid,
        Err(_) => return Ok(AppError::InvalidToken.into_response()),
    };

    // Добавляем user_id в extensions запроса
    req.extensions_mut().insert(user_id);

    // Передаём запрос дальше в обработчик
    handler(req, pool).await
}