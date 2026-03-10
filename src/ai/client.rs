use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

use crate::ai::types::{AiEventParameters, AiOperationIntent};
use crate::error::GcalError;

/// AI クライアントの抽象トレイト。テスト時にスタブを注入できる。
#[async_trait]
pub trait AiClient: Send + Sync {
    async fn parse_prompt(&self, user_prompt: &str) -> Result<AiEventParameters, GcalError>;
    async fn parse_operation_intent(&self, user_prompt: &str) -> Result<AiOperationIntent, GcalError>;
}

pub struct OllamaClient {
    http: Client,
    base_url: String,
    model: String,
}

impl OllamaClient {
    pub fn new(base_url: String, model: String) -> Self {
        Self {
            http: Client::new(),
            base_url,
            model,
        }
    }

    pub async fn parse_prompt(&self, user_prompt: &str) -> Result<AiEventParameters, GcalError> {
        let url = format!("{}/api/chat", self.base_url);

        let system_prompt = r#"
You are a Google Calendar event parameter extractor.
Extract event details from the user's text and output ONLY a valid JSON object. No explanation, no markdown.

Rules:
1. "title": If the user uses 「...」 brackets, extract exactly what's inside. Otherwise, extract a CONCISE noun phrase (the event name, 2-6 words). Do NOT include verb phrases like "をいれています", "に参加します", "で設定".
   Example: "マッサージの予約をいれています" → "マッサージの予約"
2. "date": The date in the user's text (e.g., "2/27", "3/1", "明日", "今日"). Preserve as-is.
3. "start": Start time in HH:MM 24-hour format (e.g., "08:30", "15:00").
4. "end": End time or relative duration from start.
   - Absolute: "15:00"
   - Duration in hours: "+4h", "+2h" ("4時間" → "+4h", "2時間" → "+2h")
   - Duration in minutes: "+30m" ("30分" → "+30m")
   - Hours+minutes: "+1h30m" ("1時間30分" → "+1h30m")
   - Not specified: null
   IMPORTANT: NEVER convert duration to absolute clock time. "4時間" MUST be "+4h", never "12:30".
5. "location": Physical location or room name, or null.
6. "repeat_rule": "daily", "weekly", "monthly", "yearly", or null.
7. "reminder": Comma-separated notification timings.
   For relative reminders (use directly, no calculation needed):
   - "X分前" → "popup:Xm"   (example: "30分前" → "popup:30m")
   - "X時間前" → "popup:Xh"  (example: "2時間前" → "popup:2h")
   For absolute time on the previous day (use special format, no calculation needed):
   - "前日HH時" → "popup:prev-HH:00"  (example: "前日19時" → "popup:prev-19:00")
   Multiple: "前日19時と2時間前" → "popup:prev-19:00,popup:2h"
   null if not mentioned.
8. "calendar": Extract ONLY the short name, strip "のカレンダー" suffix.
   Example: "仕事のカレンダーに" → "仕事". null if not mentioned.

Example:
Input: "3/1 10時から2時間、仕事のカレンダーに「定例会議(役員限定)」、前日17時と30分前に通知"
Output: {"title":"定例会議(役員限定)","date":"3/1","start":"10:00","end":"+2h","location":null,"repeat_rule":null,"reminder":"popup:prev-17:00,popup:30m","calendar":"仕事"}
"#;

        let payload = json!({
            "model": self.model,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt.trim()
                },
                {
                    "role": "user",
                    "content": user_prompt
                }
            ],
            "format": "json",
            "stream": false,
            "options": {
                "temperature": 0.0
            }
        });

        let res = self.http.post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| GcalError::HttpError(e))?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_else(|_| String::new());
            return Err(GcalError::ApiError { status: status.as_u16(), message: format!("Ollama APIエラー: {}", body) });
        }

        let resp_json: serde_json::Value = res.json()
            .await
            .map_err(|e| GcalError::HttpError(e))?;

        let content_str = resp_json["message"]["content"].as_str()
            .ok_or_else(|| GcalError::ApiError { status: 500, message: "Ollamaのアウトプットからcontentが見つかりません".to_string() })?;

        let params: AiEventParameters = serde_json::from_str(content_str)
            .map_err(|e| GcalError::JsonError(e))?;

        Ok(params)
    }

    /// gcal events -p の第1段階: 操作種別とイベント特定ヒントを抽出する
    pub async fn parse_operation_intent(&self, user_prompt: &str) -> Result<AiOperationIntent, GcalError> {
        let url = format!("{}/api/chat", self.base_url);

        let system_prompt = r#"
You are a Google Calendar operation classifier.
Determine the operation type and extract hints to identify the target event or date range.
Output ONLY a valid JSON object. No explanation, no markdown.

Schema:
{
  "operation": "add" | "update" | "delete" | "show",
  "target": {
    "title_hint": "<keywords from event title or null>",
    "date_hint": "<date expression as-is or null>",
    "calendar": "<calendar alias or null>"
  } | null
}

Rules:
- "add": creating a new event. "target" is null.
- "update": changing an existing event. "target" has title_hint and/or date_hint.
- "delete": removing an existing event. "target" has title_hint and/or date_hint.
- "show": viewing/listing events (e.g. "見せて", "確認", "表示", "show", "list"). "target" has date_hint (and optionally title_hint/calendar).
- "title_hint": extract the event name keywords. Keep it concise. null if not applicable.
- "date_hint": preserve the original date expression (e.g., "明日", "来週", "来週火曜", "3/15", "今週"). null if not specified.
- "calendar": extract only if explicitly mentioned, otherwise null.

Examples:
Input: "明日の14時から会議を追加して"
Output: {"operation":"add","target":null}

Input: "来週火曜の定例MTGを削除して"
Output: {"operation":"delete","target":{"title_hint":"定例MTG","date_hint":"来週火曜","calendar":null}}

Input: "明日の仕事の朝会を15時に変更して"
Output: {"operation":"update","target":{"title_hint":"朝会","date_hint":"明日","calendar":"仕事"}}

Input: "来週の予定を見せて"
Output: {"operation":"show","target":{"title_hint":null,"date_hint":"来週","calendar":null}}

Input: "今日の仕事のカレンダーを確認したい"
Output: {"operation":"show","target":{"title_hint":null,"date_hint":"今日","calendar":"仕事"}}
"#;

        let payload = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system_prompt.trim() },
                { "role": "user", "content": user_prompt }
            ],
            "format": "json",
            "stream": false,
            "options": { "temperature": 0.0 }
        });

        let res = self.http.post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(GcalError::HttpError)?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            return Err(GcalError::ApiError { status: status.as_u16(), message: format!("Ollama APIエラー: {}", body) });
        }

        let resp_json: serde_json::Value = res.json().await.map_err(GcalError::HttpError)?;

        let content_str = resp_json["message"]["content"].as_str()
            .ok_or_else(|| GcalError::ApiError { status: 500, message: "Ollamaのアウトプットからcontentが見つかりません".to_string() })?;

        serde_json::from_str(content_str).map_err(GcalError::JsonError)
    }
}

#[async_trait]
impl AiClient for OllamaClient {
    async fn parse_prompt(&self, user_prompt: &str) -> Result<AiEventParameters, GcalError> {
        OllamaClient::parse_prompt(self, user_prompt).await
    }

    async fn parse_operation_intent(&self, user_prompt: &str) -> Result<AiOperationIntent, GcalError> {
        OllamaClient::parse_operation_intent(self, user_prompt).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_client(base_url: &str) -> OllamaClient {
        OllamaClient::new(base_url.to_string(), "llama3".to_string())
    }

    #[tokio::test]
    async fn test_parse_prompt_success() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "content": r#"{"title":"チームMTG","date":"明日","start":"14:00","end":"15:00","location":null,"repeat_rule":null}"#
                }
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let params = client.parse_prompt("明日の14時からチームMTG").await.unwrap();
        assert_eq!(params.title.as_deref(), Some("チームMTG"));
        assert_eq!(params.date.as_deref(), Some("明日"));
        assert_eq!(params.start.as_deref(), Some("14:00"));
        assert_eq!(params.end.as_deref(), Some("15:00"));
        assert_eq!(params.location, None);
    }

    #[tokio::test]
    async fn test_parse_prompt_with_location() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "content": r#"{"title":"ランチ","date":"今日","start":"12:00","end":"13:00","location":"会議室A","repeat_rule":null}"#
                }
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let params = client.parse_prompt("今日12時から会議室Aでランチ").await.unwrap();
        assert_eq!(params.location.as_deref(), Some("会議室A"));
    }

    #[tokio::test]
    async fn test_parse_prompt_api_error_returns_err() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let result = client.parse_prompt("test").await;
        assert!(result.is_err());
        // ApiError であることを確認
        assert!(matches!(result.unwrap_err(), GcalError::ApiError { .. }));
    }

    #[tokio::test]
    async fn test_parse_prompt_invalid_json_in_content_returns_err() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": { "content": "これはJSONではありません" }
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let result = client.parse_prompt("test").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GcalError::JsonError(_)));
    }

    #[tokio::test]
    async fn test_parse_prompt_missing_content_field_returns_err() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {}
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let result = client.parse_prompt("test").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GcalError::ApiError { .. }));
    }

    #[tokio::test]
    async fn test_parse_operation_intent_add() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "content": r#"{"operation":"add","target":null}"#
                }
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let intent = client.parse_operation_intent("明日の14時から会議を追加して").await.unwrap();
        assert_eq!(intent.operation, "add");
        assert!(intent.target.is_none());
    }

    #[tokio::test]
    async fn test_parse_operation_intent_delete_with_target() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "content": r#"{"operation":"delete","target":{"title_hint":"定例MTG","date_hint":"来週火曜","calendar":null}}"#
                }
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let intent = client.parse_operation_intent("来週火曜の定例MTGを削除して").await.unwrap();
        assert_eq!(intent.operation, "delete");
        let target = intent.target.unwrap();
        assert_eq!(target.title_hint.as_deref(), Some("定例MTG"));
        assert_eq!(target.date_hint.as_deref(), Some("来週火曜"));
        assert_eq!(target.calendar, None);
    }

    #[tokio::test]
    async fn test_parse_operation_intent_update_with_calendar() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "message": {
                    "content": r#"{"operation":"update","target":{"title_hint":"朝会","date_hint":"明日","calendar":"仕事"}}"#
                }
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let intent = client.parse_operation_intent("明日の仕事の朝会を変更して").await.unwrap();
        assert_eq!(intent.operation, "update");
        let target = intent.target.unwrap();
        assert_eq!(target.calendar.as_deref(), Some("仕事"));
    }
}
