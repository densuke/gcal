use std::io::Write;

use chrono::Duration;

use crate::domain::EventQuery;
use crate::error::GcalError;
use crate::output::{write_calendars, write_events};
use crate::ports::{CalendarClient, Clock};

/// カレンダー・イベント系コマンドのハンドラ
/// 依存はすべてトレイト経由で注入するため、ネットワークなしでテスト可能
pub struct App<CAL, CLK> {
    pub calendar_client: CAL,
    pub clock: CLK,
}

impl<CAL: CalendarClient, CLK: Clock> App<CAL, CLK> {
    pub async fn handle_calendars<W: Write>(&self, out: &mut W) -> Result<(), GcalError> {
        let calendars = self.calendar_client.list_calendars().await?;
        write_calendars(out, &calendars)?;
        Ok(())
    }

    pub async fn handle_events<W: Write>(
        &self,
        calendar_id: &str,
        days: u64,
        out: &mut W,
    ) -> Result<(), GcalError> {
        let now = self.clock.now();
        let query = EventQuery {
            calendar_id: calendar_id.to_string(),
            time_min: now,
            time_max: now + Duration::days(days as i64),
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
    use chrono::{DateTime, NaiveDate, TimeZone, Utc};

    use crate::domain::{CalendarSummary, EventQuery, EventStart, EventSummary};
    use crate::error::GcalError;
    use crate::ports::{CalendarClient, Clock};

    // --- テスト用フェイク ---

    struct FixedClock(DateTime<Utc>);
    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

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

    fn fixed_now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 2, 24, 12, 0, 0).unwrap()
    }

    // --- テスト ---

    #[tokio::test]
    async fn test_handle_calendars_prints_names() {
        let app = App {
            calendar_client: FakeCalendarClient {
                calendars: vec![
                    CalendarSummary {
                        id: "primary".into(),
                        summary: "My Calendar".into(),
                        primary: true,
                    },
                    CalendarSummary {
                        id: "work@example.com".into(),
                        summary: "Work".into(),
                        primary: false,
                    },
                ],
                events: vec![],
            },
            clock: FixedClock(fixed_now()),
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
            calendar_client: FakeCalendarClient {
                calendars: vec![],
                events,
            },
            clock: FixedClock(fixed_now()),
        };

        let mut out = Vec::new();
        app.handle_events("primary", 7, &mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("朝会"));
        assert!(s.contains("祝日"));
    }

    #[tokio::test]
    async fn test_handle_calendars_empty() {
        let app = App {
            calendar_client: FakeCalendarClient {
                calendars: vec![],
                events: vec![],
            },
            clock: FixedClock(fixed_now()),
        };

        let mut out = Vec::new();
        app.handle_calendars(&mut out).await.unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("カレンダーが見つかりません"));
    }
}
