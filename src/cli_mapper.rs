use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use crate::error::GcalError;
use crate::domain::{NewEvent, UpdateEvent};
use crate::parser::{parse_datetime_expr, parse_datetime_range_expr, parse_end_expr, resolve_event_range};
use crate::parser::{parse_recurrence, parse_reminders};

pub struct CliMapper;

impl CliMapper {
    pub fn map_add_command(
        title: String,
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
        today: NaiveDate,
    ) -> Result<NewEvent, GcalError> {
        let (start_dt, end_dt) = if let Some(d) = date {
            parse_datetime_range_expr(&d, today)?
        } else {
            let s = start.ok_or_else(|| {
                GcalError::ConfigError(
                    "--date か --start のいずれかを指定してください".to_string(),
                )
            })?;
            let start_dt = parse_datetime_expr(&s, today)?;
            let end_dt = match end {
                Some(e) => parse_end_expr(&e, start_dt, today)?,
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
        let reminders_payload = parse_reminders(
            reminder,
            reminders.as_deref(),
        )?;

        Ok(NewEvent {
            summary: title,
            calendar_id: calendar,
            start: start_dt,
            end: end_dt,
            recurrence: recurrence_payload,
            reminders: reminders_payload,
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
        today: NaiveDate,
    ) -> Result<UpdateEvent, GcalError> {
        if title.is_none() && start.is_none() && date.is_none() && repeat.is_none() && recur.is_none() && reminder.is_none() && reminders.is_none() && !clear_repeat && !clear_reminders && !clear_location {
            return Err(GcalError::ConfigError(
                "更新する項目 (--title / --start / --date / --repeat / --reminder など) を指定してください".to_string(),
            ));
        }

        let (start_dt, end_dt) = if let Some(d) = date {
            let (s, e) = parse_datetime_range_expr(&d, today)?;
            (Some(s), Some(e))
        } else {
            match (start, end) {
                (Some(s), Some(e)) => {
                    let start_dt = parse_datetime_expr(&s, today)?;
                    let end_dt = parse_end_expr(&e, start_dt, today)?;
                    (Some(start_dt), Some(end_dt))
                }
                _ => (None, None),
            }
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

        Ok(UpdateEvent {
            event_id,
            calendar_id: calendar,
            title,
            start: start_dt,
            end: end_dt,
            recurrence: recurrence_payload,
            reminders: reminders_payload,
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

    #[test]
    fn test_map_add_command_all_args() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 24).unwrap();
        let event = CliMapper::map_add_command(
            "Test Event".to_string(),
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
            today
        ).unwrap();

        assert_eq!(event.summary, "Test Event");
        assert_eq!(event.calendar_id, "primary");
        assert_eq!(event.start.format("%Y-%m-%d %H:%M").to_string(), "2026-05-10 10:00");
        assert_eq!(event.end.format("%Y-%m-%d %H:%M").to_string(), "2026-05-10 11:00");
        assert_eq!(event.recurrence, Some(vec!["RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=5".to_string()]));
        assert_eq!(event.reminders.unwrap().overrides.unwrap().len(), 1);
    }

    #[test]
    fn test_map_update_command_clear_flags() {
        let today = NaiveDate::from_ymd_opt(2026, 2, 24).unwrap();
        let event = CliMapper::map_update_command(
            "event_123".to_string(),
            None, None, None, None, "primary".to_string(),
            true, true, true, None, None, None, None, None, None, None, None, today
        ).unwrap();

        assert_eq!(event.event_id, "event_123");
        assert_eq!(event.title, None);
        assert_eq!(event.recurrence, Some(vec![]));
        assert!(!event.reminders.unwrap().use_default);
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

