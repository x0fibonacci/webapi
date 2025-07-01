use http_body_util::Full;
use hyper::{Response, StatusCode};
use thiserror::Error;

// Enum для ошибок приложения
#[derive(Error, Debug)]
pub enum AppError {
    #[error("authorization error")]
    Unauthorized, // Ошибка авторизации (401)

    #[error("authorization error")]
    InvalidToken, // Неверный JWT-токен (401)

    #[error("bad request: {0}")]
    BadRequest(String), // Ошибка запроса (400)

    #[error("internal server error")]
    Internal(#[source] anyhow::Error), // Внутренняя ошибка сервера (500)

    #[error("database error")]
    Database(#[source] sqlx::Error), // Ошибка базы данных (500)
}

// Реализация преобразования ошибок в HTTP-ответы Hyper
impl AppError {
    pub fn into_response(self) -> Response<Full<hyper::body::Bytes>> {
        let (status, body) = match self {
            AppError::Unauthorized | AppError::InvalidToken => {
                (StatusCode::UNAUTHORIZED, "authorization error".to_string())
            }
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Internal(_) | AppError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal server error".to_string(),
            ),
        };

        Response::builder()
            .status(status)
            .body(Full::new(hyper::body::Bytes::from(body)))
            .unwrap_or_else(|_| {
                Response::new(Full::new(hyper::body::Bytes::from("internal server error")))
            })
    }
}

// Преобразование sqlx::Error в AppError
impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err)
    }
}

// Преобразование jsonwebtoken::errors::Error в AppError
impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(_: jsonwebtoken::errors::Error) -> Self {
        AppError::InvalidToken
    }
}
