use crate::error::GcalError;
use crate::parser::util::strip_suffix_u64;

pub fn parse_reminders(
    reminder: Option<Vec<String>>,
    reminders: Option<&str>,
) -> Result<Option<crate::gcal_api::models::EventReminders>, GcalError> {
    if let Some(preset) = reminders {
        if preset == "default" {
            return Ok(Some(crate::gcal_api::models::EventReminders {
                use_default: true,
                overrides: None,
            }));
        } else if preset == "none" {
            return Ok(Some(crate::gcal_api::models::EventReminders {
                use_default: false,
                overrides: Some(vec![]),
            }));
        } else {
            return Err(GcalError::ConfigError(format!(
                "不明なremindersプリセット: {}",
                preset
            )));
        }
    }

    if let Some(list) = reminder {
        let mut overrides = Vec::new();
        for item in list {
            let parts: Vec<&str> = item.split(':').collect();
            if parts.len() != 2 {
                return Err(GcalError::ConfigError(format!(
                    "無効なreminder指定: {}",
                    item
                )));
            }
            let method = parts[0].to_string();
            let time_str = parts[1];

            let minutes = if let Some(m) = strip_suffix_u64(time_str, "m") {
                m as i32
            } else if let Some(h) = strip_suffix_u64(time_str, "h") {
                (h * 60) as i32
            } else if let Some(d) = strip_suffix_u64(time_str, "d") {
                (d * 24 * 60) as i32
            } else {
                return Err(GcalError::ConfigError(format!(
                    "無効な時間指定: {}",
                    time_str
                )));
            };

            overrides.push(crate::gcal_api::models::EventReminderOverride { method, minutes });
        }
        return Ok(Some(crate::gcal_api::models::EventReminders {
            use_default: false,
            overrides: Some(overrides),
        }));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reminders_default() {
        let rems = parse_reminders(None, Some("default")).unwrap().unwrap();
        assert!(rems.use_default);
        assert!(rems.overrides.is_none());
    }

    #[test]
    fn test_parse_reminders_none() {
        let rems = parse_reminders(None, Some("none")).unwrap().unwrap();
        assert!(!rems.use_default);
        assert_eq!(rems.overrides.unwrap().len(), 0);
    }

    #[test]
    fn test_parse_reminders_custom() {
        let overrides = vec!["popup:10m".to_string(), "email:1d".to_string()];
        let rems = parse_reminders(Some(overrides), None).unwrap().unwrap();
        assert!(!rems.use_default);
        let o = rems.overrides.unwrap();
        assert_eq!(o.len(), 2);
        assert_eq!(o[0].method, "popup");
        assert_eq!(o[0].minutes, 10);
        assert_eq!(o[1].method, "email");
        assert_eq!(o[1].minutes, 1440);
    }
}
