//! Text-to-SQL 服务：检索 Schema + 示例、构建 Prompt、生成 SQL、执行（双输出）、可选 Tool Memory

use crate::agent::{AgentError, AgentService};
use crate::dao;
use serde::Serialize;
use sqlx::{Column, MySqlPool, Row};
use std::sync::Arc;

const EXAMPLE_LIMIT: u32 = 5;

/// 执行 SQL 后的双输出：给 LLM 的摘要 + 给前端的完整结果
#[derive(Debug, Clone, Serialize)]
pub struct SqlExecutionResult {
    /// 给 Agent 的简短摘要（省 token）
    pub for_llm: String,
    /// 给用户/前端的完整结果（表格或错误详情）
    pub for_user: serde_json::Value,
}

/// Text-to-SQL 服务：依赖 Agent + DB Pool，提供生成与执行
pub struct TextToSqlService {
    pub agent: Arc<dyn AgentService + Send + Sync>,
    pub pool: MySqlPool,
}

impl TextToSqlService {
    pub fn new(agent: Arc<dyn AgentService + Send + Sync>, pool: MySqlPool) -> Self {
        Self { agent, pool }
    }

    /// 从当前库拉取 Schema 文本，并合并「自定义 DDL」（schema_ddl 表）中固化的表结构
    async fn get_schema(&self) -> Result<String, sqlx::Error> {
        let from_db = dao::get_schema_context(&self.pool).await?;
        let custom = dao::get_schema_ddl_list(&self.pool).await?;
        if custom.is_empty() {
            return Ok(from_db);
        }
        let mut out = from_db;
        out.push_str("\n-- 以下为自定义表结构（schema_ddl 中配置，供生成 SQL 时使用）\n\n");
        for (ddl, comment) in custom {
            if let Some(c) = comment {
                out.push_str(&format!("-- {}\n", c));
            }
            out.push_str(&ddl);
            out.push_str("\n\n");
        }
        Ok(out)
    }

    /// 拉取最近 N 条问句-SQL 示例
    async fn get_examples(&self) -> Result<Vec<(String, String)>, sqlx::Error> {
        dao::get_training_examples(&self.pool, EXAMPLE_LIMIT).await
    }

    /// 构建 RAG Prompt：Schema + 示例 + 用户问题
    fn build_prompt(schema: &str, examples: &[(String, String)], question: &str) -> String {
        let mut s = String::from(
            "你是 Text-to-SQL 助手。根据下面「当前数据库 Schema」和「示例问句与 SQL」，\
            仅根据用户问题生成一条 MySQL 的 SQL。不要解释，不要多余文字。\
            若无法根据 Schema 生成则回复：无法生成。\n\n",
        );
        s.push_str("## 当前数据库 Schema\n");
        s.push_str(schema);
        if !examples.is_empty() {
            s.push_str("## 示例（问句 -> SQL）\n");
            for (q, sql) in examples {
                s.push_str(&format!("问句: {}\nSQL: {}\n\n", q, sql));
            }
        }
        s.push_str("## 用户问题\n");
        s.push_str(question);
        s.push_str("\n\n请只输出一条 SQL，或说明无法生成。");
        s
    }

    /// 从模型回复中解析出 SQL（支持 ```sql ... ``` 或首行即 SQL）
    fn parse_sql_from_response(response: &str) -> Option<String> {
        let trimmed = response.trim();
        // 1) 尝试 ```sql ... ``` 或 ``` ... ```
        if let Some(start) = trimmed.find("```") {
            let after = trimmed[start + 3..].trim_start();
            let (code, _) = if after.to_lowercase().starts_with("sql") {
                after[3..].split_once("```").unwrap_or((after, ""))
            } else {
                after.split_once("```").unwrap_or((after, ""))
            };
            let sql = code.trim();
            if !sql.is_empty() && (sql.contains("SELECT") || sql.contains("INSERT") || sql.contains("UPDATE") || sql.contains("DELETE")) {
                return Some(sql.to_string());
            }
        }
        // 2) 取第一行或整段若包含 SQL 关键字
        let first_line = trimmed.lines().next().unwrap_or(trimmed).trim();
        if !first_line.is_empty()
            && (first_line.to_uppercase().starts_with("SELECT")
            || first_line.to_uppercase().starts_with("INSERT")
            || first_line.to_uppercase().starts_with("UPDATE")
            || first_line.to_uppercase().starts_with("DELETE"))
        {
            return Some(first_line.to_string());
        }
        None
    }

    /// 生成 SQL：检索 Schema + 示例 -> 构建 Prompt -> 调用 Agent -> 解析 SQL
    pub async fn generate_sql(&self, question: &str) -> Result<String, TextToSqlError> {
        eprintln!("[text_to_sql] generate_sql 开始: question={:?}", question);
        let schema = self.get_schema().await.map_err(|e| {
            eprintln!("[text_to_sql] get_schema 失败: {}", e);
            TextToSqlError::Db(e)
        })?;
        let examples = self.get_examples().await.map_err(|e| {
            eprintln!("[text_to_sql] get_examples 失败: {}", e);
            TextToSqlError::Db(e)
        })?;
        let prompt = Self::build_prompt(&schema, &examples, question);
        let response = self
            .agent
            .chat(&prompt, vec![])
            .await
            .map_err(|e| {
                eprintln!("[text_to_sql] agent.chat 失败: {}", e.0);
                TextToSqlError::Agent(e)
            })?;
        if let Some(sql) = Self::parse_sql_from_response(&response) {
            return Ok(sql);
        }
        let trimmed = response.trim();
        if trimmed.contains("无法生成") || trimmed.contains("无法根据") || trimmed.is_empty() || trimmed.len() < 50 {
            eprintln!("[text_to_sql] 模型表示无法生成 SQL，回复: {}", trimmed.chars().take(200).collect::<String>());
            return Err(TextToSqlError::CannotGenerate(
                "当前数据库 Schema 中未包含问题里提到的表（如「三方游戏记录表」），无法生成该查询。请向用户说明：根据当前库结构无法生成，建议确认表名或联系管理员补充该表的 DDL/示例到训练数据。".to_string(),
            ));
        }
        eprintln!(
            "[text_to_sql] parse_sql 失败，模型回复(前 500 字): {}",
            response.chars().take(500).collect::<String>()
        );
        Err(TextToSqlError::Parse(response))
    }

    /// 执行 SQL，返回双输出：for_llm 摘要 + for_user 完整结果
    pub async fn execute_sql(&self, sql: &str) -> Result<SqlExecutionResult, TextToSqlError> {
        // 仅允许 SELECT / 或可配置为允许 DML；这里为演示允许 SELECT 与只读语义的 DML，生产应加白名单或 RLS
        let sql_upper = sql.trim().to_uppercase();
        if sql_upper.starts_with("SELECT") {
            let rows = sqlx::query(sql)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| {
                    eprintln!("[text_to_sql] execute_sql SELECT 失败: sql={}, error={}", sql, e);
                    TextToSqlError::Db(e)
                })?;
            let columns: Vec<String> = rows
                .first()
                .map(|row| {
                    row.columns()
                        .iter()
                        .map(|c| c.name().to_string())
                        .collect()
                })
                .unwrap_or_default();
            let mut arr = Vec::new();
            for row in &rows {
                let mut obj: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
                for (i, col) in columns.iter().enumerate() {
                    let val = cell_to_json_value(row, i);
                    obj.insert(col.clone(), val);
                }
                arr.push(serde_json::Value::Object(obj));
            }
            let count = rows.len();
            let for_llm = if count == 1 && !columns.is_empty() {
                // 单行结果（如 COUNT(*)）：显式写出数值，避免 LLM 编造
                let first_row = &rows[0];
                let values: Vec<String> = columns
                    .iter()
                    .enumerate()
                    .map(|(i, col)| {
                        let v = cell_to_json_value(first_row, i);
                        let s = match &v {
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            _ => v.to_string(),
                        };
                        format!("{}={}", col, s)
                    })
                    .collect();
                format!(
                    "查询成功，返回 1 行。数值结果：{}。请向用户准确报告上述数字，不要编造。",
                    values.join(", ")
                )
            } else if count <= 3 {
                format!("查询成功，返回 {} 行。", count)
            } else {
                format!("查询成功，返回 {} 行。前几条已展示。", count)
            };
            Ok(SqlExecutionResult {
                for_llm,
                for_user: serde_json::Value::Array(arr),
            })
        } else {
            let result = sqlx::query(sql)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    eprintln!("[text_to_sql] execute_sql DML 失败: sql={}, error={}", sql, e);
                    TextToSqlError::Db(e)
                })?;
            let affected = result.rows_affected();
            Ok(SqlExecutionResult {
                for_llm: format!("执行成功，影响 {} 行。", affected),
                for_user: serde_json::json!({ "rows_affected": affected }),
            })
        }
    }

    /// Tool Memory：将成功执行的问句-SQL 写入训练表，供后续 RAG 检索
    pub async fn save_tool_memory(
        &self,
        question: &str,
        sql_text: &str,
    ) -> Result<u64, sqlx::Error> {
        dao::insert_training_pair(&self.pool, question, sql_text, "tool_memory").await
    }
}

#[derive(Debug)]
pub enum TextToSqlError {
    Db(sqlx::Error),
    Agent(AgentError),
    /// 无法从回复中解析出 SQL，附带原始回复
    Parse(String),
    /// 模型明确表示无法生成（如 Schema 中无对应表），附带可转述给用户的说明
    CannotGenerate(String),
}

impl std::fmt::Display for TextToSqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextToSqlError::Db(e) => write!(f, "DB: {}", e),
            TextToSqlError::Agent(e) => write!(f, "Agent: {}", e.0),
            TextToSqlError::Parse(s) => write!(f, "无法解析 SQL，模型回复: {}", s),
            TextToSqlError::CannotGenerate(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for TextToSqlError {}

/// 将 sqlx 行中某列转为 serde_json::Value（尝试多种类型）
fn cell_to_json_value(row: &sqlx::mysql::MySqlRow, index: usize) -> serde_json::Value {
    use sqlx::Row;
    if let Ok(v) = row.try_get::<Option<String>, _>(index) {
        return v
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<i64>, _>(index) {
        return v
            .map(|n| serde_json::Value::Number(serde_json::Number::from(n)))
            .unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<f64>, _>(index) {
        return v
            .and_then(|f| serde_json::Number::from_f64(f).map(serde_json::Value::Number))
            .unwrap_or(serde_json::Value::Null);
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(index) {
        return v
            .map(serde_json::Value::Bool)
            .unwrap_or(serde_json::Value::Null);
    }
    serde_json::Value::Null
}
