# gcal - Google Calendar CLI Tool: 設計ドキュメント

## プロジェクト概要

Google Calendar にアクセスし、カレンダーの一覧取得・読み書きを CLI で行う Rust 製ツール。

- ツール名: `gcal`
- インストール方法: `cargo install gcal`（crates.io 公開予定）
- Rust Edition: 2024

---

## バージョンロードマップと履歴

| バージョン | リリース内容 |
|-----------|-------------|
| **v0.1.0** | 初期化（OAuth2 設定）、カレンダー一覧、直近1週間のイベント一覧（`calendars`, `events`） |
| **v0.2.0** | 自然言語による日付指定オプション（`--date`）追加 |
| **v0.2.1** | 自然言語による期間指定オプション（`--from`, `--to`）追加 |
| **v0.3.0** | イベント作成コマンド（`add`）追加 |
| **v0.3.1** | イベント更新・削除コマンド（`update`, `delete`）およびイベントID表示（`--ids`）追加 |
| **v0.3.2** | 指定日時範囲と相対時間による終了日時指定オプション追加 |
| **v0.3.3** | 繰り返し予定指定（`--repeat`）、通知指定（`--reminder`）追加 |
| **v0.4.0** | イベント場所指定（`--location`）追加、`date_parser`を`parser`モジュールとして責務ごとに分割リファクタリング（v0.3.3とv0.4.0の内容を1コミットでv0.4.0タグ付け） |
| **v0.5.0** | ローカルOllamaサーバー連携による自然言語からのイベント構造化・自動入力機能（`--ai`）追加 |

---

## 現在のスコープ (v0.5.0時点)

### 提供するコマンド

```
gcal init                          # OAuth2 認証情報の初期設定
gcal calendars                     # カレンダー一覧表示
gcal events [--calendar <id>]      # イベント一覧・検索
gcal add <title>                   # 予定の追加
gcal update <event_id>             # 予定の更新
gcal delete <event_id>             # 予定の削除
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
- 時間範囲: 指定がなければ実行時刻 〜 7日後（`Clock` トレイトで注入、テスト可能）
- `--date`, `--from`, `--to` を用いて、自然言語で柔軟に期間を指定可能（例: `今日`, `来週`, `3/19`）
- イベントの開始日時とサマリー（タイトル）を表示する
- `--ids` フラグでイベント ID も表示
- デフォルトカレンダー: `primary`

**`gcal add`**
- 対象カレンダーに新しいイベントを作成する（`/calendars/{id}/events` への POST リクエスト）
- 必須引数: イベント名 (`title`) または AIプロンプト (`--ai`)
- 時間の指定オプション: `--date` (範囲指定), `--start`, `--end` を利用して日時を柔軟に設定可能
- 場所の指定: `--location`
- 繰り返し・通知の指定（後述の「高度な入力パース」参照）
- AIサポート: `--ai` を使って日時・場所・タイトルを自然言語から自動抽出可能

**`gcal update`**
- 既存のイベントを更新する（`/calendars/{id}/events/{eventId}` への PUT リクエスト）
- オプション: `--title`, `--start`, `--end`, `--date`, `--location` を用いて一部または全てを更新可能
- 付随情報のクリア: `--clear-location`, `--clear-repeat`, `--clear-reminders` メディアクリアや新たなルール適用が可能
- AIサポート: `--ai` を使って更新内容を自然言語で指示可能
- 更新対象のイベントIDを必須とする

**`gcal delete`**
- 既存のイベントを削除する（`/calendars/{id}/events/{eventId}` への DELETE リクエスト）
- 確認プロンプトを表示（`--force` または `-f` オプションでスキップ可能）

---

## 日時解析 (`date_parser` モジュール)

CLIに入力される自然言語の日付や相対時間を解析し、`DateRange` や `DateTime<Local>` に変換するモジュールです。
- **キーワード**: `今日`, `明日`, `明後日`, `昨日`, `今週`, `来週`, `今月`, `来月`
- **相対日付**: `N日後`, `N週間後`
- **スラッシュ/日本語表記**: `MM/DD`, `YYYY/MM/DD`, `MM月DD日`, `YYYY年MM月DD日`
- **相対時間指定 (`--end`)**: `+1h`, `+30m`, `+1h30m` などの形式で終了時間を指定
**日時範囲のパース (`--date`)**: `"今日 12:00-13:00"`, `"明日 10:00+1h"` など、開始と終了を一括で解析

---

## 高度な入力パース

### 場所の指定 (Location) [v0.4.0追加]
イベントの開催場所（文字列）を保存するためのオプションです。
- **場所の指定**:
  - `--location <text>` (例: `--location "会議室A"`)
- **更新時のクリア**:
  - `--clear-location` (既存予定から場所情報を削除)

### 繰り返し予定 (Recurrence) [v0.4.0追加 (v0.3.3先行実装)]
Google Calendar の `recurrence` (RRULE形式の配列) に対し、ユーザーフレンドリーなDSLと生指定（エスケープハッチ）の2層構造を提供します。
- **シンプルなDSL**:
  - `--repeat <daily|weekly|monthly|yearly>`
  - `--every <N>` （例: 2週間ごとの場合 `--repeat weekly --every 2`）
  - `--on <mon,tue...>` （例: 毎週月・水曜日の場合 `--repeat weekly --on mon,wed`）
  - `--until <DATE>` または `--count <N>` 終了条件
- **生指定（エスケープハッチ）**:
  - `--recur "RRULE:FREQ=WEEKLY;UNTIL=20261231..."` (複数指定可)
- **更新時のクリア**:
  - `--clear-repeat` (既存予定から繰り返しを削除)

### 通知・リマインダー (Reminders) [v0.4.0追加 (v0.3.3先行実装)]
Google Calendar の `reminders` オブジェクトに対して、複数条件を直感的に指定できるようにします。
- **通知オーバーライド指定 (`--reminder`)**:
  - `メソッド:オフセット` フォーマットで指定（例: `--reminder popup:10m`, `--reminder email:1d`）
  - カンマ区切りおよび複数フラグの両方に対応
  - 時間単位: `m` (分), `h` (時間), `d` (日), `w` (週) をパースして分(minutes)に変換
- **プリセット指定 (`--reminders`)**:
  - `--reminders default` (カレンダーのデフォルト通知を利用)
  - `--reminders none` (通知なし)
- **更新時のクリア**:
  - `--clear-reminders` 

### AIによる自然言語解析 (Ollama連携) [v0.5.0追加]
ローカルのOllamaサーバーと通信し、自然言語からGoogle Calendarイベントの構造化データ（JSON）を自動生成します。
- **入力インターフェース**:
  - `--ai "<プロンプト>"` （例: `gcal add --ai "明日の14時から1時間、会議室Aでチームミーティング"`）
  - `--ai` の出力と既存コマンドライン引数が競合した場合、**明示的なコマンドライン引数を優先**する仕様（例: `--location` の明示を優先）
- **設定 (config.toml)**:
  - `[ai]` セクションにて以下を設定可能（CLI引数 `--ai-url`, `--ai-model` でオーバーライド可能）
  - `base_url`: (デフォルト: `http://localhost:11434`)
  - `model`: 使用するLLM名 (デフォルト: `llama3` またはユーザー環境のモデル)

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
    async fn insert_event(&self, calendar_id: &str, event: NewEvent) -> Result<EventSummary, GcalError>;
    async fn update_event(&self, calendar_id: &str, event_id: &str, event: UpdateEvent) -> Result<EventSummary, GcalError>;
    async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<(), GcalError>;
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
pub struct App<CAL> {
    pub calendar_client: CAL,
}

impl<CAL: CalendarClient> App<CAL> {
    pub async fn handle_calendars<W: Write>(&self, out: &mut W) -> Result<(), GcalError>;
    pub async fn handle_events<W: Write>(&self, calendar_id: &str, time_min: DateTime<Utc>, time_max: DateTime<Utc>, show_ids: bool, out: &mut W) -> Result<(), GcalError>;
    pub async fn handle_add_event<W: Write>(&self, event: NewEvent, out: &mut W) -> Result<(), GcalError>;
    pub async fn handle_update_event<W: Write>(&self, event: UpdateEvent, out: &mut W) -> Result<(), GcalError>;
    pub async fn handle_delete_event<W: Write>(&self, calendar_id: &str, event_id: &str, out: &mut W) -> Result<(), GcalError>;
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
| `/calendars/{calendarId}/events` | POST | イベント作成 |
| `/calendars/{calendarId}/events/{eventId}` | PUT | イベント更新 |
| `/calendars/{calendarId}/events/{eventId}` | DELETE | イベント削除 |

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
