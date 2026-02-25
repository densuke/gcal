use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone, Weekday};
use crate::error::GcalError;
use crate::parser::duration::parse_duration_str;
use crate::parser::util::{normalize, take_number, strip_suffix_u64};

/// 日付範囲（from 以上 to 以下、両端含む）
#[derive(Debug, Clone, PartialEq)]
pub struct DateRange {
    pub from: NaiveDate,
    pub to: NaiveDate,
}

impl DateRange {
    pub fn single(date: NaiveDate) -> Self {
        Self { from: date, to: date }
    }
}

/// 自然言語の日付表現を解析して DateRange を返す
///
/// `today` を引数で受け取ることでテストが固定時刻で動く
pub fn parse_date_expr(input: &str, today: NaiveDate) -> Result<DateRange, GcalError> {
    let s = normalize(input);

    // --- キーワード ---
    match s.as_str() {
        "今日" | "today" => return Ok(DateRange::single(today)),
        "明日" | "tomorrow" => return Ok(DateRange::single(today + Duration::days(1))),
        "明後日" | "asatte" => return Ok(DateRange::single(today + Duration::days(2))),
        "昨日" | "yesterday" => return Ok(DateRange::single(today - Duration::days(1))),
        "今週" => return Ok(this_week(today)),
        "来週" => return Ok(next_week(today)),
        "今月" => return Ok(this_month(today)),
        "来月" => return Ok(next_month(today)),
        _ => {}
    }

    // --- N日後 ---
    if let Some(n) = strip_suffix_u64(&s, "日後") {
        return Ok(DateRange::single(today + Duration::days(n as i64)));
    }

    // --- N週間後 / N週後 ---
    if let Some(n) = strip_suffix_u64(&s, "週間後").or_else(|| strip_suffix_u64(&s, "週後")) {
        return Ok(DateRange::single(today + Duration::weeks(n as i64)));
    }

    // --- YYYY/M/D または YYYY年M月D日 ---
    if let Some(d) = parse_full_date(&s) {
        return Ok(DateRange::single(d));
    }

    // --- M/D または M月D日（今年） ---
    if let Some(d) = parse_month_day(&s, today.year()) {
        return Ok(DateRange::single(d));
    }

    Err(GcalError::ConfigError(format!(
        "日付の解釈ができません: '{input}'\n\
         例: 今日, 明日, 来週, 今月, 3/19, 3月19日, 2026/3/19, 3日後, 2週間後"
    )))
}

// --- 週・月の範囲計算 ---

/// 今日〜今週日曜
fn this_week(today: NaiveDate) -> DateRange {
    let days_to_sunday = days_until_sunday(today);
    DateRange {
        from: today,
        to: today + Duration::days(days_to_sunday as i64),
    }
}

/// 来週月曜〜来週日曜
fn next_week(today: NaiveDate) -> DateRange {
    let days_to_sunday = days_until_sunday(today);
    let next_monday = today + Duration::days(days_to_sunday as i64 + 1);
    let next_sunday = next_monday + Duration::days(6);
    DateRange {
        from: next_monday,
        to: next_sunday,
    }
}

/// 今月1日〜今月末日
fn this_month(today: NaiveDate) -> DateRange {
    let from = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
    let to = last_day_of_month(today.year(), today.month());
    DateRange { from, to }
}

/// 翌月1日〜翌月末日
fn next_month(today: NaiveDate) -> DateRange {
    let (year, month) = if today.month() == 12 {
        (today.year() + 1, 1)
    } else {
        (today.year(), today.month() + 1)
    };
    let from = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let to = last_day_of_month(year, month);
    DateRange { from, to }
}

fn days_until_sunday(date: NaiveDate) -> u32 {
    match date.weekday() {
        Weekday::Mon => 6,
        Weekday::Tue => 5,
        Weekday::Wed => 4,
        Weekday::Thu => 3,
        Weekday::Fri => 2,
        Weekday::Sat => 1,
        Weekday::Sun => 0,
    }
}

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let (next_year, next_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(next_year, next_month, 1).unwrap() - Duration::days(1)
}

// --- パースヘルパー ---

/// "YYYY/M/D" または "YYYY年M月D日" を解析
fn parse_full_date(s: &str) -> Option<NaiveDate> {
    // YYYY/M/D
    if let Some(d) = parse_ymd_slash(s) {
        return Some(d);
    }
    // YYYY年M月D日
    parse_ymd_japanese(s)
}

fn parse_ymd_slash(s: &str) -> Option<NaiveDate> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 3 {
        let y = parts[0].parse::<i32>().ok()?;
        let m = parts[1].parse::<u32>().ok()?;
        let d = parts[2].parse::<u32>().ok()?;
        if y > 31 {
            // 年として解釈できる（M/Dと区別）
            return NaiveDate::from_ymd_opt(y, m, d);
        }
    }
    None
}

fn parse_ymd_japanese(s: &str) -> Option<NaiveDate> {
    // "YYYY年M月D日"
    let (rest, year) = take_number(s)?;
    let rest = rest.strip_prefix('年')?;
    let (rest, month) = take_number(rest)?;
    let rest = rest.strip_prefix('月')?;
    let (rest, day) = take_number(rest)?;
    let _ = rest.strip_prefix('日')?; // 末尾に '日' が必要
    NaiveDate::from_ymd_opt(year as i32, month, day)
}

/// "M/D" または "M月D日" を今年の日付として解析
fn parse_month_day(s: &str, year: i32) -> Option<NaiveDate> {
    // M/D
    if let Some((m_str, d_str)) = s.split_once('/') {
        let m = m_str.parse::<u32>().ok()?;
        let d = d_str.parse::<u32>().ok()?;
        return NaiveDate::from_ymd_opt(year, m, d);
    }
    // M月D日
    let (rest, month) = take_number(s)?;
    let rest = rest.strip_prefix('月')?;
    let (rest, day) = take_number(rest)?;
    rest.strip_prefix('日')?;
    NaiveDate::from_ymd_opt(year, month, day)
}

// ============================================================
// --from / --to / --date / --days の組み合わせ解決
// ============================================================

/// CLI 引数の組み合わせから DateRange を解決する
pub fn resolve_event_range(
    date: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    days: Option<u64>,
    today: NaiveDate,
) -> Result<DateRange, GcalError> {
    // --date が指定されていればそれを使う
    if let Some(expr) = date {
        return parse_date_expr(expr, today);
    }

    // --from / --to の組み合わせ
    if from.is_some() || to.is_some() {
        let from_date = match from {
            Some(expr) => parse_date_expr(expr, today)?.from,
            None => today,
        };
        let to_date = match to {
            Some(expr) => parse_date_expr(expr, today)?.from, // 単日指定は .from を使う
            None => from_date + Duration::days(6),            // --from のみ: 7日間
        };

        if from_date > to_date {
            return Err(GcalError::ConfigError(format!(
                "--from ({from_date}) が --to ({to_date}) より後になっています"
            )));
        }

        return Ok(DateRange { from: from_date, to: to_date });
    }

    // デフォルト: 今日から N 日間
    let n = days.unwrap_or(7);
    Ok(DateRange {
        from: today,
        to: today + Duration::days(n as i64 - 1),
    })
}

// ============================================================
// 日時（日付 + 時刻）パース
// ============================================================

/// "今日 14:00" や "3/19 10:00" など、"<日付表現> HH:MM" 形式の入力を
/// DateTime<Local> に変換する
pub fn parse_datetime_expr(input: &str, today: NaiveDate) -> Result<DateTime<Local>, GcalError> {
    let s = input.trim();

    // 末尾の "HH:MM" を分離する
    let (date_part, time_part) = s.rsplit_once(' ').ok_or_else(|| {
        GcalError::ConfigError(format!(
            "日時の形式が不正です: '{input}'\n\
             例: \"今日 14:00\", \"3/19 10:00\", \"2026/3/19 09:30\""
        ))
    })?;

    // 時刻部分をパース
    let time = NaiveTime::parse_from_str(time_part, "%H:%M").map_err(|_| {
        GcalError::ConfigError(format!(
            "時刻の形式が不正です: '{time_part}'\n例: \"14:00\", \"9:30\""
        ))
    })?;

    // 日付部分をパース（DateRange の from を使う）
    let range = parse_date_expr(date_part, today)?;
    let date = range.from;

    // NaiveDateTime → DateTime<Local>
    let naive_dt = date.and_time(time);
    Local
        .from_local_datetime(&naive_dt)
        .single()
        .ok_or_else(|| {
            GcalError::ConfigError(format!("ローカル時刻の変換に失敗しました: '{input}'"))
        })
}

/// 終了日時を解析する
pub fn parse_end_expr(
    input: &str,
    start: DateTime<Local>,
    today: NaiveDate,
) -> Result<DateTime<Local>, GcalError> {
    if input.starts_with('+') {
        let dur = parse_duration_str(input)?;
        Ok(start + dur)
    } else {
        parse_datetime_expr(input, today)
    }
}

/// 内部ヘルパー: "HH:MM[-HH:MM]" または "H:MM[+duration]" を (time_str, end_spec) に分割
fn split_time_and_end_spec(s: &str) -> Option<(&str, Option<&str>)> {
    let colon_pos = s.find(':')?;
    let end_of_time = colon_pos + 3;
    if end_of_time > s.len() {
        return None;
    }
    let time_str = &s[..end_of_time];
    let rest = &s[end_of_time..];
    if rest.is_empty() {
        return Some((time_str, None));
    }
    if rest.starts_with('-') || rest.starts_with('+') {
        return Some((time_str, Some(rest)));
    }
    None
}

/// "今日 12:00-13:00" や "明日 10:00+1h" など、日時範囲を1フラグで指定する形式を解析する
pub fn parse_datetime_range_expr(
    input: &str,
    today: NaiveDate,
) -> Result<(DateTime<Local>, DateTime<Local>), GcalError> {
    let s = input.trim();
    let (date_part, time_spec) = s.rsplit_once(' ').ok_or_else(|| {
        GcalError::ConfigError(format!(
            "日時範囲の形式が不正です: '{input}'\n\
             例: \"今日 12:00\", \"今日 12:00-13:00\", \"今日 12:00+1h\""
        ))
    })?;

    let (time_str, end_spec) = split_time_and_end_spec(time_spec).ok_or_else(|| {
        GcalError::ConfigError(format!(
            "時刻の形式が不正です: '{time_spec}'\n例: \"12:00\", \"12:00-13:00\", \"12:00+1h\""
        ))
    })?;

    let time = NaiveTime::parse_from_str(time_str, "%H:%M").map_err(|_| {
        GcalError::ConfigError(format!(
            "時刻の形式が不正です: '{time_str}'\n例: \"14:00\", \"9:30\""
        ))
    })?;

    let date = parse_date_expr(date_part, today)?.from;
    let start_dt = Local
        .from_local_datetime(&date.and_time(time))
        .single()
        .ok_or_else(|| {
            GcalError::ConfigError(format!("ローカル時刻の変換に失敗しました: '{input}'"))
        })?;

    let end_dt = match end_spec {
        None => start_dt + Duration::hours(1),
        Some(spec) if spec.starts_with('+') => {
            let dur = parse_duration_str(spec)?;
            start_dt + dur
        }
        Some(spec) => {
            let end_time_str = &spec[1..];
            let end_time = NaiveTime::parse_from_str(end_time_str, "%H:%M").map_err(|_| {
                GcalError::ConfigError(format!("終了時刻の形式が不正です: '{end_time_str}'"))
            })?;
            Local
                .from_local_datetime(&date.and_time(end_time))
                .single()
                .ok_or_else(|| {
                    GcalError::ConfigError("ローカル時刻の変換に失敗しました".to_string())
                })?
        }
    };

    Ok((start_dt, end_dt))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 2, 24).unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn range(from: NaiveDate, to: NaiveDate) -> DateRange {
        DateRange { from, to }
    }

    fn local_dt(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Local> {
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

    // --- キーワード ---

    #[test]
    fn test_today() {
        assert_eq!(parse_date_expr("今日", today()).unwrap(), DateRange::single(date(2026, 2, 24)));
    }

    #[test]
    fn test_today_english() {
        assert_eq!(parse_date_expr("today", today()).unwrap(), DateRange::single(date(2026, 2, 24)));
    }

    #[test]
    fn test_tomorrow() {
        assert_eq!(parse_date_expr("明日", today()).unwrap(), DateRange::single(date(2026, 2, 25)));
    }

    #[test]
    fn test_tomorrow_english() {
        assert_eq!(parse_date_expr("tomorrow", today()).unwrap(), DateRange::single(date(2026, 2, 25)));
    }

    #[test]
    fn test_day_after_tomorrow() {
        assert_eq!(parse_date_expr("明後日", today()).unwrap(), DateRange::single(date(2026, 2, 26)));
    }

    #[test]
    fn test_yesterday() {
        assert_eq!(parse_date_expr("昨日", today()).unwrap(), DateRange::single(date(2026, 2, 23)));
    }

    #[test]
    fn test_yesterday_english() {
        assert_eq!(parse_date_expr("yesterday", today()).unwrap(), DateRange::single(date(2026, 2, 23)));
    }

    // --- 今週・来週 ---
    #[test]
    fn test_this_week_from_tuesday() {
        assert_eq!(
            parse_date_expr("今週", today()).unwrap(),
            range(date(2026, 2, 24), date(2026, 3, 1))
        );
    }

    #[test]
    fn test_this_week_from_sunday() {
        let sunday = date(2026, 3, 1);
        assert_eq!(
            parse_date_expr("今週", sunday).unwrap(),
            range(date(2026, 3, 1), date(2026, 3, 1))
        );
    }

    #[test]
    fn test_this_week_from_monday() {
        // 月曜 → 今日〜同週日曜 (days_until_sunday = 6)
        let monday = date(2026, 2, 23);
        assert_eq!(
            parse_date_expr("今週", monday).unwrap(),
            range(date(2026, 2, 23), date(2026, 3, 1))
        );
    }

    #[test]
    fn test_this_week_from_wednesday() {
        // 水曜 → 今日〜同週日曜 (days_until_sunday = 4)
        let wednesday = date(2026, 2, 25);
        assert_eq!(
            parse_date_expr("今週", wednesday).unwrap(),
            range(date(2026, 2, 25), date(2026, 3, 1))
        );
    }

    #[test]
    fn test_this_week_from_thursday() {
        // 木曜 → 今日〜同週日曜 (days_until_sunday = 3)
        let thursday = date(2026, 2, 26);
        assert_eq!(
            parse_date_expr("今週", thursday).unwrap(),
            range(date(2026, 2, 26), date(2026, 3, 1))
        );
    }

    #[test]
    fn test_this_week_from_friday() {
        // 金曜 → 今日〜同週日曜 (days_until_sunday = 2)
        let friday = date(2026, 2, 27);
        assert_eq!(
            parse_date_expr("今週", friday).unwrap(),
            range(date(2026, 2, 27), date(2026, 3, 1))
        );
    }

    #[test]
    fn test_this_week_from_saturday() {
        // 土曜 → 今日〜同週日曜 (days_until_sunday = 1)
        let saturday = date(2026, 2, 28);
        assert_eq!(
            parse_date_expr("今週", saturday).unwrap(),
            range(date(2026, 2, 28), date(2026, 3, 1))
        );
    }

    #[test]
    fn test_next_week() {
        assert_eq!(
            parse_date_expr("来週", today()).unwrap(),
            range(date(2026, 3, 2), date(2026, 3, 8))
        );
    }

    // --- 今月・来月 ---
    #[test]
    fn test_this_month() {
        assert_eq!(
            parse_date_expr("今月", today()).unwrap(),
            range(date(2026, 2, 1), date(2026, 2, 28))
        );
    }

    #[test]
    fn test_next_month() {
        assert_eq!(
            parse_date_expr("来月", today()).unwrap(),
            range(date(2026, 3, 1), date(2026, 3, 31))
        );
    }

    #[test]
    fn test_next_month_december() {
        let dec = date(2026, 12, 15);
        assert_eq!(
            parse_date_expr("来月", dec).unwrap(),
            range(date(2027, 1, 1), date(2027, 1, 31))
        );
    }

    // --- N日後 / N週間後 ---
    #[test]
    fn test_n_days_later() {
        assert_eq!(parse_date_expr("3日後", today()).unwrap(), DateRange::single(date(2026, 2, 27)));
    }

    #[test]
    fn test_n_days_later_large() {
        assert_eq!(parse_date_expr("10日後", today()).unwrap(), DateRange::single(date(2026, 3, 6)));
    }

    #[test]
    fn test_n_weeks_later() {
        assert_eq!(parse_date_expr("2週間後", today()).unwrap(), DateRange::single(date(2026, 3, 10)));
    }

    #[test]
    fn test_n_weeks_later_short() {
        assert_eq!(parse_date_expr("1週後", today()).unwrap(), DateRange::single(date(2026, 3, 3)));
    }

    // --- M/D 形式 ---
    #[test]
    fn test_month_day_slash() {
        assert_eq!(parse_date_expr("3/19", today()).unwrap(), DateRange::single(date(2026, 3, 19)));
    }

    #[test]
    fn test_month_day_japanese() {
        assert_eq!(parse_date_expr("3月19日", today()).unwrap(), DateRange::single(date(2026, 3, 19)));
    }

    // --- YYYY/M/D 形式 ---
    #[test]
    fn test_full_date_slash() {
        assert_eq!(parse_date_expr("2027/1/5", today()).unwrap(), DateRange::single(date(2027, 1, 5)));
    }

    #[test]
    fn test_full_date_japanese() {
        assert_eq!(parse_date_expr("2027年1月5日", today()).unwrap(), DateRange::single(date(2027, 1, 5)));
    }

    // --- 全角入力の正規化 ---
    #[test]
    fn test_fullwidth_month_day() {
        assert_eq!(parse_date_expr("３月１９日", today()).unwrap(), DateRange::single(date(2026, 3, 19)));
    }

    #[test]
    fn test_fullwidth_slash() {
        assert_eq!(parse_date_expr("３／１９", today()).unwrap(), DateRange::single(date(2026, 3, 19)));
    }

    #[test]
    fn test_trim_whitespace() {
        assert_eq!(parse_date_expr("  今日  ", today()).unwrap(), DateRange::single(date(2026, 2, 24)));
    }

    // --- エラーケース ---
    #[test]
    fn test_unknown_expression_returns_error() {
        let result = parse_date_expr("来年", today());
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_date_returns_error() {
        let result = parse_date_expr("13/40", today());
        assert!(result.is_err());
    }

    // --- resolve_event_range のテスト ---
    #[test]
    fn test_resolve_date_option() {
        let r = resolve_event_range(Some("来週"), None, None, None, today()).unwrap();
        assert_eq!(r, range(date(2026, 3, 2), date(2026, 3, 8)));
    }

    #[test]
    fn test_resolve_from_and_to() {
        let r = resolve_event_range(None, Some("3/1"), Some("3/15"), None, today()).unwrap();
        assert_eq!(r, range(date(2026, 3, 1), date(2026, 3, 15)));
    }

    #[test]
    fn test_resolve_from_only_defaults_7_days() {
        let r = resolve_event_range(None, Some("3/1"), None, None, today()).unwrap();
        assert_eq!(r, range(date(2026, 3, 1), date(2026, 3, 7)));
    }

    #[test]
    fn test_resolve_to_only_defaults_from_today() {
        let r = resolve_event_range(None, None, Some("3/5"), None, today()).unwrap();
        assert_eq!(r, range(date(2026, 2, 24), date(2026, 3, 5)));
    }

    #[test]
    fn test_resolve_days_option() {
        let r = resolve_event_range(None, None, None, Some(3), today()).unwrap();
        assert_eq!(r, range(date(2026, 2, 24), date(2026, 2, 26)));
    }

    #[test]
    fn test_resolve_default_7_days() {
        let r = resolve_event_range(None, None, None, None, today()).unwrap();
        assert_eq!(r, range(date(2026, 2, 24), date(2026, 3, 2)));
    }

    #[test]
    fn test_resolve_from_after_to_returns_error() {
        let result = resolve_event_range(None, Some("3/15"), Some("3/1"), None, today());
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_from_equals_to() {
        let r = resolve_event_range(None, Some("3/5"), Some("3/5"), None, today()).unwrap();
        assert_eq!(r, DateRange::single(date(2026, 3, 5)));
    }

    #[test]
    fn test_resolve_from_with_natural_language() {
        let r = resolve_event_range(None, Some("明日"), Some("来週"), None, today()).unwrap();
        assert_eq!(r, range(date(2026, 2, 25), date(2026, 3, 2)));
    }

    // --- parse_datetime_expr のテスト ---
    #[test]
    fn test_datetime_today() {
        let dt = parse_datetime_expr("今日 14:00", today()).unwrap();
        assert_eq!(dt.date_naive(), date(2026, 2, 24));
        assert_eq!(dt.format("%H:%M").to_string(), "14:00");
    }

    #[test]
    fn test_datetime_tomorrow() {
        let dt = parse_datetime_expr("明日 9:30", today()).unwrap();
        assert_eq!(dt.date_naive(), date(2026, 2, 25));
        assert_eq!(dt.format("%H:%M").to_string(), "09:30");
    }

    #[test]
    fn test_datetime_month_day_slash() {
        let dt = parse_datetime_expr("3/19 10:00", today()).unwrap();
        assert_eq!(dt.date_naive(), date(2026, 3, 19));
        assert_eq!(dt.format("%H:%M").to_string(), "10:00");
    }

    #[test]
    fn test_datetime_full_date() {
        let dt = parse_datetime_expr("2026/3/19 10:00", today()).unwrap();
        assert_eq!(dt.date_naive(), date(2026, 3, 19));
        assert_eq!(dt.format("%H:%M").to_string(), "10:00");
    }

    #[test]
    fn test_datetime_japanese_month_day() {
        let dt = parse_datetime_expr("3月19日 10:00", today()).unwrap();
        assert_eq!(dt.date_naive(), date(2026, 3, 19));
        assert_eq!(dt.format("%H:%M").to_string(), "10:00");
    }

    #[test]
    fn test_datetime_no_time_returns_error() {
        let result = parse_datetime_expr("今日", today());
        assert!(result.is_err());
    }

    #[test]
    fn test_datetime_invalid_time_returns_error() {
        let result = parse_datetime_expr("今日 25:00", today());
        assert!(result.is_err());
    }

    #[test]
    fn test_datetime_invalid_date_returns_error() {
        let result = parse_datetime_expr("来年 10:00", today());
        assert!(result.is_err());
    }

    // --- parse_end_expr のテスト ---
    #[test]
    fn test_end_expr_relative_1h() {
        let start = local_dt(2026, 2, 24, 13, 0);
        let end = parse_end_expr("+1h", start, today()).unwrap();
        assert_eq!(end, local_dt(2026, 2, 24, 14, 0));
    }

    #[test]
    fn test_end_expr_relative_30m() {
        let start = local_dt(2026, 2, 24, 13, 0);
        let end = parse_end_expr("+30m", start, today()).unwrap();
        assert_eq!(end, local_dt(2026, 2, 24, 13, 30));
    }

    #[test]
    fn test_end_expr_absolute() {
        let start = local_dt(2026, 2, 24, 13, 0);
        let end = parse_end_expr("明日 15:00", start, today()).unwrap();
        assert_eq!(end, local_dt(2026, 2, 25, 15, 0));
    }

    #[test]
    fn test_end_expr_absolute_same_day() {
        let dt1 = local_dt(2026, 2, 24, 13, 0);
        let dt2 = parse_end_expr("今日 18:00", dt1, today()).unwrap();
        assert_eq!(dt2, local_dt(2026, 2, 24, 18, 0));
    }

    #[test]
    fn test_end_expr_absolute_different_day() {
        let dt1 = local_dt(2026, 2, 24, 10, 0);
        let dt2 = parse_end_expr("明日 10:00", dt1, today()).unwrap();
        assert_eq!(dt2, local_dt(2026, 2, 25, 10, 0));
    }

    // --- parse_datetime_range_expr のテスト ---

    #[test]
    fn test_range_default_1h() {
        let (s, e) = parse_datetime_range_expr("今日 12:00", today()).unwrap();
        assert_eq!(s, local_dt(2026, 2, 24, 12, 0));
        assert_eq!(e, local_dt(2026, 2, 24, 13, 0));
    }

    #[test]
    fn test_range_absolute_end() {
        let (s, e) = parse_datetime_range_expr("今日 12:00-13:30", today()).unwrap();
        assert_eq!(s, local_dt(2026, 2, 24, 12, 0));
        assert_eq!(e, local_dt(2026, 2, 24, 13, 30));
    }

    #[test]
    fn test_range_relative_end_1h() {
        let (s, e) = parse_datetime_range_expr("今日 12:00+1h", today()).unwrap();
        assert_eq!(s, local_dt(2026, 2, 24, 12, 0));
        assert_eq!(e, local_dt(2026, 2, 24, 13, 0));
    }

    #[test]
    fn test_range_relative_end_30m() {
        let (s, e) = parse_datetime_range_expr("明日 10:00+30m", today()).unwrap();
        assert_eq!(s, local_dt(2026, 2, 25, 10, 0));
        assert_eq!(e, local_dt(2026, 2, 25, 10, 30));
    }

    #[test]
    fn test_range_relative_end_1h30m() {
        let (s, e) = parse_datetime_range_expr("3/20 14:00+1h30m", today()).unwrap();
        assert_eq!(s, local_dt(2026, 3, 20, 14, 0));
        assert_eq!(e, local_dt(2026, 3, 20, 15, 30));
    }

    #[test]
        fn test_range_no_space_returns_error() {
        assert!(parse_datetime_range_expr("9:30", today()).is_err());
    }

    #[test]
    fn test_range_invalid_time_spec_returns_error() {
        // split_time_and_end_spec が None → 時刻形式エラー
        // "12:00X" は '-'/'+'以外の文字が続くので None になる
        assert!(parse_datetime_range_expr("今日 12:00X", today()).is_err());
    }

    #[test]
    fn test_range_invalid_time_value_returns_error() {
        // split_time_and_end_spec は成功するが NaiveTime::parse_from_str が失敗
        // "99:99" は HH:MM として正しい長さだが値が不正
        assert!(parse_datetime_range_expr("今日 99:99", today()).is_err());
    }

    #[test]
    fn test_range_invalid_absolute_end_time_returns_error() {
        // 絶対終了時刻が不正な値
        assert!(parse_datetime_range_expr("今日 12:00-99:99", today()).is_err());
    }

    #[test]
    fn test_range_short_time_spec_returns_error() {
        // "9:0" は colon_pos=1, end_of_time=4 > len=3 → split_time_and_end_spec が None
        assert!(parse_datetime_range_expr("今日 9:0", today()).is_err());
    }
}
