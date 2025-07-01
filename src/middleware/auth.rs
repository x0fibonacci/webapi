use hyper::{Body, Request, Response, header};
use jsonwebtoken::{DecodingKey, Validation, decode, Algorithm};
use sqlx::PgPool;
use std::env;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::{Claims, UserRole};

// Тип для request_id в extensions
type RequestIdKey = &'static str;

// Используем OnceLock для загрузки JWT ключа только один раз
static DECODING_KEY: OnceLock<DecodingKey> = OnceLock::new();
static JWT_VALIDATION: OnceLock<Validation> = OnceLock::new();

// Функция для получения ключа JWT, инициализируется при первом вызове
fn get_jwt_key() -> &'static DecodingKey {
    DECODING_KEY.get_or_init(|| {
        let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET должен быть задан в .env");
        DecodingKey::from_secret(jwt_secret.as_bytes())
    })
}

// Функция для получения настроек валидации JWT, инициализируется при первом вызове
fn get_jwt_validation() -> &'static Validation {
    JWT_VALIDATION.get_or_init(|| {
        let mut validation = Validation::new(Algorithm::HS256);
        
        // Добавляем валидацию issuer, если задан
        if let Ok(issuer) = env::var("JWT_ISSUER") {
            validation.set_issuer(&[&issuer]);
        }
        
        // Добавляем валидацию audience, если задан
        if let Ok(audience) = env::var("JWT_AUDIENCE") {
            validation.set_audience(&[&audience]);
        }
        
        // Устанавливаем leeway (буфер времени) для учета разницы часов между серверами
        validation.leeway = 60; // 60 секунд
        
        validation
    })
}

// Извлекает токен из различных мест в запросе
fn extract_token(req: &Request<Body>) -> Option<String> {
    // 1. Пытаемся получить из заголовка Authorization
    if let Some(auth_header) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                return Some(auth_str[7..].to_string());
            }
        }
    }
    
    // 2. Пытаемся получить из кастомного заголовка (для обратной совместимости)
    if let Some(token_header) = req.headers().get("X-User-Access-Token") {
        if let Ok(token) = token_header.to_str() {
            return Some(token.to_string());
        }
    }
    
    // 3. Пытаемся получить из cookie (если используется)
    if let Some(cookie_header) = req.headers().get(header::COOKIE) {
        if let Ok(cookie_str) = cookie_header.to_str() {
            for cookie in cookie_str.split(';') {
                if let Some(token_part) = cookie.trim().strip_prefix("auth_token=") {
                    return Some(token_part.to_string());
                }
            }
        }
    }
    
    None
}

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
    // Получаем IP адрес для логирования
    let remote_addr = req
        .extensions()
        .get::<std::net::SocketAddr>()
        .map(|addr| addr.to_string())
        .unwrap_or_else(|| "unknown".to_string());
        
    // Получаем ID запроса для трассировки, если есть
    let request_id = req
        .headers()
        .get("X-Request-ID")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    
    // Извлекаем токен из запроса
    let token = match extract_token(&req) {
        Some(token) => token,
        None => {
            log::warn!(
                "Отсутствует токен аутентификации [ip={}] [request_id={}]",
                remote_addr,
                request_id.as_deref().unwrap_or("unknown")
            );
            return Ok(AppError::Unauthorized.into_response(request_id.as_deref()));
        }
    };

    // Проверяем JWT-токен
    let token_data = match decode::<Claims>(
        &token,
        get_jwt_key(),
        get_jwt_validation(),
    ) {
        Ok(token_data) => token_data,
        Err(err) => {
            // Логируем различные ошибки валидации токена
            use jsonwebtoken::errors::ErrorKind;
            match err.kind() {
                ErrorKind::ExpiredSignature => {
                    log::info!(
                        "Истекший токен [ip={}] [request_id={}]",
                        remote_addr,
                        request_id.as_deref().unwrap_or("unknown")
                    );
                    return Ok(AppError::InvalidToken.into_response(request_id.as_deref()));
                }
                ErrorKind::InvalidSignature => {
                    log::warn!(
                        "Недействительная подпись токена [ip={}] [request_id={}]",
                        remote_addr,
                        request_id.as_deref().unwrap_or("unknown")
                    );
                    return Ok(AppError::InvalidToken.into_response(request_id.as_deref()));
                }
                _ => {
                    log::warn!(
                        "Ошибка валидации токена: {:?} [ip={}] [request_id={}]",
                        err,
                        remote_addr,
                        request_id.as_deref().unwrap_or("unknown")
                    );
                    return Ok(AppError::InvalidToken.into_response(request_id.as_deref()));
                }
            }
        }
    };

    let claims = token_data.claims;

    // Проверяем срок действия токена (дополнительная проверка, хотя JWT валидация уже делает это)
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as i64;
    
    if claims.exp < now {
        log::info!(
            "Токен истек [ip={}] [request_id={}] [user_email={}]",
            remote_addr,
            request_id.as_deref().unwrap_or("unknown"),
            claims.email
        );
        return Ok(AppError::InvalidToken.into_response(request_id.as_deref()));
    }

    // Извлекаем user_id из claims
    let user_id = match Uuid::parse_str(&claims.sub) {
        Ok(uuid) => uuid,
        Err(_) => {
            log::warn!(
                "Неверный формат UUID в токене [ip={}] [request_id={}]",
                remote_addr,
                request_id.as_deref().unwrap_or("unknown")
            );
            return Ok(AppError::InvalidToken.into_response(request_id.as_deref()));
        }
    };

    // Добавляем информацию в extensions запроса для использования в обработчиках
    req.extensions_mut().insert(user_id);
    req.extensions_mut().insert(claims.role);
    
    // Сохраняем request_id с явным типом
    if let Some(id) = request_id {
        req.extensions_mut().insert(("request_id", id));
        
        // Добавляем еще и в заголовки для корреляции
        req.headers_mut().insert(
            "X-User-ID", 
            header::HeaderValue::from_str(&user_id.to_string())
                .unwrap_or_else(|_| header::HeaderValue::from_static("invalid"))
        );
    }

    // Логируем успешную аутентификацию
    log::debug!(
        "Успешная аутентификация [ip={}] [user_id={}] [email={}] [role={:?}]",
        remote_addr,
        user_id,
        claims.email,
        claims.role
    );

    // Передаём запрос дальше в обработчик
    handler(req, pool).await
}

// Middleware для проверки роли пользователя (используется после auth_middleware)
pub async fn role_middleware<F, Fut>(
    req: Request<Body>,
    pool: PgPool,
    required_role: UserRole,
    handler: F,
) -> Result<Response<Body>, hyper::Error>
where
    F: Fn(Request<Body>, PgPool) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Response<Body>, hyper::Error>> + Send,
{
    // Получаем роль пользователя из extensions (добавлена auth_middleware)
    let user_role = match req.extensions().get::<UserRole>() {
        Some(role) => *role,
        None => {
            // Это не должно произойти, если auth_middleware был вызван перед этим middleware
            log::error!("Роль пользователя отсутствует в extensions, возможный баг в коде");
            return Ok(AppError::Unauthorized.into_response(None));
        }
    };
    
    // Проверяем достаточность прав (администратор имеет все права)
    if user_role != required_role && user_role != UserRole::Admin {
        let request_id = req
            .headers()
            .get("X-Request-ID")
            .and_then(|v| v.to_str().ok());
            
        let user_id = req
            .extensions()
            .get::<Uuid>()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "unknown".to_string());
            
        log::warn!(
            "Доступ запрещен: недостаточно прав [request_id={}] [user_id={}] [роль={:?}, требуется={:?}]",
            request_id.unwrap_or("unknown"),
            user_id,
            user_role,
            required_role
        );
        
        return Ok(AppError::Forbidden(format!(
            "Недостаточно прав для этой операции. Требуется роль: {:?}", 
            required_role
        )).into_response(request_id));
    }

    // Передаём запрос дальше в обработчик
    handler(req, pool).await
}