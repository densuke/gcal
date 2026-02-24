pub(crate) fn strip_suffix_u64(s: &str, suffix: &str) -> Option<u64> {
    s.strip_suffix(suffix)?.parse::<u64>().ok()
}

/// 先頭の数字列を取り出し (残り文字列, 数値) を返す
pub(crate) fn take_number(s: &str) -> Option<(&str, u32)> {
    let end = s.find(|c: char| !c.is_ascii_digit())?;
    if end == 0 {
        return None;
    }
    let n = s[..end].parse::<u32>().ok()?;
    Some((&s[end..], n))
}

/// 入力を正規化する（全角数字→半角、全角スラッシュ→半角、trim）
pub(crate) fn normalize(s: &str) -> String {
    s.trim()
        .chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            '／' => '/',
            _ => c,
        })
        .collect()
}
