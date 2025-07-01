use hyper::{Body, Client, Method, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tokio::task::JoinHandle;
use webapi::main;

#[tokio::test]
async fn test_api() {
    // Запускаем сервер в фоновом task'е
    let pool = setup_test_db().await;
    let server_handle = tokio::spawn(async {
        main().await;
    });

    // Даём серверу время запуститься
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let client = Client::new();
    let base_url = "http://localhost:8080";

    // Тест 1: Создание пользователя (POST /api/users)
    let user_data = json!({
        "name": "Test User",
        "email": "test@example.com",
        "password": "password123",
        "age": 25
    });
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("{}/api/users", base_url))
        .header("Content-Type", "application/json")
        .body(Body::from(user_data.to_string()))
        .unwrap();
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body_bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["name"], "Test User");
    assert_eq!(body["email"], "test@example.com");
    assert_eq!(body["age"], 25);

    // Тест 2: Авторизация (POST /api/login)
    let login_data = json!({
        "email": "test@example.com",
        "password": "password123"
    });
    let req = Request::builder()
        .method(Method::POST)
        .uri(format!("{}/api/login", base_url))
        .header("Content-Type", "application/json")
        .body(Body::from(login_data.to_string()))
        .unwrap();
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    let token = body["token"].as_str().unwrap().to_string();

    // Тест 3: Обновление пользователя (PATCH /api/users/me)
    let update_data = json!({
        "name": "Updated User",
        "age": 30
    });
    let req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("{}/api/users/me", base_url))
        .header("Content-Type", "application/json")
        .header("X-User-Access-Token", &token)
        .body(Body::from(update_data.to_string()))
        .unwrap();
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body_bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body["name"], "Updated User");
    assert_eq!(body["age"], 30);

    // Тест 4: Неверный токен (PATCH /api/users/me)
    let req = Request::builder()
        .method(Method::PATCH)
        .uri(format!("{}/api/users/me", base_url))
        .header("Content-Type", "application/json")
        .header("X-User-Access-Token", "invalid_token")
        .body(Body::from(update_data.to_string()))
        .unwrap();
    let resp = client.request(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body_bytes = hyper::body::to_bytes(resp.into_body()).await.unwrap();
    assert_eq!(String::from_utf8(body_bytes.to_vec()).unwrap(), "authorization error");

    // Останавливаем сервер
    server_handle.abort();
    cleanup_test_db(&pool).await;
}

// Настройка тестовой базы данных
async fn setup_test_db() -> sqlx::PgPool {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL должен быть задан");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .expect("Не удалось подключиться к тестовой базе данных");

    // Создаём таблицу users
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE,
            password TEXT NOT NULL,
            age INTEGER NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await
    .expect("Не удалось создать таблицу users");

    pool
}

// Очистка тестовой базы данных
async fn cleanup_test_db(pool: &sqlx::PgPool) {
    sqlx::query("DROP TABLE IF EXISTS users")
        .execute(pool)
        .await
        .expect("Не удалось очистить таблицу users");
}