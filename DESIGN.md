# gcal - Google Calendar CLI Tool: 設計ドキュメント

## プロジェクト概要

Google Calendar にアクセスし、カレンダーの一覧取得・読み書きを CLI で行う Rust 製ツール。

- ツール名: `gcal`
- インストール方法: `cargo install gcal`（crates.io 公開予定）
- Rust Edition: 2024

---

## バージョンロードマップ

| バージョン | 機能 |
|-----------|------|
| **v0.1.0** | 初期化（OAuth2 設定）、カレンダー一覧、直近1週間のイベント名取得 |
| v0.2.0 | 日付・期間指定フィルタ |
| v0.3.0 | イベント作成 |
| v0.4.0 | イベント更新・削除 |
| v0.5.0 | 複数カレンダー横断表示 |

---

## v0.1.0 スコープ

### 提供するコマンド

```
gcal init                          # OAuth2 認証情報の初期設定
gcal calendars                     # カレンダー一覧表示
gcal events [--calendar <id>]      # 直近1週間のイベント名一覧（デフォルト: primary）
```

### 各コマンドの動作

**`gcal init`**
1. Google Cloud Console で取得した client_id と client_secret の入力を求める
2. OAuth2 PKCE + state を生成し、認可 URL をブラウザで開く
3. ローカルの一時 HTTP リスナー（`127.0.0.1:エフェメラルポート`）でコールバックを受け取る
   - SSH 環境などでブラウザが開けない場合は手動入力フォールバックを提供
4. state 検証 → 認証コードを access_token / refresh_token と交換
5. トークンを設定ファイル `~/.config/gcal/config.toml` に保存

**`gcal calendars`**
- Google Calendar API の `/users/me/calendarList` を呼び出す
- カレンダーの ID・名前を一覧表示する

**`gcal events`**
- 対象カレンダーの `/calendars/{id}/events` を呼び出す
- 時間範囲: コマンド実行時刻 〜 7日後（`Clock` トレイトで注入、テスト可能）
- イベントの開始日時とサマリー（タイトル）を表示する
- デフォルトカレンダー: `primary`

---

## アーキテクチャ

### ディレクトリ構成

```
gcal/
├── Cargo.toml
├── DESIGN.md
└── src/
    ├── main.rs            # エントリポイント（最小限）: parse args + lib::run
    ├── lib.rs             # ライブラリルート: モジュールエクスポート + run()
    ├── cli.rs             # clap derive マクロでサブコマンド定義のみ
    ├── app.rs             # コマンドハンドラ（orchestration のみ、IO 詳細を知らない）
    ├── domain.rs          # データ構造体（CalendarSummary, EventSummary, StoredTokens 等）
    ├── ports.rs           # テスト可能にするためのトレイト群
    ├── config.rs          # 設定ファイルの読み書き（TokenStore 実装含む）
    ├── output.rs          # 表示フォーマット（純粋関数、テストしやすい）
    ├── error.rs           # thiserror でエラー型定義
    ├── auth/
    │   ├── mod.rs
    │   ├── flow.rs        # init コマンド: PKCE, ブラウザ起動, コールバック, コード交換
    │   ├── callback.rs    # AuthCodeReceiver 実装（LoopbackReceiver, ManualReceiver）
    │   └── provider.rs    # RefreshingTokenProvider（期限切れ自動更新）
    └── gcal_api/
        ├── mod.rs
        ├── client.rs      # GoogleCalendarHttpClient（reqwest + TokenProvider）
        └── models.rs      # API レスポンスの serde 構造体
```

### モジュール責務

| モジュール | 責務 |
|-----------|------|
| `main.rs` | 引数解析、`lib::run()` 呼び出しのみ |
| `lib.rs` | モジュール公開、run() で具体実装を組み立てて `app` へ渡す |
| `cli.rs` | clap のコマンド定義のみ、ロジックなし |
| `app.rs` | コマンドごとのハンドラ。依存は全てトレイト経由 |
| `domain.rs` | ビジネスロジックのデータ型（serde 含む） |
| `ports.rs` | テスト可能にするためのトレイト群（下記参照） |
| `config.rs` | TOML 読み書き、`TokenStore` の具体実装 |
| `output.rs` | カレンダー・イベントのテキスト整形（`Write` trait で出力） |
| `error.rs` | `GcalError` 統一エラー型 |
| `auth/flow.rs` | init コマンドの OAuth2 フロー全体 |
| `auth/callback.rs` | コールバック受信（ローカルサーバー / 手動入力） |
| `auth/provider.rs` | `RefreshingTokenProvider`: `TokenStore` + `Clock` でトークン自動更新 |
| `gcal_api/client.rs` | HTTP リクエスト実装、base_url をコンストラクタで受け取り（テスト注入可） |
| `gcal_api/models.rs` | API レスポンス JSON の構造体 |

---

## トレイト設計（`ports.rs`）

```rust
// テスト可能にするための境界トレイト
// 外部環境との接点のみをトレイト化し、過剰な抽象化を避ける

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub trait BrowserOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<(), GcalError>;
}

pub trait AuthCodeReceiver: Send + Sync {
    fn receive_code(&self) -> Result<OAuthCallback, GcalError>;
}

pub trait TokenStore: Send + Sync {
    fn load_tokens(&self) -> Result<Option<StoredTokens>, GcalError>;
    fn save_tokens(&self, tokens: &StoredTokens) -> Result<(), GcalError>;
}

#[async_trait]
pub trait TokenProvider: Send + Sync {
    // 常に有効な access_token を返す（必要なら自動 refresh）
    async fn access_token(&self) -> Result<String, GcalError>;
}

#[async_trait]
pub trait CalendarClient: Send + Sync {
    async fn list_calendars(&self) -> Result<Vec<CalendarSummary>, GcalError>;
    async fn list_events(&self, query: EventQuery) -> Result<Vec<EventSummary>, GcalError>;
}
```

### トレイト化する理由

| トレイト | 理由 |
|---------|------|
| `Clock` | "7日後" の計算をテストで固定時刻にするため |
| `BrowserOpener` | テスト中にブラウザを実際に開かないようにするため |
| `AuthCodeReceiver` | ローカルサーバー / 手動入力を切り替え可能にするため |
| `TokenStore` | config ファイル実装をテスト用 in-memory 実装に差し替えるため |
| `TokenProvider` | API クライアントのテストで OAuth を切り離すため |
| `CalendarClient` | app.rs のコマンドハンドラをネットワークなしでテストするため |

### トレイト化しないもの（過剰抽象化を避ける）

- `Config` 解析/書き込み → 具体的な `config.rs` 実装で十分
- `GoogleOAuthClient` の HTTP トークン交換 → `wiremock` でテスト
- `GoogleCalendarHttpClient` → `wiremock` でテスト（base_url 注入）

---

## `app.rs` のハンドラ構造

```rust
// app.rs のイメージ（型パラメータでトレイト注入）
pub struct App<CAL, CLK> {
    pub calendar_client: CAL,
    pub clock: CLK,
}

impl<CAL: CalendarClient, CLK: Clock> App<CAL, CLK> {
    pub async fn handle_calendars<W: Write>(&self, out: &mut W) -> Result<(), GcalError>;
    pub async fn handle_events<W: Write>(&self, days: u64, out: &mut W) -> Result<(), GcalError>;
}

// init コマンドは別の依存を持つため分離
pub async fn handle_init(
    browser: &dyn BrowserOpener,
    receiver: &dyn AuthCodeReceiver,
    store: &dyn TokenStore,
    client_id: String,
    client_secret: String,
) -> Result<(), GcalError>;
```

---

## トークン管理フロー

```
RefreshingTokenProvider
  ├── TokenStore（config.toml から読み書き）
  ├── Clock（期限切れ判定）
  └── 期限切れなら Google token endpoint へ refresh リクエスト
        └── 更新後に TokenStore へ保存

GoogleCalendarHttpClient
  └── TokenProvider.access_token() で毎回有効なトークンを取得
```

トークン更新ロジックは `app.rs` に漏れず `TokenProvider` の実装内に閉じる。

---

## 依存クレート

```toml
[dependencies]
# 非同期ランタイム
tokio = { version = "1", features = ["full"] }

# HTTP クライアント
reqwest = { version = "0.12", features = ["json"] }

# シリアライズ
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# CLI
clap = { version = "4", features = ["derive"] }

# OAuth2 (PKCE + state サポート)
oauth2 = "4"

# 非同期トレイト
async-trait = "0.1"

# 設定ディレクトリ解決
dirs = "5"

# エラーハンドリング
anyhow = "1"
thiserror = "2"

# 日時
chrono = { version = "0.4", features = ["serde"] }

# ターミナル入力（init で秘密情報入力）
rpassword = "7"

# ブラウザを開く
open = "5"

[dev-dependencies]
# CLI 統合テスト
assert_cmd = "2"
predicates = "3"

# ファイルシステム隔離
tempfile = "3"

# HTTP モックサーバー
wiremock = "0.6"

# テスト用シリアライズ
serde_json = "1"
```

---

## 設定ファイル仕様

パス: `~/.config/gcal/config.toml`

```toml
[credentials]
client_id = "..."
client_secret = "..."

[token]
access_token = "..."
refresh_token = "..."
expires_at = "2026-02-24T12:00:00Z"  # RFC3339
```

---

## Google Calendar API

- ベース URL: `https://www.googleapis.com/calendar/v3`（コンストラクタで注入、テスト時は wiremock サーバーの URL を使用）
- 認証: `Authorization: Bearer <access_token>`

### 使用するエンドポイント

| エンドポイント | メソッド | 用途 |
|--------------|---------|------|
| `/users/me/calendarList` | GET | カレンダー一覧 |
| `/calendars/{calendarId}/events` | GET | イベント一覧 |

### イベント取得パラメータ（v0.1.0）

```
timeMin = <現在時刻 RFC3339>
timeMax = <現在時刻 + 7日 RFC3339>
singleEvents = true
orderBy = startTime
```

---

## OAuth2 フロー詳細

### ライブラリ選定

`oauth2` crate（v4）を採用。`yup-oauth2`（Google 特化）より制御しやすく、
テスト用の seam（境界）を自分で設計できる。

### init フロー

1. ユーザーが `client_id`, `client_secret` を入力
2. PKCE (S256) + CSRF state を生成
3. 認可 URL をブラウザで開く（`BrowserOpener` 経由）
4. `TcpListener::bind("127.0.0.1:0")` でエフェメラルポートをリッスン
   - redirect_uri = `http://127.0.0.1:<port>/callback`
5. `GET /callback?code=...&state=...` を受け取る
6. state 検証 → PKCE verifier でトークン交換
7. `TokenStore::save_tokens()` で保存
8. SSH 環境向け: `--manual` フラグでブラウザ不使用、URL をコピーして貼り付け

### スコープ

```
https://www.googleapis.com/auth/calendar.readonly  # v0.1.0（読み取り専用）
```

---

## エラーハンドリング

```rust
// error.rs
#[derive(Debug, thiserror::Error)]
pub enum GcalError {
    #[error("未初期化: `gcal init` を実行してください")]
    NotInitialized,
    #[error("OAuth state 検証失敗")]
    OAuthStateMismatch,
    #[error("認証エラー: {0}")]
    AuthError(String),
    #[error("API エラー: {0}")]
    ApiError(String),
    #[error("設定ファイルエラー: {0}")]
    ConfigError(String),
    #[error("IO エラー: {0}")]
    IoError(#[from] std::io::Error),
}
```

---

## 出力フォーマット（v0.1.0）

### `gcal calendars`

```
ID                                   名前
-----------------------------------  --------------------
primary                              Densuke's Calendar
xxxxxxxx@group.calendar.google.com   Work
```

### `gcal events`

```
2026-02-24 (Mon)
  10:00  定例ミーティング
  14:00  1on1

2026-02-25 (Tue)
  09:00  朝会
```

---

## TDD 実装戦略

### テスト構成レイヤー

1. **純粋ユニットテスト**（ネットワーク・ファイルなし）
   - config のパス解決・読み書き
   - OAuth URL 生成・state/PKCE 生成
   - コールバック URL のパース
   - イベント時間ウィンドウ計算（`Clock` 固定）
   - 出力フォーマット（`output.rs`）

2. **HTTP 統合テスト**（`wiremock` でモックサーバー）
   - カレンダー一覧取得（認証ヘッダー・レスポンス検証）
   - イベント一覧取得（クエリパラメータ・JSON パース検証）
   - トークン期限切れ → 自動 refresh の流れ

3. **CLI 統合テスト**（`assert_cmd`）
   - `gcal calendars` の出力形式検証
   - `gcal events` の出力形式検証
   - 未初期化状態のエラーメッセージ検証

### テスト用ユーティリティ

```rust
// tests/helpers.rs
// FakeCalendarClient, FixedClock, InMemoryTokenStore, NoopBrowserOpener 等の
// 手書きフェイクを用意（mockall は必要になれば追加）
```

### TDD ワークフロー（各モジュール）

1. `error.rs` → 型定義（テスト不要）
2. `config.rs` → tempfile でパス隔離してテスト
3. `output.rs` → 純粋関数テスト
4. `auth/callback.rs` → ManualReceiver ユニットテスト
5. `gcal_api/client.rs` → wiremock で HTTP テスト
6. `auth/provider.rs` → InMemoryTokenStore + FixedClock でテスト
7. `app.rs` → FakeCalendarClient + FixedClock でコマンドハンドラテスト
8. `auth/flow.rs` → 結合が多いため最後に統合テスト

---

## `cargo install` 対応

```toml
[package]
name = "gcal"
version = "0.1.0"
edition = "2024"
description = "Google Calendar CLI tool"
license = "MIT"
repository = "https://github.com/densuke/gcal"
keywords = ["google", "calendar", "cli"]
categories = ["command-line-utilities"]
```

---

## 実装順序（v0.1.0 TDD）

| ステップ | 対象 | 内容 |
|---------|------|------|
| 1 | `Cargo.toml` | 依存クレートの追加 |
| 2 | `error.rs` | エラー型定義 |
| 3 | `domain.rs` | データ構造体 |
| 4 | `ports.rs` | トレイト定義 |
| 5 | `output.rs` | フォーマット（テスト先行） |
| 6 | `config.rs` | 設定ファイル（テスト先行） |
| 7 | `gcal_api/models.rs` | API レスポンス構造体 |
| 8 | `gcal_api/client.rs` | API クライアント（wiremock テスト先行） |
| 9 | `auth/callback.rs` | ManualReceiver（テスト先行）、LoopbackReceiver |
| 10 | `auth/provider.rs` | RefreshingTokenProvider（テスト先行） |
| 11 | `auth/flow.rs` | init フロー |
| 12 | `app.rs` | コマンドハンドラ（FakeClient テスト先行） |
| 13 | `cli.rs` | clap コマンド定義 |
| 14 | `lib.rs` / `main.rs` | 組み立て・統合 |
| 15 | CLI テスト | assert_cmd での E2E |
