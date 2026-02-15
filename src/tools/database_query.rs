//! 数据库查询工具：将用户自然语言问题转为 SQL 并执行，返回摘要给 Agent（页面可直接提问数据类问题）

use crate::text_to_sql::{TextToSqlError, TextToSqlService};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
pub struct QueryDatabaseArgs {
    /// 用户的自然语言问题（如「查询2月12日的FB体育有多少条数据？」）
    pub question: String,
}

#[derive(Debug)]
pub struct DatabaseQueryToolError(String);

impl std::fmt::Display for DatabaseQueryToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for DatabaseQueryToolError {}

impl From<TextToSqlError> for DatabaseQueryToolError {
    fn from(e: TextToSqlError) -> Self {
        DatabaseQueryToolError(e.to_string())
    }
}

pub struct DatabaseQueryTool {
    service: Arc<TextToSqlService>,
}

impl DatabaseQueryTool {
    pub fn new(service: Arc<TextToSqlService>) -> Self {
        Self { service }
    }
}

impl Tool for DatabaseQueryTool {
    const NAME: &'static str = "query_database";
    type Error = DatabaseQueryToolError;
    type Args = QueryDatabaseArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "根据用户的自然语言问题查询数据库。当用户问数据、统计、条数、列表、某日某业务等时使用此工具，将用户问题原样传入 question。例如：查询2月12日的FB体育有多少条数据、列出所有会话、统计今日消息数。"
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "用户的完整问题，如：查询2月12日的FB体育有多少条数据？"
                    }
                },
                "required": ["question"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let question = args.question.trim();
        if question.is_empty() {
            eprintln!("[query_database] ERROR: question 为空");
            return Err(DatabaseQueryToolError("question 不能为空".to_string()));
        }
        eprintln!("[query_database] 开始处理: question={:?}", question);
        let sql = match self.service.generate_sql(question).await {
            Ok(s) => s,
            Err(TextToSqlError::CannotGenerate(msg)) => {
                eprintln!("[query_database] 模型无法生成 SQL，转述给用户: {}", msg);
                return Ok(msg);
            }
            Err(e) => {
                eprintln!("[query_database] generate_sql 失败: question={:?}, error={}", question, e);
                return Err(e.into());
            }
        };
        eprintln!("[query_database] 生成 SQL: {}", sql);
        let result = match self.service.execute_sql(&sql).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[query_database] execute_sql 失败: sql={}, error={}", sql, e);
                return Ok(format!(
                    "执行失败：{}。您可以使用以下 SQL 自行执行：\n```sql\n{}\n```",
                    e, sql
                ));
            }
        };
        Ok(result.for_llm)
    }
}
