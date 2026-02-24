use thiserror::Error;

#[derive(Debug, Error)]
pub enum GcalError {
    #[error("未初期化: `gcal init` を実行してください")]
    NotInitialized,

    #[error("OAuth state 検証失敗")]
    OAuthStateMismatch,

    #[error("認証エラー: {0}")]
    AuthError(String),

    #[error("API エラー ({status}): {message}")]
    ApiError { status: u16, message: String },

    #[error("JSON パースエラー: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("設定ファイルエラー: {0}")]
    ConfigError(String),

    #[error("IO エラー: {0}")]
    IoError(#[from] std::io::Error),

    #[error("コールバック待機タイムアウト")]
    CallbackTimeout,

    #[error("HTTP エラー: {0}")]
    HttpError(#[from] reqwest::Error),
}
