use crate::error::GcalError;
use crate::parser::datetime::parse_date_expr;

pub fn parse_recurrence(
    repeat: Option<&str>,
    every: Option<u32>,
    on: Option<&str>,
    until: Option<&str>,
    count: Option<u32>,
    recur: Option<Vec<String>>,
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
        let today = chrono::Local::now().date_naive();
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

    #[test]
    fn test_parse_recurrence_daily() {
        let rrule = parse_recurrence(Some("daily"), None, None, None, None, None).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=DAILY"]);
    }

    #[test]
    fn test_parse_recurrence_weekly_with_interval_and_count() {
        let rrule = parse_recurrence(Some("weekly"), Some(2), Some("mon,wed"), None, Some(10), None).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;COUNT=10"]);
    }

    #[test]
    fn test_parse_recurrence_monthly_with_until() {
        let rrule = parse_recurrence(Some("monthly"), None, None, Some("2026/12/31"), None, None).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=MONTHLY;UNTIL=20261231T235959Z"]);
    }

    #[test]
    fn test_parse_recurrence_raw_rrule() {
        let raw = vec!["RRULE:FREQ=YEARLY".to_string()];
        let rrule = parse_recurrence(None, None, None, None, None, Some(raw)).unwrap().unwrap();
        assert_eq!(rrule, vec!["RRULE:FREQ=YEARLY"]);
    }
}
