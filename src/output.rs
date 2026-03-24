use std::io::Write;

use chrono::{DateTime, Datelike, Local, NaiveDate, Weekday};

use crate::domain::{CalendarSummary, EventStart, EventSummary, NewEvent, UpdateEvent};
use crate::error::GcalError;
use crate::gcal_api::models::EventReminders;

/// カレンダー一覧を書き出す
pub fn write_calendars<W: Write>(
    out: &mut W,
    calendars: &[CalendarSummary],
) -> Result<(), GcalError> {
    if calendars.is_empty() {
        writeln!(out, "カレンダーが見つかりません")?;
        return Ok(());
    }

    writeln!(out, "{:<40}  名前", "ID")?;
    writeln!(out, "{:-<40}  {:-<20}", "", "")?;

    for cal in calendars {
        let marker = if cal.primary { " *" } else { "" };
        writeln!(out, "{:<40}  {}{}", cal.id, cal.summary, marker)?;
    }

    Ok(())
}

/// イベント一覧を日付ごとにグループ化して書き出す
///
/// `show_ids` が true のとき、各行の末尾にイベント ID を表示する。
/// 本日のイベント一覧の場合、現在時刻マーカーと進行中イベントマーカーを表示する。
pub fn write_events<W: Write>(
    out: &mut W,
    events: &[EventSummary],
    show_ids: bool,
) -> Result<(), GcalError> {
    if events.is_empty() {
        writeln!(out, "イベントが見つかりません")?;
        return Ok(());
    }

    let now: DateTime<Local> = Local::now();
    let today = now.date_naive();

    // 本日に進行中のイベントがあるか先に確認（あれば現在時刻マーカーは表示しない）
    let any_running_today = events.iter().any(|e| {
        if let EventStart::DateTime(start_dt) = &e.start {
            let start_local: DateTime<Local> = DateTime::from(*start_dt);
            if start_local.date_naive() == today
                && let Some(EventStart::DateTime(end_dt)) = &e.end
            {
                let end_local: DateTime<Local> = DateTime::from(*end_dt);
                return start_local <= now && now < end_local;
            }
        }
        false
    });

    let mut current_date: Option<NaiveDate> = None;
    let mut current_time_marker_shown = false;

    for event in events {
        let (date, time_str) = match &event.start {
            EventStart::DateTime(dt) => {
                let start_local: DateTime<Local> = DateTime::from(*dt);
                let end_str = event.end.as_ref().and_then(|e| match e {
                    EventStart::DateTime(edt) => {
                        let end_local: DateTime<Local> = DateTime::from(*edt);
                        Some(end_local.format("%H:%M").to_string())
                    }
                    _ => None,
                });
                let time_str = if let Some(end) = end_str {
                    format!("{}-{}", start_local.format("%H:%M"), end)
                } else {
                    start_local.format("%H:%M").to_string()
                };
                (start_local.date_naive(), time_str)
            }
            EventStart::Date(d) => (*d, "終日".to_string()),
        };

        if current_date != Some(date) {
            if current_date.is_some() {
                writeln!(out)?;
            }
            writeln!(out, "{}", format_date(date))?;
            current_date = Some(date);
            current_time_marker_shown = false;
        }

        // 進行中のイベントがない場合のみ、最初の未来イベントの前に現在時刻マーカーを挿入
        if date == today
            && !current_time_marker_shown
            && !any_running_today
            && let EventStart::DateTime(dt) = &event.start
        {
            let start_local: DateTime<Local> = DateTime::from(*dt);
            if start_local > now {
                writeln!(out, "  —— 現在 ({}) ——", now.format("%H:%M"))?;
                current_time_marker_shown = true;
            }
        }

        // 現在進行中かどうか判定
        let is_running = if let EventStart::DateTime(dt) = &event.start {
            let start_local: DateTime<Local> = DateTime::from(*dt);
            if let Some(EventStart::DateTime(end_dt)) = &event.end {
                let end_local: DateTime<Local> = DateTime::from(*end_dt);
                start_local <= now && now < end_local
            } else {
                false
            }
        } else {
            false
        };

        let prefix = if is_running { "> " } else { "  " };
        if show_ids {
            writeln!(
                out,
                "{}{}  {:<40}  [{}]",
                prefix,
                pad_time_display(&time_str),
                event.summary,
                event.id
            )?;
        } else {
            writeln!(
                out,
                "{}{}  {}",
                prefix,
                pad_time_display(&time_str),
                event.summary
            )?;
        }
    }

    Ok(())
}

/// `--dry-run` 時に NewEvent の内容を人間が読める形式で出力する
pub fn write_new_event_dry_run<W: Write>(event: &NewEvent, out: &mut W) -> Result<(), GcalError> {
    writeln!(out, "[ドライラン] 以下の内容で登録されます:")?;
    writeln!(out, "  タイトル:   {}", event.summary)?;
    writeln!(
        out,
        "  開始:       {}",
        event.start.format("%Y-%m-%d %H:%M")
    )?;
    writeln!(out, "  終了:       {}", event.end.format("%Y-%m-%d %H:%M"))?;
    writeln!(
        out,
        "  場所:       {}",
        event.location.as_deref().unwrap_or("(なし)")
    )?;
    match event.recurrence.as_deref() {
        None | Some([]) => writeln!(out, "  繰り返し:   (なし)")?,
        Some(rules) => writeln!(out, "  繰り返し:   {}", rules.join(", "))?,
    }
    writeln!(out, "  通知:       {}", format_reminders(&event.reminders))?;
    let cal_display = event
        .calendar_display_name
        .as_ref()
        .unwrap_or(&event.calendar_id);
    writeln!(out, "  カレンダー: {}", cal_display)?;
    Ok(())
}

/// `--dry-run` 時に UpdateEvent の内容を人間が読める形式で出力する
pub fn write_update_event_dry_run<W: Write>(
    event: &UpdateEvent,
    out: &mut W,
) -> Result<(), GcalError> {
    writeln!(
        out,
        "[ドライラン] 以下の内容で更新されます (ID: {}):",
        event.event_id
    )?;
    writeln!(
        out,
        "  タイトル:   {}",
        event.title.as_deref().unwrap_or("(変更なし)")
    )?;
    match (&event.start, &event.end) {
        (Some(s), Some(e)) => {
            writeln!(out, "  開始:       {}", s.format("%Y-%m-%d %H:%M"))?;
            writeln!(out, "  終了:       {}", e.format("%Y-%m-%d %H:%M"))?;
        }
        _ => writeln!(out, "  開始/終了:  (変更なし)")?,
    }
    writeln!(
        out,
        "  場所:       {}",
        event.location.as_deref().unwrap_or("(変更なし)")
    )?;
    let reminders_str = match &event.reminders {
        None => "(変更なし)".to_string(),
        Some(r) => format_reminders(&Some(r.clone())),
    };
    writeln!(out, "  通知:       {}", reminders_str)?;
    let cal_display = event
        .calendar_display_name
        .as_ref()
        .unwrap_or(&event.calendar_id);
    writeln!(out, "  カレンダー: {}", cal_display)?;
    Ok(())
}

fn format_reminders(reminders: &Option<EventReminders>) -> String {
    match reminders {
        None => "(カレンダーデフォルト)".to_string(),
        Some(r) if r.use_default => "(カレンダーデフォルト)".to_string(),
        Some(r) => {
            let overrides = r.overrides.as_deref().unwrap_or(&[]);
            if overrides.is_empty() {
                return "(なし)".to_string();
            }
            overrides
                .iter()
                .map(|o| {
                    let method = match o.method.as_str() {
                        "popup" => "アプリ通知",
                        "email" => "メール通知",
                        other => other,
                    };
                    format!("{} {}分前", method, o.minutes)
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    }
}

/// 全角文字（CJK・Hiragana・Katakana 等）の表示幅を返す（全角=2, それ以外=1）
fn char_display_width(c: char) -> usize {
    match c as u32 {
        0x1100..=0x115F   // Hangul Jamo
        | 0x2329..=0x232A // Wide angle brackets
        | 0x2E80..=0x303E // CJK Radicals / Kangxi / Punctuation
        | 0x3041..=0x33BF // Hiragana, Katakana, Bopomofo
        | 0x3400..=0x4DBF // CJK Extension A
        | 0x4E00..=0x9FFF // CJK Unified Ideographs
        | 0xA000..=0xA4CF // Yi Syllables
        | 0xA960..=0xA97F // Hangul Jamo Extended-A
        | 0xAC00..=0xD7AF // Hangul Syllables
        | 0xF900..=0xFAFF // CJK Compatibility Ideographs
        | 0xFE10..=0xFE19 // Vertical Forms
        | 0xFE30..=0xFE4F // CJK Compatibility Forms
        | 0xFF01..=0xFF60 // Fullwidth Latin / Halfwidth Katakana
        | 0xFFE0..=0xFFE6 // Fullwidth Signs
        => 2,
        _ => 1,
    }
}

/// 時刻文字列をターミナル表示幅 11 になるようにスペースでパディングする。
/// "10:00-14:30" (11) → "10:00-14:30", "終日" (4) → "終日       "
fn pad_time_display(s: &str) -> String {
    const TARGET: usize = 11;
    let width: usize = s.chars().map(char_display_width).sum();
    let padding = TARGET.saturating_sub(width);
    format!("{}{}", s, " ".repeat(padding))
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

    fn make_event_dt(
        id: &str,
        summary: &str,
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        min: u32,
    ) -> EventSummary {
        let dt = Utc
            .with_ymd_and_hms(year, month, day, hour, min, 0)
            .unwrap();
        EventSummary {
            id: id.to_string(),
            summary: summary.to_string(),
            start: EventStart::DateTime(dt),
            end: None,
            location: None,
        }
    }

    fn make_event_allday(id: &str, summary: &str, year: i32, month: u32, day: u32) -> EventSummary {
        let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        EventSummary {
            id: id.to_string(),
            summary: summary.to_string(),
            start: EventStart::Date(date),
            end: None,
            location: None,
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
        let events = vec![make_event_dt(
            "abc1def2ghi3jkl4",
            "定例MTG",
            2026,
            2,
            24,
            1,
            0,
        )];
        let mut out = Vec::new();
        write_events(&mut out, &events, true).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("定例MTG"));
        assert!(
            s.contains("abc1def2ghi3jkl4"),
            "ID が表示されていない: {}",
            s
        );
    }

    #[test]
    fn test_write_events_no_ids_by_default() {
        let events = vec![make_event_dt(
            "abc1def2ghi3jkl4",
            "定例MTG",
            2026,
            2,
            24,
            1,
            0,
        )];
        let mut out = Vec::new();
        write_events(&mut out, &events, false).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("定例MTG"));
        assert!(
            !s.contains("abc1def2ghi3jkl4"),
            "ID が表示されてはいけない: {}",
            s
        );
    }

    // --- pad_time_display のテスト ---

    #[test]
    fn test_pad_time_display_range_11chars_no_padding() {
        // "10:00-14:30" は11文字 → パディングなし
        assert_eq!(pad_time_display("10:00-14:30"), "10:00-14:30");
    }

    #[test]
    fn test_pad_time_display_ascii_5chars_pads_6() {
        // "08:00" は5文字 → 表示幅11に合わせて6スペース補完
        assert_eq!(pad_time_display("08:00"), "08:00      ");
    }

    #[test]
    fn test_pad_time_display_allday_kanji_pads_7() {
        // "終日" は2全角文字 → 表示幅4 → 7スペースでパディング
        assert_eq!(pad_time_display("終日"), "終日       ");
    }

    #[test]
    fn test_write_events_allday_padding_consistent() {
        // 終日予定の行で "終日" が表示幅11にパディングされることを確認
        let events = vec![make_event_allday("1", "祝日", 2026, 2, 24)];
        let mut out = Vec::new();
        write_events(&mut out, &events, false).unwrap();
        let s = String::from_utf8(out).unwrap();
        let allday_line = s.lines().find(|l| l.contains("祝日")).unwrap();
        // "終日" (表示幅4) → 7スペースパディング → + 区切り2スペース = 合計9スペース
        assert!(
            allday_line.contains("終日         "),
            "パディングが正しくない: {:?}",
            allday_line
        );
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
                &NaiveDate::from_ymd_opt(y, m, d)
                    .unwrap()
                    .and_hms_opt(h, min, 0)
                    .unwrap(),
            )
            .single()
            .unwrap()
    }

    fn base_new_event() -> NewEvent {
        NewEvent {
            summary: "チームMTG".to_string(),
            calendar_id: "primary".to_string(),
            calendar_display_name: None,
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
        let event = NewEvent {
            location: Some("会議室A".to_string()),
            ..base_new_event()
        };
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
        assert!(
            s.contains("(なし)"),
            "場所なしプレースホルダーが含まれない: {s}"
        );
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

    // --- format_reminders のテスト ---

    use crate::gcal_api::models::EventReminderOverride;

    #[test]
    fn test_format_reminders_none_shows_calendar_default() {
        assert_eq!(format_reminders(&None), "(カレンダーデフォルト)");
    }

    #[test]
    fn test_format_reminders_use_default_shows_calendar_default() {
        let r = EventReminders {
            use_default: true,
            overrides: None,
        };
        assert_eq!(format_reminders(&Some(r)), "(カレンダーデフォルト)");
    }

    #[test]
    fn test_format_reminders_empty_overrides_shows_nashi() {
        let r = EventReminders {
            use_default: false,
            overrides: Some(vec![]),
        };
        assert_eq!(format_reminders(&Some(r)), "(なし)");
    }

    #[test]
    fn test_format_reminders_popup_10m() {
        let r = EventReminders {
            use_default: false,
            overrides: Some(vec![EventReminderOverride {
                method: "popup".to_string(),
                minutes: 10,
            }]),
        };
        assert_eq!(format_reminders(&Some(r)), "アプリ通知 10分前");
    }

    #[test]
    fn test_format_reminders_email_60m() {
        let r = EventReminders {
            use_default: false,
            overrides: Some(vec![EventReminderOverride {
                method: "email".to_string(),
                minutes: 60,
            }]),
        };
        assert_eq!(format_reminders(&Some(r)), "メール通知 60分前");
    }

    #[test]
    fn test_format_reminders_multiple() {
        let r = EventReminders {
            use_default: false,
            overrides: Some(vec![
                EventReminderOverride {
                    method: "popup".to_string(),
                    minutes: 10,
                },
                EventReminderOverride {
                    method: "email".to_string(),
                    minutes: 60,
                },
            ]),
        };
        let s = format_reminders(&Some(r));
        assert!(s.contains("アプリ通知 10分前"), "{s}");
        assert!(s.contains("メール通知 60分前"), "{s}");
    }

    // --- write_new_event_dry_run: 通知表示テスト ---

    #[test]
    fn test_dry_run_new_event_shows_reminders_none_as_calendar_default() {
        let event = base_new_event(); // reminders = None
        let mut buf = Vec::new();
        write_new_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(
            s.contains("カレンダーデフォルト"),
            "通知なし時のプレースホルダーが含まれない: {s}"
        );
    }

    #[test]
    fn test_dry_run_new_event_shows_popup_reminder() {
        let event = NewEvent {
            reminders: Some(EventReminders {
                use_default: false,
                overrides: Some(vec![EventReminderOverride {
                    method: "popup".to_string(),
                    minutes: 10,
                }]),
            }),
            ..base_new_event()
        };
        let mut buf = Vec::new();
        write_new_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("アプリ通知 10分前"), "通知内容が含まれない: {s}");
    }

    // --- write_update_event_dry_run: 通知表示テスト ---

    #[test]
    fn test_dry_run_update_event_shows_reminders() {
        let event = UpdateEvent {
            reminders: Some(EventReminders {
                use_default: false,
                overrides: Some(vec![EventReminderOverride {
                    method: "popup".to_string(),
                    minutes: 30,
                }]),
            }),
            ..base_update_event()
        };
        let mut buf = Vec::new();
        write_update_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("アプリ通知 30分前"), "通知内容が含まれない: {s}");
    }

    #[test]
    fn test_dry_run_update_event_shows_reminders_unchanged_when_none() {
        let event = base_update_event(); // reminders = None
        let mut buf = Vec::new();
        write_update_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("変更なし"), "通知変更なしが含まれない: {s}");
    }

    // --- write_update_event_dry_run のテスト ---

    fn base_update_event() -> UpdateEvent {
        UpdateEvent {
            event_id: "evt_123".to_string(),
            calendar_id: "primary".to_string(),
            calendar_display_name: None,
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
        let event = UpdateEvent {
            title: None,
            start: None,
            end: None,
            ..base_update_event()
        };
        let mut buf = Vec::new();
        write_update_event_dry_run(&event, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(
            s.contains("(変更なし)"),
            "未変更プレースホルダーが含まれない: {s}"
        );
    }
}
