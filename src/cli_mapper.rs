use crate::ai::types::AiEventParameters;
use crate::domain::{NewEvent, UpdateEvent};
use crate::error::GcalError;
use crate::parser::{
    parse_datetime_expr, parse_datetime_range_expr, parse_end_expr, resolve_event_range,
};
use crate::parser::{parse_recurrence, parse_reminders};
use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Timelike, Utc};

pub struct AddCommandInput {
    pub title: Option<String>,
    pub date: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub calendar: String,
    pub location: Option<String>,
    pub recurrence: crate::cli::RecurrenceArgs,
    pub reminder_args: crate::cli::ReminderArgs,
    pub today: NaiveDate,
    pub ai_params: Option<AiEventParameters>,
}

impl Default for AddCommandInput {
    fn default() -> Self {
        Self {
            title: None,
            date: None,
            start: None,
            end: None,
            calendar: "primary".to_string(),
            location: None,
            recurrence: Default::default(),
            reminder_args: Default::default(),
            today: NaiveDate::from_ymd_opt(2026, 2, 24).unwrap(),
            ai_params: None,
        }
    }
}

pub struct UpdateCommandInput {
    pub event_id: String,
    pub calendar: String,
    pub title: Option<String>,
    pub date: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub clear_repeat: bool,
    pub clear_reminders: bool,
    pub clear_location: bool,
    pub location: Option<String>,
    pub recurrence: crate::cli::RecurrenceArgs,
    pub reminder_args: crate::cli::ReminderArgs,
    pub today: NaiveDate,
    pub ai_params: Option<AiEventParameters>,
}

impl Default for UpdateCommandInput {
    fn default() -> Self {
        Self {
            event_id: "".to_string(),
            calendar: "primary".to_string(),
            title: None,
            date: None,
            start: None,
            end: None,
            clear_repeat: false,
            clear_reminders: false,
            clear_location: false,
            location: None,
            recurrence: Default::default(),
            reminder_args: Default::default(),
            today: NaiveDate::from_ymd_opt(2026, 2, 24).unwrap(),
            ai_params: None,
        }
    }
}

pub struct CliMapper;

impl CliMapper {
    pub fn map_add_command(input: AddCommandInput) -> Result<NewEvent, GcalError> {
        let AddCommandInput {
            title,
            date,
            start,
            end,
            calendar,
            location,
            recurrence,
            reminder_args,
            today,
            ai_params,
        } = input;
        let effective_title = title
            .or_else(|| ai_params.as_ref().and_then(|p| p.title.clone()))
            .ok_or_else(|| {
                GcalError::ConfigError(
                    "タイトルを指定してください（--title または --ai）".to_string(),
                )
            })?;

        let effective_location =
            location.or_else(|| ai_params.as_ref().and_then(|p| p.location.clone()));

        let (start_dt, end_dt) = if let Some(d) = date {
            parse_datetime_range_expr(&d, today)?
        } else {
            let start_str = start
                .or_else(|| {
                    ai_params.as_ref().and_then(|p| match (&p.date, &p.start) {
                        (Some(d), Some(t)) => Some(format!("{d} {t}")),
                        _ => None,
                    })
                })
                .ok_or_else(|| {
                    GcalError::ConfigError(
                        "--date か --start（または --ai）で日時を指定してください".to_string(),
                    )
                })?;

            let start_dt = parse_datetime_expr(&start_str, today)?;

            let end_str = end.or_else(|| ai_params.as_ref().and_then(|p| p.end.clone()));
            let end_dt = match end_str {
                Some(e) => parse_end_expr(&normalize_ai_end(&e, &start_dt), start_dt, today)?,
                None => start_dt + Duration::hours(1),
            };
            (start_dt, end_dt)
        };

        let recurrence_payload = parse_recurrence_args(recurrence, today)?;
        let reminders_payload =
            if reminder_args.reminder.is_some() || reminder_args.reminders.is_some() {
                parse_reminders(reminder_args.reminder, reminder_args.reminders.as_deref())?
            } else if let Some(ref ai) = ai_params {
                ai.reminder
                    .as_deref()
                    .map(|r| parse_ai_reminders(r, Some(start_dt)))
                    .transpose()?
                    .flatten()
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

    pub fn map_update_command(input: UpdateCommandInput) -> Result<UpdateEvent, GcalError> {
        let UpdateCommandInput {
            event_id,
            calendar,
            title,
            date,
            start,
            end,
            clear_repeat,
            clear_reminders,
            clear_location,
            location,
            recurrence,
            reminder_args,
            today,
            ai_params,
        } = input;
        // CLI か AI のいずれかで何か更新対象が必要
        let has_cli_update = title.is_some()
            || start.is_some()
            || date.is_some()
            || recurrence.repeat.is_some()
            || recurrence.recur.is_some()
            || reminder_args.reminder.is_some()
            || reminder_args.reminders.is_some()
            || location.is_some()
            || clear_repeat
            || clear_reminders
            || clear_location;
        if !has_cli_update && ai_params.is_none() {
            return Err(GcalError::ConfigError(
                "更新する項目 (--title / --start / --date / --location / --ai など) を指定してください".to_string(),
            ));
        }

        let effective_title = title.or_else(|| ai_params.as_ref().and_then(|p| p.title.clone()));

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
                    let end_dt = match &ai.end {
                        Some(e) => Some(parse_end_expr(
                            &normalize_ai_end(e, &start_dt),
                            start_dt,
                            today,
                        )?),
                        None => Some(start_dt + Duration::hours(1)),
                    };
                    (Some(start_dt), end_dt)
                }
                _ => (None, None),
            }
        } else {
            (None, None)
        };

        let recurrence_payload = if clear_repeat {
            Some(vec![])
        } else {
            parse_recurrence_args(recurrence, today)?
        };

        let reminders_payload = if clear_reminders {
            Some(crate::gcal_api::models::EventReminders {
                use_default: false,
                overrides: Some(vec![]),
            })
        } else if reminder_args.reminder.is_some() || reminder_args.reminders.is_some() {
            parse_reminders(reminder_args.reminder, reminder_args.reminders.as_deref())?
        } else if let Some(ref ai) = ai_params {
            ai.reminder
                .as_deref()
                .map(|r| parse_ai_reminders(r, start_dt))
                .transpose()?
                .flatten()
        } else {
            None
        };

        let effective_location = if clear_location {
            Some("".to_string())
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
        let range =
            resolve_event_range(date.as_deref(), from.as_deref(), to.as_deref(), days, today)?;

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

/// RecurrenceArgs の各フィールドを parse_recurrence に委譲するヘルパー
fn parse_recurrence_args(
    r: crate::cli::RecurrenceArgs,
    today: NaiveDate,
) -> Result<Option<Vec<String>>, GcalError> {
    parse_recurrence(
        r.repeat.as_deref(),
        r.every,
        r.on.as_deref(),
        r.until.as_deref(),
        r.count,
        r.recur,
        today,
    )
}

/// AI がカンマ区切りで返す reminder 文字列を EventReminders に変換するヘルパー。
/// "popup:prev-HH:MM" 形式は start を使って分数に変換する。
fn parse_ai_reminders(
    s: &str,
    start: Option<DateTime<Local>>,
) -> Result<Option<crate::gcal_api::models::EventReminders>, GcalError> {
    let resolved: Result<Vec<String>, GcalError> = s
        .split(',')
        .map(|r| r.trim().to_string())
        .filter(|r| !r.is_empty())
        .map(|item| resolve_ai_reminder_item(&item, start))
        .collect();
    parse_reminders(Some(resolved?), None)
}

/// "method:prev-HH:MM" 形式を "method:Xm"（開始時刻から X 分前）に変換する。
/// 非 prev 形式はそのまま返す。
fn resolve_ai_reminder_item(
    item: &str,
    start: Option<DateTime<Local>>,
) -> Result<String, GcalError> {
    if let Some((method, after_colon)) = item.split_once(':') {
        if let Some(time_str) = after_colon.strip_prefix("prev-") {
            let start_dt = start.ok_or_else(|| {
                GcalError::ConfigError("「前日HH時」リマインダーには開始日時が必要です".to_string())
            })?;
            if let Some((hh_str, mm_str)) = time_str.split_once(':') {
                let hh: u32 = hh_str
                    .parse()
                    .map_err(|_| GcalError::ConfigError(format!("無効な時刻指定: {}", time_str)))?;
                let mm: u32 = mm_str
                    .parse()
                    .map_err(|_| GcalError::ConfigError(format!("無効な時刻指定: {}", time_str)))?;
                let start_mins = start_dt.hour() * 60 + start_dt.minute();
                let prev_mins = hh * 60 + mm;
                let total = start_mins + (24 * 60 - prev_mins);
                return Ok(format!("{}:{}m", method, total));
            }
        }
    }
    Ok(item.to_string())
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
    use crate::ai::types::AiEventParameters;
    use chrono::NaiveDate;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 2, 24).unwrap()
    }

    fn make_add_input() -> AddCommandInput {
        AddCommandInput {
            calendar: "primary".to_string(),
            today: today(),
            ..Default::default()
        }
    }

    fn make_update_input() -> UpdateCommandInput {
        UpdateCommandInput {
            calendar: "primary".to_string(),
            today: today(),
            ..Default::default()
        }
    }

    // --- map_add_command: リグレッションテスト ---

    #[test]
    fn test_map_add_command_all_args() {
        let event = CliMapper::map_add_command(AddCommandInput {
            title: Some("Test Event".to_string()),
            date: Some("2026/05/10 10:00-11:00".to_string()),
            location: Some("Tokyo Tower".to_string()),
            recurrence: crate::cli::RecurrenceArgs {
                repeat: Some("weekly".to_string()),
                every: Some(2),
                on: Some("mon,wed".to_string()),
                until: None,
                count: Some(5),
                recur: None,
            },
            reminder_args: crate::cli::ReminderArgs {
                reminder: Some(vec!["popup:10m".to_string()]),
                reminders: None,
            },
            ..make_add_input()
        })
        .unwrap();

        assert_eq!(event.summary, "Test Event");
        assert_eq!(event.calendar_id, "primary");
        assert_eq!(
            event.start.format("%Y-%m-%d %H:%M").to_string(),
            "2026-05-10 10:00"
        );
        assert_eq!(
            event.end.format("%Y-%m-%d %H:%M").to_string(),
            "2026-05-10 11:00"
        );
        assert_eq!(
            event.recurrence,
            Some(vec![
                "RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=5".to_string()
            ])
        );
        assert_eq!(event.reminders.unwrap().overrides.unwrap().len(), 1);
        assert_eq!(event.location.unwrap(), "Tokyo Tower");
    }

    // --- map_add_command: title マージテスト ---

    #[test]
    fn test_map_add_no_title_no_ai_returns_error() {
        // title=None、AI なし → エラー
        let result = CliMapper::map_add_command(AddCommandInput {
            date: Some("2026/3/20 10:00-11:00".to_string()),
            ..make_add_input()
        });
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("タイトル"),
            "エラーメッセージにタイトルが含まれていません: {msg}"
        );
    }

    #[test]
    fn test_map_add_title_from_ai() {
        // title=None、AI が title を提供 → AI の title を使用
        let ai = AiEventParameters {
            title: Some("AI MTG".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("10:00".to_string()),
            end: Some("11:00".to_string()),
            ..Default::default()
        };
        let event = CliMapper::map_add_command(AddCommandInput {
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
        assert_eq!(event.summary, "AI MTG");
    }

    #[test]
    fn test_map_add_cli_title_overrides_ai_title() {
        // CLI title と AI title が両方ある → CLI が優先
        let ai = AiEventParameters {
            title: Some("AI title".to_string()),
            date: Some("2026/3/20".to_string()),
            start: Some("10:00".to_string()),
            ..Default::default()
        };
        let event = CliMapper::map_add_command(AddCommandInput {
            title: Some("CLI title".to_string()),
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
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
        let event = CliMapper::map_add_command(AddCommandInput {
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
        assert_eq!(
            event.start.format("%Y-%m-%d %H:%M").to_string(),
            "2026-03-20 09:00"
        );
        assert_eq!(
            event.end.format("%Y-%m-%d %H:%M").to_string(),
            "2026-03-20 09:30"
        );
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
        let event = CliMapper::map_add_command(AddCommandInput {
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
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
        let event = CliMapper::map_add_command(AddCommandInput {
            start: Some("2026/3/20 14:00".to_string()),
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
        assert_eq!(
            event.start.format("%Y-%m-%d %H:%M").to_string(),
            "2026-03-20 14:00"
        );
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
        let result = CliMapper::map_add_command(AddCommandInput {
            ai_params: Some(ai),
            ..make_add_input()
        });
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
        let event = CliMapper::map_add_command(AddCommandInput {
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
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
        let event = CliMapper::map_add_command(AddCommandInput {
            location: Some("CLI 場所".to_string()),
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
        assert_eq!(event.location.as_deref(), Some("CLI 場所"));
    }

    // --- normalize_ai_end のテスト ---

    #[test]
    fn test_normalize_ai_end_relative_unchanged() {
        // "+1h" はそのまま
        let start = Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2026, 3, 20)
                    .unwrap()
                    .and_hms_opt(10, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap();
        assert_eq!(normalize_ai_end("+1h", &start), "+1h");
    }

    #[test]
    fn test_normalize_ai_end_time_only_combined_with_start_date() {
        // "11:00" → "2026/03/20 11:00"
        let start = Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2026, 3, 20)
                    .unwrap()
                    .and_hms_opt(10, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap();
        assert_eq!(normalize_ai_end("11:00", &start), "2026/03/20 11:00");
    }

    #[test]
    fn test_normalize_ai_end_full_datetime_unchanged() {
        // スペース付きの日時はそのまま
        let start = Local
            .from_local_datetime(
                &NaiveDate::from_ymd_opt(2026, 3, 20)
                    .unwrap()
                    .and_hms_opt(10, 0, 0)
                    .unwrap(),
            )
            .single()
            .unwrap();
        assert_eq!(normalize_ai_end("明日 15:00", &start), "明日 15:00");
    }

    // --- map_update_command: リグレッションテスト ---

    #[test]
    fn test_map_update_command_clear_flags() {
        let event = CliMapper::map_update_command(UpdateCommandInput {
            event_id: "event_123".to_string(),
            clear_repeat: true,
            clear_reminders: true,
            clear_location: true,
            ..make_update_input()
        })
        .unwrap();

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
            date: None,
            start: None,
            end: None,
            location: None,
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_update_command(UpdateCommandInput {
            event_id: "evt_1".to_string(),
            ai_params: Some(ai),
            ..make_update_input()
        })
        .unwrap();
        assert_eq!(event.title.as_deref(), Some("AI更新タイトル"));
    }

    #[test]
    fn test_map_update_cli_title_overrides_ai_title() {
        // CLI の --title が AI のタイトルより優先
        let ai = AiEventParameters {
            title: Some("AI title".to_string()),
            date: None,
            start: None,
            end: None,
            location: None,
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_update_command(UpdateCommandInput {
            event_id: "evt_1".to_string(),
            title: Some("CLI title".to_string()),
            ai_params: Some(ai),
            ..make_update_input()
        })
        .unwrap();
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
        let event = CliMapper::map_update_command(UpdateCommandInput {
            event_id: "evt_1".to_string(),
            ai_params: Some(ai),
            ..make_update_input()
        })
        .unwrap();
        let start = event.start.unwrap();
        let end = event.end.unwrap();
        assert_eq!(
            start.format("%Y-%m-%d %H:%M").to_string(),
            "2026-03-20 09:00"
        );
        assert_eq!(end.format("%Y-%m-%d %H:%M").to_string(), "2026-03-20 09:30");
    }

    #[test]
    fn test_map_update_ai_location() {
        // AI が場所を提供
        let ai = AiEventParameters {
            title: Some("ミーティング".to_string()),
            date: None,
            start: None,
            end: None,
            location: Some("AI会議室".to_string()),
            repeat_rule: None,
            reminder: None,
            calendar: None,
        };
        let event = CliMapper::map_update_command(UpdateCommandInput {
            event_id: "evt_1".to_string(),
            ai_params: Some(ai),
            ..make_update_input()
        })
        .unwrap();
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
        let event = CliMapper::map_add_command(AddCommandInput {
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
        let rem = event.reminders.unwrap();
        assert!(!rem.use_default);
        let overrides = rem.overrides.unwrap();
        assert_eq!(overrides[0].method, "popup");
        assert_eq!(overrides[0].minutes, 10);
    }

    #[test]
    fn test_map_add_ai_no_reminder_is_none() {
        // AI を使用、reminder フィールドなし → None（カレンダーデフォルトに委ねる）
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
        let event = CliMapper::map_add_command(AddCommandInput {
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
        assert!(event.reminders.is_none());
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
        let event = CliMapper::map_add_command(AddCommandInput {
            reminder_args: crate::cli::ReminderArgs {
                reminder: Some(vec!["popup:30m".to_string()]),
                reminders: None,
            },
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
        let overrides = event.reminders.unwrap().overrides.unwrap();
        assert_eq!(overrides[0].method, "popup");
        assert_eq!(overrides[0].minutes, 30);
    }

    #[test]
    fn test_map_add_no_ai_no_reminder_is_none() {
        // AI なし・CLI reminder なし → None（カレンダーデフォルト）
        let event = CliMapper::map_add_command(AddCommandInput {
            title: Some("MTG".to_string()),
            date: Some("2026/3/20 10:00-11:00".to_string()),
            ..make_add_input()
        })
        .unwrap();
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
        let event = CliMapper::map_add_command(AddCommandInput {
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
        let overrides = event.reminders.unwrap().overrides.unwrap();
        assert_eq!(overrides.len(), 2);
        assert_eq!(overrides[0].method, "popup");
        assert_eq!(overrides[0].minutes, 15);
        assert_eq!(overrides[1].method, "popup");
        assert_eq!(overrides[1].minutes, 120);
    }

    #[test]
    fn test_resolve_ai_reminder_item_prev_format_08_30() {
        // event 08:30, 前日19時 → (8*60+30)+(24-19)*60 = 510+300 = 810m
        use chrono::TimeZone;
        let start = chrono::Local
            .with_ymd_and_hms(2026, 3, 20, 8, 30, 0)
            .unwrap();
        let result = resolve_ai_reminder_item("popup:prev-19:00", Some(start)).unwrap();
        assert_eq!(result, "popup:810m");
    }

    #[test]
    fn test_resolve_ai_reminder_item_prev_format_10_00() {
        // event 10:00, 前日17時 → (10*60+0)+(24-17)*60 = 600+420 = 1020m
        use chrono::TimeZone;
        let start = chrono::Local
            .with_ymd_and_hms(2026, 3, 1, 10, 0, 0)
            .unwrap();
        let result = resolve_ai_reminder_item("popup:prev-17:00", Some(start)).unwrap();
        assert_eq!(result, "popup:1020m");
    }

    #[test]
    fn test_resolve_ai_reminder_item_plain_passthrough() {
        // "popup:30m" は変換なしでそのまま返る
        let result = resolve_ai_reminder_item("popup:30m", None).unwrap();
        assert_eq!(result, "popup:30m");
    }

    #[test]
    fn test_resolve_ai_reminder_item_prev_without_start_returns_error() {
        // start が None のとき prev 形式はエラー
        let result = resolve_ai_reminder_item("popup:prev-19:00", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_map_add_ai_reminder_prev_day_format() {
        // "popup:prev-19:00" + start 08:30 → 810m, "popup:2h" → 120m
        let ai = AiEventParameters {
            title: Some("半日勤務(午後有休)".to_string()),
            date: Some("2026/2/27".to_string()),
            start: Some("08:30".to_string()),
            end: Some("+4h".to_string()),
            location: None,
            repeat_rule: None,
            reminder: Some("popup:prev-19:00,popup:2h".to_string()),
            calendar: None,
        };
        let event = CliMapper::map_add_command(AddCommandInput {
            ai_params: Some(ai),
            ..make_add_input()
        })
        .unwrap();
        let overrides = event.reminders.unwrap().overrides.unwrap();
        assert_eq!(overrides.len(), 2);
        assert_eq!(overrides[0].method, "popup");
        assert_eq!(overrides[0].minutes, 810);
        assert_eq!(overrides[1].method, "popup");
        assert_eq!(overrides[1].minutes, 120);
    }

    #[test]
    fn test_map_update_no_fields_no_ai_returns_error() {
        // 何も指定しない → エラー
        let result = CliMapper::map_update_command(UpdateCommandInput {
            event_id: "evt_1".to_string(),
            ..make_update_input()
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_map_events_command() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 24).unwrap();
        let (min, max) = CliMapper::map_events_command(
            None,
            Some("2026/3/1".to_string()),
            Some("2026/3/15".to_string()),
            None,
            today,
        )
        .unwrap();
        let local_min = min.with_timezone(&Local);
        let local_max = max.with_timezone(&Local);
        assert_eq!(
            local_min.date_naive(),
            NaiveDate::from_ymd_opt(2026, 3, 1).unwrap()
        );
        assert_eq!(
            local_max.date_naive(),
            NaiveDate::from_ymd_opt(2026, 3, 15).unwrap()
        );
    }
}
