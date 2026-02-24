use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::domain::{CalendarSummary, EventQuery, EventSummary, OAuthCallback, StoredTokens};
use crate::error::GcalError;

/// 現在時刻を提供するトレイト（テスト時に固定時刻を注入するため）
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// ブラウザを開くトレイト（テスト時に no-op にするため）
pub trait BrowserOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<(), GcalError>;
}

/// OAuth2 認可コードを受け取るトレイト
/// - LoopbackReceiver: ローカル HTTP サーバーで受け取る
/// - ManualReceiver: ユーザーが URL を貼り付ける
pub trait AuthCodeReceiver: Send + Sync {
    /// OAuth2 プロバイダーに登録する redirect_uri を返す
    fn redirect_uri(&self) -> String;
    /// 認可コールバックを受け取って OAuthCallback を返す
    fn receive_code(&self) -> Result<OAuthCallback, GcalError>;
}

/// トークンの永続化トレイト
pub trait TokenStore: Send + Sync {
    fn load_tokens(&self) -> Result<Option<StoredTokens>, GcalError>;
    fn save_tokens(&self, tokens: &StoredTokens) -> Result<(), GcalError>;
}

/// 有効な access_token を返すトレイト（期限切れなら自動 refresh）
#[async_trait]
pub trait TokenProvider: Send + Sync {
    async fn access_token(&self) -> Result<String, GcalError>;
}

/// Google Calendar API クライアントトレイト
#[async_trait]
pub trait CalendarClient: Send + Sync {
    async fn list_calendars(&self) -> Result<Vec<CalendarSummary>, GcalError>;
    async fn list_events(&self, query: EventQuery) -> Result<Vec<EventSummary>, GcalError>;
}

// --- 本番用具体実装 ---

/// システムクロック（本番用）
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// 実際にブラウザを開く実装（本番用）
pub struct SystemBrowserOpener;

impl BrowserOpener for SystemBrowserOpener {
    fn open(&self, url: &str) -> Result<(), GcalError> {
        open::that(url).map_err(|e| GcalError::IoError(e))
    }
}
