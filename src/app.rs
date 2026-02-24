use std::io::Write;

use chrono::{DateTime, Utc};

use crate::domain::EventQuery;
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

    /// 時間範囲は呼び出し元（main.rs）が計算して渡す
    pub async fn handle_events<W: Write>(
        &self,
        calendar_id: &str,
        time_min: DateTime<Utc>,
        time_max: DateTime<Utc>,
        out: &mut W,
    ) -> Result<(), GcalError> {
        let query = EventQuery {
            calendar_id: calendar_id.to_string(),
            time_min,
            time_max,
        };
        let events = self.calendar_client.list_events(query).await?;
        write_events(out, &events)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{NaiveDate, TimeZone, Utc};

    use crate::domain::{CalendarSummary, EventQuery, EventStart, EventSummary};
    use crate::error::GcalError;
    use crate::ports::CalendarClient;

    struct FakeCalendarClient {
        calendars: Vec<CalendarSummary>,
        events: Vec<EventSummary>,
    }

    #[async_trait]
    impl CalendarClient for FakeCalendarClient {
        async fn list_calendars(&self) -> Result<Vec<CalendarSummary>, GcalError> {
            Ok(self.calendars.clone())
        }
        async fn list_events(&self, _query: EventQuery) -> Result<Vec<EventSummary>, GcalError> {
            Ok(self.events.clone())
        }
    }

    fn time_min() -> DateTime<Utc> { Utc.with_ymd_and_hms(2026, 2, 24, 0, 0, 0).unwrap() }
    fn time_max() -> DateTime<Utc> { Utc.with_ymd_and_hms(2026, 3,  3, 23, 59, 59).unwrap() }

    #[tokio::test]
    async fn test_handle_calendars_prints_names() {
        let app = App {
            calendar_client: FakeCalendarClient {
                calendars: vec![
                    CalendarSummary { id: "primary".into(), summary: "My Calendar".into(), primary: true },
                    CalendarSummary { id: "work@example.com".into(), summary: "Work".into(), primary: false },
                ],
                events: vec![],
            },
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
            calendar_client: FakeCalendarClient { calendars: vec![], events },
        };

        let mut out = Vec::new();
        app.handle_events("primary", time_min(), time_max(), &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("朝会"));
        assert!(s.contains("祝日"));
    }

    #[tokio::test]
    async fn test_handle_calendars_empty() {
        let app = App {
            calendar_client: FakeCalendarClient { calendars: vec![], events: vec![] },
        };

        let mut out = Vec::new();
        app.handle_calendars(&mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("カレンダーが見つかりません"));
    }
}
