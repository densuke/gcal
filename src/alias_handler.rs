use std::io::Write;

use crate::config::Config;
use crate::error::GcalError;

/// エイリアスを追加または更新（upsert）する
pub fn handle_set_alias<W: Write>(
    config_path: &std::path::Path,
    name: &str,
    calendar_id: &str,
    out: &mut W,
) -> Result<(), GcalError> {
    let mut config = Config::load(config_path).unwrap_or_default();
    config
        .calendars
        .insert(name.to_string(), calendar_id.to_string());
    config.save(config_path)?;
    writeln!(out, "エイリアスを設定しました: {} → {}", name, calendar_id)?;
    Ok(())
}

/// 設定済みエイリアスを一覧表示する
pub fn handle_list_aliases<W: Write>(
    config_path: &std::path::Path,
    out: &mut W,
) -> Result<(), GcalError> {
    let config = Config::load(config_path).unwrap_or_default();
    if config.calendars.is_empty() {
        writeln!(out, "エイリアスが設定されていません")?;
        return Ok(());
    }
    writeln!(out, "{:<20}  カレンダーID", "エイリアス")?;
    writeln!(out, "{:-<20}  {:-<40}", "", "")?;
    let mut entries: Vec<_> = config.calendars.iter().collect();
    entries.sort_by_key(|(k, _)| k.as_str());
    for (alias, id) in entries {
        writeln!(out, "{:<20}  {}", alias, id)?;
    }
    Ok(())
}

/// エイリアスを削除する
pub fn handle_remove_alias<W: Write>(
    config_path: &std::path::Path,
    name: &str,
    out: &mut W,
) -> Result<(), GcalError> {
    let mut config = Config::load(config_path).unwrap_or_default();
    if config.calendars.remove(name).is_none() {
        return Err(GcalError::ConfigError(format!(
            "エイリアス '{}' が見つかりません",
            name
        )));
    }
    config.save(config_path)?;
    writeln!(out, "エイリアスを削除しました: {}", name)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_config_path(dir: &TempDir) -> std::path::PathBuf {
        dir.path().join("gcal").join("config.toml")
    }

    fn init_config(path: &std::path::Path) {
        use crate::config::{AiConfig, Config, Credentials};
        let config = Config {
            credentials: Credentials {
                client_id: "x".to_string(),
                client_secret: "y".to_string(),
            },
            token: None,
            ai: AiConfig::default(),
            calendars: Default::default(),
            events: Default::default(),
        };
        config.save(path).unwrap();
    }

    #[test]
    fn test_handle_set_alias_creates_entry() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        init_config(&path);

        let mut out = Vec::new();
        handle_set_alias(&path, "仕事", "work@google.com", &mut out).unwrap();

        let config = crate::config::Config::load(&path).unwrap();
        assert_eq!(
            config.calendars.get("仕事").map(|s| s.as_str()),
            Some("work@google.com")
        );
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("仕事"), "{s}");
        assert!(s.contains("work@google.com"), "{s}");
    }

    #[test]
    fn test_handle_set_alias_updates_existing() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        init_config(&path);

        handle_set_alias(&path, "仕事", "old@google.com", &mut Vec::new()).unwrap();
        handle_set_alias(&path, "仕事", "new@google.com", &mut Vec::new()).unwrap();

        let config = crate::config::Config::load(&path).unwrap();
        assert_eq!(
            config.calendars.get("仕事").map(|s| s.as_str()),
            Some("new@google.com")
        );
    }

    #[test]
    fn test_handle_list_aliases_empty() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        init_config(&path);

        let mut out = Vec::new();
        handle_list_aliases(&path, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("設定されていません"), "{s}");
    }

    #[test]
    fn test_handle_list_aliases_shows_entries() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        init_config(&path);

        handle_set_alias(&path, "仕事", "work@google.com", &mut Vec::new()).unwrap();
        handle_set_alias(&path, "個人", "personal@google.com", &mut Vec::new()).unwrap();

        let mut out = Vec::new();
        handle_list_aliases(&path, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("仕事"), "{s}");
        assert!(s.contains("work@google.com"), "{s}");
        assert!(s.contains("個人"), "{s}");
    }

    #[test]
    fn test_handle_remove_alias_removes_entry() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        init_config(&path);

        handle_set_alias(&path, "仕事", "work@google.com", &mut Vec::new()).unwrap();
        let mut out = Vec::new();
        handle_remove_alias(&path, "仕事", &mut out).unwrap();

        let config = crate::config::Config::load(&path).unwrap();
        assert!(!config.calendars.contains_key("仕事"));
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("削除"), "{s}");
    }

    #[test]
    fn test_handle_remove_alias_unknown_returns_error() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        init_config(&path);

        let result = handle_remove_alias(&path, "存在しない", &mut Vec::new());
        assert!(result.is_err());
    }
}
