use hyper::body::{Body, Bytes};
use hyper::header::{HeaderValue, CACHE_CONTROL, CONTENT_TYPE};
use hyper::{Request, Response, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;
use validator::Validate;

use crate::errors::AppError;
use crate::models::{LoginRequest, UpdateUserRequest, UserRequest, UserResponse, ChangePasswordRequest};
use crate::services::user::{create_user_service, login_service, update_user_service, change_password_service};

// Вспомогательная функция для парсинга JSON-тела запроса
async fn parse_json<T: serde::de::DeserializeOwned + std::fmt::Debug>(
    req: &Request<Body>,
) -> Result<(T, Option<String>), AppError> {
    // Извлекаем request_id из заголовка, если есть
    let request_id = req
        .headers()
        .get("X-Request-ID")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    // Парсим тело запроса
    let body_bytes: Bytes = hyper::body::to_bytes(req.into_body())
        .await
        .map_err(|e| {
            log::error!(
                "Ошибка чтения тела запроса [request_id={}]: {:?}",
                request_id.as_deref().unwrap_or("unknown"),
                e
            );
            AppError::BadRequest("Не удалось прочитать тело запроса".to_string())
        })?;

    // Проверяем, что тело не пустое
    if body_bytes.is_empty() {
        return Err(AppError::BadRequest("Тело запроса не может быть пустым".to_string()));
    }

    // Ограничиваем размер для защиты от DoS
    if body_bytes.len() > 1024 * 1024 {
        // 1 MB лимит
        return Err(AppError::BadRequest("Тело запроса слишком большое".to_string()));
    }

    // Парсим JSON
    let result: T = serde_json::from_slice(&body_bytes).map_err(|e| {
        log::warn!(
            "Ошибка парсинга JSON [request_id={}]: {:?}",
            request_id.as_deref().unwrap_or("unknown"),
            e
        );
        AppError::BadRequest(format!("Некорректный JSON: {}", e))
    })?;

    Ok((result, request_id))
}

// Вспомогательная функция для создания JSON-ответа
fn json_response<T: serde::Serialize>(
    data: &T,
    status: StatusCode,
    request_id: Option<&str>,
) -> Result<Response<Body>, AppError> {
    let json = serde_json::to_string(data).map_err(|e| {
        log::error!(
            "Ошибка сериализации JSON [request_id={}]: {:?}",
            request_id.unwrap_or("unknown"),
            e
        );
        AppError::Internal(anyhow::anyhow!(e))
    })?;

    let mut response = Response::builder()
        .status(status)
        .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
        .header(CACHE_CONTROL, HeaderValue::from_static("no-store"))
        .body(Body::from(json))
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    // Добавляем request_id в заголовок ответа, если он был
    if let Some(id) = request_id {
        if let Ok(value) = HeaderValue::from_str(id) {
            response.headers_mut().insert("X-Request-ID", value);
        }
    }

    Ok(response)
}

// Обработчик для POST /api/users — создание пользователя
pub async fn create_user(req: Request<Body>, pool: PgPool) -> Result<Response<Body>, hyper::Error> {
    // Начало обработки запроса
    let start_time = std::time::Instant::now();
    log::info!("Начало обработки запроса на создание пользователя");

    // Используем вспомогательную функцию для парсинга JSON
    let (user_request, request_id) = match parse_json::<UserRequest>(req).await {
        Ok(result) => result,
        Err(e) => return Ok(e.into_response(None)),
    };

    // Валидируем данные
    if let Err(validation_errors) = user_request.validate() {
        log::warn!(
            "Ошибки валидации при создании пользователя [request_id={}]: {:?}",
            request_id.as_deref().unwrap_or("unknown"),
            validation_errors
        );
        return Ok(AppError::from(validation_errors).into_response(request_id.as_deref()));
    }

    // Вызываем сервис для создания пользователя
    let user = match create_user_service(user_request, &pool).await {
        Ok(user) => {
            log::info!(
                "Пользователь успешно создан [request_id={}] [user_id={}] [email={}]",
                request_id.as_deref().unwrap_or("unknown"),
                user.id,
                user.email
            );
            user
        }
        Err(e) => {
            log::error!(
                "Ошибка при создании пользователя [request_id={}]: {:?}",
                request_id.as_deref().unwrap_or("unknown"),
                e
            );
            return Ok(e.into_response(request_id.as_deref()));
        }
    };

    // Создаем безопасный ответ (без чувствительных данных)
    let user_response = UserResponse::from(&user);

    // Формируем и возвращаем ответ
    let response = json_response(&user_response, StatusCode::CREATED, request_id.as_deref())
        .unwrap_or_else(|e| e.into_response(request_id.as_deref()));

    // Логируем время выполнения
    let elapsed = start_time.elapsed();
    log::debug!(
        "Запрос на создание пользователя обработан за {:?} [request_id={}]",
        elapsed,
        request_id.as_deref().unwrap_or("unknown")
    );

    Ok(response)
}

// Обработчик для POST /api/login — авторизация пользователя
pub async fn login(req: Request<Body>, pool: PgPool) -> Result<Response<Body>, hyper::Error> {
    // Получаем IP адрес (для аудита безопасности)
    let remote_addr = req
        .extensions()
        .get::<std::net::SocketAddr>()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Используем вспомогательную функцию для парсинга JSON
    let (login_request, request_id) = match parse_json::<LoginRequest>(req).await {
        Ok(result) => result,
        Err(e) => {
            log::warn!(
                "Ошибка парсинга запроса авторизации [ip={}]: {:?}",
                remote_addr,
                e
            );
            return Ok(e.into_response(None));
        }
    };

    // Валидируем данные
    if let Err(validation_errors) = login_request.validate() {
        log::warn!(
            "Ошибки валидации при входе [ip={}] [request_id={}]",
            remote_addr,
            request_id.as_deref().unwrap_or("unknown")
        );
        return Ok(AppError::from(validation_errors).into_response(request_id.as_deref()));
    }

    // Логируем попытку авторизации (без пароля)
    log::info!(
        "Попытка авторизации [ip={}] [request_id={}] [email={}]",
        remote_addr,
        request_id.as_deref().unwrap_or("unknown"),
        login_request.email
    );

    // Вызываем сервис для авторизации
    let auth_result = match login_service(login_request.clone(), &pool).await {
        Ok(result) => {
            log::info!(
                "Успешная авторизация [ip={}] [request_id={}] [email={}] [user_id={}]",
                remote_addr,
                request_id.as_deref().unwrap_or("unknown"),
                login_request.email,
                result.user.id
            );
            result
        }
        Err(e) => {
            log::warn!(
                "Неудачная авторизация [ip={}] [request_id={}] [email={}]: {:?}",
                remote_addr,
                request_id.as_deref().unwrap_or("unknown"),
                login_request.email,
                e
            );
            return Ok(e.into_response(request_id.as_deref()));
        }
    };

    // Формируем и возвращаем ответ с токеном и данными пользователя
    let response = json_response(&auth_result, StatusCode::OK, request_id.as_deref())
        .unwrap_or_else(|e| e.into_response(request_id.as_deref()));

    Ok(response)
}

// Обработчик для PATCH /api/users/me — обновление данных пользователя
pub async fn update_user(req: Request<Body>, pool: PgPool) -> Result<Response<Body>, hyper::Error> {
    // Извлекаем user_id из extensions (добавлен middleware)
    let user_id = match req.extensions().get::<Uuid>() {
        Some(id) => *id,
        None => {
            log::error!("user_id отсутствует в middleware, возможный баг в коде");
            return Ok(AppError::Unauthorized.into_response(None));
        }
    };

    // Используем вспомогательную функцию для парсинга JSON
    let (update_request, request_id) = match parse_json::<UpdateUserRequest>(req).await {
        Ok(result) => result,
        Err(e) => return Ok(e.into_response(None)),
    };

    // Проверяем, что хотя бы одно поле задано
    if update_request.name.is_none() && update_request.age.is_none() {
        let error = AppError::BadRequest("Необходимо указать хотя бы одно поле для обновления".to_string());
        return Ok(error.into_response(request_id.as_deref()));
    }

    // Валидируем данные
    if let Err(validation_errors) = update_request.validate() {
        log::warn!(
            "Ошибки валидации при обновлении пользователя [request_id={}] [user_id={}]: {:?}",
            request_id.as_deref().unwrap_or("unknown"),
            user_id,
            validation_errors
        );
        return Ok(AppError::from(validation_errors).into_response(request_id.as_deref()));
    }

    // Логируем запрос на обновление
    log::info!(
        "Запрос на обновление пользователя [request_id={}] [user_id={}]",
        request_id.as_deref().unwrap_or("unknown"),
        user_id
    );

    // Вызываем сервис для обновления пользователя
    let updated_user = match update_user_service(user_id, update_request, &pool).await {
        Ok(user) => {
            log::info!(
                "Пользователь успешно обновлен [request_id={}] [user_id={}]",
                request_id.as_deref().unwrap_or("unknown"),
                user_id
            );
            user
        }
        Err(e) => {
            log::error!(
                "Ошибка при обновлении пользователя [request_id={}] [user_id={}]: {:?}",
                request_id.as_deref().unwrap_or("unknown"),
                user_id,
                e
            );
            return Ok(e.into_response(request_id.as_deref()));
        }
    };

    // Создаем безопасный ответ (без чувствительных данных)
    let user_response = UserResponse::from(&updated_user);

    // Формируем и возвращаем ответ
    let response = json_response(&user_response, StatusCode::OK, request_id.as_deref())
        .unwrap_or_else(|e| e.into_response(request_id.as_deref()));

    Ok(response)
}

// Новый обработчик для POST /api/users/me/change-password — смена пароля пользователя
pub async fn change_password(req: Request<Body>, pool: PgPool) -> Result<Response<Body>, hyper::Error> {
    // Извлекаем user_id из extensions (добавлен middleware)
    let user_id = match req.extensions().get::<Uuid>() {
        Some(id) => *id,
        None => {
            log::error!("user_id отсутствует в middleware, возможный баг в коде");
            return Ok(AppError::Unauthorized.into_response(None));
        }
    };

    // Используем вспомогательную функцию для парсинга JSON
    let (change_pwd_request, request_id) = match parse_json::<ChangePasswordRequest>(req).await {
        Ok(result) => result,
        Err(e) => return Ok(e.into_response(None)),
    };

    // Валидируем данные
    if let Err(validation_errors) = change_pwd_request.validate() {
        log::warn!(
            "Ошибки валидации при смене пароля [request_id={}] [user_id={}]: {:?}",
            request_id.as_deref().unwrap_or("unknown"),
            user_id,
            validation_errors
        );
        return Ok(AppError::from(validation_errors).into_response(request_id.as_deref()));
    }

    // Логируем запрос на смену пароля (без самого пароля)
    log::info!(
        "Запрос на смену пароля [request_id={}] [user_id={}]",
        request_id.as_deref().unwrap_or("unknown"),
        user_id
    );

    // Вызываем сервис для смены пароля
    match change_password_service(user_id, &change_pwd_request, &pool).await {
        Ok(_) => {
            log::info!(
                "Пароль успешно изменен [request_id={}] [user_id={}]",
                request_id.as_deref().unwrap_or("unknown"),
                user_id
            );
            
            // Возвращаем успешный ответ
            let success_response = json!({
                "success": true,
                "message": "Пароль успешно изменен"
            });
            
            let response = json_response(&success_response, StatusCode::OK, request_id.as_deref())
                .unwrap_or_else(|e| e.into_response(request_id.as_deref()));
                
            Ok(response)
        }
        Err(e) => {
            log::error!(
                "Ошибка при смене пароля [request_id={}] [user_id={}]: {:?}",
                request_id.as_deref().unwrap_or("unknown"),
                user_id,
                e
            );
            Ok(e.into_response(request_id.as_deref()))
        }
    }
}