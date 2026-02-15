//! Text-to-SQL：RAG（Schema + 问句-SQL 示例）+ 生成 + 执行 + DDL 解析
//!
//! - [service]：TextToSqlService（生成 SQL、执行、Tool Memory）
//! - [ddl_parser]：CREATE TABLE 解析与反序列化，供 DDL 上传页与 schema_ddl 使用

mod ddl_parser;
mod service;

#[allow(unused_imports)]
pub use ddl_parser::{build_ddl_from_parsed, parse_create_table, ParsedColumn, ParsedTable};
#[allow(unused_imports)]
pub use service::{SqlExecutionResult, TextToSqlError, TextToSqlService};
