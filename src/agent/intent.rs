//! 意图识别：RouterAgent 分类结果，用于分发到对应 SubAgent。

/// 用户消息意图，由 RouterAgent 识别后分发到对应 SubAgent。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    /// 查天气 → WeatherSubAgent
    Weather,
    /// 查数据库/数据/统计 → SqlSubAgent
    Sql,
    /// 其他对话 → GeneralSubAgent
    General,
}

impl Intent {
    /// 从分类器 LLM 回复中解析意图（期望单词：weather / sql / general）。
    pub fn from_classifier_reply(reply: &str) -> Self {
        let s = reply.trim().to_lowercase();
        if s.contains("weather") || s.contains("天气") {
            return Intent::Weather;
        }
        if s.contains("sql") || s.contains("数据库") || s.contains("数据") || s.contains("查询") {
            return Intent::Sql;
        }
        Intent::General
    }
}
