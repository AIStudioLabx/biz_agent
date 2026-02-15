//! 与厂商无关的 LLM/Agent 类型（流式项、错误、消息角色）

use std::str::FromStr;

/// 与厂商无关的流式输出项，供 handler 统一转成 SSE。
#[derive(Debug, Clone)]
pub enum AgentStreamItem {
    /// 正文片段
    Text(String),
    /// 推理/思考过程（如 Ollama reasoning）
    Reasoning(String),
    /// 流结束，包含完整回复
    Final { full_text: String },
    /// 流中错误
    Error(String),
}

/// 与厂商无关的 Agent 错误，便于上层统一处理。
#[derive(Debug, Clone)]
pub struct AgentError(pub String);

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AgentError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
}

impl MessageRole {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
        }
    }
}

impl FromStr for MessageRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "tool" => Ok(MessageRole::Tool),
            _ => Err(format!("unknown message role: {}", s)),
        }
    }
}
