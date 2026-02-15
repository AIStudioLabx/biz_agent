//! Ollama 实现的 [AgentService]，可无工具 / 带天气 / 带查库等。

use async_stream::stream;
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use rig::client::completion::CompletionClient;
use rig::client::ProviderClient;
use rig::completion::{message::Message as RigMessage, Chat};
use rig::providers::ollama;
use rig::streaming::StreamingChat;
use std::pin::Pin;
use std::sync::Arc;

use super::types::{AgentError, AgentStreamItem, MessageRole};
use super::AgentService;

pub type OllamaAgent = rig::agent::Agent<rig::providers::ollama::CompletionModel>;

fn to_rig_message(role: MessageRole, content: String) -> RigMessage {
    match role {
        MessageRole::User => RigMessage::user(content),
        MessageRole::Assistant => RigMessage::assistant(content),
        MessageRole::Tool => RigMessage::tool_result("0", content),
    }
}

#[derive(Clone)]
pub struct OllamaAgentService {
    inner: Arc<OllamaAgent>,
}

impl OllamaAgentService {
    /// 创建仅对话的 Agent（无工具），供 classifier / GeneralSubAgent 等使用。
    pub fn new_ollama(model: &str, preamble: &str) -> Result<Self, String> {
        if std::env::var("OLLAMA_API_BASE_URL").is_err() {
            std::env::set_var("OLLAMA_API_BASE_URL", "http://localhost:11434");
        }
        let client = ollama::Client::from_env();
        let agent = client.agent(model).preamble(preamble).build();
        Ok(Self {
            inner: Arc::new(agent),
        })
    }

    /// 创建带「查天气」工具的 Agent，供 WeatherSubAgent 使用。
    pub fn new_ollama_with_weather(model: &str, preamble: &str) -> Result<Self, String> {
        if std::env::var("OLLAMA_API_BASE_URL").is_err() {
            std::env::set_var("OLLAMA_API_BASE_URL", "http://localhost:11434");
        }
        let client = ollama::Client::from_env();
        let weather_tool = crate::tools::WeatherTool::new();
        let agent = client
            .agent(model)
            .preamble(preamble)
            .tool(weather_tool)
            .default_max_turns(2)
            .build();
        Ok(Self {
            inner: Arc::new(agent),
        })
    }

    /// 创建仅带「查数据库」工具的 Agent（供 SqlSubAgent 使用）。
    pub fn new_ollama_with_db(
        model: &str,
        preamble: &str,
        db_tool: crate::tools::DatabaseQueryTool,
    ) -> Result<Self, String> {
        if std::env::var("OLLAMA_API_BASE_URL").is_err() {
            std::env::set_var("OLLAMA_API_BASE_URL", "http://localhost:11434");
        }
        let client = ollama::Client::from_env();
        let agent = client
            .agent(model)
            .preamble(preamble)
            .tool(db_tool)
            .default_max_turns(2)
            .build();
        Ok(Self {
            inner: Arc::new(agent),
        })
    }

    /// 创建带「查天气」+「查数据库」工具的 Agent（单 Agent 模式，可选保留）。
    #[allow(dead_code)]
    pub fn new_ollama_with_weather_and_db(
        model: &str,
        preamble: &str,
        db_tool: crate::tools::DatabaseQueryTool,
    ) -> Result<Self, String> {
        if std::env::var("OLLAMA_API_BASE_URL").is_err() {
            std::env::set_var("OLLAMA_API_BASE_URL", "http://localhost:11434");
        }
        let client = ollama::Client::from_env();
        let weather_tool = crate::tools::WeatherTool::new();
        let agent = client
            .agent(model)
            .preamble(preamble)
            .tool(weather_tool)
            .tool(db_tool)
            .default_max_turns(2)
            .build();
        Ok(Self {
            inner: Arc::new(agent),
        })
    }
}

#[async_trait]
impl AgentService for OllamaAgentService {
    async fn chat(
        &self,
        prompt: &str,
        history: Vec<(MessageRole, String)>,
    ) -> Result<String, AgentError> {
        let messages: Vec<RigMessage> = history
            .into_iter()
            .map(|(role, content)| to_rig_message(role, content))
            .collect();
        self.inner
            .chat(prompt, messages)
            .await
            .map_err(|e| AgentError(e.to_string()))
    }

    async fn stream_chat(
        &self,
        prompt: &str,
        history: Vec<(MessageRole, String)>,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<AgentStreamItem, AgentError>> + Send>>,
        AgentError,
    > {
        let messages: Vec<RigMessage> = history
            .into_iter()
            .map(|(role, content)| to_rig_message(role, content))
            .collect();
        let request = self.inner.stream_chat(prompt, messages);
        let inner_stream = request.await;

        let s = stream! {
            use rig::agent::MultiTurnStreamItem;
            use rig::message::Text;
            use rig::streaming::StreamedAssistantContent;

            let mut inner = inner_stream;
            while let Some(item) = inner.next().await {
                match item {
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Text(Text { text }),
                    )) => yield Ok(AgentStreamItem::Text(text)),
                    Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Reasoning(r),
                    )) => {
                        let t = r.reasoning.join("");
                        if !t.is_empty() {
                            yield Ok(AgentStreamItem::Reasoning(t));
                        }
                    }
                    Ok(MultiTurnStreamItem::FinalResponse(f)) => {
                        let full_text = f.response().to_string();
                        yield Ok(AgentStreamItem::Final { full_text });
                        break;
                    }
                    Err(e) => {
                        yield Err(AgentError(e.to_string()));
                        break;
                    }
                    _ => {}
                }
            }
        };

        Ok(Box::pin(s))
    }
}
