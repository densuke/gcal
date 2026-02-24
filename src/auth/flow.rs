use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenUrl,
};

use crate::config::{AiConfig, Config, Credentials};
use crate::domain::StoredTokens;
use crate::error::GcalError;
use crate::ports::{AuthCodeReceiver, BrowserOpener, TokenStore};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar";

/// `gcal init` コマンドの実行ロジック
pub async fn run_init(
    browser: &dyn BrowserOpener,
    receiver: &dyn AuthCodeReceiver,
    store: &dyn TokenStore,
    config_path: &std::path::Path,
    client_id: String,
    client_secret: String,
    ai: AiConfig,
) -> Result<(), GcalError> {
    let redirect_uri = receiver.redirect_uri();
    let oauth_client = build_oauth_client(&client_id, &client_secret, redirect_uri)?;

    // PKCE チャレンジと CSRF state を生成
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf_token) = oauth_client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(CALENDAR_SCOPE.to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    println!("⚠ 書き込みスコープを要求します。既存のトークンがある場合は上書きされます。");
    println!("ブラウザで認証を行ってください...");
    browser.open(auth_url.as_str())?;

    println!("コールバックを待機中...");
    let callback = receiver.receive_code()?;

    // CSRF state の検証
    if callback.state != *csrf_token.secret() {
        return Err(GcalError::OAuthStateMismatch);
    }

    // 認証コードをトークンと交換
    let token_result = oauth_client
        .exchange_code(AuthorizationCode::new(callback.code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(oauth2::reqwest::async_http_client)
        .await
        .map_err(|e| GcalError::AuthError(format!("トークン交換失敗: {e}")))?;

    use oauth2::TokenResponse;
    let access_token = token_result.access_token().secret().clone();
    let refresh_token = token_result
        .refresh_token()
        .map(|t| t.secret().clone());
    let expires_at = token_result
        .expires_in()
        .map(|d| chrono::Utc::now() + chrono::Duration::from_std(d).unwrap_or_default());

    let tokens = StoredTokens {
        access_token,
        refresh_token,
        expires_at,
    };

    // credentials と token を保存（AI 設定も含める）
    let config = Config {
        credentials: Credentials {
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
        },
        token: None,
        ai,
    };
    config.save(config_path)?;
    store.save_tokens(&tokens)?;

    println!("認証が完了しました。");
    Ok(())
}

fn build_oauth_client(
    client_id: &str,
    client_secret: &str,
    redirect_uri: String,
) -> Result<BasicClient, GcalError> {
    let client = BasicClient::new(
        ClientId::new(client_id.to_string()),
        Some(ClientSecret::new(client_secret.to_string())),
        AuthUrl::new(AUTH_URL.to_string())
            .map_err(|e| GcalError::AuthError(format!("認証URL設定エラー: {e}")))?,
        Some(
            TokenUrl::new(TOKEN_URL.to_string())
                .map_err(|e| GcalError::AuthError(format!("トークンURL設定エラー: {e}")))?,
        ),
    )
    .set_redirect_uri(
        RedirectUrl::new(redirect_uri)
            .map_err(|e| GcalError::AuthError(format!("リダイレクトURL設定エラー: {e}")))?,
    );
    Ok(client)
}
