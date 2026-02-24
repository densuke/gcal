use serde::{Deserialize, Serialize};

/// カレンダーリスト API レスポンス
#[derive(Debug, Deserialize)]
pub struct CalendarListResponse {
    pub items: Option<Vec<CalendarListEntry>>,
}

#[derive(Debug, Deserialize)]
pub struct CalendarListEntry {
    pub id: String,
    pub summary: String,
    pub primary: Option<bool>,
}

/// イベントリスト API レスポンス
#[derive(Debug, Deserialize)]
pub struct EventListResponse {
    pub items: Option<Vec<EventEntry>>,
}

#[derive(Debug, Deserialize)]
pub struct EventEntry {
    pub id: Option<String>,
    pub summary: Option<String>,
    pub start: Option<EventStartTime>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventStartTime {
    /// 時刻指定イベント（RFC3339）
    pub date_time: Option<String>,
    /// 終日イベント（YYYY-MM-DD）
    pub date: Option<String>,
}

/// イベント作成リクエスト
#[derive(Debug, Serialize)]
pub struct CreateEventRequest {
    pub summary: String,
    pub start: EventTimeSpec,
    pub end: EventTimeSpec,
}

/// イベント時刻指定（RFC3339 + IANA タイムゾーン名）
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventTimeSpec {
    pub date_time: String,
    pub time_zone: String,
}

/// イベント作成 API レスポンス
#[derive(Debug, Deserialize)]
pub struct CreateEventResponse {
    pub id: String,
}
