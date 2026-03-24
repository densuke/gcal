use crate::error::GcalError;
use chrono::Duration;

/// "+1h", "+30m", "+1h30m", "+90m" などの相対時間を Duration に変換する
///
/// `+` プレフィックスが必須。なければエラー。
pub fn parse_duration_str(s: &str) -> Result<Duration, GcalError> {
    let rest = s.strip_prefix('+').ok_or_else(|| {
        GcalError::ConfigError(format!(
            "相対時間には '+' が必要です: '{s}'\n例: \"+1h\", \"+30m\", \"+1h30m\""
        ))
    })?;
    if rest.is_empty() {
        return Err(GcalError::ConfigError(format!(
            "相対時間の値がありません: '{s}'"
        )));
    }

    let mut hours: i64 = 0;
    let mut minutes: i64 = 0;
    let mut remaining = rest;

    if let Some(h_pos) = remaining.find('h') {
        let h_str = &remaining[..h_pos];
        hours = h_str
            .parse::<i64>()
            .map_err(|_| GcalError::ConfigError(format!("相対時間の形式が不正です: '{s}'")))?;
        remaining = &remaining[h_pos + 1..];
    }

    if !remaining.is_empty() {
        let m_str = remaining.strip_suffix('m').ok_or_else(|| {
            GcalError::ConfigError(format!(
                "相対時間の形式が不正です: '{s}'\n例: \"+1h\", \"+30m\", \"+1h30m\""
            ))
        })?;
        minutes = m_str
            .parse::<i64>()
            .map_err(|_| GcalError::ConfigError(format!("相対時間の形式が不正です: '{s}'")))?;
    }

    Ok(Duration::hours(hours) + Duration::minutes(minutes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_1h() {
        assert_eq!(parse_duration_str("+1h").unwrap(), Duration::hours(1));
    }

    #[test]
    fn test_duration_30m() {
        assert_eq!(parse_duration_str("+30m").unwrap(), Duration::minutes(30));
    }

    #[test]
    fn test_duration_1h30m() {
        assert_eq!(
            parse_duration_str("+1h30m").unwrap(),
            Duration::hours(1) + Duration::minutes(30)
        );
    }

    #[test]
    fn test_duration_90m() {
        assert_eq!(parse_duration_str("+90m").unwrap(), Duration::minutes(90));
    }

    #[test]
    fn test_duration_no_plus_returns_error() {
        assert!(parse_duration_str("1h").is_err());
    }

    #[test]
    fn test_duration_invalid_returns_error() {
        assert!(parse_duration_str("+abc").is_err());
    }

    #[test]
    fn test_duration_plus_only_returns_error() {
        // "+" のみ（値なし）→ "相対時間の値がありません" エラー
        assert!(parse_duration_str("+").is_err());
    }

    #[test]
    fn test_duration_invalid_hours_digit_returns_error() {
        // 時間部分が数値でない → parse::<i64>() 失敗パス
        assert!(parse_duration_str("+xh").is_err());
    }

    #[test]
    fn test_duration_invalid_minutes_digit_returns_error() {
        // 分部分が数値でない（時間部分は OK）→ parse::<i64>() 失敗パス
        assert!(parse_duration_str("+1hxm").is_err());
    }
}
