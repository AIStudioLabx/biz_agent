//! 功能型 SubAgent：无工具通用对话，供 RouterAgent 分发。

use std::sync::Arc;

use crate::agent::llm::{AgentService, OllamaAgentService};

const DEFAULT_PREAMBLE: &str = "你是通用对话助手。根据用户消息友好、简洁地回复。";

/// 构建无工具通用对话 SubAgent（Ollama）。
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
