use serde::Deserialize;

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
