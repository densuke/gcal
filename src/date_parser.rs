use chrono::{Datelike, Duration, NaiveDate, Weekday};

use crate::error::GcalError;

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

/// 先頭の数字列を取り出し (残り文字列, 数値) を返す
fn take_number(s: &str) -> Option<(&str, u32)> {
    let end = s.find(|c: char| !c.is_ascii_digit())?;
    if end == 0 {
        return None;
    }
    let n = s[..end].parse::<u32>().ok()?;
    Some((&s[end..], n))
}

/// "N<suffix>" の形式から N を取り出す（例: "3日後" → Some(3)）
fn strip_suffix_u64(s: &str, suffix: &str) -> Option<u64> {
    s.strip_suffix(suffix)?.parse::<u64>().ok()
}

/// 入力を正規化する（全角数字→半角、全角スラッシュ→半角、trim）
fn normalize(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            '／' => '/',
            _ => c,
        })
        .collect()
}

// ============================================================
// --from / --to / --date / --days の組み合わせ解決
// ============================================================

/// CLI 引数の組み合わせから DateRange を解決する
///
/// 優先順位:
///   1. `--date` → parse_date_expr で解決
///   2. `--from` / `--to`（片方のみでも可）
///   3. `--days`（デフォルト 7）
///
/// `today` を引数で受け取ることでテスト可能
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
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト基準日: 2026-02-24 (火曜日)
    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 2, 24).unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn range(from: NaiveDate, to: NaiveDate) -> DateRange {
        DateRange { from, to }
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
    // 基準日: 2026-02-24 (火曜)
    // 今週日曜: 2026-03-01
    // 来週月曜: 2026-03-02、来週日曜: 2026-03-08

    #[test]
    fn test_this_week_from_tuesday() {
        // 火曜〜日曜
        assert_eq!(
            parse_date_expr("今週", today()).unwrap(),
            range(date(2026, 2, 24), date(2026, 3, 1))
        );
    }

    #[test]
    fn test_this_week_from_sunday() {
        // 日曜の場合は当日のみ
        let sunday = date(2026, 3, 1);
        assert_eq!(
            parse_date_expr("今週", sunday).unwrap(),
            range(date(2026, 3, 1), date(2026, 3, 1))
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
        // 12月の翌月は来年1月
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
        // --date 来週 → 来週月〜日
        let r = resolve_event_range(Some("来週"), None, None, None, today()).unwrap();
        assert_eq!(r, range(date(2026, 3, 2), date(2026, 3, 8)));
    }

    #[test]
    fn test_resolve_from_and_to() {
        // --from 3/1 --to 3/15
        let r = resolve_event_range(None, Some("3/1"), Some("3/15"), None, today()).unwrap();
        assert_eq!(r, range(date(2026, 3, 1), date(2026, 3, 15)));
    }

    #[test]
    fn test_resolve_from_only_defaults_7_days() {
        // --from 3/1 のみ → 3/1〜3/7
        let r = resolve_event_range(None, Some("3/1"), None, None, today()).unwrap();
        assert_eq!(r, range(date(2026, 3, 1), date(2026, 3, 7)));
    }

    #[test]
    fn test_resolve_to_only_defaults_from_today() {
        // --to 3/5 のみ → 今日〜3/5
        let r = resolve_event_range(None, None, Some("3/5"), None, today()).unwrap();
        assert_eq!(r, range(date(2026, 2, 24), date(2026, 3, 5)));
    }

    #[test]
    fn test_resolve_days_option() {
        // --days 3 → 今日〜今日+2
        let r = resolve_event_range(None, None, None, Some(3), today()).unwrap();
        assert_eq!(r, range(date(2026, 2, 24), date(2026, 2, 26)));
    }

    #[test]
    fn test_resolve_default_7_days() {
        // 何も指定しない → 今日〜今日+6
        let r = resolve_event_range(None, None, None, None, today()).unwrap();
        assert_eq!(r, range(date(2026, 2, 24), date(2026, 3, 2)));
    }

    #[test]
    fn test_resolve_from_after_to_returns_error() {
        // --from が --to より後 → エラー
        let result = resolve_event_range(None, Some("3/15"), Some("3/1"), None, today());
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_from_equals_to() {
        // --from と --to が同じ日 → 1日分
        let r = resolve_event_range(None, Some("3/5"), Some("3/5"), None, today()).unwrap();
        assert_eq!(r, DateRange::single(date(2026, 3, 5)));
    }

    #[test]
    fn test_resolve_from_with_natural_language() {
        // --from 明日 --to 来週 は "来週" の from を使う
        let r = resolve_event_range(None, Some("明日"), Some("来週"), None, today()).unwrap();
        // 明日=2/25、来週 の from=3/2
        assert_eq!(r, range(date(2026, 2, 25), date(2026, 3, 2)));
    }
}
