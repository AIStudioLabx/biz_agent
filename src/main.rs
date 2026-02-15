mod agent;
mod dao;
mod handler;
mod text_to_sql;
mod tools;

use axum::routing::{get, post};
use axum::Router;
use sqlx::MySqlPool;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use agent::{chat_agent, text_to_sql_agent, AgentService};
use handler::{
    add_schema_ddl_handler, chat_handler, chat_stream_handler, get_session_messages,
    get_sessions, parse_ddl_handler, serve_ddl_page, serve_index, text_to_sql_handler,
};
use text_to_sql::TextToSqlService;
use tools::DatabaseQueryTool;

#[derive(Clone)]
pub struct AppState {
    pub pool: MySqlPool,
    pub agent: Arc<dyn AgentService + Send + Sync>,
    pub text_to_sql_service: Option<Arc<TextToSqlService>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "mysql://root:root@localhost:3306/biz_agent".to_string());
    println!("Connecting to database...");
    let pool = tokio::time::timeout(Duration::from_secs(5), MySqlPool::connect(&database_url))
        .await
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Database connection timeout (is MySQL running?)",
            )
        })?
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!("Database connection failed: {}", e),
            )
        })?;
    println!("Database connected.");

    let text_to_sql_agent = text_to_sql_agent::build("qwen2.5:7b")?;
    let text_to_sql_service = Arc::new(TextToSqlService::new(text_to_sql_agent, pool.clone()));
    let db_tool = DatabaseQueryTool::new(text_to_sql_service.clone());
    let agent = chat_agent::build("qwen2.5:7b", db_tool)?;

    let state = AppState {
        pool,
        agent,
        text_to_sql_service: Some(text_to_sql_service),
    };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/ddl", get(serve_ddl_page))
        .route("/v1/chat", post(chat_handler))
        .route("/v1/chat/stream", post(chat_stream_handler))
        .route("/v1/text-to-sql", post(text_to_sql_handler))
        .route("/v1/parse-ddl", post(parse_ddl_handler))
        .route("/v1/schema-ddl", post(add_schema_ddl_handler))
        .route("/v1/sessions", get(get_sessions))
        .route("/v1/sessions/:id/messages", get(get_session_messages))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    println!("listening on http://{}", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;

    Ok(())
}
