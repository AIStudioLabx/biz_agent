//! 功能型 Agent：仅用于 Text-to-SQL 生成（无工具），供 [crate::text_to_sql::TextToSqlService] 使用。

use std::sync::Arc;

use crate::agent::llm::{AgentService, OllamaAgentService};

const DEFAULT_PREAMBLE: &str = "你是 Text-to-SQL 助手。根据用户提供的数据库 Schema 和示例，只生成一条 MySQL 的 SQL，不要解释。若无法生成请说明原因。";

/// 构建用于 SQL 生成的 Agent（无工具，Ollama）。
pub fn build(model: &str) -> Result<Arc<dyn AgentService + Send + Sync>, String> {
    build_with_preamble(model, DEFAULT_PREAMBLE)
}

/// 使用自定义 preamble 构建。
pub fn build_with_preamble(
    model: &str,
    preamble: &str,
) -> Result<Arc<dyn AgentService + Send + Sync>, String> {
    Ok(Arc::new(OllamaAgentService::new_ollama(model, preamble)?))
}
