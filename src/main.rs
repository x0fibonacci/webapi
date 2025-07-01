use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::net::SocketAddr;
use tokio::net::TcpListener;

// Декларация модулей
mod controllers;
mod errors;
mod middleware;
mod models;
mod repositories;
mod services;

use crate::controllers::user::{create_user, login, update_user};
use crate::middleware::auth::auth_middleware;

// Запускает сервер и инициализирует маршрутизацию
#[tokio::main]
async fn main() {
    // Инициализируем логирование
    env_logger::init();
    log::info!("Запуск сервера...");

    // Загружаем переменные окружения из .env
    if dotenv::dotenv().is_err() {
        log::warn!("Не удалось загрузить .env файл, используются переменные окружения");
    }

    // Инициализируем пул соединений с PostgreSQL
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL должен быть задан в .env");
    let pool = match PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            log::error!("Не удалось подключиться к PostgreSQL: {}", e);
            std::process::exit(1);
        }
    };

    // Настраиваем адрес сервера
    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = TcpListener::bind(addr).await.unwrap();
    log::info!("Сервер запущен на {}", addr);

    // Запускаем сервер
    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let io = TokioIo::new(stream);
        let pool_clone = pool.clone();

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(
                    io,
                    service_fn(move |req| handle_request(req, pool_clone.clone())),
                )
                .await
            {
                log::error!("Ошибка обслуживания соединения: {:?}", err);
            }
        });
    }
}

// Обрабатывает входящие запросы и маршрутизирует их
async fn handle_request(
    req: Request<Incoming>,
    pool: sqlx::PgPool,
) -> Result<Response<Full<hyper::body::Bytes>>, hyper::Error> {
    let path = req.uri().path();
    let method = req.method();

    match (method, path) {
        // POST /api/users — создание пользователя (без JWT)
        (&Method::POST, "/api/users") => create_user(req, pool).await,
        // POST /api/login — авторизация (с JWT)
        (&Method::POST, "/api/login") => auth_middleware(req, pool, login).await,
        // PATCH /api/users/me — обновление данных пользователя (с JWT)
        (&Method::PATCH, "/api/users/me") => auth_middleware(req, pool, update_user).await,
        // Обработка неподдерживаемых маршрутов
        _ => {
            let mut response = Response::new(Full::new(hyper::body::Bytes::from("Not Found")));
            *response.status_mut() = StatusCode::NOT_FOUND;
            Ok(response)
        }
    }
}
