// Модуль вспомогательных функций для приложения
use chrono::{DateTime, Utc};
use uuid::Uuid;

// Генерирует уникальный идентификатор запроса
pub fn generate_request_id() -> String {
    let uuid = Uuid::new_v4();
    format!("{}", uuid.as_simple())
}

// Возвращает текущее время с форматированием для логов
pub fn current_timestamp() -> String {
    let now: DateTime<Utc> = Utc::now();
    now.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

// Функция для безопасного получения подстроки
pub fn safe_substring(s: &str, start: usize, end: usize) -> &str {
    let len = s.len();
    if start >= len {
        return "";
    }
    
    let real_end = if end > len { len } else { end };
    &s[start..real_end]
}