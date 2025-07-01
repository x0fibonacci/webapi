use chrono::Utc;  // Удален неиспользуемый импорт DateTime
use sqlx::PgPool;  // Удален неиспользуемый импорт postgres::PgQueryResult
use uuid::Uuid;
use log::debug;

use crate::errors::AppError;
use crate::models::{UpdateUserRequest, User, UserRole};

// Создаёт пользователя в базе данных
pub async fn create_user(user: &User, pool: &PgPool) -> Result<User, AppError> {
    debug!("Создание пользователя в БД: email={}, id={}", user.email, user.id);
    
    let result = sqlx::query_as::<_, User>(
        r#"
        INSERT INTO users (id, name, email, password_hash, age, role, created_at, updated_at, is_active)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING id, name, email, password_hash, age, role, created_at, updated_at, is_active
        "#,
    )
    .bind(&user.id)
    .bind(&user.name)
    .bind(&user.email)
    .bind(&user.password_hash)
    .bind(user.age)  // i32 вместо u16
    .bind(user.role)
    .bind(user.created_at)
    .bind(user.updated_at)
    .bind(user.is_active)
    .fetch_one(pool)
    .await
    .map_err(|err| {
        // Проверяем ошибки нарушения ограничений
        if let sqlx::Error::Database(ref db_err) = err {
            if let Some(constraint) = db_err.constraint() {
                if constraint == "users_email_key" {
                    return AppError::Conflict(format!(
                        "Пользователь с email '{}' уже существует", user.email
                    ));
                }
            }
        }
        debug!("Ошибка при создании пользователя: {:?}", err);
        AppError::from(err)  // Явно указываем преобразование в AppError
    })?;

    debug!("Пользователь успешно создан: id={}", user.id);
    Ok(result)
}

// Находит пользователя по email
pub async fn find_user_by_email(email: &str, pool: &PgPool) -> Result<User, AppError> {
    debug!("Поиск пользователя по email: {}", email);
    
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, name, email, password_hash, age, role, created_at, updated_at, is_active
        FROM users
        WHERE email = $1
        "#,
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .map_err(|err| {
        if let sqlx::Error::RowNotFound = err {
            debug!("Пользователь с email '{}' не найден", email);
            AppError::NotFound(format!("Пользователь с email '{}' не найден", email))
        } else {
            debug!("Ошибка при поиске пользователя по email: {:?}", err);
            AppError::from(err)  // Явно указываем преобразование в AppError
        }
    })?;

    // Проверяем активность пользователя
    if !user.is_active {
        return Err(AppError::Forbidden(
            "Аккаунт пользователя деактивирован".to_string()
        ));
    }

    debug!("Пользователь найден: id={}", user.id);
    Ok(user)
}

// Находит пользователя по ID
pub async fn find_user_by_id(id: Uuid, pool: &PgPool) -> Result<User, AppError> {
    debug!("Поиск пользователя по ID: {}", id);
    
    let user = sqlx::query_as::<_, User>(
        r#"
        SELECT id, name, email, password_hash, age, role, created_at, updated_at, is_active
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|err| {
        if let sqlx::Error::RowNotFound = err {
            debug!("Пользователь с ID '{}' не найден", id);
            AppError::NotFound(format!("Пользователь с ID '{}' не найден", id))
        } else {
            debug!("Ошибка при поиске пользователя по ID: {:?}", err);
            AppError::from(err)  // Явно указываем преобразование в AppError
        }
    })?;

    debug!("Пользователь найден: id={}", id);
    Ok(user)
}

// Обновляет данные пользователя
pub async fn update_user(
    user_id: Uuid,
    update_request: UpdateUserRequest,
    pool: &PgPool,
) -> Result<User, AppError> {
    debug!("Обновление пользователя: id={}", user_id);
    
    // Проверяем существование пользователя
    let _current_user = find_user_by_id(user_id, pool).await?;
    
    // Формируем SQL запрос с использованием COALESCE для обновления только заданных полей
    let result = sqlx::query_as::<_, User>(
        r#"
        UPDATE users 
        SET 
            name = COALESCE($1, name),
            age = COALESCE($2, age),
            updated_at = $3
        WHERE id = $4
        RETURNING id, name, email, password_hash, age, role, created_at, updated_at, is_active
        "#,
    )
    .bind(update_request.name.as_ref())
    .bind(update_request.age)  // i32 вместо u16
    .bind(Utc::now())
    .bind(user_id)
    .fetch_one(pool)
    .await?;  // Здесь ? автоматически преобразует sqlx::Error в AppError

    debug!("Пользователь успешно обновлен: id={}", user_id);
    Ok(result)
}

// Изменяет роль пользователя (для админов)
pub async fn update_user_role(
    user_id: Uuid,
    new_role: UserRole,
    pool: &PgPool,
) -> Result<User, AppError> {
    debug!("Изменение роли пользователя: id={}, новая роль={:?}", user_id, new_role);
    
    let result = sqlx::query_as::<_, User>(
        r#"
        UPDATE users 
        SET 
            role = $1,
            updated_at = $2
        WHERE id = $3
        RETURNING id, name, email, password_hash, age, role, created_at, updated_at, is_active
        "#,
    )
    .bind(new_role)
    .bind(Utc::now())
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|err| {
        if let sqlx::Error::RowNotFound = err {
            debug!("Пользователь с ID '{}' не найден при обновлении роли", user_id);
            AppError::NotFound(format!("Пользователь с ID '{}' не найден", user_id))
        } else {
            debug!("Ошибка при обновлении роли пользователя: {:?}", err);
            AppError::from(err)  // Явно указываем преобразование в AppError
        }
    })?;

    debug!("Роль пользователя успешно обновлена: id={}", user_id);
    Ok(result)
}

// Изменяет статус активации пользователя (для админов)
pub async fn update_user_status(
    user_id: Uuid,
    is_active: bool,
    pool: &PgPool,
) -> Result<User, AppError> {
    debug!("Изменение статуса активации пользователя: id={}, active={}", user_id, is_active);
    
    let result = sqlx::query_as::<_, User>(
        r#"
        UPDATE users 
        SET 
            is_active = $1,
            updated_at = $2
        WHERE id = $3
        RETURNING id, name, email, password_hash, age, role, created_at, updated_at, is_active
        "#,
    )
    .bind(is_active)
    .bind(Utc::now())
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|err| {
        if let sqlx::Error::RowNotFound = err {
            debug!("Пользователь с ID '{}' не найден при обновлении статуса", user_id);
            AppError::NotFound(format!("Пользователь с ID '{}' не найден", user_id))
        } else {
            debug!("Ошибка при обновлении статуса пользователя: {:?}", err);
            AppError::from(err)  // Явно указываем преобразование в AppError
        }
    })?;

    debug!("Статус пользователя успешно обновлен: id={}", user_id);
    Ok(result)
}

// Изменяет пароль пользователя
pub async fn update_user_password(
    user_id: Uuid,
    password_hash: &str,
    pool: &PgPool,
) -> Result<(), AppError> {
    debug!("Изменение пароля пользователя: id={}", user_id);
    
    let result = sqlx::query(
        r#"
        UPDATE users 
        SET 
            password_hash = $1,
            updated_at = $2
        WHERE id = $3
        "#,
    )
    .bind(password_hash)
    .bind(Utc::now())
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|err| {
        debug!("Ошибка при обновлении пароля пользователя: {:?}", err);
        AppError::from(err)  // Явно указываем преобразование в AppError
    })?;

    if result.rows_affected() == 0 {
        debug!("Пользователь с ID '{}' не найден при смене пароля", user_id);
        return Err(AppError::NotFound(format!("Пользователь с ID '{}' не найден", user_id)));
    }

    debug!("Пароль пользователя успешно обновлен: id={}", user_id);
    Ok(())
}

// Удаляет пользователя (мягкое удаление путём деактивации)
pub async fn soft_delete_user(user_id: Uuid, pool: &PgPool) -> Result<(), AppError> {
    debug!("Мягкое удаление пользователя: id={}", user_id);
    
    let result = sqlx::query(
        r#"
        UPDATE users 
        SET 
            is_active = false,
            updated_at = $1
        WHERE id = $2
        "#,
    )
    .bind(Utc::now())
    .bind(user_id)
    .execute(pool)
    .await
    .map_err(|err| {
        debug!("Ошибка при мягком удалении пользователя: {:?}", err);
        AppError::from(err)  // Явно указываем преобразование в AppError
    })?;

    if result.rows_affected() == 0 {
        debug!("Пользователь с ID '{}' не найден при удалении", user_id);
        return Err(AppError::NotFound(format!("Пользователь с ID '{}' не найден", user_id)));
    }

    debug!("Пользователь успешно деактивирован: id={}", user_id);
    Ok(())
}

// Список пользователей с пагинацией (для админов)
pub async fn list_users(
    offset: i64,
    limit: i64,
    pool: &PgPool,
) -> Result<Vec<User>, AppError> {
    debug!("Получение списка пользователей: offset={}, limit={}", offset, limit);
    
    let users = sqlx::query_as::<_, User>(
        r#"
        SELECT id, name, email, password_hash, age, role, created_at, updated_at, is_active
        FROM users
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|err| {
        debug!("Ошибка при получении списка пользователей: {:?}", err);
        AppError::from(err)  // Явно указываем преобразование в AppError
    })?;

    debug!("Получено {} пользователей", users.len());
    Ok(users)
}

// Подсчет общего количества пользователей
pub async fn count_users(pool: &PgPool) -> Result<i64, AppError> {
    debug!("Подсчет общего количества пользователей");
    
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .map_err(|err| {
            debug!("Ошибка при подсчете пользователей: {:?}", err);
            AppError::from(err)  // Явно указываем преобразование в AppError
        })?;

    debug!("Общее количество пользователей: {}", count.0);
    Ok(count.0)
}