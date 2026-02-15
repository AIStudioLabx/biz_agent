//! 主对话 Agent：RouterAgent（意图识别 + 分发）+ WeatherSubAgent / SqlSubAgent / GeneralSubAgent。

use std::sync::Arc;

use crate::agent::general_agent;
use crate::agent::llm::{AgentService, OllamaAgentService};
use crate::agent::router_agent::{RouterAgent, CLASSIFIER_PREAMBLE};
use crate::agent::sql_agent;
use crate::agent::weather_agent;
use crate::tools::DatabaseQueryTool;

/// 构建主对话 Agent：先做意图识别，再分发到天气 / SQL / 通用 SubAgent。
pub fn build(
    model: &str,
    db_tool: DatabaseQueryTool,
) -> Result<Arc<dyn AgentService + Send + Sync>, String> {
    let classifier = Arc::new(OllamaAgentService::new_ollama(model, CLASSIFIER_PREAMBLE)?);
    let weather_sub = weather_agent::build(model, "你是天气助手。根据用户问题使用 get_weather 工具查天气并回答。")?;
    let sql_sub = sql_agent::build(model, db_tool)?;
    let general_sub = general_agent::build(model)?;

    Ok(Arc::new(RouterAgent::new(
        classifier,
        weather_sub,
        sql_sub,
        general_sub,
    )))
}
