//! RouterAgent：意图识别 + 分发到 WeatherSubAgent / SqlSubAgent / GeneralSubAgent。

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::Stream;

use crate::agent::llm::{AgentError, AgentService, AgentStreamItem, MessageRole};

use super::intent::Intent;

/// 意图分类器 Agent 的 preamble，供 chat_agent 构建 classifier 使用。
pub const CLASSIFIER_PREAMBLE: &str = "你是指令分类器。根据用户最后一条消息判断意图，只回复一个英文词：weather（查天气）、sql（查数据库/数据/统计）、general（其他对话）。不要解释。";

/// RouterAgent：先做意图识别，再分发到对应 SubAgent。
pub struct RouterAgent {
    /// 用于意图分类的无工具 Agent（小模型即可）
    classifier: Arc<dyn AgentService + Send + Sync>,
    weather_sub: Arc<dyn AgentService + Send + Sync>,
    sql_sub: Arc<dyn AgentService + Send + Sync>,
    general_sub: Arc<dyn AgentService + Send + Sync>,
}

impl RouterAgent {
    pub fn new(
        classifier: Arc<dyn AgentService + Send + Sync>,
        weather_sub: Arc<dyn AgentService + Send + Sync>,
        sql_sub: Arc<dyn AgentService + Send + Sync>,
        general_sub: Arc<dyn AgentService + Send + Sync>,
    ) -> Self {
        Self {
            classifier,
            weather_sub,
            sql_sub,
            general_sub,
        }
    }

    fn sub_for_intent(&self, intent: Intent) -> &Arc<dyn AgentService + Send + Sync> {
        match intent {
            Intent::Weather => &self.weather_sub,
            Intent::Sql => &self.sql_sub,
            Intent::General => &self.general_sub,
        }
    }

    /// 意图识别：根据用户消息返回应派发的 SubAgent 类型。
    pub async fn route(&self, message: &str) -> Result<Intent, AgentError> {
        let prompt = format!(
            "【分类】用户消息：{}\n请只回复一个词：weather、sql 或 general。",
            message
        );
        let reply = self
            .classifier
            .chat(&prompt, vec![])
            .await
            .map_err(|e| AgentError(e.0))?;
        Ok(Intent::from_classifier_reply(&reply))
    }
}

#[async_trait]
impl AgentService for RouterAgent {
    async fn chat(
        &self,
        prompt: &str,
        history: Vec<(MessageRole, String)>,
    ) -> Result<String, AgentError> {
        let intent = self.route(prompt).await?;
        let sub = self.sub_for_intent(intent);
        sub.chat(prompt, history).await
    }

    async fn stream_chat(
        &self,
        prompt: &str,
        history: Vec<(MessageRole, String)>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<AgentStreamItem, AgentError>> + Send>>, AgentError>
    {
        let intent = self.route(prompt).await?;
        let sub = self.sub_for_intent(intent);
        sub.stream_chat(prompt, history).await
    }
}
