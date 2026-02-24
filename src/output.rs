use std::io::Write;

use chrono::{DateTime, Datelike, Local, NaiveDate, Weekday};

use crate::domain::{CalendarSummary, EventStart, EventSummary};
use crate::error::GcalError;

/// カレンダー一覧を書き出す
pub fn write_calendars<W: Write>(out: &mut W, calendars: &[CalendarSummary]) -> Result<(), GcalError> {
    if calendars.is_empty() {
        writeln!(out, "カレンダーが見つかりません")?;
        return Ok(());
    }

    writeln!(out, "{:<40}  {}", "ID", "名前")?;
    writeln!(out, "{:-<40}  {:-<20}", "", "")?;

    for cal in calendars {
        let marker = if cal.primary { " *" } else { "" };
        writeln!(out, "{:<40}  {}{}", cal.id, cal.summary, marker)?;
    }

    Ok(())
}

/// イベント一覧を日付ごとにグループ化して書き出す
pub fn write_events<W: Write>(out: &mut W, events: &[EventSummary]) -> Result<(), GcalError> {
    if events.is_empty() {
        writeln!(out, "イベントが見つかりません")?;
        return Ok(());
    }

    let mut current_date: Option<NaiveDate> = None;

    for event in events {
        let (date, time_str) = match &event.start {
            EventStart::DateTime(dt) => {
                let local: DateTime<Local> = DateTime::from(*dt);
                (local.date_naive(), local.format("%H:%M").to_string())
            }
            EventStart::Date(d) => (*d, "終日".to_string()),
        };

        if current_date != Some(date) {
            if current_date.is_some() {
                writeln!(out)?;
            }
            writeln!(out, "{}", format_date(date))?;
            current_date = Some(date);
        }

        writeln!(out, "  {:5}  {}", time_str, event.summary)?;
    }

    Ok(())
}

fn format_date(date: NaiveDate) -> String {
    let weekday = match date.weekday() {
        Weekday::Mon => "Mon",
        Weekday::Tue => "Tue",
        Weekday::Wed => "Wed",
        Weekday::Thu => "Thu",
        Weekday::Fri => "Fri",
        Weekday::Sat => "Sat",
        Weekday::Sun => "Sun",
    };
    format!("{} ({})", date.format("%Y-%m-%d"), weekday)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make_calendar(id: &str, name: &str, primary: bool) -> CalendarSummary {
        CalendarSummary {
            id: id.to_string(),
            summary: name.to_string(),
            primary,
        }
    }

    fn make_event_dt(id: &str, summary: &str, year: i32, month: u32, day: u32, hour: u32, min: u32) -> EventSummary {
        let dt = Utc.with_ymd_and_hms(year, month, day, hour, min, 0).unwrap();
        EventSummary {
            id: id.to_string(),
            summary: summary.to_string(),
            start: EventStart::DateTime(dt),
        }
    }

    fn make_event_allday(id: &str, summary: &str, year: i32, month: u32, day: u32) -> EventSummary {
        let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        EventSummary {
            id: id.to_string(),
            summary: summary.to_string(),
            start: EventStart::Date(date),
        }
    }

    #[test]
    fn test_write_calendars_empty() {
        let mut out = Vec::new();
        write_calendars(&mut out, &[]).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("カレンダーが見つかりません"));
    }

    #[test]
    fn test_write_calendars_shows_id_and_name() {
        let cals = vec![
            make_calendar("primary", "My Calendar", true),
            make_calendar("work@group.calendar.google.com", "Work", false),
        ];
        let mut out = Vec::new();
        write_calendars(&mut out, &cals).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("primary"));
        assert!(s.contains("My Calendar"));
        assert!(s.contains("work@group.calendar.google.com"));
        assert!(s.contains("Work"));
    }

    #[test]
    fn test_write_calendars_primary_has_marker() {
        let cals = vec![
            make_calendar("primary", "My Calendar", true),
            make_calendar("other", "Other", false),
        ];
        let mut out = Vec::new();
        write_calendars(&mut out, &cals).unwrap();
        let s = String::from_utf8(out).unwrap();

        // primary には " *" マーカーが付く
        let primary_line = s.lines().find(|l| l.contains("My Calendar")).unwrap();
        assert!(primary_line.contains('*'));

        let other_line = s.lines().find(|l| l.contains("Other")).unwrap();
        assert!(!other_line.contains('*'));
    }

    #[test]
    fn test_write_events_empty() {
        let mut out = Vec::new();
        write_events(&mut out, &[]).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("イベントが見つかりません"));
    }

    #[test]
    fn test_write_events_shows_summary() {
        // UTC 01:00 = JST 10:00（テスト環境がUTCと仮定しない。内容の存在だけ確認）
        let events = vec![make_event_dt("1", "定例ミーティング", 2026, 2, 24, 1, 0)];
        let mut out = Vec::new();
        write_events(&mut out, &events).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("定例ミーティング"));
        assert!(s.contains("2026-02-24"));
    }

    #[test]
    fn test_write_events_date_grouping() {
        let events = vec![
            make_event_dt("1", "朝会", 2026, 2, 24, 0, 0),
            make_event_dt("2", "夕会", 2026, 2, 24, 9, 0),
            make_event_dt("3", "翌日MTG", 2026, 2, 25, 1, 0),
        ];
        let mut out = Vec::new();
        write_events(&mut out, &events).unwrap();
        let s = String::from_utf8(out).unwrap();

        // 2月24日の見出しは1回だけ
        let count_24 = s.matches("2026-02-24").count();
        assert_eq!(count_24, 1, "日付見出しは1回のみ: {}", s);

        // 2月25日の見出しも1回
        let count_25 = s.matches("2026-02-25").count();
        assert_eq!(count_25, 1, "日付見出しは1回のみ: {}", s);
    }

    #[test]
    fn test_write_events_allday() {
        let events = vec![make_event_allday("1", "祝日", 2026, 2, 24)];
        let mut out = Vec::new();
        write_events(&mut out, &events).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("終日"));
        assert!(s.contains("祝日"));
    }

    #[test]
    fn test_format_date_weekday() {
        let date = NaiveDate::from_ymd_opt(2026, 2, 24).unwrap(); // 火曜
        let s = format_date(date);
        assert_eq!(s, "2026-02-24 (Tue)");
    }
}
