use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use crate::ai::types::AiEventParameters;
use crate::error::GcalError;
use crate::domain::{NewEvent, UpdateEvent};
use crate::parser::{parse_datetime_expr, parse_datetime_range_expr, parse_end_expr, resolve_event_range};
use crate::parser::{parse_recurrence, parse_reminders};

pub struct CliMapper;

impl CliMapper {
    pub fn map_add_command(
        title: Option<String>,
        date: Option<String>,
        start: Option<String>,
        end: Option<String>,
        calendar: String,
        repeat: Option<String>,
        every: Option<u32>,
        on: Option<String>,
        until: Option<String>,
        count: Option<u32>,
        recur: Option<Vec<String>>,
        reminder: Option<Vec<String>>,
        reminders: Option<String>,
        location: Option<String>,
        today: NaiveDate,
        ai_params: Option<AiEventParameters>,
    ) -> Result<NewEvent, GcalError> {
        // title: CLI が優先、なければ AI から取得
        let effective_title = title
            .or_else(|| ai_params.as_ref().and_then(|p| p.title.clone()))
            .ok_or_else(|| {
                GcalError::ConfigError(
                    "タイトルを指定してください（--title または --ai）".to_string(),
                )
            })?;

        // location: CLI が優先、なければ AI から取得
        let effective_location = location
            .or_else(|| ai_params.as_ref().and_then(|p| p.location.clone()));

        // 時刻解決: CLI --date > CLI --start > AI の date+start を合成
        let (start_dt, end_dt) = if let Some(d) = date {
            parse_datetime_range_expr(&d, today)?
        } else {
            let start_str = start.or_else(|| {
                ai_params.as_ref().and_then(|p| {
                    match (&p.date, &p.start) {
                        (Some(d), Some(t)) => Some(format!("{d} {t}")),
                        _ => None,
                    }
                })
            }).ok_or_else(|| {
                GcalError::ConfigError(
                    "--date か --start（または --ai）で日時を指定してください".to_string(),
                )
            })?;

            let start_dt = parse_datetime_expr(&start_str, today)?;

            // end: CLI --end > AI の end > デフォルト +1h
            // AI の end が時刻のみ（HH:MM）の場合は開始日と合成する
            let end_str = end.or_else(|| ai_params.as_ref().and_then(|p| p.end.clone()));
            let end_dt = match end_str {
                Some(e) => parse_end_expr(&normalize_ai_end(&e, &start_dt), start_dt, today)?,
                None => start_dt + Duration::hours(1),
            };
            (start_dt, end_dt)
        };

        let recurrence_payload = parse_recurrence(
            repeat.as_deref(),
            every,
            on.as_deref(),
            until.as_deref(),
            count,
            recur,
        )?;
        // reminders: CLI --reminder/--reminders が優先。
        // AI を使用した場合は AI の reminder または デフォルト popup:10m を適用。
        // AI なし・CLI reminder なし → None（カレンダーデフォルト）
        let reminders_payload = if reminder.is_some() || reminders.is_some() {
            parse_reminders(reminder, reminders.as_deref())?
        } else if let Some(ref ai) = ai_params {
            let ai_reminder_str = ai.reminder.as_deref().unwrap_or("popup:10m");
            // AI がカンマ区切りで複数の reminder を返す場合があるため分割する
            let ai_reminders: Vec<String> = ai_reminder_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            parse_reminders(Some(ai_reminders), None)?
        } else {
            None
        };

        Ok(NewEvent {
            summary: effective_title,
            calendar_id: calendar,
            start: start_dt,
            end: end_dt,
            recurrence: recurrence_payload,
            reminders: reminders_payload,
            location: effective_location,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn map_update_command(
        event_id: String,
        title: Option<String>,
        date: Option<String>,
        start: Option<String>,
        end: Option<String>,
        calendar: String,
        clear_repeat: bool,
        clear_reminders: bool,
        clear_location: bool,
        repeat: Option<String>,
        every: Option<u32>,
        on: Option<String>,
        until: Option<String>,
        count: Option<u32>,
        recur: Option<Vec<String>>,
        reminder: Option<Vec<String>>,
        reminders: Option<String>,
        location: Option<String>,
        today: NaiveDate,
        ai_params: Option<AiEventParameters>,
    ) -> Result<UpdateEvent, GcalError> {
        // CLI か AI のいずれかで何か更新対象が必要
        let has_cli_update = title.is_some() || start.is_some() || date.is_some()
            || repeat.is_some() || recur.is_some() || reminder.is_some()
            || reminders.is_some() || location.is_some()
            || clear_repeat || clear_reminders || clear_location;
        if !has_cli_update && ai_params.is_none() {
            return Err(GcalError::ConfigError(
                "更新する項目 (--title / --start / --date / --location / --ai など) を指定してください".to_string(),
            ));
        }

        // title: CLI > AI
        let effective_title = title.or_else(|| ai_params.as_ref().and_then(|p| p.title.clone()));

        // 時刻解決: CLI --date > CLI --start > AI date+start
        let (start_dt, end_dt) = if let Some(d) = date {
            let (s, e) = parse_datetime_range_expr(&d, today)?;
            (Some(s), Some(e))
        } else if start.is_some() {
            match (start, end) {
                (Some(s), Some(e)) => {
                    let start_dt = parse_datetime_expr(&s, today)?;
                    let end_dt = parse_end_expr(&e, start_dt, today)?;
                    (Some(start_dt), Some(end_dt))
                }
                _ => (None, None),
            }
        } else if let Some(ref ai) = ai_params {
            match (&ai.date, &ai.start) {
                (Some(d), Some(t)) => {
                    let combined = format!("{d} {t}");
                    let start_dt = parse_datetime_expr(&combined, today)?;
                    let end_str = ai.end.as_deref();
                    let end_dt = match end_str {
                        Some(e) => parse_end_expr(&normalize_ai_end(e, &start_dt), start_dt, today)?,
                        None => start_dt + Duration::hours(1),
                    };
                    (Some(start_dt), Some(end_dt))
                }
                _ => (None, None),
            }
        } else {
            (None, None)
        };

        let mut recurrence_payload = parse_recurrence(
            repeat.as_deref(),
            every,
            on.as_deref(),
            until.as_deref(),
            count,
            recur,
        )?;
        if clear_repeat {
            recurrence_payload = Some(vec![]);
        }

        let mut reminders_payload = parse_reminders(
            reminder,
            reminders.as_deref(),
        )?;
        if clear_reminders {
            reminders_payload = Some(crate::gcal_api::models::EventReminders {
                use_default: false,
                overrides: Some(vec![]),
            });
        }

        // location: clear_location > CLI > AI
        let effective_location = if clear_location {
            Some(String::new())
        } else {
            location.or_else(|| ai_params.as_ref().and_then(|p| p.location.clone()))
        };

        Ok(UpdateEvent {
            event_id,
            calendar_id: calendar,
            title: effective_title,
            start: start_dt,
            end: end_dt,
            recurrence: recurrence_payload,
            reminders: reminders_payload,
            location: effective_location,
        })
    }

    pub fn map_events_command(
        date: Option<String>,
        from: Option<String>,
        to: Option<String>,
        days: Option<u64>,
        today: NaiveDate,
    ) -> Result<(DateTime<Utc>, DateTime<Utc>), GcalError> {
        let range = resolve_event_range(
            date.as_deref(),
            from.as_deref(),
            to.as_deref(),
            days,
            today,
        )?;

        let time_min = naive_date_to_utc_start(range.from)?;
        let time_max = naive_date_to_utc_end(range.to)?;

        Ok((time_min, time_max))
    }
}

/// AI の end フィールドを正規化する
///
/// - `+1h` などの相対指定 → そのまま（`parse_end_expr` が処理）
/// - スペースを含む日時文字列 → そのまま（`parse_datetime_expr` が処理）
/// - `HH:MM` 形式（時刻のみ）→ 開始日と合成して `"YYYY/MM/DD HH:MM"` に変換
fn normalize_ai_end(end: &str, start: &DateTime<Local>) -> String {
    if end.starts_with('+') || end.contains(' ') {
        return end.to_string();
    }
    if end.contains(':') {
        let date_str = start.format("%Y/%m/%d").to_string();
        return format!("{date_str} {end}");
    }
    end.to_string()
}

pub fn naive_date_to_utc_start(date: NaiveDate) -> Result<DateTime<Utc>, GcalError> {
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("0:00:00 は常に有効"))
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(|| GcalError::ConfigError("ローカル時刻の変換に失敗しました".to_string()))
}

pub fn naive_date_to_utc_end(date: NaiveDate) -> Result<DateTime<Utc>, GcalError> {
    Local
        .from_local_datetime(&date.and_hms_opt(23, 59, 59).expect("23:59:59 は常に有効"))
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(|| GcalError::ConfigError("ローカル時刻の変換に失敗しました".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use crate::ai::types::AiEventParameters;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 2, 24).unwrap()
    }

    // --- map_add_command: リグレッションテスト ---

    #[test]
    fn test_map_add_command_all_args() {
        let event = CliMapper::map_add_command(
            Some("Test Event".to_string()),
            Some("2026/05/10 10:00-11:00".to_string()),
            None,
            None,
            "primary".to_string(),
            Some("weekly".to_string()),
            Some(2),
            Some("mon,wed".to_string()),
            None,
            Some(5),
            None,
            Some(vec!["popup:10m".to_string()]),
            None,
            Some("Tokyo Tower".to_string()),
            today(),
            None,
        ).unwrap();

        assert_eq!(event.summary, "Test Event");
        assert_eq!(event.calendar_id, "primary");
        assert_eq!(event.start.format("%Y-%m-%d %H:%M").to_string(), "2026-05-10 10:00");
        assert_eq!(event.end.format("%Y-%m-%d %H:%M").to_string(), "2026-05-10 11:00");
        assert_eq!(event.recurrence, Some(vec!["RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=5".to_string()]));
        assert_eq!(event.reminders.unwrap().overrides.unwrap().len(), 1);
        assert_eq!(event.location.unwrap(), "Tokyo Tower");
    }

    // --- map_add_command: title マージテスト ---

    #[test]
    fn test_map_add_no_title_no_ai_returns_error() {
        // title=None、AI なし → エラー
        let result = CliMapper::map_add_command(
            None,
            Some("2026/3/20 10:00-11:00".to_string()),
            None, None,
            "primary".to_string(),
            None, None, None, None, None, None, None, None, None,
            today(),
            None,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("タイトル"), "エラーメッセージにタイトルが含まれていません: {msg}");
    }

    #[test]
    fn test_map_add_title_from_ai() {
        // title=None、AI が title を提供 → AI の title を使用
        let ai = AiEventParameters {
            title: Some("AI MTG".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("10:00".to_string()),
            end: Some("11:00".to_string()),
            location: None,
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            None, None, None, None,
            "primary".to_string(),
            None, None, None, None, None, None, None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        assert_eq!(event.summary, "AI MTG");
    }

    #[test]
    fn test_map_add_cli_title_overrides_ai_title() {
        // CLI title と AI title が両方ある → CLI が優先
        let ai = AiEventParameters {
            title: Some("AI title".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("10:00".to_string()),
            end: None,
            location: None,
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            Some("CLI title".to_string()),
            None, None, None,
            "primary".to_string(),
            None, None, None, None, None, None, None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        assert_eq!(event.summary, "CLI title");
    }

    // --- map_add_command: 時刻マージテスト ---

    #[test]
    fn test_map_add_time_from_ai_date_and_start() {
        // CLI に --date/--start なし、AI が date + start を提供 → 合成して使用
        let ai = AiEventParameters {
            title: Some("朝会".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("9:00".to_string()),
            end: Some("9:30".to_string()),
            location: None,
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            None, None, None, None,
            "primary".to_string(),
            None, None, None, None, None, None, None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        assert_eq!(event.start.format("%Y-%m-%d %H:%M").to_string(), "2026-03-20 09:00");
        assert_eq!(event.end.format("%Y-%m-%d %H:%M").to_string(), "2026-03-20 09:30");
    }

    #[test]
    fn test_map_add_ai_time_default_end_1h() {
        // AI が end を持たない → デフォルト +1h
        let ai = AiEventParameters {
            title: Some("会議".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("14:00".to_string()),
            end: None,
            location: None,
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            None, None, None, None,
            "primary".to_string(),
            None, None, None, None, None, None, None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        assert_eq!(event.start.format("%H:%M").to_string(), "14:00");
        assert_eq!(event.end.format("%H:%M").to_string(), "15:00");
    }

    #[test]
    fn test_map_add_cli_start_overrides_ai_time() {
        // CLI --start がある → AI の時刻情報は無視
        let ai = AiEventParameters {
            title: Some("会議".to_string()),
            date: Some("2026/3/19".to_string()),
            start: Some("10:00".to_string()),
            end: None,
            location: None,
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            None,
            None,
            Some("2026/3/20 14:00".to_string()), // CLI --start が優先
            None,
            "primary".to_string(),
            None, None, None, None, None, None, None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        assert_eq!(event.start.format("%Y-%m-%d %H:%M").to_string(), "2026-03-20 14:00");
    }

    #[test]
    fn test_map_add_ai_no_date_no_start_returns_error() {
        // AI に title はあるが date/start がない → エラー
        let ai = AiEventParameters {
            title: Some("会議".to_string()),
            date: None,
            start: None,
            end: None,
            location: None,
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let result = CliMapper::map_add_command(
            None, None, None, None,
            "primary".to_string(),
            None, None, None, None, None, None, None, None, None,
            today(),
            Some(ai),
        );
        assert!(result.is_err());
    }

    // --- map_add_command: location マージテスト ---

    #[test]
    fn test_map_add_location_from_ai() {
        // CLI --location なし、AI が location を提供
        let ai = AiEventParameters {
            title: Some("会議".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("10:00".to_string()),
            end: None,
            location: Some("会議室A".to_string()),
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            None, None, None, None,
            "primary".to_string(),
            None, None, None, None, None, None, None, None,
            None, // CLI location なし
            today(),
            Some(ai),
        ).unwrap();
        assert_eq!(event.location.as_deref(), Some("会議室A"));
    }

    #[test]
    fn test_map_add_cli_location_overrides_ai() {
        // CLI --location が AI の location を上書き
        let ai = AiEventParameters {
            title: Some("会議".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("10:00".to_string()),
            end: None,
            location: Some("AI 場所".to_string()),
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            None, None, None, None,
            "primary".to_string(),
            None, None, None, None, None, None, None, None,
            Some("CLI 場所".to_string()), // CLI が優先
            today(),
            Some(ai),
        ).unwrap();
        assert_eq!(event.location.as_deref(), Some("CLI 場所"));
    }

    // --- normalize_ai_end のテスト ---

    #[test]
    fn test_normalize_ai_end_relative_unchanged() {
        // "+1h" はそのまま
        let start = Local.from_local_datetime(
            &NaiveDate::from_ymd_opt(2026, 3, 20).unwrap().and_hms_opt(10, 0, 0).unwrap()
        ).single().unwrap();
        assert_eq!(normalize_ai_end("+1h", &start), "+1h");
    }

    #[test]
    fn test_normalize_ai_end_time_only_combined_with_start_date() {
        // "11:00" → "2026/03/20 11:00"
        let start = Local.from_local_datetime(
            &NaiveDate::from_ymd_opt(2026, 3, 20).unwrap().and_hms_opt(10, 0, 0).unwrap()
        ).single().unwrap();
        assert_eq!(normalize_ai_end("11:00", &start), "2026/03/20 11:00");
    }

    #[test]
    fn test_normalize_ai_end_full_datetime_unchanged() {
        // スペース付きの日時はそのまま
        let start = Local.from_local_datetime(
            &NaiveDate::from_ymd_opt(2026, 3, 20).unwrap().and_hms_opt(10, 0, 0).unwrap()
        ).single().unwrap();
        assert_eq!(normalize_ai_end("明日 15:00", &start), "明日 15:00");
    }

    // --- map_update_command: リグレッションテスト ---

    #[test]
    fn test_map_update_command_clear_flags() {
        let event = CliMapper::map_update_command(
            "event_123".to_string(),
            None, None, None, None, "primary".to_string(),
            true, true, true, None, None, None, None, None, None, None, None, None,
            today(),
            None,
        ).unwrap();

        assert_eq!(event.event_id, "event_123");
        assert_eq!(event.title, None);
        assert_eq!(event.recurrence, Some(vec![]));
        assert!(!event.reminders.unwrap().use_default);
        assert_eq!(event.location.unwrap(), "");
    }

    // --- map_update_command: AI マージテスト ---

    #[test]
    fn test_map_update_ai_provides_title() {
        // AI がタイトルを提供 → タイトルが更新される
        let ai = AiEventParameters {
            title: Some("AI更新タイトル".to_string()),
            date: None, start: None, end: None, location: None, repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_update_command(
            "evt_1".to_string(),
            None, None, None, None, "primary".to_string(),
            false, false, false, None, None, None, None, None, None, None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        assert_eq!(event.title.as_deref(), Some("AI更新タイトル"));
    }

    #[test]
    fn test_map_update_cli_title_overrides_ai_title() {
        // CLI の --title が AI のタイトルより優先
        let ai = AiEventParameters {
            title: Some("AI title".to_string()),
            date: None, start: None, end: None, location: None, repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_update_command(
            "evt_1".to_string(),
            Some("CLI title".to_string()),
            None, None, None, "primary".to_string(),
            false, false, false, None, None, None, None, None, None, None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        assert_eq!(event.title.as_deref(), Some("CLI title"));
    }

    #[test]
    fn test_map_update_ai_provides_time() {
        // AI が日時を提供 → start/end が更新される
        let ai = AiEventParameters {
            title: Some("朝会".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("9:00".to_string()),
            end: Some("9:30".to_string()),
            location: None,
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_update_command(
            "evt_1".to_string(),
            None, None, None, None, "primary".to_string(),
            false, false, false, None, None, None, None, None, None, None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        let start = event.start.unwrap();
        let end = event.end.unwrap();
        assert_eq!(start.format("%Y-%m-%d %H:%M").to_string(), "2026-03-20 09:00");
        assert_eq!(end.format("%Y-%m-%d %H:%M").to_string(), "2026-03-20 09:30");
    }

    #[test]
    fn test_map_update_ai_location() {
        // AI が場所を提供
        let ai = AiEventParameters {
            title: Some("ミーティング".to_string()),
            date: None, start: None, end: None,
            location: Some("AI会議室".to_string()),
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_update_command(
            "evt_1".to_string(),
            None, None, None, None, "primary".to_string(),
            false, false, false, None, None, None, None, None, None, None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        assert_eq!(event.location.as_deref(), Some("AI会議室"));
    }

    // --- map_add_command: AI 通知マージテスト ---

    #[test]
    fn test_map_add_ai_reminder_used_when_no_cli_reminder() {
        // CLI に --reminder なし、AI が "popup:10m" を指定 → AI の通知を使用
        let ai = AiEventParameters {
            title: Some("MTG".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("10:00".to_string()),
            end: None,
            location: None,
            repeat_rule: None,
            reminder: Some("popup:10m".to_string()),
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            None, None, None, None, "primary".to_string(),
            None, None, None, None, None, None,
            None, None, None, // reminder, reminders, location
            today(),
            Some(ai),
        ).unwrap();
        let rem = event.reminders.unwrap();
        assert!(!rem.use_default);
        let overrides = rem.overrides.unwrap();
        assert_eq!(overrides[0].method, "popup");
        assert_eq!(overrides[0].minutes, 10);
    }

    #[test]
    fn test_map_add_ai_no_reminder_defaults_to_popup_10m() {
        // AI を使用、reminder フィールドなし → デフォルト popup:10m
        let ai = AiEventParameters {
            title: Some("MTG".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("10:00".to_string()),
            end: None,
            location: None,
            repeat_rule: None,
            reminder: None, // AI が通知を抽出しなかった
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            None, None, None, None, "primary".to_string(),
            None, None, None, None, None, None,
            None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        let rem = event.reminders.unwrap();
        assert!(!rem.use_default);
        let overrides = rem.overrides.unwrap();
        assert_eq!(overrides[0].method, "popup");
        assert_eq!(overrides[0].minutes, 10);
    }

    #[test]
    fn test_map_add_cli_reminder_overrides_ai_reminder() {
        // CLI --reminder が AI の通知より優先
        let ai = AiEventParameters {
            title: Some("MTG".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("10:00".to_string()),
            end: None,
            location: None,
            repeat_rule: None,
            reminder: Some("email:1h".to_string()),
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            None, None, None, None, "primary".to_string(),
            None, None, None, None, None, None,
            Some(vec!["popup:30m".to_string()]), // CLI --reminder
            None, None,
            today(),
            Some(ai),
        ).unwrap();
        let overrides = event.reminders.unwrap().overrides.unwrap();
        assert_eq!(overrides[0].method, "popup");
        assert_eq!(overrides[0].minutes, 30);
    }

    #[test]
    fn test_map_add_no_ai_no_reminder_is_none() {
        // AI なし・CLI reminder なし → None（カレンダーデフォルト）
        let event = CliMapper::map_add_command(
            Some("MTG".to_string()),
            Some("2026/3/20 10:00-11:00".to_string()),
            None, None, "primary".to_string(),
            None, None, None, None, None, None,
            None, None, None,
            today(),
            None,
        ).unwrap();
        assert!(event.reminders.is_none());
    }

    #[test]
    fn test_map_add_ai_multiple_reminders_comma_separated() {
        // AI がカンマ区切りで複数 reminder を返した場合、全て解析される
        let ai = AiEventParameters {
            title: Some("MTG".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("15:00".to_string()),
            end: None,
            location: None,
            repeat_rule: None,
            reminder: Some("popup:15m, popup:120m".to_string()),
            calendar: None,
        };
        let event = CliMapper::map_add_command(
            None, None, None, None, "primary".to_string(),
            None, None, None, None, None, None,
            None, None, None,
            today(),
            Some(ai),
        ).unwrap();
        let overrides = event.reminders.unwrap().overrides.unwrap();
        assert_eq!(overrides.len(), 2);
        assert_eq!(overrides[0].method, "popup");
        assert_eq!(overrides[0].minutes, 15);
        assert_eq!(overrides[1].method, "popup");
        assert_eq!(overrides[1].minutes, 120);
    }

    #[test]
    fn test_map_update_no_fields_no_ai_returns_error() {
        // 何も指定しない → エラー
        let result = CliMapper::map_update_command(
            "evt_1".to_string(),
            None, None, None, None, "primary".to_string(),
            false, false, false, None, None, None, None, None, None, None, None, None,
            today(),
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_map_events_command() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 24).unwrap();
        let (min, max) = CliMapper::map_events_command(
            None, Some("2026/3/1".to_string()), Some("2026/3/15".to_string()), None, today
        ).unwrap();
        let local_min = min.with_timezone(&Local);
        let local_max = max.with_timezone(&Local);
        assert_eq!(local_min.date_naive(), NaiveDate::from_ymd_opt(2026, 3, 1).unwrap());
        assert_eq!(local_max.date_naive(), NaiveDate::from_ymd_opt(2026, 3, 15).unwrap());
    }
}

