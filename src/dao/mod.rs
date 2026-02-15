use sqlx::MySqlPool;

pub mod text_to_sql;
pub use text_to_sql::{
    get_schema_context, get_schema_ddl_list, get_training_examples,
    insert_schema_ddl, insert_training_pair,
};

pub async fn create_session(pool: &MySqlPool, session_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO chat_sessions (id) VALUES (?)")
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn insert_message(
    pool: &MySqlPool,
    session_id: &str,
    role: &str,
    content: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO chat_messages (session_id, role, content) VALUES (?, ?, ?)")
        .bind(session_id)
        .bind(role)
        .bind(content)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_messages_by_session(
    pool: &MySqlPool,
    session_id: &str,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT role, content FROM chat_messages WHERE session_id = ? ORDER BY id ASC",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 会话列表项：id + 第一条用户消息作为 summary（截断）。
pub async fn list_sessions(
    pool: &MySqlPool,
) -> Result<Vec<(String, Option<String>)>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        summary: Option<String>,
    }
    const SUMMARY_LEN: usize = 80;
    let rows = sqlx::query_as::<_, Row>(
        "SELECT s.id,
         (SELECT LEFT(content, ?) FROM chat_messages WHERE session_id = s.id AND role = 'user' ORDER BY id ASC LIMIT 1) AS summary
         FROM chat_sessions s ORDER BY s.created_at DESC",
    )
    .bind(SUMMARY_LEN as i32)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| (r.id, r.summary)).collect())
}
