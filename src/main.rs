use hyper::body::Body;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Method, Request, Response, server::Server, StatusCode};
use sqlx::postgres::PgPoolOptions;
use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;

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
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL должен быть задан в .env");
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

    // Создаём сервис Hyper с маршрутизацией
    let make_service = make_service_fn(move |_conn| {
        let pool = pool.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| handle_request(req, pool.clone())))
        }
    });

    // Запускаем сервер
    let server = Server::bind(&addr).serve(make_service);
    log::info!("Сервер запущен на {}", addr);
    if let Err(e) = server.await {
        log::error!("Ошибка сервера: {}", e);
        std::process::exit(1);
    }
}

// Обрабатывает входящие запросы и маршрутизирует их
async fn handle_request(
    req: Request<Body>,
    pool: sqlx::PgPool,
) -> Result<Response<Body>, hyper::Error> {
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
            let mut response = Response::new(Body::from("Not Found"));
            *response.status_mut() = StatusCode::NOT_FOUND;
            Ok(response)
        }
    }
}