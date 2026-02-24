use std::io::Write;

use chrono::{DateTime, Datelike, Local, NaiveDate, Weekday};

use crate::domain::{CalendarSummary, EventStart, EventSummary, NewEvent, UpdateEvent};
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
///
/// `show_ids` が true のとき、各行の末尾にイベント ID を表示する
pub fn write_events<W: Write>(out: &mut W, events: &[EventSummary], show_ids: bool) -> Result<(), GcalError> {
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

        if show_ids {
            writeln!(out, "  {:5}  {:<40}  [{}]", time_str, event.summary, event.id)?;
        } else {
            writeln!(out, "  {:5}  {}", time_str, event.summary)?;
        }
    }

    Ok(())
}

/// `--dry-run` 時に NewEvent の内容を人間が読める形式で出力する
pub fn write_new_event_dry_run<W: Write>(event: &NewEvent, out: &mut W) -> Result<(), GcalError> {
    writeln!(out, "[ドライラン] 以下の内容で登録されます:")?;
    writeln!(out, "  タイトル:   {}", event.summary)?;
    writeln!(out, "  開始:       {}", event.start.format("%Y-%m-%d %H:%M"))?;
    writeln!(out, "  終了:       {}", event.end.format("%Y-%m-%d %H:%M"))?;
    writeln!(out, "  場所:       {}", event.location.as_deref().unwrap_or("(なし)"))?;
    match event.recurrence.as_deref() {
        None | Some([]) => writeln!(out, "  繰り返し:   (なし)")?,
        Some(rules) => writeln!(out, "  繰り返し:   {}", rules.join(", "))?,
    }
    writeln!(out, "  カレンダー: {}", event.calendar_id)?;
    Ok(())
}

/// `--dry-run` 時に UpdateEvent の内容を人間が読める形式で出力する
pub fn write_update_event_dry_run<W: Write>(event: &UpdateEvent, out: &mut W) -> Result<(), GcalError> {
    writeln!(out, "[ドライラン] 以下の内容で更新されます (ID: {}):", event.event_id)?;
    writeln!(out, "  タイトル:   {}", event.title.as_deref().unwrap_or("(変更なし)"))?;
    match (&event.start, &event.end) {
        (Some(s), Some(e)) => {
            writeln!(out, "  開始:       {}", s.format("%Y-%m-%d %H:%M"))?;
            writeln!(out, "  終了:       {}", e.format("%Y-%m-%d %H:%M"))?;
        }
        _ => writeln!(out, "  開始/終了:  (変更なし)")?,
    }
    writeln!(out, "  場所:       {}", event.location.as_deref().unwrap_or("(変更なし)"))?;
    writeln!(out, "  カレンダー: {}", event.calendar_id)?;
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
        write_events(&mut out, &[], false).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("イベントが見つかりません"));
    }

    #[test]
    fn test_write_events_shows_summary() {
        // UTC 01:00 = JST 10:00（テスト環境がUTCと仮定しない。内容の存在だけ確認）
        let events = vec![make_event_dt("1", "定例ミーティング", 2026, 2, 24, 1, 0)];
        let mut out = Vec::new();
        write_events(&mut out, &events, false).unwrap();
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
        write_events(&mut out, &events, false).unwrap();
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
        write_events(&mut out, &events, false).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("終日"));
        assert!(s.contains("祝日"));
    }

    #[test]
    fn test_write_events_show_ids() {
        let events = vec![make_event_dt("abc1def2ghi3jkl4", "定例MTG", 2026, 2, 24, 1, 0)];
        let mut out = Vec::new();
        write_events(&mut out, &events, true).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("定例MTG"));
        assert!(s.contains("abc1def2ghi3jkl4"), "ID が表示されていない: {}", s);
    }

    #[test]
    fn test_write_events_no_ids_by_default() {
        let events = vec![make_event_dt("abc1def2ghi3jkl4", "定例MTG", 2026, 2, 24, 1, 0)];
        let mut out = Vec::new();
        write_events(&mut out, &events, false).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("定例MTG"));
        assert!(!s.contains("abc1def2ghi3jkl4"), "ID が表示されてはいけない: {}", s);
    }

    #[test]
    fn test_format_date_weekday() {
        let date = NaiveDate::from_ymd_opt(2026, 2, 24).unwrap(); // 火曜
        let s = format_date(date);
        assert_eq!(s, "2026-02-24 (Tue)");
    }

    // --- write_new_event_dry_run のテスト ---

    fn local_dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> chrono::DateTime<Local> {
        use chrono::TimeZone;
        Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(h, min, 0).unwrap(),
            )
            .single()
            .unwrap()
    }

    fn base_new_event() -> NewEvent {
        NewEvent {
            summary: "チームMTG".to_string(),
            calendar_id: "primary".to_string(),
            start: local_dt(2026, 3, 20, 14, 0),
            end: local_dt(2026, 3, 20, 15, 0),
            recurrence: None,
            reminders: None,
            location: None,
        }
    }

    #[test]
    fn test_dry_run_new_event_shows_title_and_times() {
        let event = base_new_event();
        let mut buf = Vec::new();
        write_new_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("チームMTG"), "タイトルが含まれない: {s}");
        assert!(s.contains("2026-03-20"), "日付が含まれない: {s}");
        assert!(s.contains("14:00"), "開始時刻が含まれない: {s}");
        assert!(s.contains("15:00"), "終了時刻が含まれない: {s}");
        assert!(s.contains("primary"), "カレンダーIDが含まれない: {s}");
    }

    #[test]
    fn test_dry_run_new_event_shows_location() {
        let event = NewEvent { location: Some("会議室A".to_string()), ..base_new_event() };
        let mut buf = Vec::new();
        write_new_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("会議室A"), "場所が含まれない: {s}");
    }

    #[test]
    fn test_dry_run_new_event_no_location_shows_placeholder() {
        let event = base_new_event(); // location = None
        let mut buf = Vec::new();
        write_new_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("(なし)"), "場所なしプレースホルダーが含まれない: {s}");
    }

    #[test]
    fn test_dry_run_new_event_shows_recurrence() {
        let event = NewEvent {
            recurrence: Some(vec!["RRULE:FREQ=WEEKLY".to_string()]),
            ..base_new_event()
        };
        let mut buf = Vec::new();
        write_new_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("RRULE:FREQ=WEEKLY"), "繰り返しが含まれない: {s}");
    }

    #[test]
    fn test_dry_run_new_event_has_header() {
        let event = base_new_event();
        let mut buf = Vec::new();
        write_new_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("[ドライラン]"), "ヘッダーが含まれない: {s}");
    }

    // --- write_update_event_dry_run のテスト ---

    fn base_update_event() -> UpdateEvent {
        UpdateEvent {
            event_id: "evt_123".to_string(),
            calendar_id: "primary".to_string(),
            title: Some("新タイトル".to_string()),
            start: Some(local_dt(2026, 3, 20, 14, 0)),
            end: Some(local_dt(2026, 3, 20, 15, 0)),
            recurrence: None,
            reminders: None,
            location: None,
        }
    }

    #[test]
    fn test_dry_run_update_event_shows_event_id() {
        let event = base_update_event();
        let mut buf = Vec::new();
        write_update_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("evt_123"), "イベントIDが含まれない: {s}");
    }

    #[test]
    fn test_dry_run_update_event_shows_title_and_times() {
        let event = base_update_event();
        let mut buf = Vec::new();
        write_update_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("新タイトル"), "タイトルが含まれない: {s}");
        assert!(s.contains("14:00"), "開始時刻が含まれない: {s}");
        assert!(s.contains("15:00"), "終了時刻が含まれない: {s}");
    }

    #[test]
    fn test_dry_run_update_event_none_fields_show_unchanged() {
        // title=None → "(変更なし)" 表示
        let event = UpdateEvent { title: None, start: None, end: None, ..base_update_event() };
        let mut buf = Vec::new();
        write_update_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("(変更なし)"), "未変更プレースホルダーが含まれない: {s}");
    }
}
