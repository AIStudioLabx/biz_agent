//! 功能型 Agent：仅带「查天气」工具，用于天气类对话。

use std::sync::Arc;

use crate::agent::llm::{AgentService, OllamaAgentService};

/// 构建带天气工具的对话 Agent（Ollama），供 RouterAgent 作为 WeatherSubAgent。
pub fn build(model: &str, preamble: &str) -> Result<Arc<dyn AgentService + Send + Sync>, String> {
    Ok(Arc::new(OllamaAgentService::new_ollama_with_weather(
        model, preamble,
    )?))
}
