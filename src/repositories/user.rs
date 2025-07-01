use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::models::{UpdateUserRequest, User};

// Создаёт пользователя в базе данных
pub async fn create_user(user: &User, pool: &PgPool) -> Result<User, AppError> {
    let mut tx = pool.begin().await?;
    let result = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (id, name, email, password, age)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, name, email, password, age
        "#,
    )
    .bind(user.id)
    .bind(&user.name)
    .bind(&user.email)
    .bind(&user.password)
    .bind(user.age)
    .fetch_one(&mut *tx) // Исправлено: используем &mut *tx
    .await?;

    tx.commit().await?;
    Ok(result)
}

// Находит пользователя по email
pub async fn find_user_by_email(email: &str, pool: &PgPool) -> Result<User, AppError> {
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, name, email, password, age
        FROM users
        WHERE email = $1
        "#,
    )
    .bind(email)
    .fetch_one(pool)
    .await?;

    Ok(user)
}

// Обновляет данные пользователя
pub async fn update_user(
    user_id: Uuid,
    update_request: UpdateUserRequest,
    pool: &PgPool,
) -> Result<User, AppError> {
    let mut tx = pool.begin().await?;

    // Формируем динамический SQL-запрос для обновления только указанных полей
    let mut set_clauses = Vec::new();
    let mut param_index = 1;

    if update_request.name.is_some() {
        set_clauses.push(format!("name = ${}", param_index));
        param_index += 1;
    }
    if update_request.age.is_some() {
        set_clauses.push(format!("age = ${}", param_index));
        param_index += 1;
    }

    if set_clauses.is_empty() {
        return Err(AppError::BadRequest(
            "Нет данных для обновления".to_string(),
        ));
    }

    let query = format!(
        "UPDATE users SET {} WHERE id = ${} RETURNING id, name, email, password, age",
        set_clauses.join(", "),
        param_index
    );

    // Строим запрос с правильной привязкой параметров
    let mut query_builder = sqlx::query_as::<_, User>(&query);

    if let Some(name) = &update_request.name {
        query_builder = query_builder.bind(name);
    }
    if let Some(age) = update_request.age {
        query_builder = query_builder.bind(age);
    }
    query_builder = query_builder.bind(user_id);

    let user = query_builder.fetch_one(&mut *tx).await?; // Исправлено: используем &mut *tx

    tx.commit().await?;
    Ok(user)
}
