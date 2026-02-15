//! 简单 DDL 解析：从 CREATE TABLE 文本提取表名与列定义，供前端表格展示与二次编辑

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedColumn {
    pub name: String,
    pub data_type: String,
    pub is_nullable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedTable {
    pub table_name: String,
    pub columns: Vec<ParsedColumn>,
}

/// 解析 CREATE TABLE 语句，提取表名和列（列名、类型、是否可空）。
/// 支持常见 MySQL 写法，不处理复杂约束。
pub fn parse_create_table(ddl: &str) -> Result<ParsedTable, String> {
    let ddl = ddl.trim();
    if ddl.is_empty() {
        return Err("DDL 为空".to_string());
    }
    let upper = ddl.to_uppercase();
    if !upper.contains("CREATE") || !upper.contains("TABLE") {
        return Err("未找到 CREATE TABLE".to_string());
    }

    // 表名：CREATE TABLE `name` 或 CREATE TABLE name
    let table_name = extract_table_name(ddl).ok_or_else(|| {
        eprintln!(
            "[parse_ddl] 无法解析表名，ddl 开头(80字): {:?}",
            ddl.chars().take(80).collect::<String>()
        );
        "无法解析表名".to_string()
    })?;

    // 括号内的主体
    let body = extract_body(ddl).ok_or_else(|| {
        eprintln!(
            "[parse_ddl] 无法解析表体 (  )，ddl 开头(80字): {:?}",
            ddl.chars().take(80).collect::<String>()
        );
        "无法解析表体 (  )".to_string()
    })?;

    let columns = parse_columns(&body).map_err(|e| {
        eprintln!("[parse_ddl] 解析列失败: {}, body 开头(120字): {:?}", e, body.chars().take(120).collect::<String>());
        e
    })?;
    if columns.is_empty() {
        eprintln!("[parse_ddl] 未解析到任何列，body 开头(120字): {:?}", body.chars().take(120).collect::<String>());
        return Err("未解析到任何列".to_string());
    }

    Ok(ParsedTable {
        table_name,
        columns,
    })
}

fn extract_table_name(ddl: &str) -> Option<String> {
    let rest = ddl
        .trim_start()
        .get(13..)?; // "CREATE TABLE "
    let rest = rest.trim_start();
    let name = if rest.starts_with('`') {
        let close = rest.get(1..)?.find('`')?;
        rest.get(1..1 + close)?.trim().to_string()
    } else {
        let end = rest
            .find(|c: char| c.is_whitespace() || c == '(')
            .unwrap_or(rest.len());
        rest.get(..end)?.trim().to_string()
    };
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn extract_body(ddl: &str) -> Option<String> {
    let start = ddl.find('(')?;
    let mut depth = 0;
    let mut end = None;
    for (i, c) in ddl.chars().enumerate().skip(start) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    Some(ddl.get(start + 1..end)?.to_string())
}

fn parse_columns(body: &str) -> Result<Vec<ParsedColumn>, String> {
    let mut columns = Vec::new();
    let segments = split_at_top_level_comma(body);
    for seg in segments {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        let upper = seg.to_uppercase();
        if upper.starts_with("PRIMARY KEY")
            || upper.starts_with("KEY ")
            || upper.starts_with("UNIQUE ")
            || upper.starts_with("CONSTRAINT ")
            || upper.starts_with("INDEX ")
            || upper.starts_with("FOREIGN KEY")
        {
            continue;
        }
        if let Some(col) = parse_one_column(seg) {
            columns.push(col);
        }
    }
    Ok(columns)
}

/// 按顶层逗号分割，不分割引号内、括号内的逗号（避免 COMMENT '...'、DEFAULT '...' 被拆断）
fn split_at_top_level_comma(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => {
                if in_single && chars.peek() == Some(&'\'') {
                    chars.next();
                    current.push('\'');
                } else {
                    in_single = !in_single;
                    current.push(c);
                }
            }
            '"' if !in_single => {
                in_double = !in_double;
                current.push(c);
            }
            '(' if !in_single && !in_double => {
                depth += 1;
                current.push(c);
            }
            ')' if !in_single && !in_double => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 && !in_single && !in_double => {
                out.push(current.trim().to_string());
                current.clear();
            }
            _ => {
                current.push(c);
            }
        }
    }
    if !current.is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

fn parse_one_column(line: &str) -> Option<ParsedColumn> {
    let line = line.trim();
    let name = if line.starts_with('`') {
        let close = line.get(1..)?.find('`')?;
        line.get(1..1 + close)?.to_string()
    } else {
        let end = line
            .find(|c: char| c.is_whitespace())
            .unwrap_or(line.len());
        line.get(..end)?.to_string()
    };
    if name.is_empty() {
        return None;
    }
    let consumed = if line.starts_with('`') {
        name.len() + 2
    } else {
        line.find(|c: char| c.is_whitespace()).unwrap_or(line.len())
    };
    let rest = line.get(consumed..)?.trim_start();
    let (data_type, is_nullable) = type_and_nullable(rest);
    Some(ParsedColumn {
        name,
        data_type: data_type.unwrap_or_else(|| "varchar(255)".to_string()),
        is_nullable,
    })
}

fn type_and_nullable(rest: &str) -> (Option<String>, bool) {
    let upper = rest.to_uppercase();
    let is_nullable = !upper.contains("NOT NULL");
    let mut type_end = rest.len();
    for (i, _) in rest.char_indices() {
        let part = rest.get(i..).unwrap_or("");
        let u = part.to_uppercase();
        if u.starts_with("NULL")
            || u.starts_with("NOT NULL")
            || u.starts_with("DEFAULT ")
            || u.starts_with("COMMENT ")
            || u.starts_with("AUTO_INCREMENT")
            || u.starts_with("PRIMARY")
        {
            type_end = i;
            break;
        }
    }
    let data_type = rest.get(..type_end).map(|s| s.trim().to_string());
    (data_type, is_nullable)
}

/// 从解析结果重新生成 CREATE TABLE 文本（用于保存到 schema_ddl）
pub fn build_ddl_from_parsed(table_name: &str, columns: &[ParsedColumn]) -> String {
    let mut s = format!("CREATE TABLE `{}` (\n", table_name.replace('`', ""));
    let parts: Vec<String> = columns
        .iter()
        .map(|c| {
            let null = if c.is_nullable { " NULL" } else { " NOT NULL" };
            format!("  `{}` {}{}", c.name.replace('`', ""), c.data_type, null)
        })
        .collect();
    s.push_str(&parts.join(",\n"));
    s.push_str("\n);");
    s
}
