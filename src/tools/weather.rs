//! DOC: https://www.sojson.com/api/weather.html
//! API: http://t.weather.sojson.com/api/weather/city/{cityId}
//! API: http://t.weather.itboy.net/api/weather/city/{cityId}
//! 城市: 本地 city.json（可通过环境变量 CITY_JSON_PATH 指定路径，默认 "city.json"）

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

const WEATHER_API: &str = "http://t.weather.sojson.com/api/weather/city";

#[derive(Debug, Deserialize)]
pub struct GetWeatherArgs {
    /// 城市名称（如「天津」「北京」）或城市编码（如 101030100）
    pub city: String,
}

#[derive(Debug)]
pub struct WeatherToolError(String);

impl std::fmt::Display for WeatherToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for WeatherToolError {}

/// 城市名称 -> city_code 缓存（sojson 城市表可能为省->市列表的嵌套）
fn parse_city_list(value: &serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    fn collect(obj: &serde_json::Value, out: &mut Vec<(String, String)>) {
        match obj {
            serde_json::Value::Array(arr) => {
                for item in arr {
                    if let Some(code) = item.get("city_code").and_then(|c| c.as_str()) {
                        if code.is_empty() {
                            continue;
                        }
                        let name = item
                            .get("city_name")
                            .or(item.get("city"))
                            .or(item.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !name.is_empty() {
                            out.push((name, code.to_string()));
                        }
                    }
                    collect(item, out);
                }
            }
            serde_json::Value::Object(map) => {
                for v in map.values() {
                    collect(v, out);
                }
            }
            _ => {}
        }
    }
    collect(value, &mut out);
    out
}

fn resolve_city_id(city: &str, cache: &[(String, String)]) -> Option<String> {
    let s = city.trim();
    if s.is_empty() {
        return None;
    }
    if s.chars().all(|c| c.is_ascii_digit()) {
        return Some(s.to_string());
    }
    for (name, code) in cache {
        if name == s || name.contains(s) || s.contains(name.as_str()) {
            return Some(code.clone());
        }
    }
    None
}

pub struct WeatherTool {
    client: reqwest::Client,
    city_json_path: String,
    city_cache: Arc<RwLock<Option<Vec<(String, String)>>>>,
}

impl WeatherTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("reqwest client"),
            city_json_path: std::env::var("CITY_JSON_PATH")
                .unwrap_or_else(|_| "city.json".to_string()),
            city_cache: Arc::new(RwLock::new(None)),
        }
    }

    async fn ensure_city_list(&self) -> Result<(), WeatherToolError> {
        let mut guard = self.city_cache.write().await;
        if guard.is_some() {
            return Ok(());
        }
        let content = tokio::fs::read_to_string(&self.city_json_path)
            .await
            .map_err(|e| {
                WeatherToolError(format!("读取城市文件 {} 失败: {}", self.city_json_path, e))
            })?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| WeatherToolError(format!("解析城市文件失败: {}", e)))?;
        *guard = Some(parse_city_list(&value));
        Ok(())
    }

    async fn get_weather_by_id(&self, city_id: &str) -> Result<String, WeatherToolError> {
        let url = format!("{}/{}", WEATHER_API, city_id);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| WeatherToolError(format!("请求天气接口失败: {}", e)))?;
        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| WeatherToolError(format!("解析天气响应失败: {}", e)))?;
        let status_code = body.get("status").and_then(|s| s.as_i64()).unwrap_or(0);
        if status_code != 200 {
            let msg = body
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("未知错误");
            return Err(WeatherToolError(format!(
                "天气接口返回错误: {} (status {})",
                msg, status_code
            )));
        }
        if !status.is_success() {
            return Err(WeatherToolError(format!("HTTP {}", status)));
        }
        let city_info = body
            .get("cityInfo")
            .and_then(|c| c.get("city"))
            .and_then(|c| c.as_str())
            .unwrap_or("未知");
        let data = match body.get("data") {
            Some(d) => d,
            None => return Err(WeatherToolError("响应缺少 data".to_string())),
        };
        let wendu = data.get("wendu").and_then(|w| w.as_str()).unwrap_or("-");
        let shidu = data.get("shidu").and_then(|s| s.as_str()).unwrap_or("-");
        let quality = data.get("quality").and_then(|q| q.as_str()).unwrap_or("-");
        let ganmao = data.get("ganmao").and_then(|g| g.as_str()).unwrap_or("-");
        let forecast = data.get("forecast").and_then(|f| f.as_array());
        let mut summary = format!(
            "{}：当前温度 {}℃，湿度 {}，空气质量 {}。{}",
            city_info, wendu, shidu, quality, ganmao
        );
        if let Some(arr) = forecast {
            let first = arr.first();
            if let Some(day) = first {
                let high = day.get("high").and_then(|h| h.as_str()).unwrap_or("");
                let low = day.get("low").and_then(|l| l.as_str()).unwrap_or("");
                let wtype = day.get("type").and_then(|t| t.as_str()).unwrap_or("");
                summary.push_str(&format!(" 今日 {}，{} / {}。", wtype, high, low));
            }
        }
        Ok(summary)
    }
}

impl Tool for WeatherTool {
    const NAME: &'static str = "get_weather";
    type Error = WeatherToolError;
    type Args = GetWeatherArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description:
                "查询指定城市的天气。支持城市名称（如天津、北京）或城市编码（如101030100）。"
                    .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "city": {
                        "type": "string",
                        "description": "城市名称或城市编码，例如：天津、北京、101030100"
                    }
                },
                "required": ["city"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        self.ensure_city_list().await?;
        let cache = self.city_cache.read().await;
        let list = cache
            .as_ref()
            .ok_or_else(|| WeatherToolError("城市列表未加载".to_string()))?;
        let city_id = resolve_city_id(args.city.trim(), list)
            .ok_or_else(|| WeatherToolError(format!("未找到城市: {}", args.city)))?;
        self.get_weather_by_id(&city_id).await
    }
}
