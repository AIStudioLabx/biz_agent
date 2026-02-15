//! LLM 厂商统一抽象：与具体 Provider 无关的 [AgentService] trait，由 Ollama / OpenAI / Gemini 等实现。

mod types;

pub mod ollama;
pub mod openai;
pub mod gemini;

use async_trait::async_trait;
use futures_util::Stream;
use std::pin::Pin;

pub use ollama::OllamaAgentService;
pub use types::{AgentError, AgentStreamItem, MessageRole};

/// 与厂商无关的 Agent 能力：对话与流式对话。可由 Ollama、OpenAI、Gemini 等实现。
#[async_trait]
pub trait AgentService: Send + Sync {
    /// 非流式单轮回复
    async fn chat(
        &self,
        prompt: &str,
        history: Vec<(MessageRole, String)>,
    ) -> Result<String, AgentError>;

    /// 流式对话：返回流，每项为正文/推理/结束/错误之一
    async fn stream_chat(
        &self,
        prompt: &str,
        history: Vec<(MessageRole, String)>,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<AgentStreamItem, AgentError>> + Send>>,
        AgentError,
    >;
}
