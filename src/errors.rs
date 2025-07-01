use hyper::body::Body;
use hyper::header::{HeaderValue, CONTENT_TYPE};
use hyper::{Response, StatusCode};
use serde::Serialize;
// Удален неиспользуемый импорт: use std::fmt;
use thiserror::Error;
use uuid::Uuid;

// Enum для ошибок приложения с расширенными типами
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Ошибка аутентификации: требуется авторизация")]
    Unauthorized,
    
    #[error("Ошибка авторизации: недействительный токен")]
    InvalidToken,
    
    #[error("Ошибка авторизации: недостаточно прав или {0}")]
    Forbidden(String), // Изменено: добавлен параметр для передачи сообщения
    
    #[error("Ресурс не найден: {0}")]
    NotFound(String),
    
    #[error("Ошибка запроса: {0}")]
    BadRequest(String),
    
    #[error("Ошибка валидации: {0}")]
    ValidationError(String),
    
    #[error("Конфликт данных: {0}")]
    Conflict(String),
    
    #[error("Превышен лимит запросов")]
    RateLimited,
    
    #[error("Внутренняя ошибка сервера")]
    Internal(#[source] anyhow::Error),
    
    #[error("Ошибка базы данных")]
    Database(#[source] sqlx::Error),
    
    #[error("Сервис временно недоступен")]
    ServiceUnavailable,
}

// Структура для сериализации ошибок в JSON
#[derive(Serialize)]
struct ErrorResponse {
    status: u16,
    error: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    field_errors: Option<Vec<FieldError>>,
    timestamp: String,
}

// Структура для сериализации ошибок валидации полей
#[derive(Serialize)]
struct FieldError {
    field: String,
    message: String,
}

// Расширенная реализация преобразования ошибок в HTTP-ответы
impl AppError {
    pub fn into_response(self, request_id: Option<&str>) -> Response<Body> {
        // Создаем уникальный ID для трассировки, если не предоставлен
        let trace_id = request_id
            .map(String::from)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
            
        // Получаем текущую дату и время в формате ISO
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        
        // Определяем статус и сообщение на основе типа ошибки
        let (status, error_type, message, details) = match &self {
            AppError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "Unauthorized", "Требуется авторизация", None)
            }
            AppError::InvalidToken => {
                (StatusCode::UNAUTHORIZED, "InvalidToken", "Недействительный токен авторизации", None)
            }
            AppError::Forbidden(msg) => {
                // Исправлено: используем переданное сообщение или дефолтное
                let message = if msg.is_empty() {
                    "Доступ запрещен"
                } else {
                    msg
                };
                (StatusCode::FORBIDDEN, "Forbidden", message, None)
            }
            AppError::NotFound(resource) => {
                // Формируем сообщение
                let message = format!("Ресурс не найден: {}", resource);
                (StatusCode::NOT_FOUND, "NotFound", message.as_str(), None)
            }
            AppError::BadRequest(msg) => {
               // Конвертируем String в &str для согласованности с другими вариантами
              (StatusCode::BAD_REQUEST, "BadRequest", msg.as_str(), None)
            }
            AppError::ValidationError(msg) => {
                (StatusCode::BAD_REQUEST, "ValidationError", "Ошибка валидации данных", Some(msg.clone()))
            }
            AppError::Conflict(msg) => {
                (StatusCode::CONFLICT, "Conflict", msg.as_str(), None)
            }
            AppError::RateLimited => {
                (StatusCode::TOO_MANY_REQUESTS, "RateLimited", "Превышен лимит запросов", None)
            }
            AppError::Internal(err) => {
                // Логируем внутренние ошибки
                log::error!("Внутренняя ошибка [{}]: {:?}", trace_id, err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalServerError",
                    "Внутренняя ошибка сервера",
                    None,
                )
            }
            AppError::Database(err) => {
                // Логируем ошибки базы данных
                log::error!("Ошибка БД [{}]: {:?}", trace_id, err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DatabaseError",
                    "Ошибка при работе с базой данных",
                    None,
                )
            }
            AppError::ServiceUnavailable => {
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "ServiceUnavailable",
                    "Сервис временно недоступен",
                    None,
                )
            }
        };
        
        // Создаем структуру ответа
        let error_response = ErrorResponse {
            status: status.as_u16(),
            error: error_type.to_string(),
            message: message.to_string(),
            details: details.clone(),
            trace_id,
            field_errors: None, // Здесь можно добавить ошибки полей при необходимости
            timestamp: now,
        };
        
        // Сериализуем в JSON
        let body = match serde_json::to_string(&error_response) {
            Ok(json) => Body::from(json),
            Err(e) => {
                log::error!("Ошибка сериализации JSON: {}", e);
                Body::from(r#"{"status":500,"error":"InternalServerError","message":"Ошибка сериализации ответа"}"#)
            }
        };
        
        // Создаем ответ с заголовками
        let mut response = Response::builder()
            .status(status)
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))
            .body(body)
            .unwrap_or_else(|_| Response::new(Body::from(r#"{"error":"InternalServerError"}"#)));
        
        // Добавляем трассировочный ID в заголовок
        if let Ok(value) = HeaderValue::from_str(&error_response.trace_id) {
            response.headers_mut().insert("X-Trace-ID", value);
        }
        
        response
    }
    
    // Вспомогательный метод для создания ошибки валидации с несколькими полями
    pub fn validation_errors(errors: Vec<(String, String)>) -> Self {
        // Создаем строку с описанием всех ошибок
        let message = errors
            .iter()
            .map(|(field, msg)| format!("{}: {}", field, msg))
            .collect::<Vec<_>>()
            .join("; ");
        
        AppError::ValidationError(message)
    }
}

// Конвертация различных типов ошибок в AppError

// Из sqlx::Error в AppError
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => AppError::NotFound("Запись не найдена".to_string()),
            sqlx::Error::Database(dberr) if dberr.constraint().is_some() => {
                let constraint = dberr.constraint().unwrap_or("unknown");
                if constraint.contains("email") {
                    AppError::Conflict("Пользователь с таким email уже существует".to_string())
                } else {
                    AppError::Database(sqlx::Error::Database(dberr))
                }
            },
            _ => AppError::Database(err),
        }
    }
}

// Из jsonwebtoken::errors::Error в AppError
impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        use jsonwebtoken::errors::ErrorKind;
        match err.kind() {
            ErrorKind::ExpiredSignature => AppError::InvalidToken,
            ErrorKind::InvalidToken => AppError::InvalidToken,
            _ => AppError::Unauthorized,
        }
    }
}

// Из std::io::Error в AppError
impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Internal(anyhow::anyhow!("Ошибка ввода-вывода: {}", err))
    }
}

// Из serde_json::Error в AppError
impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::BadRequest(format!("Некорректный JSON: {}", err))
    }
}

// Из anyhow::Error в AppError
impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err)
    }
}

// Из validator::ValidationErrors в AppError
impl From<validator::ValidationErrors> for AppError {
    fn from(err: validator::ValidationErrors) -> Self {
        let mut field_errors = Vec::new();
        
        for (field, errors) in err.field_errors() {
            if let Some(error) = errors.first() {
                if let Some(message) = &error.message {
                    field_errors.push((field.to_string(), message.to_string()));
                } else {
                    field_errors.push((field.to_string(), "Ошибка валидации".to_string()));
                }
            }
        }
        
        AppError::validation_errors(field_errors)
    }
}