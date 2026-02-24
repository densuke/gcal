use chrono::NaiveDate;

use crate::error::GcalError;
use crate::parser::datetime::parse_date_expr;

pub fn parse_recurrence(
    repeat: Option<&str>,
    every: Option<u32>,
    on: Option<&str>,
    until: Option<&str>,
    count: Option<u32>,
    recur: Option<Vec<String>>,
    today: NaiveDate,
) -> Result<Option<Vec<String>>, GcalError> {
    if let Some(rlist) = recur {
        return Ok(Some(rlist));
    }
    
    let freq = match repeat {
        Some("daily") => "DAILY",
        Some("weekly") => "WEEKLY",
        Some("monthly") => "MONTHLY",
        Some("yearly") => "YEARLY",
        Some(other) => return Err(GcalError::ConfigError(format!("未検証のrepeat値: {}", other))),
        None => return Ok(None),
    };

    let mut parts = vec![format!("FREQ={}", freq)];

    if let Some(interval) = every {
        parts.push(format!("INTERVAL={}", interval));
    }

    if let Some(days) = on {
        // e.g., "mon,wed" -> "MO,WE"
        let mapped: Vec<String> = days.split(',').map(|d| {
            match d.trim().to_lowercase().as_str() {
                "mon" | "monday" | "月" => "MO".to_string(),
                "tue" | "tuesday" | "火" => "TU".to_string(),
                "wed" | "wednesday" | "水" => "WE".to_string(),
                "thu" | "thursday" | "木" => "TH".to_string(),
                "fri" | "friday" | "金" => "FR".to_string(),
                "sat" | "saturday" | "土" => "SA".to_string(),
                "sun" | "sunday" | "日" => "SU".to_string(),
                other => other.to_uppercase(),
            }
        }).collect();
        parts.push(format!("BYDAY={}", mapped.join(",")));
    }

    if let Some(u) = until {
        let range = parse_date_expr(u, today)?;
        let date_str = range.from.format("%Y%m%d").to_string();
        // Append T235959Z for accurate until handling 
        parts.push(format!("UNTIL={}T235959Z", date_str));
    } else if let Some(c) = count {
        parts.push(format!("COUNT={}", c));
    }

    let rrule = format!("RRULE:{}", parts.join(";"));
    Ok(Some(vec![rrule]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 2, 24).unwrap()
    }

    #[test]
    fn test_parse_recurrence_none_returns_none() {
        let result = parse_recurrence(None, None, None, None, None, None, today()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_recurrence_daily() {
        let rrule = parse_recurrence(Some("daily"), None, None, None, None, None, today()).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=DAILY"]);
    }

    #[test]
    fn test_parse_recurrence_weekly() {
        let rrule = parse_recurrence(Some("weekly"), None, None, None, None, None, today()).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=WEEKLY"]);
    }

    #[test]
    fn test_parse_recurrence_monthly() {
        let rrule = parse_recurrence(Some("monthly"), None, None, None, None, None, today()).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=MONTHLY"]);
    }

    #[test]
    fn test_parse_recurrence_yearly() {
        let rrule = parse_recurrence(Some("yearly"), None, None, None, None, None, today()).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=YEARLY"]);
    }

    #[test]
    fn test_parse_recurrence_weekly_with_interval_and_count() {
        let rrule = parse_recurrence(Some("weekly"), Some(2), Some("mon,wed"), None, Some(10), None, today()).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=10"]);
    }

    #[test]
    fn test_parse_recurrence_monthly_with_until() {
        let rrule = parse_recurrence(Some("monthly"), None, None, Some("2026/12/31"), None, None, today()).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=MONTHLY;UNTIL=20261231T235959Z"]);
    }

    #[test]
    fn test_parse_recurrence_raw_rrule() {
        let raw = vec!["RRULE:FREQ=YEARLY".to_string()];
        let rrule = parse_recurrence(None, None, None, None, None, Some(raw), today()).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=YEARLY"]);
    }

    #[test]
    fn test_parse_recurrence_unknown_repeat_returns_error() {
        let result = parse_recurrence(Some("hourly"), None, None, None, None, None, today());
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_recurrence_byday_japanese() {
        let rrule = parse_recurrence(Some("weekly"), None, Some("月,水,金"), None, None, None, today()).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR"]);
    }

    #[test]
    fn test_parse_recurrence_interval_only() {
        let rrule = parse_recurrence(Some("daily"), Some(3), None, None, None, None, today()).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=DAILY;INTERVAL=3"]);
    }
}
