use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::domain::StoredTokens;
use crate::error::GcalError;
use crate::ports::TokenStore;

/// `gcal events` コマンドの設定
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventsConfig {
    /// events コマンドでデフォルトに使うカレンダー（エイリアス名または生 ID）
    /// 省略時は空 → "primary" にフォールバック
    #[serde(default)]
    pub default_calendars: Vec<String>,
}

/// 設定ファイル全体の構造
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub credentials: Credentials,
    #[serde(default)]
    pub token: Option<TokenSection>,
    #[serde(default)]
    pub ai: AiConfig,
    /// カレンダーエイリアス: エイリアス名 → Google カレンダー ID
    #[serde(default)]
    pub calendars: HashMap<String, String>,
    /// events コマンドの設定
    #[serde(default)]
    pub events: EventsConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSection {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default = "default_ai_base_url")]
    pub base_url: String,
    #[serde(default = "default_ai_model")]
    pub model: String,
    #[serde(default = "default_ai_enabled")]
    pub enabled: bool,
}

pub const DEFAULT_AI_BASE_URL: &str = "http://localhost:11434";
pub const DEFAULT_AI_MODEL: &str = "gemma3:4b";
const DEFAULT_AI_ENABLED: bool = true;

fn default_ai_base_url() -> String { DEFAULT_AI_BASE_URL.to_string() }
fn default_ai_model() -> String { DEFAULT_AI_MODEL.to_string() }
fn default_ai_enabled() -> bool { DEFAULT_AI_ENABLED }

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_AI_BASE_URL.to_string(),
            model: DEFAULT_AI_MODEL.to_string(),
            enabled: DEFAULT_AI_ENABLED,
        }
    }
}

impl Config {
    /// カレンダー名/エイリアスを Google カレンダー ID に解決する。
    /// エイリアスが登録されていない場合は入力をそのまま返す（"primary" 等も通る）。
    pub fn resolve_calendar_id(&self, input: &str) -> String {
        self.calendars.get(input).cloned().unwrap_or_else(|| input.to_string())
    }

    /// CLI 引数と設定から events コマンド用カレンダー ID リストを解決する。
    ///
    /// 優先順位:
    /// 1. `calendars` (--calendars カンマ区切り) が Some → 分割・解決・重複除去
    /// 2. `calendar` (--calendar 単一) が Some → 解決して 1 要素 Vec
    /// 3. 両方 None → `config.events.default_calendars` を解決（空なら ["primary"]）
    pub fn resolve_event_calendars(
        &self,
        calendar: Option<&str>,
        calendars: Option<&str>,
    ) -> Vec<String> {
        if let Some(multi) = calendars {
            let mut seen = std::collections::HashSet::new();
            return multi
                .split(',')
                .map(|s| self.resolve_calendar_id(s.trim()))
                .filter(|id| seen.insert(id.clone()))
                .collect();
        }
        if let Some(single) = calendar {
            return vec![self.resolve_calendar_id(single)];
        }
        // config デフォルト
        if !self.events.default_calendars.is_empty() {
            return self.events.default_calendars
                .iter()
                .map(|s| self.resolve_calendar_id(s))
                .collect();
        }
        vec!["primary".to_string()]
    }

    /// デフォルトの設定ファイルパスを返す（~/.config/gcal/config.toml）
    pub fn default_path() -> Result<PathBuf, GcalError> {
        let dir = dirs::config_dir()
            .ok_or_else(|| GcalError::ConfigError("設定ディレクトリが見つかりません".to_string()))?;
        Ok(dir.join("gcal").join("config.toml"))
    }

    /// 設定ファイルを読み込む
    pub fn load(path: &Path) -> Result<Self, GcalError> {
        if !path.exists() {
            return Err(GcalError::NotInitialized);
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| GcalError::ConfigError(format!("読み込みエラー: {e}")))?;
        toml::from_str(&content)
            .map_err(|e| GcalError::ConfigError(format!("パースエラー: {e}")))
    }

    /// 設定ファイルに書き込む（親ディレクトリがなければ作成）
    pub fn save(&self, path: &Path) -> Result<(), GcalError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| GcalError::ConfigError(format!("ディレクトリ作成エラー: {e}")))?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| GcalError::ConfigError(format!("シリアライズエラー: {e}")))?;
        std::fs::write(path, content)
            .map_err(|e| GcalError::ConfigError(format!("書き込みエラー: {e}")))
    }
}

/// ファイルベースの TokenStore 実装
pub struct FileTokenStore {
    path: PathBuf,
}

impl FileTokenStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl TokenStore for FileTokenStore {
    fn load_tokens(&self) -> Result<Option<StoredTokens>, GcalError> {
        match Config::load(&self.path) {
            Ok(config) => {
                let tokens = config.token.map(|t| StoredTokens {
                    access_token: t.access_token,
                    refresh_token: t.refresh_token,
                    expires_at: t.expires_at,
                });
                Ok(tokens)
            }
            Err(GcalError::NotInitialized) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn save_tokens(&self, tokens: &StoredTokens) -> Result<(), GcalError> {
        // 既存の config を読んで token セクションだけ更新する
        let mut config = match Config::load(&self.path) {
            Ok(c) => c,
            Err(GcalError::NotInitialized) => Config::default(),
            Err(e) => return Err(e),
        };

        config.token = Some(TokenSection {
            access_token: tokens.access_token.clone(),
            refresh_token: tokens.refresh_token.clone(),
            expires_at: tokens.expires_at,
        });

        config.save(&self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use tempfile::TempDir;

    fn temp_config_path(dir: &TempDir) -> PathBuf {
        dir.path().join("gcal").join("config.toml")
    }

    // --- AiConfig のデフォルト値テスト ---

    #[test]
    fn test_ai_config_default_model_is_gemma3() {
        assert_eq!(AiConfig::default().model, "gemma3:4b");
    }

    #[test]
    fn test_ai_config_default_base_url() {
        assert_eq!(AiConfig::default().base_url, "http://localhost:11434");
    }

    #[test]
    fn test_ai_config_default_enabled() {
        assert!(AiConfig::default().enabled);
    }

    #[test]
    fn test_config_load_without_ai_section_uses_defaults() {
        // v0.4.0 以前の設定ファイル（[ai] セクションなし）を読んでもデフォルト値が入る
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[credentials]\nclient_id = \"x\"\nclient_secret = \"y\"\n",
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        assert_eq!(config.ai.base_url, "http://localhost:11434");
        assert_eq!(config.ai.model, "gemma3:4b");
        assert!(config.ai.enabled);
    }

    #[test]
    fn test_load_returns_not_initialized_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        let result = Config::load(&path);
        assert!(matches!(result, Err(GcalError::NotInitialized)));
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);

        let config = Config {
            credentials: Credentials {
                client_id: "my_client_id".to_string(),
                client_secret: "my_secret".to_string(),
            },
            token: None,
            ai: AiConfig::default(),
            calendars: Default::default(),
            events: Default::default(),
        };
        config.save(&path).unwrap();

        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.credentials.client_id, "my_client_id");
        assert_eq!(loaded.credentials.client_secret, "my_secret");
        assert!(loaded.token.is_none());
    }

    #[test]
    fn test_save_creates_parent_directory() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir); // gcal/ ディレクトリはまだ存在しない

        let config = Config::default();
        config.save(&path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_file_token_store_load_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        let store = FileTokenStore::new(path);

        let result = store.load_tokens().unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_file_token_store_save_and_load() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        let store = FileTokenStore::new(path);

        let expires = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();
        let tokens = StoredTokens {
            access_token: "acc_token_123".to_string(),
            refresh_token: Some("ref_token_456".to_string()),
            expires_at: Some(expires),
        };

        store.save_tokens(&tokens).unwrap();
        let loaded = store.load_tokens().unwrap().expect("トークンが存在するはず");

        assert_eq!(loaded.access_token, "acc_token_123");
        assert_eq!(loaded.refresh_token.as_deref(), Some("ref_token_456"));
        assert_eq!(loaded.expires_at, Some(expires));
    }

    #[test]
    fn test_file_token_store_preserves_credentials_on_save() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);

        // まず credentials を含む config を作成
        let config = Config {
            credentials: Credentials {
                client_id: "cid".to_string(),
                client_secret: "csecret".to_string(),
            },
            token: None,
            ai: AiConfig::default(),
            calendars: Default::default(),
            events: Default::default(),
        };
        config.save(&path).unwrap();

        // token を上書き保存
        let store = FileTokenStore::new(path.clone());
        let tokens = StoredTokens {
            access_token: "new_acc".to_string(),
            refresh_token: None,
            expires_at: None,
        };
        store.save_tokens(&tokens).unwrap();

        // credentials が消えていないことを確認
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.credentials.client_id, "cid");
        assert_eq!(loaded.token.unwrap().access_token, "new_acc");
    }

    // --- resolve_calendar_id のテスト ---

    #[test]
    fn test_resolve_calendar_known_alias() {
        let mut config = Config::default();
        config.calendars.insert("仕事".to_string(), "work@group.calendar.google.com".to_string());
        assert_eq!(config.resolve_calendar_id("仕事"), "work@group.calendar.google.com");
    }

    #[test]
    fn test_resolve_calendar_primary_passthrough() {
        let config = Config::default();
        assert_eq!(config.resolve_calendar_id("primary"), "primary");
    }

    #[test]
    fn test_resolve_calendar_unknown_returns_input() {
        let config = Config::default(); // エイリアスなし
        assert_eq!(config.resolve_calendar_id("unknown_alias"), "unknown_alias");
    }

    #[test]
    fn test_resolve_calendar_raw_id_passthrough() {
        let config = Config::default();
        assert_eq!(
            config.resolve_calendar_id("abc@group.calendar.google.com"),
            "abc@group.calendar.google.com",
        );
    }

    #[test]
    fn test_config_load_without_calendars_section_uses_empty_map() {
        // v0.5.x 以前の設定ファイル（[calendars] セクションなし）でも空 HashMap になる
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[credentials]\nclient_id = \"x\"\nclient_secret = \"y\"\n",
        )
        .unwrap();
        let config = Config::load(&path).unwrap();
        assert!(config.calendars.is_empty());
    }

    #[test]
    fn test_config_save_and_load_with_calendars() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        let mut config = Config::default();
        config.credentials = Credentials { client_id: "cid".to_string(), client_secret: "cs".to_string() };
        config.calendars.insert("仕事".to_string(), "work@google.com".to_string());
        config.save(&path).unwrap();

        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.calendars.get("仕事").map(|s| s.as_str()), Some("work@google.com"));
    }

    // --- EventsConfig のテスト ---

    #[test]
    fn test_events_config_default_is_empty() {
        assert!(EventsConfig::default().default_calendars.is_empty());
    }

    #[test]
    fn test_config_load_without_events_section_uses_default() {
        // [events] セクションなしの旧設定ファイルでも空になる（後方互換）
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[credentials]\nclient_id = \"x\"\nclient_secret = \"y\"\n").unwrap();
        let config = Config::load(&path).unwrap();
        assert!(config.events.default_calendars.is_empty());
    }

    #[test]
    fn test_config_save_and_load_with_events() {
        let dir = TempDir::new().unwrap();
        let path = temp_config_path(&dir);
        let mut config = Config::default();
        config.credentials = Credentials { client_id: "cid".to_string(), client_secret: "cs".to_string() };
        config.events.default_calendars = vec!["仕事".to_string(), "個人".to_string()];
        config.save(&path).unwrap();

        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.events.default_calendars, vec!["仕事", "個人"]);
    }

    // --- resolve_event_calendars のテスト ---

    fn config_with_aliases() -> Config {
        let mut config = Config::default();
        config.calendars.insert("仕事".to_string(), "work@group.calendar.google.com".to_string());
        config.calendars.insert("個人".to_string(), "personal@group.calendar.google.com".to_string());
        config
    }

    #[test]
    fn test_resolve_event_calendars_single_flag() {
        // --calendar 仕事 → エイリアス解決して単一要素 Vec
        let config = config_with_aliases();
        let result = config.resolve_event_calendars(Some("仕事"), None);
        assert_eq!(result, vec!["work@group.calendar.google.com"]);
    }

    #[test]
    fn test_resolve_event_calendars_multi_flag() {
        // --calendars 仕事,個人 → カンマ分割してエイリアス解決
        let config = config_with_aliases();
        let result = config.resolve_event_calendars(None, Some("仕事,個人"));
        assert_eq!(result, vec![
            "work@group.calendar.google.com",
            "personal@group.calendar.google.com",
        ]);
    }

    #[test]
    fn test_resolve_event_calendars_multi_flag_with_spaces() {
        // --calendars "仕事, 個人" → trim して解決
        let config = config_with_aliases();
        let result = config.resolve_event_calendars(None, Some("仕事, 個人"));
        assert_eq!(result, vec![
            "work@group.calendar.google.com",
            "personal@group.calendar.google.com",
        ]);
    }

    #[test]
    fn test_resolve_event_calendars_deduplication() {
        // --calendars 仕事,仕事 → 重複除去して1件
        let config = config_with_aliases();
        let result = config.resolve_event_calendars(None, Some("仕事,仕事"));
        assert_eq!(result, vec!["work@group.calendar.google.com"]);
    }

    #[test]
    fn test_resolve_event_calendars_uses_config_defaults() {
        // --calendar / --calendars 未指定 → config.events.default_calendars を使う
        let mut config = config_with_aliases();
        config.events.default_calendars = vec!["仕事".to_string(), "個人".to_string()];
        let result = config.resolve_event_calendars(None, None);
        assert_eq!(result, vec![
            "work@group.calendar.google.com",
            "personal@group.calendar.google.com",
        ]);
    }

    #[test]
    fn test_resolve_event_calendars_fallback_to_primary() {
        // 何も指定なし・config も空 → ["primary"]
        let config = Config::default();
        let result = config.resolve_event_calendars(None, None);
        assert_eq!(result, vec!["primary"]);
    }

    #[test]
    fn test_resolve_event_calendars_raw_id_passthrough() {
        // エイリアス未登録の生 ID はそのまま通る
        let config = Config::default();
        let result = config.resolve_event_calendars(None, Some("abc@group.calendar.google.com"));
        assert_eq!(result, vec!["abc@group.calendar.google.com"]);
    }
}
