//! Text-to-SQL 相关 DAO：训练数据、Schema 拉取

use sqlx::MySqlPool;

/// 插入一条问句-SQL 训练数据
pub async fn insert_training_pair(
    pool: &MySqlPool,
    question: &str,
    sql_text: &str,
    source: &str,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO sql_training_data (question, sql_text, source) VALUES (?, ?, ?)",
    )
        .bind(question)
        .bind(sql_text)
        .bind(source)
        .execute(pool)
        .await?;
    Ok(result.last_insert_id())
}

/// 取最近 N 条训练示例（问句, SQL），用于 RAG 上下文
pub async fn get_training_examples(
    pool: &MySqlPool,
    limit: u32,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT question, sql_text FROM sql_training_data ORDER BY id DESC LIMIT ?",
    )
        .bind(limit as i32)
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// 取所有自定义 DDL（表结构说明），用于与当前库 Schema 合并
pub async fn get_schema_ddl_list(pool: &MySqlPool) -> Result<Vec<(String, Option<String>)>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct Row {
        ddl_text: String,
        comment: Option<String>,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT ddl_text, comment FROM schema_ddl ORDER BY id ASC",
    )
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| (r.ddl_text, r.comment)).collect())
}

/// 插入一条自定义 DDL（固化表结构供模型使用）
pub async fn insert_schema_ddl(
    pool: &MySqlPool,
    ddl_text: &str,
    comment: Option<&str>,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO schema_ddl (ddl_text, comment) VALUES (?, ?)",
    )
        .bind(ddl_text)
        .bind(comment)
        .execute(pool)
        .await?;
    Ok(result.last_insert_id())
}

/// 从 information_schema 拉取当前库的表与列信息，拼成可读的 Schema 文本
pub async fn get_schema_context(pool: &MySqlPool) -> Result<String, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct SchemaRow {
        table_name: String,
        column_name: String,
        data_type: String,
        is_nullable: String,
    }
    // 使用 CAST 避免部分 MySQL 将 information_schema 列以 BLOB 返回导致解码失败
    let rows = sqlx::query_as::<_, SchemaRow>(
        r#"
        SELECT CAST(TABLE_NAME AS CHAR) AS table_name, CAST(COLUMN_NAME AS CHAR) AS column_name,
               CAST(DATA_TYPE AS CHAR) AS data_type, CAST(IS_NULLABLE AS CHAR) AS is_nullable
        FROM information_schema.COLUMNS
        WHERE TABLE_SCHEMA = DATABASE()
        ORDER BY TABLE_NAME, ORDINAL_POSITION
        "#,
    )
        .fetch_all(pool)
        .await?;

    let mut by_table: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for r in rows {
        let nullable = if r.is_nullable == "YES" { " NULL" } else { " NOT NULL" };
        by_table
            .entry(r.table_name)
            .or_default()
            .push(format!("  {} {} {}", r.column_name, r.data_type, nullable));
    }
    let mut out = String::from("-- 当前数据库 Schema（表与列）\n");
    for (table, cols) in by_table {
        out.push_str(&format!("-- 表: {}\n", table));
        out.push_str(&cols.join("\n"));
        out.push_str("\n\n");
    }
    Ok(out)
}
