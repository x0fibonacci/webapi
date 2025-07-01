use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Full};
use serde_json::json;
use sqlx::PgPool;

use crate::errors::AppError;
use crate::models::{LoginRequest, UpdateUserRequest, UserRequest};
use crate::services::user::{create_user_service, login_service, update_user_service};

// Обработчик для POST /api/users — создание пользователя
pub async fn create_user(req: Request<Incoming>, pool: PgPool) -> Result<Response<Full<hyper::body::Bytes>>, hyper::Error> {
    // Парсим тело запроса в UserRequest
    let body_bytes = req.collect().await?.to_bytes();
    let user_request: UserRequest = match serde_json::from_slice(&body_bytes) {
        Ok(user) => user,
        Err(e) => {
            return Ok(AppError::BadRequest(format!("Некорректный JSON: {}", e)).into_response());
        }
    };

    // Вызываем сервис для создания пользователя
    let user = match create_user_service(user_request, &pool).await {
        Ok(user) => user,
        Err(e) => return Ok(e.into_response()),
    };

    // Формируем ответ с созданным пользователем
    let response_body = match serde_json::to_string(&user) {
        Ok(body) => body,
        Err(e) => return Ok(AppError::Internal(anyhow::anyhow!(e)).into_response()),
    };
    Ok(Response::builder()
        .status(StatusCode::CREATED)
        .body(Full::new(hyper::body::Bytes::from(response_body)))
        .unwrap_or_else(|_| AppError::Internal(anyhow::anyhow!("Ошибка ответа")).into_response()))
}

// Обработчик для POST /api/login — авторизация пользователя
pub async fn login(req: Request<Incoming>, pool: PgPool) -> Result<Response<Full<hyper::body::Bytes>>, hyper::Error> {
    // Парсим тело запроса в LoginRequest
    let body_bytes = req.collect().await?.to_bytes();
    let login_request: LoginRequest = match serde_json::from_slice(&body_bytes) {
        Ok(login) => login,
        Err(e) => {
            return Ok(AppError::BadRequest(format!("Некорректный JSON: {}", e)).into_response());
        }
    };

    // Вызываем сервис для авторизации
    let token = match login_service(login_request, &pool).await {
        Ok(token) => token,
        Err(e) => return Ok(e.into_response()),
    };

    // Формируем ответ с JWT-токеном
    let response_body = json!({ "token": token }).to_string();
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Full::new(hyper::body::Bytes::from(response_body)))
        .unwrap_or_else(|_| AppError::Internal(anyhow::anyhow!("Ошибка ответа")).into_response()))
}

// Обработчик для PATCH /api/users/me — обновление данных пользователя
pub async fn update_user(req: Request<Incoming>, pool: PgPool) -> Result<Response<Full<hyper::body::Bytes>>, hyper::Error> {
    // Извлекаем user_id из extensions (добавлен middleware)
    let user_id = match req.extensions().get::<uuid::Uuid>() {
        Some(id) => *id,
        None => return Ok(AppError::Unauthorized.into_response()),
    };

    // Парсим тело запроса в UpdateUserRequest
    let body_bytes = req.collect().await?.to_bytes();
    let update_request: UpdateUserRequest = match serde_json::from_slice(&body_bytes) {
        Ok(update) => update,
        Err(e) => {
            return Ok(AppError::BadRequest(format!("Некорректный JSON: {}", e)).into_response());
        }
    };

    // Вызываем сервис для обновления пользователя
    let updated_user = match update_user_service(user_id, update_request, &pool).await {
        Ok(user) => user,
        Err(e) => return Ok(e.into_response()),
    };

    // Формируем ответ с обновлённым пользователем
    let response_body = match serde_json::to_string(&updated_user) {
        Ok(body) => body,
        Err(e) => return Ok(AppError::Internal(anyhow::anyhow!(e)).into_response()),
    };
    Ok(Response::builder()
        .status(StatusCode::OK)
        .body(Full::new(hyper::body::Bytes::from(response_body)))
        .unwrap_or_else(|_| AppError::Internal(anyhow::anyhow!("Ошибка ответа")).into_response()))
}