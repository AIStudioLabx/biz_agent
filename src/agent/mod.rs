//! Agent 模块：LLM 厂商抽象 + 功能型 Agent 定义
//!
//! - [llm]：与厂商无关的 [AgentService] trait，Ollama / OpenAI / Gemini 实现
//! - [router_agent]：RouterAgent 意图识别 + 分发到 SubAgent
//! - [weather_agent] / [sql_agent] / [general_agent]：SubAgent（天气 / 查库 / 通用）
//! - [text_to_sql_agent]：用于 SQL 生成的 Agent（无工具，供 TextToSqlService）
//! - [chat_agent]：主对话 Agent（Router + SubAgents）

pub mod chat_agent;
pub mod general_agent;
pub mod intent;
pub mod llm;
pub mod router_agent;
pub mod sql_agent;
pub mod text_to_sql_agent;
pub mod weather_agent;

#[allow(unused_imports)]
pub use intent::Intent;
pub use llm::{AgentError, AgentService, AgentStreamItem, MessageRole};
