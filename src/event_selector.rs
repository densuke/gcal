use chrono::{Local, NaiveDate};

use crate::ai::types::AiEventTarget;
use crate::domain::{EventStart, EventSummary};
use crate::parser::parse_date_expr;

/// AiEventTarget でイベントリストをフィルタリングし、マッチしたインデックスを返す。
///
/// - `title_hint`: 大文字小文字を区別しない部分一致
/// - `date_hint`: "明日", "3/15" などの日付表現 → parse_date_expr で DateRange に変換
/// - いずれも None のときは全イベントがマッチする。
pub fn filter_by_target(
    events: &[EventSummary],
    target: &AiEventTarget,
    today: NaiveDate,
) -> Vec<usize> {
    let date_range = target.date_hint.as_deref().and_then(|hint| {
        parse_date_expr(hint, today).ok()
    });

    events
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            if let Some(hint) = &target.title_hint {
                let hint_lower = hint.to_lowercase();
                if !e.summary.to_lowercase().contains(&hint_lower) {
                    return false;
                }
            }
            if let Some(ref range) = date_range {
                let event_date = match &e.start {
                    EventStart::Date(d) => *d,
                    EventStart::DateTime(dt) => dt.with_timezone(&Local).date_naive(),
                };
                if event_date < range.from || event_date > range.to {
                    return false;
                }
            }
            true
        })
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone, Utc};

    use crate::domain::{EventStart, EventSummary};

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()
    }

    fn make_event(id: &str, summary: &str, date: NaiveDate) -> EventSummary {
        EventSummary {
            id: id.to_string(),
            summary: summary.to_string(),
            start: EventStart::Date(date),
            end: None,
            location: None,
        }
    }

    fn make_dt_event(id: &str, summary: &str, y: i32, m: u32, d: u32, h: u32) -> EventSummary {
        let dt = chrono::Local
            .with_ymd_and_hms(y, m, d, h, 0, 0)
            .unwrap()
            .with_timezone(&Utc);
        EventSummary {
            id: id.to_string(),
            summary: summary.to_string(),
            start: EventStart::DateTime(dt),
            end: None,
            location: None,
        }
    }

    fn target_title(hint: &str) -> AiEventTarget {
        AiEventTarget {
            title_hint: Some(hint.to_string()),
            date_hint: None,
            calendar: None,
        }
    }

    fn target_date(hint: &str) -> AiEventTarget {
        AiEventTarget {
            title_hint: None,
            date_hint: Some(hint.to_string()),
            calendar: None,
        }
    }

    fn target_both(title: &str, date: &str) -> AiEventTarget {
        AiEventTarget {
            title_hint: Some(title.to_string()),
            date_hint: Some(date.to_string()),
            calendar: None,
        }
    }

    fn target_none() -> AiEventTarget {
        AiEventTarget { title_hint: None, date_hint: None, calendar: None }
    }

    #[test]
    fn test_filter_title_hint_matches_substring() {
        let events = vec![
            make_event("1", "定例MTG", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            make_event("2", "ランチ", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        ];
        let result = filter_by_target(&events, &target_title("MTG"), today());
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_filter_title_hint_no_match() {
        let events = vec![
            make_event("1", "ランチ", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        ];
        let result = filter_by_target(&events, &target_title("MTG"), today());
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_title_hint_case_insensitive() {
        let events = vec![
            make_event("1", "Daily MTG", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        ];
        let result = filter_by_target(&events, &target_title("mtg"), today());
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_filter_date_hint_specific_date() {
        // "2026/3/11" → 3/11 のイベントのみ
        let events = vec![
            make_event("1", "朝会", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            make_event("2", "夕会", NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()),
        ];
        let result = filter_by_target(&events, &target_date("2026/3/11"), today());
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_filter_date_hint_today_relative() {
        // "今日" (today=2026/3/10) → 3/10 のイベント
        let events = vec![
            make_event("1", "朝会", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            make_event("2", "明日の会議", NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()),
        ];
        let result = filter_by_target(&events, &target_date("今日"), today());
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_filter_date_hint_tomorrow_relative() {
        // "明日" (today=2026/3/10) → 3/11 のイベント
        let events = vec![
            make_event("1", "朝会", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            make_event("2", "夕会", NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()),
        ];
        let result = filter_by_target(&events, &target_date("明日"), today());
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_filter_both_hints_and_logic() {
        // title "MTG" AND date "2026/3/11" → 両方満たすもの
        let events = vec![
            make_event("1", "定例MTG", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            make_event("2", "定例MTG", NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()),
            make_event("3", "ランチ", NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()),
        ];
        let result = filter_by_target(&events, &target_both("MTG", "2026/3/11"), today());
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_filter_no_hints_returns_all() {
        let events = vec![
            make_event("1", "朝会", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            make_event("2", "ランチ", NaiveDate::from_ymd_opt(2026, 3, 11).unwrap()),
        ];
        let result = filter_by_target(&events, &target_none(), today());
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn test_filter_datetime_event_matches_by_local_date() {
        // DateTime イベントもローカル日付でフィルタ
        let events = vec![
            make_dt_event("1", "朝会", 2026, 3, 10, 10),
            make_dt_event("2", "夕会", 2026, 3, 11, 18),
        ];
        let result = filter_by_target(&events, &target_date("2026/3/10"), today());
        assert_eq!(result, vec![0]);
    }

    #[test]
    fn test_filter_empty_events_returns_empty() {
        let result = filter_by_target(&[], &target_title("MTG"), today());
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_multiple_matches() {
        let events = vec![
            make_event("1", "朝のMTG", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            make_event("2", "夕のMTG", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
            make_event("3", "ランチ", NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()),
        ];
        let result = filter_by_target(&events, &target_title("MTG"), today());
        assert_eq!(result, vec![0, 1]);
    }
}
