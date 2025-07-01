use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use webapi::errors::AppError;
use webapi::models::{LoginRequest, UpdateUserRequest, UserRequest};
use webapi::services::user::{create_user_service, login_service, update_user_service};

#[tokio::test]
async fn test_user_service() {
    let pool = setup_test_db().await;

    // Тест 1: Создание пользователя
    let user_request = UserRequest {
        name: "Test User".to_string(),
        email: "test@example.com".to_string(),
        password: "password123".to_string(),
        age: 25,
    };
    let user = create_user_service(user_request, &pool).await.unwrap();
    assert_eq!(user.name, "Test User");
    assert_eq!(user.email, "test@example.com");
    assert_eq!(user.age, 25);

    // Тест 2: Провал создания пользователя (дублирующий email)
    let duplicate_request = UserRequest {
        name: "Another User".to_string(),
        email: "test@example.com".to_string(),
        password: "password456".to_string(),
        age: 30,
    };
    let result = create_user_service(duplicate_request, &pool).await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));

    // Тест 3: Успешная авторизация
    let login_request = LoginRequest {
        email: "test@example.com".to_string(),
        password: "password123".to_string(),
    };
    let token = login_service(login_request, &pool).await.unwrap();
    assert!(!token.is_empty());

    // Тест 4: Провал авторизации (неверный пароль)
    let wrong_login = LoginRequest {
        email: "test@example.com".to_string(),
        password: "wrong_password".to_string(),
    };
    let result = login_service(wrong_login, &pool).await;
    assert!(matches!(result, Err(AppError::Unauthorized)));

    // Тест 5: Обновление пользователя
    let update_request = UpdateUserRequest {
        name: Some("Updated User".to_string()),
        age: Some(30),
    };
    let updated_user = update_user_service(user.id, update_request, &pool)
        .await
        .unwrap();
    assert_eq!(updated_user.name, "Updated User");
    assert_eq!(updated_user.age, 30);

    // Тест 6: Провал обновления (неверный user_id)
    let wrong_id = uuid::Uuid::new_v4();
    let update_request = UpdateUserRequest {
        name: Some("Wrong User".to_string()),
        age: None,
    };
    let result = update_user_service(wrong_id, update_request, &pool).await;
    assert!(matches!(result, Err(AppError::Unauthorized)));

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
