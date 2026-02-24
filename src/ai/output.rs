use std::io::Write;

use crate::ai::types::AiEventParameters;
use crate::error::GcalError;

/// `--ai-dry-run` 時に AI パース結果を人間が読める形式で出力する
pub fn write_ai_params<W: Write>(params: &AiEventParameters, out: &mut W) -> Result<(), GcalError> {
    writeln!(out, "AI パース結果:")?;
    writeln!(out, "  タイトル: {}", params.title.as_deref().unwrap_or("(未設定)"))?;
    writeln!(out, "  日付:     {}", params.date.as_deref().unwrap_or("(未設定)"))?;
    writeln!(out, "  開始:     {}", params.start.as_deref().unwrap_or("(未設定)"))?;
    writeln!(out, "  終了:     {}", params.end.as_deref().unwrap_or("(未設定)"))?;
    writeln!(out, "  場所:     {}", params.location.as_deref().unwrap_or("(なし)"))?;
    writeln!(out, "  繰り返し: {}", params.repeat_rule.as_deref().unwrap_or("(なし)"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_ai_params_all_fields() {
        let params = AiEventParameters {
            title: Some("チームMTG".to_string()),
            date: Some("明日".to_string()),
            start: Some("14:00".to_string()),
            end: Some("15:00".to_string()),
            location: Some("会議室A".to_string()),
            repeat_rule: Some("weekly".to_string()),
        };
        let mut buf = Vec::new();
        write_ai_params(&params, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("チームMTG"), "title が出力に含まれない");
        assert!(output.contains("明日"), "date が出力に含まれない");
        assert!(output.contains("14:00"), "start が出力に含まれない");
        assert!(output.contains("15:00"), "end が出力に含まれない");
        assert!(output.contains("会議室A"), "location が出力に含まれない");
        assert!(output.contains("weekly"), "repeat_rule が出力に含まれない");
    }

    #[test]
    fn test_write_ai_params_missing_fields_shows_placeholder() {
        let params = AiEventParameters {
            title: None,
            date: None,
            start: None,
            end: None,
            location: None,
            repeat_rule: None,
        };
        let mut buf = Vec::new();
        write_ai_params(&params, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        // 未設定フィールドはプレースホルダー表示
        assert!(output.contains("(未設定)"), "未設定プレースホルダーが出力に含まれない");
        assert!(output.contains("(なし)"), "なしプレースホルダーが出力に含まれない");
    }

    #[test]
    fn test_write_ai_params_header_present() {
        let params = AiEventParameters {
            title: Some("test".to_string()),
            date: None, start: None, end: None, location: None, repeat_rule: None,
        };
        let mut buf = Vec::new();
        write_ai_params(&params, &mut buf).unwrap();
        let output = String::from_utf8(buf).unwrap();
        assert!(output.starts_with("AI パース結果:"), "ヘッダーが出力に含まれない");
    }
}
