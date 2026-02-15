use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{sse::Event, Html, Sse},
    Json,
};
use futures_util::stream::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::io;
use uuid::Uuid;

use crate::agent::{AgentStreamItem, MessageRole};
use crate::dao;
use crate::text_to_sql::{build_ddl_from_parsed, parse_create_table, ParsedColumn};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub reply: String,
    pub session_id: String,
}

#[derive(Debug, Serialize)]
pub struct MessageRecord {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SessionItem {
    pub id: String,
    /// 第一条用户消息摘要，空会话为 None
    pub summary: Option<String>,
}

fn err_json(status: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (status, Json(serde_json::json!({ "error": message })))
}

pub async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../../static/index.html"))
}

pub async fn chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, (StatusCode, Json<serde_json::Value>)> {
    if req.message.trim().is_empty() {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "message must be non-empty",
        ));
    }

    let session_id = match &req.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            let new_id = Uuid::new_v4().to_string();
            dao::create_session(&state.pool, &new_id)
                .await
                .map_err(|e| err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            new_id
        }
    };

    let rows = dao::get_messages_by_session(&state.pool, &session_id)
        .await
        .map_err(|e| err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    let history: Vec<(MessageRole, String)> = rows
        .into_iter()
        .map(|(role_str, content)| {
            (
                role_str
                    .parse::<MessageRole>()
                    .unwrap_or(MessageRole::Assistant),
                content,
            )
        })
        .collect();

    let reply = state
        .agent
        .chat(req.message.as_str(), history)
        .await
        .map_err(|e| {
            eprintln!("[chat_handler] agent.chat 失败: message={:?}, error={}", req.message, e.0);
            err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.0)
        })?;

    dao::insert_message(
        &state.pool,
        &session_id,
        MessageRole::User.as_str(),
        &req.message,
    )
    .await
    .map_err(|e| err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    dao::insert_message(
        &state.pool,
        &session_id,
        MessageRole::Assistant.as_str(),
        &reply,
    )
    .await
    .map_err(|e| err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    Ok(Json(ChatResponse { reply, session_id }))
}

/// 流式对话：返回 SSE，前端可逐块追加显示；结束时写入 DB 并发送 done 事件。
pub async fn chat_stream_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, io::Error>>>, (StatusCode, Json<serde_json::Value>)>
{
    if req.message.trim().is_empty() {
        return Err(err_json(
            StatusCode::BAD_REQUEST,
            "message must be non-empty",
        ));
    }

    let session_id = match &req.session_id {
        Some(id) if !id.is_empty() => id.clone(),
        _ => {
            let new_id = Uuid::new_v4().to_string();
            dao::create_session(&state.pool, &new_id)
                .await
                .map_err(|e| err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
            new_id
        }
    };

    let rows = dao::get_messages_by_session(&state.pool, &session_id)
        .await
        .map_err(|e| err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    let history: Vec<(MessageRole, String)> = rows
        .into_iter()
        .map(|(role_str, content)| {
            (
                role_str
                    .parse::<MessageRole>()
                    .unwrap_or(MessageRole::Assistant),
                content,
            )
        })
        .collect();

    dao::insert_message(
        &state.pool,
        &session_id,
        MessageRole::User.as_str(),
        &req.message,
    )
    .await
    .map_err(|e| err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;

    let agent = state.agent.clone();
    let pool = state.pool.clone();
    let session_id = session_id.clone();
    let prompt = req.message.clone();

    let stream = async_stream::stream! {
        let mut inner = match agent.stream_chat(prompt.as_str(), history).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[chat_stream_handler] agent.stream_chat 失败: prompt={:?}, error={}", prompt, e.0);
                yield Ok(Event::default().event("error").data(serde_json::json!({ "error": e.0 }).to_string()));
                return;
            }
        };
        while let Some(item) = inner.next().await {
            match item {
                Ok(AgentStreamItem::Text(text)) => yield Ok(Event::default().data(text)),
                Ok(AgentStreamItem::Reasoning(t)) => {
                    if !t.is_empty() {
                        yield Ok(Event::default().event("reasoning").data(t));
                    }
                }
                Ok(AgentStreamItem::Final { full_text }) => {
                    let _ = dao::insert_message(&pool, &session_id, MessageRole::Assistant.as_str(), &full_text).await;
                    let data = serde_json::json!({ "session_id": session_id, "full": full_text });
                    yield Ok(Event::default().event("done").data(data.to_string()));
                    break;
                }
                Ok(AgentStreamItem::Error(msg)) => {
                    eprintln!("[chat_stream_handler] Agent 流返回 Error: {}", msg);
                    yield Ok(Event::default().event("error").data(serde_json::json!({ "error": msg }).to_string()));
                    break;
                }
                Err(e) => {
                    eprintln!("[chat_stream_handler] Agent 流返回 Err: {}", e.0);
                    yield Ok(Event::default().event("error").data(serde_json::json!({ "error": e.0 }).to_string()));
                    break;
                }
            }
        }
    };

    Ok(Sse::new(stream))
}

pub async fn get_sessions(
    State(state): State<AppState>,
) -> Result<Json<Vec<SessionItem>>, (StatusCode, Json<serde_json::Value>)> {
    let rows = dao::list_sessions(&state.pool)
        .await
        .map_err(|e| err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    let list = rows
        .into_iter()
        .map(|(id, summary)| SessionItem { id, summary })
        .collect();
    Ok(Json(list))
}

pub async fn get_session_messages(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<MessageRecord>>, (StatusCode, Json<serde_json::Value>)> {
    let rows = dao::get_messages_by_session(&state.pool, &id)
        .await
        .map_err(|e| err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    let list = rows
        .into_iter()
        .map(|(role, content)| MessageRecord { role, content })
        .collect();
    Ok(Json(list))
}

// ---- Text-to-SQL ----

#[derive(Debug, Deserialize)]
pub struct TextToSqlRequest {
    pub question: String,
    #[serde(default)]
    pub execute: bool,
    #[serde(default)]
    pub save_to_memory: bool,
}

#[derive(Debug, Serialize)]
pub struct TextToSqlResponse {
    pub sql: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<TextToSqlResultPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TextToSqlResultPayload {
    pub for_llm: String,
    pub for_user: serde_json::Value,
}

pub async fn text_to_sql_handler(
    State(state): State<AppState>,
    Json(req): Json<TextToSqlRequest>,
) -> Result<Json<TextToSqlResponse>, (StatusCode, Json<serde_json::Value>)> {
    let service = state
        .text_to_sql_service
        .as_ref()
        .ok_or_else(|| err_json(StatusCode::SERVICE_UNAVAILABLE, "text-to-sql service not configured"))?;
    if req.question.trim().is_empty() {
        return Err(err_json(StatusCode::BAD_REQUEST, "question must be non-empty"));
    }

    let sql = match service.generate_sql(req.question.trim()).await {
        Ok(s) => s,
        Err(e) => {
            return Ok(Json(TextToSqlResponse {
                sql: String::new(),
                result: None,
                error: Some(e.to_string()),
            }))
        }
    };

    let mut result_payload: Option<TextToSqlResultPayload> = None;
    if req.execute && !sql.is_empty() {
        match service.execute_sql(&sql).await {
            Ok(r) => {
                result_payload = Some(TextToSqlResultPayload {
                    for_llm: r.for_llm,
                    for_user: r.for_user,
                });
                if req.save_to_memory {
                    let _ = service
                        .save_tool_memory(req.question.trim(), &sql)
                        .await;
                }
            }
            Err(e) => {
                return Ok(Json(TextToSqlResponse {
                    sql: sql.clone(),
                    result: None,
                    error: Some(e.to_string()),
                }))
            }
        }
    }

    Ok(Json(TextToSqlResponse {
        sql,
        result: result_payload,
        error: None,
    }))
}

// ---- 自定义表结构（DDL）固化 ----

#[derive(Debug, Deserialize)]
pub struct ParseDdlRequest {
    pub ddl_text: String,
}

#[derive(Debug, Serialize)]
pub struct ParseDdlResponse {
    pub table_name: String,
    pub columns: Vec<ParsedColumn>,
}

/// 解析 DDL，返回表名与列列表（供前端表格展示与编辑）
pub async fn parse_ddl_handler(
    Json(req): Json<ParseDdlRequest>,
) -> Result<Json<ParseDdlResponse>, (StatusCode, Json<serde_json::Value>)> {
    let trimmed = req.ddl_text.trim();
    let parsed = match parse_create_table(trimmed) {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "[parse_ddl] 解析失败: error={}, ddl_text(前500字)={:?}",
                e,
                trimmed.chars().take(500).collect::<String>()
            );
            return Err(err_json(StatusCode::BAD_REQUEST, &e));
        }
    };
    Ok(Json(ParseDdlResponse {
        table_name: parsed.table_name,
        columns: parsed.columns,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SchemaDdlRequest {
    /// 直接提交 DDL 文本
    DdlText {
        ddl_text: String,
        #[serde(default)]
        comment: Option<String>,
    },
    /// 提交解析后的表结构（表名 + 列），后端生成 DDL 再保存
    Table {
        table_name: String,
        columns: Vec<ParsedColumn>,
        #[serde(default)]
        comment: Option<String>,
    },
}

#[derive(Debug, Serialize)]
pub struct SchemaDdlResponse {
    pub id: u64,
    pub message: String,
}

/// 新增一条自定义 DDL，固化表结构供 Text-to-SQL 使用（与 information_schema 合并注入模型）
pub async fn add_schema_ddl_handler(
    State(state): State<AppState>,
    Json(req): Json<SchemaDdlRequest>,
) -> Result<Json<SchemaDdlResponse>, (StatusCode, Json<serde_json::Value>)> {
    let (ddl_text, comment) = match &req {
        SchemaDdlRequest::DdlText { ddl_text, comment } => {
            if ddl_text.trim().is_empty() {
                return Err(err_json(StatusCode::BAD_REQUEST, "ddl_text must be non-empty"));
            }
            (ddl_text.trim().to_string(), comment.clone())
        }
        SchemaDdlRequest::Table {
            table_name,
            columns,
            comment,
        } => {
            if table_name.trim().is_empty() {
                return Err(err_json(StatusCode::BAD_REQUEST, "table_name must be non-empty"));
            }
            if columns.is_empty() {
                return Err(err_json(StatusCode::BAD_REQUEST, "columns must be non-empty"));
            }
            let ddl = build_ddl_from_parsed(table_name, columns);
            (ddl, comment.clone())
        }
    };
    let id = dao::insert_schema_ddl(&state.pool, &ddl_text, comment.as_deref())
        .await
        .map_err(|e| err_json(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?;
    Ok(Json(SchemaDdlResponse {
        id,
        message: "已保存，后续生成 SQL 时会自动包含该表结构。".to_string(),
    }))
}

pub async fn serve_ddl_page() -> Html<&'static str> {
    Html(include_str!("../../static/ddl.html"))
}
