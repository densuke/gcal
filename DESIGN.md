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
| **v0.3.2** | 指定日時範囲と相対時間による終了日時指定（`--date "今日 12:00-13:00"`, `--end +1h`）追加 |
| **v0.3.3** | 繰り返し予定指定（`--repeat`）、通知指定（`--reminder`）追加 |
| **v0.4.0** | イベント場所指定（`--location`）追加、`parser` モジュールを責務ごとに分割リファクタリング |
| **v0.5.0** | Ollama連携による自然言語入力（`--ai`）、カレンダーエイリアス管理（`calendars alias`）、AI確認プロンプト（`--yes`）、コードベースリファクタリング |
| **v0.5.1** | AI 入力精度向上（システムプロンプト改善、前日時刻リマインダーの `popup:prev-HH:MM` 形式導入、AI reminder=null 時のデフォルト除去） |

---

## 現在のスコープ (v0.5.1)

### 提供するコマンド

```
gcal init                               # OAuth2 認証情報の初期設定
gcal calendars                          # カレンダー一覧表示
gcal calendars alias <name> <id>        # カレンダーエイリアスを追加・更新
gcal calendars aliases                  # 設定済みエイリアス一覧を表示
gcal calendars unalias <name>           # エイリアスを削除
gcal events [--calendar <id|alias>]     # イベント一覧・検索
gcal add <title>                        # 予定の追加
gcal update <event_id>                  # 予定の更新
gcal delete <event_id>                  # 予定の削除
```

### 各コマンドの動作

**`gcal init`**
1. Google Cloud Console で取得した client_id と client_secret の入力を求める
2. OAuth2 PKCE + state を生成し、認可 URL をブラウザで開く
3. ローカルの一時 HTTP リスナー（`127.0.0.1:エフェメラルポート`）でコールバックを受け取る
   - SSH 環境などでブラウザが開けない場合は手動入力フォールバックを提供（`--manual`）
4. state 検証 → 認証コードを access_token / refresh_token と交換
5. トークンを設定ファイル `~/.config/gcal/config.toml` に保存
6. Ollama AI 設定（base_url / model）もインタラクティブに確認・保存

**`gcal calendars`**
- Google Calendar API の `/users/me/calendarList` を呼び出す
- カレンダーの ID・名前を一覧表示する

**`gcal calendars alias <name> <id>`**
- エイリアス名（例: `仕事`, `個人`）と Google カレンダー ID を紐付けて保存
- 既存エイリアスがあれば上書き（upsert）

**`gcal calendars aliases`**
- 設定済みエイリアスの一覧を表形式で表示

**`gcal calendars unalias <name>`**
- 指定したエイリアスを削除する

**`gcal events`**
- 対象カレンダーの `/calendars/{id}/events` を呼び出す
- 時間範囲: 指定がなければ実行時刻 〜 7日後
- `--date`, `--from`, `--to` を用いて自然言語で期間を指定可能（例: `今日`, `来週`, `3/19`）
- `--ids` フラグでイベント ID も表示
- `--calendar` にエイリアスを指定可能（例: `gcal events --calendar 仕事`）

**`gcal add`**
- 対象カレンダーに新しいイベントを作成する
- 必須引数: イベント名 (`title`) または AI プロンプト (`--ai`)
- 時間指定: `--date`（一括範囲指定）、`--start` / `--end`（個別指定、`--end` は相対可）
- 場所: `--location`
- 繰り返し・通知: 後述「高度な入力パース」参照
- AI サポート: `--ai` で自然言語から日時・場所・タイトル・通知を自動抽出
- AI 使用時は実行前に内容確認プロンプトを表示（`--yes` / `-y` でスキップ）
- `--calendar` にエイリアスを指定可能

**`gcal update`**
- 既存のイベントを更新する
- `--title`, `--start`, `--end`, `--date`, `--location` で一部または全体を更新
- `--clear-location`, `--clear-repeat`, `--clear-reminders` でフィールドをクリア
- AI サポート: `--ai` で更新内容を自然言語で指示
- AI 使用時は実行前に内容確認プロンプトを表示（`--yes` / `-y` でスキップ）
- `--calendar` にエイリアスを指定可能

**`gcal delete`**
- 既存のイベントを削除する
- 確認プロンプトを表示（`--force` / `-f` でスキップ）

---

## 日時解析 (`parser` モジュール)

`src/parser/` 以下に責務ごとに分割されたパーサー群。

- **キーワード**: `今日`, `明日`, `明後日`, `昨日`, `今週`, `来週`, `今月`, `来月`
- **相対日付**: `N日後`, `N週間後`
- **スラッシュ/日本語表記**: `MM/DD`, `YYYY/MM/DD`, `MM月DD日`, `YYYY年MM月DD日`
- **相対時間指定 (`--end`)**: `+1h`, `+30m`, `+1h30m` などで終了時間を指定
- **日時範囲のパース (`--date`)**: `"今日 12:00-13:00"`, `"明日 10:00+1h"` など一括指定

---

## 高度な入力パース

### 場所の指定 (Location) [v0.4.0]
- `--location <text>` で場所を設定（例: `--location "会議室A"`）
- `--clear-location` で既存予定から場所を削除（update 時）

### 繰り返し予定 (Recurrence) [v0.3.3 / v0.4.0]
Google Calendar の `recurrence`（RRULE 形式配列）に対しユーザーフレンドリーな DSL を提供。
- `--repeat <daily|weekly|monthly|yearly>`
- `--every <N>`（例: 2週間ごと → `--repeat weekly --every 2`）
- `--on <mon,tue,...>`（例: 月・水 → `--repeat weekly --on mon,wed`）
- `--until <DATE>` または `--count <N>` で終了条件
- `--recur "RRULE:..."` で生 RRULE を直接指定（複数可）
- `--clear-repeat` で繰り返しを削除（update 時）

### 通知・リマインダー (Reminders) [v0.3.3 / v0.4.0]
- `--reminder <method:offset>`（例: `--reminder popup:10m`, `--reminder email:1d`）
  - 時間単位: `m`（分）, `h`（時間）, `d`（日）, `w`（週）
  - 複数回指定可
- `--reminders default`（カレンダーのデフォルト通知）/ `--reminders none`（通知なし）
- `--clear-reminders` で通知を削除（update 時）

### カレンダーエイリアス (Calendar Alias) [v0.5.0]
設定ファイルの `[calendars]` テーブルにエイリアス名 → カレンダー ID の対応を保存する。
- `gcal calendars alias 仕事 <ID>` で登録・更新
- `gcal calendars unalias 仕事` で削除
- `--calendar 仕事` などでエイリアスを指定すると自動解決
- 未知のエイリアスを指定した場合は `primary` にフォールバック（警告表示）

### AI による自然言語解析 (Ollama 連携) [v0.5.0 / v0.5.1]
ローカルの Ollama サーバーと通信し、自然言語からイベントの構造化データを自動生成する。
- **入力**: `--ai "<プロンプト>"`（例: `gcal add --ai "明日の14時から1時間、会議室Aでチームミーティング"`）
- **優先順位**: CLI 引数 > AI 抽出値（明示した引数が常に優先）
- **確認フロー**: AI 使用時は登録/更新前に dry-run 出力を表示し y/N で確認（`--yes` / `-y` でスキップ）
- **dry-run**: `--dry-run` で実際の書き込みを行わず確認のみ
- **設定（config.toml `[ai]` セクション）**:
  - `base_url`: Ollama サーバー URL（デフォルト: `http://localhost:11434`）
  - `model`: 使用モデル（デフォルト: `gemma3:4b`）
  - `--ai-url`, `--ai-model` CLI オプションで設定をオーバーライド可能
- **抽出フィールド**: title, date, start, end, location, repeat_rule, reminder, calendar

#### AI リマインダーフォーマット [v0.5.1]

AI は算術計算なしで以下の形式で通知を出力する。Rust 側で分数へ変換する。

| ユーザー指示 | AI 出力 | 変換結果 |
|------------|---------|---------|
| `30分前` | `popup:30m` | 30分前 |
| `2時間前` | `popup:2h` | 120分前 |
| `前日19時`（開始 08:30） | `popup:prev-19:00` | 810分前 ※ |
| `前日17時`（開始 10:00） | `popup:prev-17:00` | 1020分前 ※ |

※ `popup:prev-HH:MM` は `cli_mapper.rs` が開始時刻から計算: `(開始時:分) + (24*60 - HH*60 - MM)`

複数通知はカンマ区切り: `"前日19時と2時間前"` → `"popup:prev-19:00,popup:2h"`

AI が reminder を抽出しなかった場合（`null`）は通知なし（カレンダーのデフォルト通知を使用）。
v0.5.0 以前は `popup:10m` に強制デフォルトしていたが v0.5.1 で廃止。

---

## アーキテクチャ

### ディレクトリ構成

```
gcal/
├── Cargo.toml
├── DESIGN.md
└── src/
    ├── main.rs               # エントリポイント: CLI パース + コマンドディスパッチ
    ├── lib.rs                # ライブラリルート: モジュールエクスポート
    ├── cli.rs                # clap derive マクロでサブコマンド定義
    │                         #   RecurrenceArgs / ReminderArgs / AiArgs を flatten
    ├── cli_mapper.rs         # CLI 引数 → ドメインオブジェクト変換（AI マージロジック含む）
    ├── alias_handler.rs      # カレンダーエイリアス管理（handle_set/list/remove_alias）
    ├── app.rs                # ネットワーク系コマンドハンドラ（App<CAL>）
    ├── domain.rs             # データ構造体（CalendarSummary, EventSummary, NewEvent 等）
    ├── ports.rs              # テスト可能にするためのトレイト群
    ├── config.rs             # 設定ファイルの読み書き（TokenStore 実装含む）
    ├── output.rs             # 表示フォーマット（純粋関数）
    ├── error.rs              # thiserror でエラー型定義
    ├── ai/
    │   ├── mod.rs
    │   ├── client.rs         # OllamaClient: HTTP でプロンプト送信 → AiEventParameters
    │   └── types.rs          # AiEventParameters（AI 抽出結果の構造体）
    ├── auth/
    │   ├── mod.rs
    │   ├── flow.rs           # init コマンド: PKCE, ブラウザ起動, コールバック, コード交換
    │   ├── callback.rs       # AuthCodeReceiver 実装（LoopbackReceiver, ManualReceiver）
    │   └── provider.rs       # RefreshingTokenProvider（期限切れ自動更新）
    ├── gcal_api/
    │   ├── mod.rs
    │   ├── client.rs         # GoogleCalendarClient（reqwest + TokenProvider）
    │   └── models.rs         # API レスポンスの serde 構造体
    └── parser/
        ├── mod.rs
        ├── datetime.rs       # 自然言語日時・期間パース
        ├── duration.rs       # 相対時間（+1h, +30m）パース
        ├── recurrence.rs     # RRULE 生成
        ├── reminders.rs      # EventReminders 生成
        └── util.rs           # 共通ユーティリティ
```

### モジュール責務

| モジュール | 責務 |
|-----------|------|
| `main.rs` | CLI パース、コマンドディスパッチ、`confirm_or_cancel` / `prompt` ユーティリティ |
| `lib.rs` | モジュール公開 |
| `cli.rs` | clap コマンド定義のみ。`RecurrenceArgs` / `ReminderArgs` / `AiArgs` を flatten で共通化 |
| `cli_mapper.rs` | CLI 引数 → ドメインオブジェクト変換、AI/CLI マージ優先順位の制御 |
| `alias_handler.rs` | エイリアス CRUD（config.toml の `[calendars]` テーブル操作） |
| `app.rs` | ネットワーク系コマンドハンドラ。依存はすべてトレイト経由 |
| `domain.rs` | ビジネスロジックのデータ型（serde 含む） |
| `ports.rs` | テスト可能にするためのトレイト群 |
| `config.rs` | TOML 読み書き、`TokenStore` 実装、`AiConfig` デフォルト値 const 管理 |
| `output.rs` | カレンダー・イベントのテキスト整形（`Write` trait で出力） |
| `error.rs` | `GcalError` 統一エラー型 |
| `ai/client.rs` | Ollama HTTP API 呼び出し、JSON 応答を `AiEventParameters` にパース |
| `ai/types.rs` | `AiEventParameters`（title, date, start, end, location, repeat_rule, reminder, calendar） |
| `auth/flow.rs` | init コマンドの OAuth2 フロー全体 |
| `auth/callback.rs` | コールバック受信（ローカルサーバー / 手動入力） |
| `auth/provider.rs` | `RefreshingTokenProvider`: `TokenStore` + `Clock` でトークン自動更新 |
| `gcal_api/client.rs` | HTTP リクエスト実装（base_url コンストラクタ注入でテスト可能） |
| `gcal_api/models.rs` | API レスポンス JSON の構造体 |
| `parser/datetime.rs` | 自然言語日時・期間パース |
| `parser/recurrence.rs` | `--repeat` 等の DSL → RRULE 変換（`today: NaiveDate` 引数でテスト可能） |
| `parser/reminders.rs` | `--reminder popup:10m` 等 → `EventReminders` 変換 |

---

## トレイト設計（`ports.rs`）

```rust
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub trait BrowserOpener: Send + Sync {
    fn open(&self, url: &str) -> Result<(), GcalError>;
}

pub trait AuthCodeReceiver: Send + Sync {
    fn redirect_uri(&self) -> String;
    fn receive_code(&self) -> Result<OAuthCallback, GcalError>;
}

pub trait TokenStore: Send + Sync {
    fn load_tokens(&self) -> Result<Option<StoredTokens>, GcalError>;
    fn save_tokens(&self, tokens: &StoredTokens) -> Result<(), GcalError>;
}

pub trait TokenProvider: Send + Sync {
    async fn access_token(&self) -> Result<String, GcalError>;
}

pub trait CalendarClient: Send + Sync {
    async fn list_calendars(&self) -> Result<Vec<CalendarSummary>, GcalError>;
    async fn list_events(&self, query: EventQuery) -> Result<Vec<EventSummary>, GcalError>;
    async fn create_event(&self, event: NewEvent) -> Result<String, GcalError>;
    async fn update_event(&self, event: UpdateEvent) -> Result<(), GcalError>;
    async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<(), GcalError>;
}
```

### トレイト化する理由

| トレイト | 理由 |
|---------|------|
| `Clock` | テストで現在時刻を固定するため |
| `BrowserOpener` | テスト中にブラウザを実際に開かないようにするため |
| `AuthCodeReceiver` | ローカルサーバー / 手動入力を切り替え可能にするため |
| `TokenStore` | config ファイル実装をテスト用 in-memory 実装に差し替えるため |
| `TokenProvider` | API クライアントのテストで OAuth を切り離すため |
| `CalendarClient` | `app.rs` ハンドラをネットワークなしでテストするため |

---

## CLI 引数設計（v0.5.1）

`Add` / `Update` で共通するオプションを flatten 構造体にまとめ重複を排除している。

```rust
// 繰り返し設定（Add / Update 共通）
pub struct RecurrenceArgs {
    pub repeat: Option<String>,   // daily|weekly|monthly|yearly
    pub every: Option<u32>,
    pub on: Option<String>,       // カンマ区切り曜日
    pub until: Option<String>,
    pub count: Option<u32>,
    pub recur: Option<Vec<String>>, // 生 RRULE
}

// リマインダー設定（Add / Update 共通）
pub struct ReminderArgs {
    pub reminder: Option<Vec<String>>, // popup:10m, email:1d など
    pub reminders: Option<String>,     // "default" | "none"
}

// AI・実行制御フラグ（Add / Update 共通）
pub struct AiArgs {
    pub ai: Option<String>,
    pub ai_url: Option<String>,
    pub ai_model: Option<String>,
    pub dry_run: bool,
    pub yes: bool,              // -y / --yes: AI 確認プロンプトをスキップ
}
```

---

## カレンダー解決フロー

```
CLI --calendar <input>
        ↓
resolve_calendar() in main.rs
        ↓
Config::resolve_calendar_id(input)
        ├── エイリアス登録あり → 対応する Google カレンダー ID を返す
        └── エイリアス登録なし
                ├── "@" を含む → そのまま（生 ID として扱う）
                ├── "primary" → そのまま
                └── それ以外 → 警告 stderr + "primary" を返す
```

---

## トークン管理フロー

```
RefreshingTokenProvider
  ├── TokenStore（config.toml から読み書き）
  ├── Clock（期限切れ判定）
  └── 期限切れなら Google token endpoint へ refresh リクエスト
        └── 更新後に TokenStore へ保存

GoogleCalendarClient
  └── TokenProvider.access_token() で毎回有効なトークンを取得
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

[ai]
base_url = "http://localhost:11434"   # Ollama サーバー URL
model = "gemma3:4b"                   # 使用モデル
enabled = true

[calendars]
仕事 = "work@group.calendar.google.com"
個人 = "personal@group.calendar.google.com"
```

`[ai]` / `[calendars]` セクションはいずれも省略可能（後方互換）。

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

---

## エラーハンドリング

```rust
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

## 出力フォーマット

### `gcal calendars`

```
ID                                   名前
-----------------------------------  --------------------
primary                         *    Densuke's Calendar
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

### `gcal add --dry-run` / AI 確認プロンプト

```
[登録予定のイベント]
タイトル: チームミーティング
日時  : 2026-02-25 (Wed) 14:00 - 15:00
カレンダー: primary
場所  : 会議室A
繰り返し: (なし)
通知  : アプリ通知 10分前

この内容で登録しますか? [y/N]:
```

---

## テスト方針

### カバレッジ目標: 80%+（v0.5.1 実績: 93.1%, テスト数: 206）

### テストレイヤー

1. **純粋ユニットテスト**（ネットワーク・ファイルなし）
   - `parser/` 各モジュール: 自然言語日時・RRULE 生成・リマインダーパース
   - `cli_mapper.rs`: AI/CLI マージロジック
   - `cli.rs`: clap 引数バリデーション
   - `output.rs`: テキスト整形
   - `config.rs`: TOML 読み書き、エイリアス解決
   - `alias_handler.rs`: エイリアス CRUD（tempfile 隔離）

2. **HTTP 統合テスト**（`wiremock` でモックサーバー）
   - `gcal_api/client.rs`: カレンダー一覧・イベント取得・作成・更新・削除
   - `auth/provider.rs`: トークン期限切れ → 自動 refresh

3. **ネットワークテスト**
   - `auth/callback.rs`: LoopbackReceiver の TCP 接続テスト

---

## 依存クレート

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
clap = { version = "4", features = ["derive"] }
oauth2 = "4"
async-trait = "0.1"
dirs = "5"
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
iana-time-zone = "0.1"
rpassword = "7"
open = "5"

[dev-dependencies]
tempfile = "3"
wiremock = "0.6"
assert_cmd = "2"
predicates = "3"
```
