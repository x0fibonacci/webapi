use hyper::{Body, Client, Method, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::env;
use std::sync::Once;
use tokio::time::Duration;
use uuid::Uuid;

// Инициализируем логгер один раз
static INIT: Once = Once::new();

// Путь к тестовой базе данных
static TEST_DB_URL: &str = "postgresql://postgres:postgres@localhost/webapi_test";

// Настройка перед всеми тестами
async fn setup() -> PgPool {
    // Инициализируем логгер
    INIT.call_once(|| {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .is_test(true)
            .init();
    });

    // Устанавливаем переменные окружения для тестов
    env::set_var("DATABASE_URL", TEST_DB_URL);
    env::set_var("JWT_SECRET", "test_secret_key_for_jwt_token_generation");
    env::set_var("SERVER_PORT", "8081"); // Используем другой порт для тестов
    env::set_var("SERVER_HOST", "127.0.0.1");
    env::set_var("DB_POOL_SIZE", "2");

    // Подключаемся к тестовой базе данных
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(TEST_DB_URL)
        .await
        .expect("Не удалось подключиться к тестовой базе данных");

    // Очищаем базу данных перед тестами
    sqlx::query("DROP TABLE IF EXISTS users")
        .execute(&pool)
        .await
        .expect("Не удалось очистить таблицу users");

    // Создаём обновленную таблицу users
    sqlx::query(
        r#"
        CREATE TABLE users (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            age SMALLINT NOT NULL CHECK (age > 0),
            role TEXT NOT NULL DEFAULT 'user',
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            is_active BOOLEAN NOT NULL DEFAULT TRUE
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("Не удалось создать таблицу users");

    pool
}

// Запуск тестового сервера
async fn start_test_server() -> u16 {
    // Используем случайный порт
    let port = 8081;
    
    // Запускаем сервер в фоновом процессе
    tokio::spawn(async move {
        let mut args = std::env::args().collect::<Vec<String>>();
        args.push("--test".to_string());
        webapi::main().await;
    });

    // Даём серверу время запуститься
    tokio::time::sleep(Duration::from_millis(300)).await;
    
    port
}

#[tokio::test]
async fn test_api_flow() {
    // Подготовка тестового окружения
    let pool = setup().await;
    let port = start_test_server().await;
    
    let client = Client::new();
    let base_url = format!("http://127.0.0.1:{}", port);
    
    // Тест 1: Проверка работоспособности сервера
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("{}/health", base_url))
        .body(Body::empty())
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    
    // Тест 2: Создание пользователя с корректными данными
    let user_data = json!({
        "name": "Тестовый Пользователь",
        "email": "test@example.com",
        "password": "Password123!",
        "age": 25
    });
    
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("{}/api/v1/users", base_url))
        .header("Content-Type", "application/json")
        .body(Body::from(user_data.to_string()))
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    
    let body_bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    
    // Проверяем структуру ответа
    assert_eq!(body["name"], "Тестовый Пользователь");
    assert_eq!(body["email"], "test@example.com");
    assert_eq!(body["age"], 25);
    assert!(body["id"].is_string());
    
    // Тест 3: Попытка создания пользователя с тем же email (должен быть конфликт)
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("{}/api/v1/users", base_url))
        .header("Content-Type", "application/json")
        .body(Body::from(user_data.to_string()))
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    
    // Тест 4: Попытка создания пользователя с неверными данными
    let invalid_user_data = json!({
        "name": "T", // Слишком короткое имя
        "email": "not-an-email",
        "password": "short",
        "age": -5
    });
    
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("{}/api/v1/users", base_url))
        .header("Content-Type", "application/json")
        .body(Body::from(invalid_user_data.to_string()))
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    
    // Тест 5: Авторизация с правильными данными
    let login_data = json!({
        "email": "test@example.com",
        "password": "Password123!"
    });
    
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("{}/api/v1/login", base_url))
        .header("Content-Type", "application/json")
        .body(Body::from(login_data.to_string()))
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    
    // Проверяем заголовки ответа
    assert_eq!(
        resp.headers().get("Content-Type").unwrap(),
        "application/json"
    );
    assert_eq!(
        resp.headers().get("Cache-Control").unwrap(),
        "no-store"
    );
    
    let body_bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    
    // Проверяем структуру ответа авторизации
    assert!(body["token"].is_string());
    assert!(body["user"].is_object());
    assert_eq!(body["user"]["email"], "test@example.com");
    assert_eq!(body["user"]["name"], "Тестовый Пользователь");
    
    let token = body["token"].as_str().unwrap().to_string();
    
    // Тест 6: Авторизация с неверными данными
    let invalid_login_data = json!({
        "email": "test@example.com",
        "password": "wrong_password"
    });
    
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("{}/api/v1/login", base_url))
        .header("Content-Type", "application/json")
        .body(Body::from(invalid_login_data.to_string()))
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    
    // Тест 7: Обновление пользователя (стандартный заголовок Bearer)
    let update_data = json!({
        "name": "Обновленное Имя",
        "age": 30
    });
    
    let req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("{}/api/v1/users/me", base_url))
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::from(update_data.to_string()))
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    
    let body_bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["name"], "Обновленное Имя");
    assert_eq!(body["age"], 30);
    
    // Тест 8: Обновление пользователя (устаревший заголовок X-User-Access-Token)
    let update_data = json!({
        "name": "Другое Имя",
        "age": 31
    });
    
    let req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("{}/api/v1/users/me", base_url))
        .header("Content-Type", "application/json")
        .header("X-User-Access-Token", &token)
        .body(Body::from(update_data.to_string()))
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    
    // Тест 9: Обновление с неверным токеном
    let req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("{}/api/v1/users/me", base_url))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer invalid_token")
        .body(Body::from(update_data.to_string()))
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    
    // Тест 10: Запрос к несуществующему маршруту
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("{}/api/v1/nonexistent", base_url))
        .body(Body::empty())
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    
    // Очистка
    sqlx::query("DROP TABLE IF EXISTS users")
        .execute(&pool)
        .await
        .expect("Не удалось очистить таблицу users");
}

#[tokio::test]
async fn test_cors_support() {
    // Подготовка тестового окружения
    let pool = setup().await;
    let port = start_test_server().await;
    
    let client = Client::new();
    let base_url = format!("http://127.0.0.1:{}", port);
    
    // Тест для CORS preflight запроса
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri(format!("{}/api/v1/users", base_url))
        .header("Origin", "http://example.com")
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "Content-Type")
        .body(Body::empty())
        .unwrap();
        
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    
    // Проверяем CORS заголовки
    assert!(resp.headers().contains_key("Access-Control-Allow-Origin"));
    assert!(resp.headers().contains_key("Access-Control-Allow-Methods"));
    assert!(resp.headers().contains_key("Access-Control-Allow-Headers"));
    
    // Очистка
    sqlx::query("DROP TABLE IF EXISTS users")
        .execute(&pool)
        .await
        .expect("Не удалось очистить таблицу users");
}