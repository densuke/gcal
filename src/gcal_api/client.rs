use async_trait::async_trait;
use chrono::NaiveDate;
use iana_time_zone;

use crate::domain::{CalendarSummary, EventQuery, EventStart, EventSummary, NewEvent, UpdateEvent};
use crate::error::GcalError;
use crate::gcal_api::models::{CalendarListResponse, CreateEventRequest, CreateEventResponse, EventListResponse, EventTimeSpec, PatchEventRequest};
use crate::ports::{CalendarClient, TokenProvider};

const DEFAULT_BASE_URL: &str = "https://www.googleapis.com/calendar/v3";

/// ローカルタイムゾーン名を返す。取得できない場合は "UTC" を使用する。
fn local_timezone() -> String {
    iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string())
}

/// HTTP レスポンスのステータスを確認し、エラー時は ApiError を返す。
/// 成功時はレスポンス自体を返し、呼び出し元が続けて `.json()` 等に使用できる。
async fn check_response_status(resp: reqwest::Response) -> Result<reqwest::Response, GcalError> {
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let message = resp.text().await.unwrap_or_default();
        return Err(GcalError::ApiError { status, message });
    }
    Ok(resp)
}

pub struct GoogleCalendarClient<T: TokenProvider> {
    http: reqwest::Client,
    base_url: String,
    token_provider: T,
}

impl<T: TokenProvider> GoogleCalendarClient<T> {
    pub fn new(http: reqwest::Client, token_provider: T) -> Self {
        Self {
            http,
            base_url: DEFAULT_BASE_URL.to_string(),
            token_provider,
        }
    }

    /// テスト用: base_url を差し替えられるコンストラクタ
    pub fn with_base_url(http: reqwest::Client, token_provider: T, base_url: impl Into<String>) -> Self {
        Self {
            http,
            base_url: base_url.into(),
            token_provider,
        }
    }
}

#[async_trait]
impl<T: TokenProvider> CalendarClient for GoogleCalendarClient<T> {
    async fn list_calendars(&self) -> Result<Vec<CalendarSummary>, GcalError> {
        let token = self.token_provider.access_token().await?;
        let url = format!("{}/users/me/calendarList", self.base_url);

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await?;

        let resp = check_response_status(resp).await?;
        let body: CalendarListResponse = resp.json().await?;
        let items = body
            .items
            .unwrap_or_default()
            .into_iter()
            .map(|e| CalendarSummary {
                id: e.id,
                summary: e.summary,
                primary: e.primary.unwrap_or(false),
            })
            .collect();

        Ok(items)
    }

    async fn list_events(&self, query: EventQuery) -> Result<Vec<EventSummary>, GcalError> {
        let token = self.token_provider.access_token().await?;
        let url = format!("{}/calendars/{}/events", self.base_url, query.calendar_id);

        let time_min = query.time_min.to_rfc3339();
        let time_max = query.time_max.to_rfc3339();

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .query(&[
                ("timeMin", time_min.as_str()),
                ("timeMax", time_max.as_str()),
                ("singleEvents", "true"),
                ("orderBy", "startTime"),
            ])
            .send()
            .await?;

        let resp = check_response_status(resp).await?;
        let body: EventListResponse = resp.json().await?;
        let mut events = Vec::new();

        for entry in body.items.unwrap_or_default() {
            let id = entry.id.unwrap_or_default();
            let summary = entry.summary.unwrap_or_else(|| "(タイトルなし)".to_string());

            let start = match entry.start {
                Some(s) => {
                    if let Some(dt_str) = s.date_time {
                        let dt = chrono::DateTime::parse_from_rfc3339(&dt_str)
                            .map_err(|e| GcalError::ConfigError(format!("日時パースエラー: {e}")))?
                            .with_timezone(&chrono::Utc);
                        EventStart::DateTime(dt)
                    } else if let Some(d_str) = s.date {
                        let d = NaiveDate::parse_from_str(&d_str, "%Y-%m-%d")
                            .map_err(|e| GcalError::ConfigError(format!("日付パースエラー: {e}")))?;
                        EventStart::Date(d)
                    } else {
                        continue; // 開始日時がないイベントはスキップ
                    }
                }
                None => continue,
            };

            events.push(EventSummary { id, summary, start });
        }

        Ok(events)
    }

    async fn create_event(&self, event: NewEvent) -> Result<String, GcalError> {
        let token = self.token_provider.access_token().await?;
        let url = format!("{}/calendars/{}/events", self.base_url, event.calendar_id);

        let tz = local_timezone();
        let req = CreateEventRequest {
            summary: event.summary,
            start: EventTimeSpec {
                date_time: event.start.to_rfc3339(),
                time_zone: tz.clone(),
            },
            end: EventTimeSpec {
                date_time: event.end.to_rfc3339(),
                time_zone: tz,
            },
            recurrence: event.recurrence,
            reminders: event.reminders,
            location: event.location,
        };

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&token)
            .json(&req)
            .send()
            .await?;

        let resp = check_response_status(resp).await?;
        let body: CreateEventResponse = resp.json().await?;
        Ok(body.id)
    }

    async fn update_event(&self, event: UpdateEvent) -> Result<(), GcalError> {
        let token = self.token_provider.access_token().await?;
        let url = format!(
            "{}/calendars/{}/events/{}",
            self.base_url, event.calendar_id, event.event_id
        );

        let tz = local_timezone();
        let req = PatchEventRequest {
            summary: event.title,
            start: event.start.map(|dt| EventTimeSpec {
                date_time: dt.to_rfc3339(),
                time_zone: tz.clone(),
            }),
            end: event.end.map(|dt| EventTimeSpec {
                date_time: dt.to_rfc3339(),
                time_zone: tz,
            }),
            recurrence: event.recurrence,
            reminders: event.reminders,
            location: event.location,
        };

        let resp = self
            .http
            .patch(&url)
            .bearer_auth(&token)
            .json(&req)
            .send()
            .await?;

        check_response_status(resp).await?;

        Ok(())
    }

    async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<(), GcalError> {
        let token = self.token_provider.access_token().await?;
        let url = format!("{}/calendars/{}/events/{}", self.base_url, calendar_id, event_id);

        let resp = self
            .http
            .delete(&url)
            .bearer_auth(&token)
            .send()
            .await?;

        check_response_status(resp).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// テスト用の固定トークンプロバイダー
    struct StaticTokenProvider(String);

    #[async_trait]
    impl TokenProvider for StaticTokenProvider {
        async fn access_token(&self) -> Result<String, GcalError> {
            Ok(self.0.clone())
        }
    }

    fn make_client(base_url: &str) -> GoogleCalendarClient<StaticTokenProvider> {
        GoogleCalendarClient::with_base_url(
            reqwest::Client::new(),
            StaticTokenProvider("test-token".to_string()),
            base_url,
        )
    }

    // --- list_calendars のテスト ---

    #[tokio::test]
    async fn test_list_calendars_returns_items() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/users/me/calendarList"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": "primary", "summary": "My Calendar", "primary": true },
                    { "id": "work@example.com", "summary": "Work" }
                ]
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let calendars = client.list_calendars().await.unwrap();

        assert_eq!(calendars.len(), 2);
        assert_eq!(calendars[0].id, "primary");
        assert_eq!(calendars[0].summary, "My Calendar");
        assert!(calendars[0].primary);
        assert_eq!(calendars[1].id, "work@example.com");
        assert!(!calendars[1].primary);
    }

    #[tokio::test]
    async fn test_list_calendars_empty_items() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/users/me/calendarList"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": []
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let calendars = client.list_calendars().await.unwrap();
        assert!(calendars.is_empty());
    }

    #[tokio::test]
    async fn test_list_calendars_api_error() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/users/me/calendarList"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": { "message": "Unauthorized" }
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let result = client.list_calendars().await;
        assert!(matches!(result, Err(GcalError::ApiError { status: 401, .. })));
    }

    // --- list_events のテスト ---

    #[tokio::test]
    async fn test_list_events_sends_correct_params() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/calendars/primary/events"))
            .and(header("authorization", "Bearer test-token"))
            .and(query_param("singleEvents", "true"))
            .and(query_param("orderBy", "startTime"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": []
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let query = EventQuery {
            calendar_id: "primary".to_string(),
            time_min: Utc.with_ymd_and_hms(2026, 2, 24, 0, 0, 0).unwrap(),
            time_max: Utc.with_ymd_and_hms(2026, 3, 3, 0, 0, 0).unwrap(),
        };
        let events = client.list_events(query).await.unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn test_list_events_parses_datetime_event() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{
                    "id": "evt1",
                    "summary": "定例ミーティング",
                    "start": { "dateTime": "2026-02-25T10:00:00+09:00" }
                }]
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let query = EventQuery {
            calendar_id: "primary".to_string(),
            time_min: Utc.with_ymd_and_hms(2026, 2, 24, 0, 0, 0).unwrap(),
            time_max: Utc.with_ymd_and_hms(2026, 3, 3, 0, 0, 0).unwrap(),
        };
        let events = client.list_events(query).await.unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "定例ミーティング");
        assert!(matches!(events[0].start, EventStart::DateTime(_)));
    }

    #[tokio::test]
    async fn test_list_events_parses_allday_event() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [{
                    "id": "evt2",
                    "summary": "祝日",
                    "start": { "date": "2026-02-24" }
                }]
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let query = EventQuery {
            calendar_id: "primary".to_string(),
            time_min: Utc.with_ymd_and_hms(2026, 2, 24, 0, 0, 0).unwrap(),
            time_max: Utc.with_ymd_and_hms(2026, 3, 3, 0, 0, 0).unwrap(),
        };
        let events = client.list_events(query).await.unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "祝日");
        assert!(matches!(events[0].start, EventStart::Date(_)));
    }

    #[tokio::test]
    async fn test_list_events_skips_events_without_start() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "items": [
                    { "id": "evt1", "summary": "有効イベント", "start": { "dateTime": "2026-02-25T10:00:00Z" } },
                    { "id": "evt2", "summary": "開始なし" }
                ]
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let query = EventQuery {
            calendar_id: "primary".to_string(),
            time_min: Utc.with_ymd_and_hms(2026, 2, 24, 0, 0, 0).unwrap(),
            time_max: Utc.with_ymd_and_hms(2026, 3, 3, 0, 0, 0).unwrap(),
        };
        let events = client.list_events(query).await.unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "有効イベント");
    }

    // --- create_event のテスト ---

    #[tokio::test]
    async fn test_create_event_returns_id() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/calendars/primary/events"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "created-event-id-123",
                "summary": "テスト会議"
            })))
            .mount(&server)
            .await;

        use chrono::{Local, TimeZone as _};
        let start = Local.with_ymd_and_hms(2026, 3, 19, 10, 0, 0).unwrap();
        let end = Local.with_ymd_and_hms(2026, 3, 19, 11, 0, 0).unwrap();

        let client = make_client(&server.uri());
        let event = NewEvent {
            summary: "テスト会議".to_string(),
            calendar_id: "primary".to_string(),
            start,
            end,
            recurrence: None,
            reminders: None,
            location: None,
        };
        let id = client.create_event(event).await.unwrap();
        assert_eq!(id, "created-event-id-123");
    }

    #[tokio::test]
    async fn test_create_event_api_error_401() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": { "message": "Unauthorized" }
            })))
            .mount(&server)
            .await;

        use chrono::{Local, TimeZone as _};
        let start = Local.with_ymd_and_hms(2026, 3, 19, 10, 0, 0).unwrap();
        let end = Local.with_ymd_and_hms(2026, 3, 19, 11, 0, 0).unwrap();

        let client = make_client(&server.uri());
        let event = NewEvent {
            summary: "テスト".to_string(),
            calendar_id: "primary".to_string(),
            start,
            end,
            recurrence: None,
            reminders: None,
            location: None,
        };
        let result = client.create_event(event).await;
        assert!(matches!(result, Err(GcalError::ApiError { status: 401, .. })));
    }

    #[tokio::test]
    async fn test_create_event_api_error_400() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/calendars/primary/events"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": { "message": "Bad Request" }
            })))
            .mount(&server)
            .await;

        use chrono::{Local, TimeZone as _};
        let start = Local.with_ymd_and_hms(2026, 3, 19, 10, 0, 0).unwrap();
        let end = Local.with_ymd_and_hms(2026, 3, 19, 11, 0, 0).unwrap();

        let client = make_client(&server.uri());
        let event = NewEvent {
            summary: "テスト".to_string(),
            calendar_id: "primary".to_string(),
            start,
            end,
            recurrence: None,
            reminders: None,
            location: None,
        };
        let result = client.create_event(event).await;
        assert!(matches!(result, Err(GcalError::ApiError { status: 400, .. })));
    }

    // --- update_event のテスト ---

    #[tokio::test]
    async fn test_update_event_title_only() {
        let server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/calendars/primary/events/event-id-123"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "event-id-123",
                "summary": "新しいタイトル"
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let event = UpdateEvent {
            event_id: "event-id-123".to_string(),
            calendar_id: "primary".to_string(),
            title: Some("新しいタイトル".to_string()),
            start: None,
            end: None,
            recurrence: None,
            reminders: None,
            location: None,
        };
        client.update_event(event).await.unwrap();
    }

    #[tokio::test]
    async fn test_update_event_start_and_end() {
        let server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/calendars/primary/events/event-id-456"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "event-id-456"
            })))
            .mount(&server)
            .await;

        use chrono::{Local, TimeZone as _};
        let start = Local.with_ymd_and_hms(2026, 3, 20, 10, 0, 0).unwrap();
        let end = Local.with_ymd_and_hms(2026, 3, 20, 11, 0, 0).unwrap();

        let client = make_client(&server.uri());
        let event = UpdateEvent {
            event_id: "event-id-456".to_string(),
            calendar_id: "primary".to_string(),
            title: None,
            start: Some(start),
            end: Some(end),
            recurrence: None,
            reminders: None,
            location: None,
        };
        client.update_event(event).await.unwrap();
    }

    #[tokio::test]
    async fn test_update_event_api_error() {
        let server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/calendars/primary/events/no-such-event"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": { "message": "Not Found" }
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let event = UpdateEvent {
            event_id: "no-such-event".to_string(),
            calendar_id: "primary".to_string(),
            title: Some("test".to_string()),
            start: None,
            end: None,
            recurrence: None,
            reminders: None,
            location: None,
        };
        let result = client.update_event(event).await;
        assert!(matches!(result, Err(GcalError::ApiError { status: 404, .. })));
    }

    // --- delete_event のテスト ---

    #[tokio::test]
    async fn test_delete_event_success() {
        let server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/calendars/primary/events/event-del-123"))
            .and(header("authorization", "Bearer test-token"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        client.delete_event("primary", "event-del-123").await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_event_api_error() {
        let server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/calendars/primary/events/no-such-event"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": { "message": "Not Found" }
            })))
            .mount(&server)
            .await;

        let client = make_client(&server.uri());
        let result = client.delete_event("primary", "no-such-event").await;
        assert!(matches!(result, Err(GcalError::ApiError { status: 404, .. })));
    }
}
