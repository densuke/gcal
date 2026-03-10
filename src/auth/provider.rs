use async_trait::async_trait;
use chrono::Utc;

use crate::domain::StoredTokens;
use crate::error::GcalError;
use crate::ports::{Clock, TokenProvider, TokenStore};

/// Google のトークンリフレッシュエンドポイント
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

/// token_endpoint の URL を検証する。
/// - https:// かつホスト名が googleapis.com または *.googleapis.com のみ本番許可
/// - テスト用に http://127.0.0.1 / http://localhost も許可する
///
/// 文字列のプレフィックス/contains では "evil.com/.googleapis.com" のような
/// バイパスが可能なため、url クレートでホスト名を厳密にパースして確認する。
fn validate_token_endpoint(url_str: &str) -> Result<(), GcalError> {
    let parsed = url::Url::parse(url_str)
        .map_err(|_| GcalError::AuthError(format!("不正な token_endpoint URL: '{url_str}'")))?;

    let host = parsed
        .host_str()
        .ok_or_else(|| GcalError::AuthError(format!("token_endpoint にホストがありません: '{url_str}'")))?;

    if parsed.scheme() == "https"
        && (host == "googleapis.com" || host.ends_with(".googleapis.com"))
    {
        return Ok(());
    }
    if parsed.scheme() == "http" && (host == "127.0.0.1" || host == "localhost") {
        return Ok(());
    }
    Err(GcalError::AuthError(format!(
        "不正な token_endpoint: '{url_str}' \
        (https://*.googleapis.com または http://127.0.0.1 のみ許可)"
    )))
}

/// access_token の有効期限が切れていたら自動的に refresh するプロバイダー
pub struct RefreshingTokenProvider<S: TokenStore, C: Clock> {
    store: S,
    clock: C,
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
    /// テスト時のみ差し替え可能なエンドポイント
    token_endpoint: String,
}

impl<S: TokenStore, C: Clock> RefreshingTokenProvider<S, C> {
    pub fn new(store: S, clock: C, client_id: String, client_secret: String) -> Self {
        Self {
            store,
            clock,
            client_id,
            client_secret,
            http: reqwest::Client::new(),
            token_endpoint: TOKEN_ENDPOINT.to_string(),
        }
    }

    /// テスト用: token_endpoint を差し替えられるコンストラクタ
    pub fn with_token_endpoint(
        store: S,
        clock: C,
        client_id: String,
        client_secret: String,
        token_endpoint: impl Into<String>,
    ) -> Self {
        Self {
            store,
            clock,
            client_id,
            client_secret,
            http: reqwest::Client::new(),
            token_endpoint: token_endpoint.into(),
        }
    }

    async fn refresh(&self, refresh_token: &str) -> Result<StoredTokens, GcalError> {
        // token_endpoint は https:// かつ googleapis.com ドメインのみ許可
        // テスト時は with_token_endpoint() でモックサーバーを使うため
        // http://127.0.0.1 も許可する
        validate_token_endpoint(&self.token_endpoint)?;

        let params = [
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
        ];

        let resp = self
            .http
            .post(&self.token_endpoint)
            .form(&params)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let msg = resp.text().await.unwrap_or_default();
            return Err(GcalError::AuthError(format!(
                "トークン更新失敗 ({status}): {msg}"
            )));
        }

        #[derive(serde::Deserialize)]
        struct TokenResponse {
            access_token: String,
            expires_in: Option<i64>,
            refresh_token: Option<String>,
        }

        let token_resp: TokenResponse = resp.json().await?;
        let expires_at = token_resp
            .expires_in
            .map(|secs| Utc::now() + chrono::Duration::seconds(secs));

        Ok(StoredTokens {
            access_token: token_resp.access_token,
            // Google は新しい refresh_token を返さない場合があるので既存を維持
            refresh_token: token_resp.refresh_token.or(Some(refresh_token.to_string())),
            expires_at,
        })
    }
}

#[async_trait]
impl<S: TokenStore, C: Clock> TokenProvider for RefreshingTokenProvider<S, C> {
    async fn access_token(&self) -> Result<String, GcalError> {
        let tokens = self
            .store
            .load_tokens()?
            .ok_or(GcalError::NotInitialized)?;

        // 有効期限のチェック（30秒のバッファを持たせる）
        let needs_refresh = tokens
            .expires_at
            .map(|exp| exp - chrono::Duration::seconds(30) <= self.clock.now())
            .unwrap_or(false);

        if needs_refresh {
            let refresh_token = tokens
                .refresh_token
                .ok_or_else(|| GcalError::AuthError("refresh_token がありません".to_string()))?;

            let new_tokens = self.refresh(&refresh_token).await?;
            self.store.save_tokens(&new_tokens)?;
            return Ok(new_tokens.access_token);
        }

        Ok(tokens.access_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, TimeZone, Utc};
    use std::sync::{Arc, Mutex};
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::domain::StoredTokens;
    use crate::error::GcalError;
    use crate::ports::{Clock, TokenStore};

    // --- テスト用フェイク実装 ---

    struct FixedClock(DateTime<Utc>);
    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    #[derive(Clone)]
    struct InMemoryTokenStore {
        tokens: Arc<Mutex<Option<StoredTokens>>>,
    }

    impl InMemoryTokenStore {
        fn new(tokens: Option<StoredTokens>) -> Self {
            Self {
                tokens: Arc::new(Mutex::new(tokens)),
            }
        }
    }

    impl TokenStore for InMemoryTokenStore {
        fn load_tokens(&self) -> Result<Option<StoredTokens>, GcalError> {
            Ok(self.tokens.lock().unwrap().clone())
        }
        fn save_tokens(&self, t: &StoredTokens) -> Result<(), GcalError> {
            *self.tokens.lock().unwrap() = Some(t.clone());
            Ok(())
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 2, 24, 12, 0, 0).unwrap()
    }

    // --- テスト ---

    #[tokio::test]
    async fn test_returns_valid_token_without_refresh() {
        let future_expiry = now() + Duration::hours(1);
        let store = InMemoryTokenStore::new(Some(StoredTokens {
            access_token: "valid_token".to_string(),
            refresh_token: Some("ref".to_string()),
            expires_at: Some(future_expiry),
        }));
        let provider = RefreshingTokenProvider::new(
            store,
            FixedClock(now()),
            "cid".to_string(),
            "csecret".to_string(),
        );

        let token = provider.access_token().await.unwrap();
        assert_eq!(token, "valid_token");
    }

    #[tokio::test]
    async fn test_returns_error_when_not_initialized() {
        let store = InMemoryTokenStore::new(None);
        let provider = RefreshingTokenProvider::new(
            store,
            FixedClock(now()),
            "cid".to_string(),
            "csecret".to_string(),
        );

        let result = provider.access_token().await;
        assert!(matches!(result, Err(GcalError::NotInitialized)));
    }

    #[tokio::test]
    async fn test_refreshes_when_token_expired() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("grant_type=refresh_token"))
            .and(body_string_contains("refresh_token=old_refresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "new_access_token",
                "expires_in": 3600
            })))
            .mount(&server)
            .await;

        let expired = now() - Duration::hours(1); // 1時間前に期限切れ
        let store = InMemoryTokenStore::new(Some(StoredTokens {
            access_token: "old_expired_token".to_string(),
            refresh_token: Some("old_refresh".to_string()),
            expires_at: Some(expired),
        }));

        let provider = RefreshingTokenProvider::with_token_endpoint(
            store.clone(),
            FixedClock(now()),
            "cid".to_string(),
            "csecret".to_string(),
            format!("{}/token", server.uri()),
        );

        let token = provider.access_token().await.unwrap();
        assert_eq!(token, "new_access_token");

        // ストアに新しいトークンが保存されていることを確認
        let saved = store.load_tokens().unwrap().unwrap();
        assert_eq!(saved.access_token, "new_access_token");
    }

    #[tokio::test]
    async fn test_refresh_preserves_old_refresh_token_when_not_returned() {
        let server = MockServer::start().await;

        // Google がレスポンスで refresh_token を返さないパターン
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "new_token",
                "expires_in": 3600
                // refresh_token は含まない
            })))
            .mount(&server)
            .await;

        let expired = now() - Duration::hours(1);
        let store = InMemoryTokenStore::new(Some(StoredTokens {
            access_token: "old".to_string(),
            refresh_token: Some("keep_this_refresh".to_string()),
            expires_at: Some(expired),
        }));

        let provider = RefreshingTokenProvider::with_token_endpoint(
            store.clone(),
            FixedClock(now()),
            "cid".to_string(),
            "csecret".to_string(),
            format!("{}/token", server.uri()),
        );

        provider.access_token().await.unwrap();

        let saved = store.load_tokens().unwrap().unwrap();
        // 古い refresh_token が引き継がれること
        assert_eq!(saved.refresh_token.as_deref(), Some("keep_this_refresh"));
    }

    #[test]
    fn test_validate_token_endpoint_allows_googleapis() {
        assert!(validate_token_endpoint("https://oauth2.googleapis.com/token").is_ok());
        assert!(validate_token_endpoint("https://accounts.googleapis.com/token").is_ok());
    }

    #[test]
    fn test_validate_token_endpoint_allows_loopback_for_tests() {
        assert!(validate_token_endpoint("http://127.0.0.1:8080/token").is_ok());
        assert!(validate_token_endpoint("http://localhost:9000/token").is_ok());
    }

    #[test]
    fn test_validate_token_endpoint_rejects_arbitrary_https() {
        assert!(validate_token_endpoint("https://evil.example.com/token").is_err());
    }

    #[test]
    fn test_validate_token_endpoint_rejects_http_external() {
        assert!(validate_token_endpoint("http://evil.example.com/token").is_err());
    }

    #[test]
    fn test_validate_token_endpoint_rejects_contains_bypass() {
        // .contains(".googleapis.com") を使った場合にバイパスされるパターンを拒否すること
        assert!(validate_token_endpoint("https://evil.com/.googleapis.com/token").is_err());
        assert!(validate_token_endpoint("https://oauth2.googleapis.com.evil.jp/token").is_err());
        assert!(validate_token_endpoint("https://fake.googleapis.com.example.com/token").is_err());
    }
}
