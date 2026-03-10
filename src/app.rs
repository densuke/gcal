use std::io::Write;

use chrono::{DateTime, Local, NaiveDate, Timelike, Utc};

use crate::domain::{EventQuery, EventStart, NewEvent, UpdateEvent};
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

    /// 複数カレンダーのイベントを取得して時間順にマージして表示する。
    /// 時間範囲は呼び出し元（main.rs）が計算して渡す。
    pub async fn handle_events<W: Write>(
        &self,
        calendar_ids: &[String],
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
        show_ids: bool,
        out: &mut W,
    ) -> Result<(), GcalError> {
        let mut all_events = Vec::new();
        for id in calendar_ids {
            let query = EventQuery {
                calendar_id: id.clone(),
                time_min,
                time_max,
            };
            let events = self.calendar_client.list_events(query).await?;
            all_events.extend(events);
        }
        all_events.sort_by_key(|e| event_sort_key(&e.start));
        write_events(out, &all_events, show_ids)?;
        Ok(())
    }
}

/// EventStart をソートキーに変換する。
/// キー: (ローカル日付, 終日フラグ=0が先, ローカル時刻の秒数)
/// 終日イベントは同じ日の時刻指定イベントより先に並ぶ。
fn event_sort_key(start: &EventStart) -> (NaiveDate, u8, u32) {
    match start {
        EventStart::Date(d) => (*d, 0, 0),
        EventStart::DateTime(dt) => {
            let local = dt.with_timezone(&Local);
            (local.date_naive(), 1, local.time().num_seconds_from_midnight())
        }
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
    async fn test_handle_events_single_calendar() {
        // 単一カレンダー（後退互換: &[String] に1要素）
        let events = vec![
            EventSummary {
                id: "1".into(),
                summary: "朝会".into(),
                start: EventStart::DateTime(Utc.with_ymd_and_hms(2026, 2, 25, 0, 0, 0).unwrap()),
                end: None,
            },
            EventSummary {
                id: "2".into(),
                summary: "祝日".into(),
                start: EventStart::Date(NaiveDate::from_ymd_opt(2026, 2, 26).unwrap()),
                end: None,
            },
        ];

        let app = App {
            calendar_client: FakeCalendarClient::new(vec![], events),
        };

        let mut out = Vec::new();
        app.handle_events(&["primary".to_string()], time_min(), time_max(), false, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("朝会"));
        assert!(s.contains("祝日"));
    }

    // --- 複数カレンダー用 Fake クライアント ---

    struct FakeMultiCalendarClient {
        // calendar_id → 返すイベントリスト
        events_by_calendar: std::collections::HashMap<String, Vec<EventSummary>>,
    }

    impl FakeMultiCalendarClient {
        fn new(map: Vec<(&str, Vec<EventSummary>)>) -> Self {
            Self {
                events_by_calendar: map.into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            }
        }
    }

    #[async_trait]
    impl CalendarClient for FakeMultiCalendarClient {
        async fn list_calendars(&self) -> Result<Vec<CalendarSummary>, GcalError> { Ok(vec![]) }
        async fn list_events(&self, query: EventQuery) -> Result<Vec<EventSummary>, GcalError> {
            Ok(self.events_by_calendar.get(&query.calendar_id).cloned().unwrap_or_default())
        }
        async fn create_event(&self, _: NewEvent) -> Result<String, GcalError> { Ok("id".into()) }
        async fn update_event(&self, _: UpdateEvent) -> Result<(), GcalError> { Ok(()) }
        async fn delete_event(&self, _: &str, _: &str) -> Result<(), GcalError> { Ok(()) }
    }

    #[tokio::test]
    async fn test_handle_events_multiple_calendars_sorted_by_time() {
        // 2カレンダーのイベントが時間順にマージされること
        let work_events = vec![
            EventSummary { id: "w1".into(), summary: "朝会".into(),
                start: EventStart::DateTime(Utc.with_ymd_and_hms(2026, 3, 1, 1, 0, 0).unwrap()), end: None },
            EventSummary { id: "w2".into(), summary: "週次MTG".into(),
                start: EventStart::DateTime(Utc.with_ymd_and_hms(2026, 3, 1, 5, 0, 0).unwrap()), end: None },
        ];
        let personal_events = vec![
            EventSummary { id: "p1".into(), summary: "ランチ".into(),
                start: EventStart::DateTime(Utc.with_ymd_and_hms(2026, 3, 1, 3, 0, 0).unwrap()), end: None },
        ];

        let app = App {
            calendar_client: FakeMultiCalendarClient::new(vec![
                ("work", work_events),
                ("personal", personal_events),
            ]),
        };

        let mut out = Vec::new();
        let ids = vec!["work".to_string(), "personal".to_string()];
        app.handle_events(&ids, time_min(), time_max(), false, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        // 全イベントが出力されること
        assert!(s.contains("朝会"), "朝会が含まれない: {s}");
        assert!(s.contains("ランチ"), "ランチが含まれない: {s}");
        assert!(s.contains("週次MTG"), "週次MTGが含まれない: {s}");

        // 時間順: 朝会(01:00) < ランチ(03:00) < 週次MTG(05:00)
        let pos_朝会   = s.find("朝会").unwrap();
        let pos_ランチ = s.find("ランチ").unwrap();
        let pos_mtg    = s.find("週次MTG").unwrap();
        assert!(pos_朝会 < pos_ランチ, "朝会がランチより後に出力された");
        assert!(pos_ランチ < pos_mtg, "ランチが週次MTGより後に出力された");
    }

    #[tokio::test]
    async fn test_handle_events_multiple_calendars_empty() {
        let app = App {
            calendar_client: FakeMultiCalendarClient::new(vec![
                ("cal1", vec![]),
                ("cal2", vec![]),
            ]),
        };

        let mut out = Vec::new();
        let ids = vec!["cal1".to_string(), "cal2".to_string()];
        app.handle_events(&ids, time_min(), time_max(), false, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("イベントが見つかりません"));
    }

    #[tokio::test]
    async fn test_handle_events_all_day_appears_before_timed_on_same_day() {
        // 終日イベントは同じ日の時刻指定イベントより先に表示されること
        // JST (UTC+9) では 08:00 JST = 前日 23:00 UTC になるため、
        // UTC ソートだと終日イベントより前になってしまうバグの回帰テスト
        use chrono::Local;
        let local_08_00 = Local.with_ymd_and_hms(2026, 2, 25, 8, 0, 0).unwrap().with_timezone(&Utc);
        let events = vec![
            EventSummary { id: "t".into(), summary: "朝会".into(),
                start: EventStart::DateTime(local_08_00), end: None },
            EventSummary { id: "d".into(), summary: "終日行事".into(),
                start: EventStart::Date(NaiveDate::from_ymd_opt(2026, 2, 25).unwrap()), end: None },
        ];
        let app = App {
            calendar_client: FakeCalendarClient::new(vec![], events),
        };
        let mut out = Vec::new();
        app.handle_events(&["primary".to_string()], time_min(), time_max(), false, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        let pos_allday = s.find("終日行事").unwrap();
        let pos_timed  = s.find("朝会").unwrap();
        assert!(pos_allday < pos_timed, "終日イベントが時刻指定イベントより後に出力された:\n{s}");
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
            calendar_display_name: None,
            start,
            end,
            recurrence: None,
            reminders: None,
            location: None,
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
            calendar_display_name: None,
            title: Some("更新後タイトル".to_string()),
            start: None,
            end: None,
            recurrence: None,
            reminders: None,
            location: None,
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
