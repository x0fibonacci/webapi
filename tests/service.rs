use chrono::Utc;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
use std::env;
use std::sync::Once;

use webapi::errors::AppError;
use webapi::models::{LoginRequest, UpdateUserRequest, UserRequest, UserRole};
use webapi::services::user::{create_user_service, login_service, update_user_service, change_password_service};

// Инициализируем логгер один раз
static INIT: Once = Once::new();

// Путь к тестовой базе данных
static TEST_DB_URL: &str = "postgresql://postgres:postgres@localhost/webapi_test";

#[tokio::test]
async fn test_user_service() {
    // Инициализируем настройки теста
    setup_test_env();
    let pool = setup_test_db().await;

    // Тест 1: Создание пользователя с корректными данными
    let user_request = UserRequest {
        name: "Тестовый Пользователь".to_string(),
        email: "test@example.com".to_string(),
        password: "Password123!".to_string(), // Соответствует валидации
        age: 25,
    };
    
    let user = create_user_service(user_request, &pool).await.unwrap();
    assert_eq!(user.name, "Тестовый Пользователь");
    assert_eq!(user.email, "test@example.com");
    assert_eq!(user.age, 25);
    assert_eq!(user.role, UserRole::User); // Проверяем роль по умолчанию
    assert!(user.is_active); // Проверяем, что пользователь активен по умолчанию
    assert!(user.created_at <= Utc::now()); // Проверяем, что дата создания установлена

    // Тест 2: Провал создания пользователя с неверной валидацией
    let invalid_request = UserRequest {
        name: "A".to_string(), // Слишком короткое имя
        email: "not-an-email".to_string(), // Неверный формат email
        password: "123".to_string(), // Слишком короткий пароль
        age: 8, // Слишком малый возраст
    };
    
    let result = create_user_service(invalid_request, &pool).await;
    assert!(matches!(result, Err(AppError::ValidationError(_))));

    // Тест 3: Провал создания пользователя с дублирующимся email
    let duplicate_request = UserRequest {
        name: "Другой Пользователь".to_string(),
        email: "test@example.com".to_string(), // Этот email уже существует
        password: "Password456!".to_string(),
        age: 30,
    };
    
    let result = create_user_service(duplicate_request, &pool).await;
    assert!(matches!(result, Err(AppError::Conflict(_)))); // Теперь Conflict вместо BadRequest

    // Тест 4: Успешная авторизация
    let login_request = LoginRequest {
        email: "test@example.com".to_string(),
        password: "Password123!".to_string(),
    };
    
    let auth_response = login_service(login_request, &pool).await.unwrap();
    assert!(!auth_response.token.is_empty());
    assert_eq!(auth_response.user.email, "test@example.com");
    assert_eq!(auth_response.user.name, "Тестовый Пользователь");

    // Тест 5: Провал авторизации (неверный пароль)
    let wrong_login = LoginRequest {
        email: "test@example.com".to_string(),
        password: "wrong_password".to_string(),
    };
    
    let result = login_service(wrong_login, &pool).await;
    assert!(matches!(result, Err(AppError::Unauthorized)));

    // Тест 6: Провал авторизации (несуществующий email)
    let nonexistent_login = LoginRequest {
        email: "nonexistent@example.com".to_string(),
        password: "Password123!".to_string(),
    };
    
    let result = login_service(nonexistent_login, &pool).await;
    assert!(matches!(result, Err(AppError::Unauthorized))); // Замаскированная ошибка NotFound

    // Тест 7: Обновление пользователя
    let update_request = UpdateUserRequest {
        name: Some("Обновленное Имя".to_string()),
        age: Some(30),
    };
    
    let updated_user = update_user_service(user.id, update_request, &pool).await.unwrap();
    assert_eq!(updated_user.name, "Обновленное Имя");
    assert_eq!(updated_user.age, 30);
    assert!(updated_user.updated_at > user.updated_at); // Проверяем обновление timestamp

    // Тест 8: Провал обновления (неверный user_id)
    let wrong_id = Uuid::new_v4();
    let update_request = UpdateUserRequest {
        name: Some("Wrong User".to_string()),
        age: None,
    };
    
    let result = update_user_service(wrong_id, update_request, &pool).await;
    assert!(matches!(result, Err(AppError::NotFound(_)))); // Теперь NotFound вместо Unauthorized

    // Тест 9: Успешная смена пароля
    let result = change_password_service(
        user.id,
        "Password123!".to_string(),  // Текущий пароль
        "NewPassword456!".to_string(), // Новый пароль
        &pool
    ).await;
    assert!(result.is_ok());

    // Тест 10: Неуспешная смена пароля (неверный текущий пароль)
    let result = change_password_service(
        user.id,
        "WrongCurrentPassword".to_string(), // Неверный текущий пароль
        "NewPassword789!".to_string(),
        &pool
    ).await;
    assert!(matches!(result, Err(AppError::Unauthorized)));

    // Тест 11: Проверка входа с новым паролем
    let login_request = LoginRequest {
        email: "test@example.com".to_string(),
        password: "NewPassword456!".to_string(), // Новый пароль
    };
    
    let result = login_service(login_request, &pool).await;
    assert!(result.is_ok());

    // Очистка после тестов
    cleanup_test_db(&pool).await;
}

// Настройка тестовой среды
fn setup_test_env() {
    // Инициализируем логгер для тестов
    INIT.call_once(|| {
        env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .is_test(true)
            .init();
    });

    // Устанавливаем необходимые переменные окружения
    env::set_var("DATABASE_URL", TEST_DB_URL);
    env::set_var("JWT_SECRET", "test_secret_key_for_jwt_token_generation");
}

// Настройка тестовой базы данных
async fn setup_test_db() -> sqlx::PgPool {
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(TEST_DB_URL)
        .await
        .expect("Не удалось подключиться к тестовой базе данных");

    // Очищаем базу перед тестами
    sqlx::query("DROP TABLE IF EXISTS users")
        .execute(&pool)
        .await
        .expect("Не удалось очистить таблицу users");

    // Создаём обновленную таблицу users со всеми необходимыми полями
    sqlx::query(
        r#"
        CREATE TABLE users (
            id UUID PRIMARY KEY,
            name TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            age SMALLINT NOT NULL CHECK (age > 0),
            role TEXT NOT NULL DEFAULT 'User',
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

// Очистка тестовой базы данных
async fn cleanup_test_db(pool: &sqlx::PgPool) {
    sqlx::query("DROP TABLE IF EXISTS users")
        .execute(pool)
        .await
        .expect("Не удалось очистить таблицу users");
}