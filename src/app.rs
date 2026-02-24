use std::io::Write;

use chrono::{DateTime, Utc};

use crate::domain::{EventQuery, NewEvent, UpdateEvent};
use crate::error::GcalError;
use crate::output::{write_calendars, write_events};
use crate::ports::CalendarClient;

/// カレンダー・イベント系コマンドのハンドラ
/// 依存はすべてトレイト経由で注入するため、ネットワークなしでテスト可能
pub struct App<CAL> {
    pub calendar_client: CAL,
}

impl<CAL: CalendarClient> App<CAL> {
    pub async fn handle_calendars<W: Write>(&self, out: &mut W) -> Result<(), GcalError> {
        let calendars = self.calendar_client.list_calendars().await?;
        write_calendars(out, &calendars)?;
        Ok(())
    }

    pub async fn handle_update_event<W: Write>(
        &self,
        event: UpdateEvent,
        out: &mut W,
    ) -> Result<(), GcalError> {
        let event_id = event.event_id.clone();
        self.calendar_client.update_event(event).await?;
        writeln!(out, "更新しました (ID: {})", event_id)?;
        Ok(())
    }

    pub async fn handle_delete_event<W: Write>(
        &self,
        calendar_id: &str,
        event_id: &str,
        out: &mut W,
    ) -> Result<(), GcalError> {
        self.calendar_client.delete_event(calendar_id, event_id).await?;
        writeln!(out, "削除しました (ID: {})", event_id)?;
        Ok(())
    }

    pub async fn handle_add_event<W: Write>(
        &self,
        event: NewEvent,
        out: &mut W,
    ) -> Result<(), GcalError> {
        let summary = event.summary.clone();
        let id = self.calendar_client.create_event(event).await?;
        writeln!(out, "作成しました: {} (ID: {})", summary, id)?;
        Ok(())
    }

    /// 時間範囲は呼び出し元（main.rs）が計算して渡す
    pub async fn handle_events<W: Write>(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
        show_ids: bool,
        out: &mut W,
    ) -> Result<(), GcalError> {
        let query = EventQuery {
            calendar_id: calendar_id.to_string(),
            time_min,
            time_max,
        };
        let events = self.calendar_client.list_events(query).await?;
        write_events(out, &events, show_ids)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{NaiveDate, TimeZone, Utc};

    use crate::domain::{CalendarSummary, EventQuery, EventStart, EventSummary, NewEvent, UpdateEvent};
    use crate::error::GcalError;
    use crate::ports::CalendarClient;
    use std::sync::{Arc, Mutex};

    struct FakeCalendarClient {
        calendars: Vec<CalendarSummary>,
        events: Vec<EventSummary>,
        created_events: Arc<Mutex<Vec<NewEvent>>>,
        updated_events: Arc<Mutex<Vec<UpdateEvent>>>,
        deleted_ids: Arc<Mutex<Vec<String>>>,
    }

    impl FakeCalendarClient {
        fn new(calendars: Vec<CalendarSummary>, events: Vec<EventSummary>) -> Self {
            Self {
                calendars,
                events,
                created_events: Arc::new(Mutex::new(vec![])),
                updated_events: Arc::new(Mutex::new(vec![])),
                deleted_ids: Arc::new(Mutex::new(vec![])),
            }
        }
    }

    #[async_trait]
    impl CalendarClient for FakeCalendarClient {
        async fn list_calendars(&self) -> Result<Vec<CalendarSummary>, GcalError> {
            Ok(self.calendars.clone())
        }
        async fn list_events(&self, _query: EventQuery) -> Result<Vec<EventSummary>, GcalError> {
            Ok(self.events.clone())
        }
        async fn create_event(&self, event: NewEvent) -> Result<String, GcalError> {
            let id = format!("fake-id-{}", event.summary);
            self.created_events.lock().unwrap().push(event);
            Ok(id)
        }
        async fn update_event(&self, event: UpdateEvent) -> Result<(), GcalError> {
            self.updated_events.lock().unwrap().push(event);
            Ok(())
        }
        async fn delete_event(&self, _calendar_id: &str, event_id: &str) -> Result<(), GcalError> {
            self.deleted_ids.lock().unwrap().push(event_id.to_string());
            Ok(())
        }
    }

    fn time_min() -> DateTime<Utc> { Utc.with_ymd_and_hms(2026, 2, 24, 0, 0, 0).unwrap() }
    fn time_max() -> DateTime<Utc> { Utc.with_ymd_and_hms(2026, 3,  3, 23, 59, 59).unwrap() }

    #[tokio::test]
    async fn test_handle_calendars_prints_names() {
        let app = App {
            calendar_client: FakeCalendarClient::new(
                vec![
                    CalendarSummary { id: "primary".into(), summary: "My Calendar".into(), primary: true },
                    CalendarSummary { id: "work@example.com".into(), summary: "Work".into(), primary: false },
                ],
                vec![],
            ),
        };

        let mut out = Vec::new();
        app.handle_calendars(&mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("My Calendar"));
        assert!(s.contains("Work"));
        assert!(s.contains("primary"));
    }

    #[tokio::test]
    async fn test_handle_events_prints_summaries() {
        let events = vec![
            EventSummary {
                id: "1".into(),
                summary: "朝会".into(),
                start: EventStart::DateTime(Utc.with_ymd_and_hms(2026, 2, 25, 0, 0, 0).unwrap()),
            },
            EventSummary {
                id: "2".into(),
                summary: "祝日".into(),
                start: EventStart::Date(NaiveDate::from_ymd_opt(2026, 2, 26).unwrap()),
            },
        ];

        let app = App {
            calendar_client: FakeCalendarClient::new(vec![], events),
        };

        let mut out = Vec::new();
        app.handle_events("primary", time_min(), time_max(), false, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("朝会"));
        assert!(s.contains("祝日"));
    }

    #[tokio::test]
    async fn test_handle_calendars_empty() {
        let app = App {
            calendar_client: FakeCalendarClient::new(vec![], vec![]),
        };

        let mut out = Vec::new();
        app.handle_calendars(&mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("カレンダーが見つかりません"));
    }

    // --- handle_add_event のテスト ---

    #[tokio::test]
    async fn test_handle_add_event_prints_confirmation() {
        use chrono::{Local, TimeZone as _};

        let client = FakeCalendarClient::new(vec![], vec![]);
        let created = client.created_events.clone();
        let app = App { calendar_client: client };

        let start = Local.with_ymd_and_hms(2026, 3, 19, 10, 0, 0).unwrap();
        let end = Local.with_ymd_and_hms(2026, 3, 19, 11, 0, 0).unwrap();
        let event = NewEvent {
            summary: "テスト会議".to_string(),
            calendar_id: "primary".to_string(),
            start,
            end,
        };

        let mut out = Vec::new();
        app.handle_add_event(event, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("作成しました"));
        assert!(s.contains("テスト会議"));
        assert!(s.contains("fake-id-テスト会議"));

        let events = created.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].summary, "テスト会議");
    }

    // --- handle_update_event のテスト ---

    #[tokio::test]
    async fn test_handle_update_event_prints_confirmation() {
        let client = FakeCalendarClient::new(vec![], vec![]);
        let updated = client.updated_events.clone();
        let app = App { calendar_client: client };

        let event = UpdateEvent {
            event_id: "evt-abc123".to_string(),
            calendar_id: "primary".to_string(),
            title: Some("更新後タイトル".to_string()),
            start: None,
            end: None,
        };

        let mut out = Vec::new();
        app.handle_update_event(event, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("更新しました"));
        assert!(s.contains("evt-abc123"));

        let events = updated.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title.as_deref(), Some("更新後タイトル"));
    }

    // --- handle_delete_event のテスト ---

    #[tokio::test]
    async fn test_handle_delete_event_prints_confirmation() {
        let client = FakeCalendarClient::new(vec![], vec![]);
        let deleted = client.deleted_ids.clone();
        let app = App { calendar_client: client };

        let mut out = Vec::new();
        app.handle_delete_event("primary", "evt-del-456", &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("削除しました"));
        assert!(s.contains("evt-del-456"));

        let ids = deleted.lock().unwrap();
        assert_eq!(ids.as_slice(), ["evt-del-456"]);
    }
}
