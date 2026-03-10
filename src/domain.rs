use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};

/// カレンダーの概要情報
#[derive(Debug, Clone, PartialEq)]
pub struct CalendarSummary {
    pub id: String,
    pub summary: String,
    pub primary: bool,
}

/// イベントの概要情報
#[derive(Debug, Clone, PartialEq)]
pub struct EventSummary {
    pub id: String,
    pub summary: String,
    pub start: EventStart,
    pub end: Option<EventStart>,
}

/// イベント開始日時（終日イベントは date のみ、時刻指定は date_time）
#[derive(Debug, Clone, PartialEq)]
pub enum EventStart {
    DateTime(DateTime<Utc>),
    Date(chrono::NaiveDate),
}

/// イベント取得クエリ条件
#[derive(Debug, Clone)]
pub struct EventQuery {
    pub calendar_id: String,
    pub time_min: DateTime<Utc>,
    pub time_max: DateTime<Utc>,
}

/// 保存されたOAuth2トークン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// AuthCodeReceiver が返すコールバック結果
#[derive(Debug, Clone)]
pub struct OAuthCallback {
    pub code: String,
    pub state: String,
}

/// 新規作成するイベント
#[derive(Debug, Clone)]
pub struct NewEvent {
    pub summary: String,
    pub calendar_id: String,
    pub calendar_display_name: Option<String>,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub recurrence: Option<Vec<String>>,
    pub reminders: Option<crate::gcal_api::models::EventReminders>,
    pub location: Option<String>,
}

/// 既存イベントの更新内容（None のフィールドは変更しない）
#[derive(Debug, Clone)]
pub struct UpdateEvent {
    pub event_id: String,
    pub calendar_id: String,
    pub calendar_display_name: Option<String>,
    pub title: Option<String>,
    pub start: Option<DateTime<Local>>,
    pub end: Option<DateTime<Local>>,
    pub recurrence: Option<Vec<String>>,
    pub reminders: Option<crate::gcal_api::models::EventReminders>,
    pub location: Option<String>,
}
