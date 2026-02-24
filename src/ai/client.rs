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
You are a reliable Google Calendar event parameters generator. 
Extract the event details from the user's prompt and output a valid JSON object matching this schema.
Do NOT output anything other than JSON. If a detail is missing, omit the field or set it to null.

Schema:
{
  "title": "string (the event summary/title)",
  "date": "string (the date or date range in natural language, e.g. 'tomorrow', '3/19', '3月19日')",
  "start": "string (start time, e.g. '14:00', '午後2時')",
  "end": "string (end time or duration, e.g. '15:00', '+1h')",
  "location": "string (physical location or room)",
  "repeat_rule": "string (one of: 'daily', 'weekly', 'monthly', 'yearly', or null)"
}
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
