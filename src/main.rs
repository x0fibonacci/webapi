use futures_util::FutureExt;
use hyper::body::Body;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Method, Request, Response, StatusCode};
use sqlx::postgres::PgPoolOptions;
use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::ctrl_c;

// Декларация модулей
mod controllers;
mod errors;
mod middleware;
mod models;
mod repositories;
mod services;
mod utils;

use crate::controllers::user::{change_password, create_user, login, update_user};
use crate::middleware::auth::auth_middleware;
use crate::models::AppConfig;

// Структура с настройками и глобальными переменными приложения
struct AppState {
    config: AppConfig,
    db_pool: sqlx::PgPool,
    start_time: std::time::Instant,
    request_count: std::sync::atomic::AtomicUsize,
}

// Запускает сервер и инициализирует маршрутизацию
#[tokio::main]
async fn main() {
    // Инициализируем логирование
    env_logger::init();
    log::info!("Запуск сервера WebAPI v1.0.0...");

    // Загружаем переменные окружения из .env
    if dotenvy::dotenv().is_err() {
        log::warn!("Не удалось загрузить .env файл, используются переменные окружения");
    }

    // Получаем конфигурацию из переменных окружения с дефолтными значениями
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL должен быть задан в .env");
    let db_pool_size = env::var("DB_POOL_SIZE")
        .unwrap_or_else(|_| "10".to_string())
        .parse::<u32>()
        .unwrap_or(10);
    let server_port = env::var("SERVER_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .unwrap_or(8080);
    let server_host = env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let cors_origins = env::var("CORS_ORIGINS").unwrap_or_else(|_| "*".to_string());
    let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET должен быть задан в .env");
    let jwt_expiration = env::var("JWT_EXPIRATION")
        .unwrap_or_else(|_| "86400".to_string()) // 24 часа по умолчанию
        .parse::<u64>()
        .unwrap_or(86400);

    // Собираем конфигурацию
    let config = AppConfig {
        database_url,
        server_host: server_host.clone(),
        server_port,
        jwt_secret,
        jwt_expiration,
        cors_origins,
    };

    // Инициализируем пул соединений с PostgreSQL
    let pool = match PgPoolOptions::new()
        .max_connections(db_pool_size)
        .acquire_timeout(Duration::from_secs(30))
        .connect(&config.database_url)
        .await
    {
        Ok(pool) => {
            log::info!(
                "Успешное подключение к базе данных (пул соединений: {})",
                db_pool_size
            );
            pool
        }
        Err(e) => {
            log::error!("Не удалось подключиться к PostgreSQL: {}", e);
            std::process::exit(1);
        }
    };

    // Проверяем соединение с базой данных
    if let Err(e) = sqlx::query("SELECT 1").execute(&pool).await {
        log::error!("Не удалось выполнить тестовый запрос к базе данных: {}", e);
        std::process::exit(1);
    }

    // Создаем состояние приложения
    let app_state = Arc::new(AppState {
        config,
        db_pool: pool.clone(),
        start_time: std::time::Instant::now(),
        request_count: std::sync::atomic::AtomicUsize::new(0),
    });

    // Настраиваем адрес сервера
    let addr: SocketAddr = format!("{}:{}", server_host, server_port)
        .parse()
        .expect("Неверный формат адреса сервера");

    log::info!("Настройка сервера на адресе: {}", addr);

    // Создаём сервис Hyper с маршрутизацией
    let make_service = make_service_fn(move |_conn| {
        let app_state = Arc::clone(&app_state);
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                // Увеличиваем счетчик запросов
                app_state
                    .request_count
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                // Ограничиваем время выполнения запроса
                let app_state = Arc::clone(&app_state);
                let fut = handle_request(req, app_state);
                tokio::time::timeout(Duration::from_secs(30), fut).map(|result| match result {
                    Ok(response) => response,
                    Err(_) => {
                        log::error!("Запрос выполнялся слишком долго и был отменен");
                        let mut response = Response::new(Body::from(
                            r#"{"error":"Request Timeout","status":408}"#,
                        ));
                        *response.status_mut() = StatusCode::REQUEST_TIMEOUT;
                        response.headers_mut().insert(
                            hyper::header::CONTENT_TYPE,
                            hyper::header::HeaderValue::from_static("application/json"),
                        );
                        Ok(response)
                    }
                })
            }))
        }
    });

    // Создаем экземпляр сервера
    let server = hyper::Server::bind(&addr).serve(make_service);

    // Настраиваем graceful shutdown
    let server_with_shutdown = server.with_graceful_shutdown(shutdown_signal());

    log::info!("Сервер успешно запущен на {}", addr);

    if let Err(e) = server_with_shutdown.await {
        log::error!("Ошибка сервера: {}", e);
        std::process::exit(1);
    }

    log::info!("Сервер успешно завершил работу");
}

// Функция для отслеживания сигнала завершения
async fn shutdown_signal() {
    if let Err(e) = ctrl_c().await {
        log::error!("Ошибка при ожидании сигнала завершения: {}", e);
    }
    log::info!("Получен сигнал завершения, начинаем graceful shutdown");
}

// Обрабатывает входящие запросы и маршрутизирует их
async fn handle_request(
    req: Request<Body>,
    app_state: Arc<AppState>,
) -> Result<Response<Body>, hyper::Error> {
    // Логируем входящий запрос
    log::debug!(
        "Входящий запрос: {} {} от {}",
        req.method(),
        req.uri().path(),
        req.headers()
            .get("user-agent")
            .and_then(|ua| ua.to_str().ok())
            .unwrap_or("Неизвестный клиент")
    );

    // Проверяем размер тела запроса
    let content_length = req
        .headers()
        .get(hyper::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    if content_length > 1024 * 1024 * 10 {
        // Ограничение в 10 MB
        let mut response = Response::new(Body::from(r#"{"error":"Payload Too Large","status":413}"#));
        *response.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
        response.headers_mut().insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("application/json"),
        );
        return Ok(response);
    }

    let path = req.uri().path();
    let method = req.method();

    // Версионирование API
    let api_prefix = "/api/v1";
    let pool = app_state.db_pool.clone();

    // Маршрутизация запросов
    let mut response = match (method, path) {
        // Публичные маршруты (без JWT)
        (&Method::POST, path) if path == format!("{}/users", api_prefix) => {
            create_user(req, pool).await?
        }
        (&Method::POST, path) if path == format!("{}/login", api_prefix) => {
            login(req, pool).await?
        }

        // Защищенные маршруты (требуют JWT)
        (&Method::PATCH, path) if path == format!("{}/users/me", api_prefix) => {
            auth_middleware(req, pool.clone(), update_user).await?
        }
        (&Method::POST, path) if path == format!("{}/users/me/change-password", api_prefix) => {
            auth_middleware(req, pool.clone(), change_password).await?
        }

        // Пути для мониторинга и диагностики
        (&Method::GET, "/health") => {
            let uptime = app_state.start_time.elapsed().as_secs();
            let requests = app_state
                .request_count
                .load(std::sync::atomic::Ordering::SeqCst);
            
            let body = format!(
                r#"{{"status":"OK","version":"1.0.0","uptime":{},"requests":{}}}"#,
                uptime, requests
            );
            
            let mut response = Response::new(Body::from(body));
            response.headers_mut().insert(
                hyper::header::CONTENT_TYPE,
                hyper::header::HeaderValue::from_static("application/json"),
            );
            response
        }
        (&Method::GET, "/metrics") => {
            // Простые метрики для Prometheus
            let uptime = app_state.start_time.elapsed().as_secs();
            let requests = app_state
                .request_count
                .load(std::sync::atomic::Ordering::SeqCst);
            
            let metrics = format!(
                "# HELP api_uptime_seconds Время работы сервера в секундах\n\
                 # TYPE api_uptime_seconds counter\n\
                 api_uptime_seconds {}\n\
                 # HELP api_requests_total Общее число запросов\n\
                 # TYPE api_requests_total counter\n\
                 api_requests_total {}\n",
                uptime, requests
            );
            
            let mut response = Response::new(Body::from(metrics));
            response.headers_mut().insert(
                hyper::header::CONTENT_TYPE,
                hyper::header::HeaderValue::from_static("text/plain"),
            );
            response
        }

        // OPTIONS - для поддержки CORS preflight запросов
        (&Method::OPTIONS, _) => {
            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::NO_CONTENT;
            response
        }

        // Поддержка старого API (без версии) для обратной совместимости
        (&Method::POST, "/api/users") => create_user(req, pool).await?,
        (&Method::POST, "/api/login") => login(req, pool).await?,
        (&Method::PATCH, "/api/users/me") => auth_middleware(req, pool, update_user).await?,

        // Обработка неподдерживаемых маршрутов
        _ => {
            log::warn!("Запрос к несуществующему маршруту: {} {}", method, path);
            let mut response = Response::new(Body::from(r#"{"error":"Not Found","status":404}"#));
            *response.status_mut() = StatusCode::NOT_FOUND;
            response.headers_mut().insert(
                hyper::header::CONTENT_TYPE,
                hyper::header::HeaderValue::from_static("application/json"),
            );
            response
        }
    };

    // Добавляем CORS заголовки
    let headers = response.headers_mut();
    
    // Настраиваем CORS в зависимости от конфигурации
    let cors_value = if app_state.config.cors_origins == "*" {
        "*".to_string()
    } else {
        // Проверяем, что домен запроса есть в списке разрешенных
        let origin = req
            .headers()
            .get(hyper::header::ORIGIN)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        
        if app_state.config.cors_origins.split(',').any(|allowed| allowed.trim() == origin) {
            origin.to_string()
        } else {
            // Если домен не разрешен, используем первый из списка
            app_state.config.cors_origins.split(',').next().unwrap_or("*").to_string()
        }
    };
    
    headers.insert(
        hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN,
        hyper::header::HeaderValue::from_str(&cors_value).unwrap_or_else(|_| {
            hyper::header::HeaderValue::from_static("*")
        }),
    );
    
    headers.insert(
        hyper::header::ACCESS_CONTROL_ALLOW_METHODS,
        hyper::header::HeaderValue::from_static("GET, POST, PATCH, DELETE, OPTIONS"),
    );
    
    headers.insert(
        hyper::header::ACCESS_CONTROL_ALLOW_HEADERS,
        hyper::header::HeaderValue::from_static(
            "Content-Type, Authorization, X-User-Access-Token, Accept, X-Requested-With",
        ),
    );

    // Добавляем заголовок Content-Type, если его еще нет
    if !headers.contains_key(hyper::header::CONTENT_TYPE) {
        headers.insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("application/json"),
        );
    }

    // Логируем исходящий ответ
    log::debug!(
        "Исходящий ответ: статус {} для запроса {} {}",
        response.status(),
        method,
        path
    );

    Ok(response)
}