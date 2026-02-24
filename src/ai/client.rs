use reqwest::Client;
use serde_json::json;

use crate::ai::types::AiEventParameters;
use crate::error::GcalError;

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
}
