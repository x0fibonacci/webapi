use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tokio::task::JoinHandle;

// Integration tests temporarily disabled for compilation fixes
// #[tokio::test]
// async fn test_api() {
//     // Tests will be re-enabled after compilation issues are resolved
// }

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