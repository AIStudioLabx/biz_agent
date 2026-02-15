//! 功能型 SubAgent：仅带「查数据库」工具，供 RouterAgent 分发。

use std::sync::Arc;

use crate::agent::llm::{AgentService, OllamaAgentService};
use crate::tools::DatabaseQueryTool;

const DEFAULT_PREAMBLE: &str = "你是业务数据查询助手。根据用户问题使用 query_database 工具查询数据库，将用户问题原样传入 question，然后根据结果用自然语言回答。";

/// 构建仅带查库工具的 SubAgent（Ollama）。
pub fn build(
    model: &str,
    db_tool: DatabaseQueryTool,
) -> Result<Arc<dyn AgentService + Send + Sync>, String> {
    build_with_preamble(model, DEFAULT_PREAMBLE, db_tool)
}

/// 使用自定义 preamble 构建。
pub fn build_with_preamble(
    model: &str,
    preamble: &str,
    db_tool: DatabaseQueryTool,
) -> Result<Arc<dyn AgentService + Send + Sync>, String> {
    Ok(Arc::new(OllamaAgentService::new_ollama_with_db(
        model, preamble, db_tool,
    )?))
}
