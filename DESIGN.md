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
| **v0.6.0** | 複数カレンダー横断 events 表示（`--calendars`、config デフォルト複数カレンダー）、終日予定ソート修正、終日ラベル表示幅修正 |
| **v0.6.1** | イベント終了時間・現在時刻マーカー・進行中インジケーター表示、シェル補完コマンド（`gcal shell bash/zsh`）、calendar_display_name 伝達、Rust 1.94.0 対応・依存クレート更新 |
| **v0.7.0** | LLM による自然文対応の強化: `gcal events -p` による CRUD 統合ディスパッチ、各コマンドへの `--prompt/-p` 追加（`--ai` の後継）、`delete -p` による ID 省略自然文削除 |
| **v0.7.1** | 設定ファイルの多層読み込み（ホーム/XDG/OS標準/指定パス）と可視化機能（`--show-config`, `--verbose`）追加。セキュリティ上の理由でカレントディレクトリ自動探索を廃止。依存クレート更新（reqwest 0.13, toml 1.0, dirs 6） |
| **v0.8.0** | GitHub Actions によるクロスビルド対応。macOS (arm64) / Linux (x86_64, arm64) 向けバイナリを GitHub Releases へ自動公開。TLS は rustls を全面採用（native-tls/OpenSSL 依存なし） |
| **v0.8.1** | GitHub Actions のビルドターゲットに Windows (x86_64) を追加。GitHub Releases の配布バイナリを macOS (arm64) / Linux (x86_64, arm64) / Windows (x86_64) に拡張。 |

---

## 現在のスコープ (v0.8.x)

### 提供するコマンド

```
gcal init                               # OAuth2 認証情報の初期設定
gcal calendars                          # カレンダー一覧表示
gcal calendars alias <name> <id>        # カレンダーエイリアスを追加・更新
gcal calendars aliases                  # 設定済みエイリアス一覧を表示
gcal calendars unalias <name>           # エイリアスを削除
gcal events [--calendar <id|alias>]     # イベント一覧・検索
gcal events --calendars <id|alias,...>  # 複数カレンダー横断イベント一覧
gcal events -p "<自然文>"               # 自然言語による CRUD 統合ディスパッチ
gcal add <title>                        # 予定の追加
gcal add -p "<自然文>"                  # 自然言語で予定を追加（--ai の後継）
gcal update <event_id>                  # 予定の更新
gcal update <event_id> -p "<自然文>"    # 自然言語で予定を更新
gcal delete <event_id>                  # 予定の削除
gcal delete -p "<自然文>"               # 自然言語でイベントを特定して削除
gcal shell <bash|zsh>                   # シェル補完スクリプトを出力
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
- `--calendars` でカンマ区切り複数カレンダーを横断表示（例: `gcal events --calendars 仕事,個人`）
- `--calendar` と `--calendars` は排他（`conflicts_with` で制御）
- config.toml の `[events] default_calendars` に設定しておくと引数省略可

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

**`gcal shell <bash|zsh>`**
- bash または zsh 向けのタブ補完スクリプトを標準出力に出力する
- `eval "$(gcal shell bash)"` を `~/.bashrc` に追加することで有効化
- `eval "$(gcal shell zsh)"` を `~/.zshrc` に追加することで有効化

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
    ├── event_selector.rs     # 自然文ターゲットに基づくイベント絞り込みロジック
    ├── prompt_flow.rs        # events -p 時のイベント取得・候補提示・入力受付フロー
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
        └── エイリアス登録なし → 入力をそのまま返す
                ↓
        main.rs の後処理（エイリアステーブルが空でない場合のみ）
                ├── "@" を含む → そのまま（生 ID として扱う）
                ├── "primary" → そのまま
                └── それ以外 → 警告 stderr + "primary" を返す
        ※ エイリアステーブルが空の場合は入力をそのまま API に渡す
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

## 設定の読み込みと優先順位

`gcal` は起動時に以下の順序で設定ファイルを探索し、見つかったものを順次読み込んでマージ（上書き）します。

| 優先度 | 種類 | パス (例) |
|-------|------|-----------|
| 1 (低) | ホーム直下 | `~/.gcal.toml` |
| 2 | XDG 準拠 | `~/.config/gcal/config.toml` |
| 3 | OS 標準 | `~/Library/Application Support/gcal/config.toml` (macOS) |
| 4 (高) | 指定パス | `--config <PATH>` で指定されたファイル |

> **注意**: カレントディレクトリ (`./.gcal.toml`) の自動探索は廃止済み。
> 悪意ある `.gcal.toml` による `ai.base_url` 上書き（SSRF起点）を防ぐため。
> 明示指定が必要な場合は `--config .gcal.toml` を使用すること。

### 設定の可視化オプション

- **`--show-config`**: 最終的にマージされた設定内容を一覧表示します。機密情報（APIキーやトークン）は自動的にマスクされます。
- **`-v, --verbose`**: どの設定ファイルが読み込まれ、どのファイルが存在しなかったか等の詳細なログを標準エラー出力に表示します。

### 設定ファイルの内容例 (TOML)

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

[events]
# デフォルトで取得するカレンダー（エイリアス名または生 ID）
# 省略時は "primary" のみ
default_calendars = ["仕事", "個人"]
```

`[ai]` / `[calendars]` / `[events]` セクションはいずれも省略可能（後方互換）。

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
| `/calendars/{calendarId}/events/{eventId}` | PATCH | イベント更新 |
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
2026-03-10 (Tue)
  —— 現在 (09:16) ——
  10:00-14:30  留学生オープンキャンパス
  17:30-21:00  送別会

2026-03-11 (Wed)
  終日          春分の日
  10:00-11:00  定例ミーティング
```

本日のイベント一覧では以下の情報が追加表示される:
- `HH:MM-HH:MM` 形式で開始・終了時刻を表示（終日イベントは `終日`）
- `—— 現在 (HH:MM) ——` マーカーで現在時刻の位置を示す（進行中イベントがない場合）
- `> ` プレフィックスで現在進行中のイベント (`start <= 現在 < end`) を示す

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

### カバレッジ目標: 80%+（fix/security-issues 実績: 302テスト）

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
reqwest = { version = "0.13", features = ["json", "form", "query"] }  # 0.13 で form/query が独立 feature に
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "1"
clap = { version = "4", features = ["derive"] }
clap_complete = "4"
oauth2 = "4"  # v5 は reqwest 0.12 依存で reqwest 0.13 と型不一致のため移行保留
async-trait = "0.1"
dirs = "6"
thiserror = "2"
chrono = { version = "0.4", features = ["serde"] }
iana-time-zone = "0.1"
rpassword = "7"
open = "5"
percent-encoding = "2"
url = "2"

[dev-dependencies]
tempfile = "3"
wiremock = "0.6"
assert_cmd = "2"
predicates = "3"
```

---

## バグ修正記録

### 終日予定が同日の時刻付き予定より後に表示される問題（v0.6.0）

#### 現象

```
2026-02-25 (Wed)
  08:00  ゴミ出し
  終日   ノー残業デー   ← 終日予定が時刻付き予定の後に出る
```

#### 原因

`app.rs` の旧ソートキーが UTC の `DateTime` を直接比較していた。
JST 08:00 の予定は UTC 前日 23:00 相当となり、終日予定の UTC 00:00 より前にソートされてしまっていた。

#### 修正（`src/app.rs`）

ソートキーを `(NaiveDate_ローカル日付, u8=0/1, 秒数)` に変更し、
終日予定（`u8=0`）を同日の時刻付き予定（`u8=1`）より必ず先にする。

```rust
fn event_sort_key(start: &EventStart) -> (NaiveDate, u8, u32) {
    match start {
        EventStart::Date(d) => (*d, 0, 0),
        EventStart::DateTime(dt) => {
            let local = dt.with_timezone(&Local);
            (local.date_naive(), 1, local.time().num_seconds_from_midnight())
        }
    }
}
```

#### 結果

```
2026-02-25 (Wed)
  終日   ノー残業デー   ← 終日予定が先頭
  08:00  ゴミ出し
```

### 終日ラベルの表示幅ずれ問題（v0.6.0）

#### 現象

```
2026-02-25 (Wed)
  終日     ノー残業デー   ← "終日" のタイトルが右にずれる
  08:00  ゴミ出し        ← "08:00" のタイトルは正常位置
```

#### 原因

`output.rs` のフォーマット指定 `{:5}` は**文字数**で5になるようパディングする。
`"終日"` は2文字なので3スペース補填 → 合計5文字・**7表示幅**。
`"08:00"` は5文字 = 5表示幅。表示上2カラム分ずれる。

#### 修正（`src/output.rs`）

`unicode-width` クレート非依存のインライン実装で全角文字を幅2として計算する
`char_display_width` + `pad_time_display` を追加。

```rust
fn pad_time_display(s: &str) -> String {
    const TARGET: usize = 5;
    let width: usize = s.chars().map(char_display_width).sum();
    let padding = TARGET.saturating_sub(width);
    format!("{}{}", s, " ".repeat(padding))
}
```

`{:5}` を `pad_time_display(&time_str)` に置き換え（`show_ids` 有無の両パス）。

#### 結果

```
2026-02-25 (Wed)
  終日   ノー残業デー   ← タイトル列が揃う
  08:00  ゴミ出し
```

---

## セキュリティ強化 (fix/security-issues)

### 概要

コードレビューで発見されたセキュリティ上の問題点を修正した。

### 修正一覧

| # | 対象ファイル | 問題 | 修正内容 |
|---|------------|------|---------|
| 1 | `config.rs` | 設定ファイルのパーミッション未設定 | `OpenOptions::mode(0o600)` で作成時から所有者のみ読み書き可に設定（TOCTOU 排除） |
| 2 | `auth/callback.rs` | 独自 URL デコード実装のバグ | `percent-encoding` クレートに置換、UTF-8 マルチバイト文字を正しく処理 |
| 3 | `auth/callback.rs` | OAuth コールバックのパス未検証 | `/callback` または `/` のみ受け入れ、不正パスはエラー |
| 4 | `auth/provider.rs` | `token_endpoint` の URL 検証不備 | `url` クレートでホスト名を厳密にパース、`googleapis.com` 以外を拒否（バイパス攻撃対策） |
| 5 | `ai/client.rs` | `base_url` のスキーム検証不備 | `https://` は全許可、`http://` はローカルホストのみ許可（外部 SSRF 対策） |
| 6 | `config.rs` | `mask_string` が先頭・末尾4文字を露出 | 非空文字列は常に `"********"` で完全マスク |
| 7 | `main.rs` | `gcal init` 時に Client ID を平文表示 | `"********"` に置換 |
| 8 | `main.rs` | カレントディレクトリ `.gcal.toml` の自動読み込みが SSRF 起点になる | CWD 自動探索を廃止。明示指定は `--config` フラグのみ |
| 9 | `config.rs` | `Config::load()` が不正パーミッションの設定ファイルを読み込む | Unix 環境で `load()` 時に `0o600`/`0o400` 以外はエラーを返す |
| 10 | `auth/provider.rs` | `expires_at` が None の場合にトークンリフレッシュを行わない | `unwrap_or(false)` → `unwrap_or(true)` に変更。有効期限不明時はリフレッシュを試みる |

### `config.rs` — ファイルパーミッション

Unix 環境では `OpenOptions` に `mode(0o600)` を指定し、ファイル作成の瞬間から
所有者のみ読み書き可能なパーミッションを適用する。
書き込み後に `set_permissions` を呼ぶ旧方式は TOCTOU（Time-of-Check to Time-of-Use）
競合が生じるため採用しない。

Windows 環境は `std::fs::write` にフォールバックする（OS 側のアクセス制御に依存）。

### `config.rs` — ロード時パーミッション検証

`Config::load()` は Unix 環境でファイルのパーミッションを検査し、`0o600` または `0o400`
以外の場合はエラーを返す。これにより、弱いパーミッションで作成された既存ファイル（旧バージョンや
手動作成）がある場合に早期検出できる。

```
エラー例:
設定ファイルのパーミッションが不正です: "~/.config/gcal/config.toml"
(現在: 0644, 必要: 0600 または 0400)
修正方法: chmod 600 "~/.config/gcal/config.toml"
```

### `main.rs` — カレントディレクトリ自動探索の廃止

`./.gcal.toml` の自動探索を廃止した。信頼できないリポジトリ内で `gcal` を実行した際、
悪意ある設定ファイルが `ai.base_url` を外部サーバーに向け、ユーザーの入力や
カレンダー情報を送信させる SSRF 攻撃の起点になり得るため。
設定ファイルを明示的に指定したい場合は `--config <PATH>` フラグを使用すること。

### `auth/provider.rs` — token_endpoint ホワイトリスト

`.contains(".googleapis.com")` による文字列マッチは
`https://evil.com/.googleapis.com/token` のような URL でバイパスされるため、
`url::Url::parse` でホスト名を厳密にパースした上で `ends_with(".googleapis.com")` で判定する。

---

## v0.6.0 変更内容

### 複数カレンダー横断 events 表示

`gcal events` で複数カレンダーのイベントを時間順に混在表示できるようになった。

```bash
# --calendars でカンマ区切り複数指定
gcal events --calendars 仕事,個人

# config.toml に default_calendars を設定しておくと引数省略可
gcal events
```

#### 変更ファイルと内容

| ファイル | 変更内容 |
|---------|---------|
| `src/config.rs` | `EventsConfig` 構造体追加、`Config.events` フィールド追加、`resolve_event_calendars()` メソッド追加 |
| `src/cli.rs` | `--calendars` オプション追加（`conflicts_with = "calendar"`）、`--calendar` を `Option<String>` に変更 |
| `src/app.rs` | `handle_events` を `&[String]` 受け取りに変更、複数カレンダーの順次取得 + 時間順ソート |
| `src/main.rs` | Events ディスパッチを `resolve_event_calendars` + 新 `handle_events` に接続 |

#### カレンダー解決優先順位

```
--calendars "仕事,個人"  → split(',') → 各エイリアス解決 → ["work_id", "personal_id"]
--calendar  "仕事"       → 単一エイリアス解決              → ["work_id"]
（両方未指定）
    config.events.default_calendars が非空 → 各エイリアス解決
    空                                     → ["primary"]
```

`--calendar` と `--calendars` は `conflicts_with` で排他。

#### app.rs の変更概要

全カレンダーのイベントをマージ後に `start` 昇順でソートし、既存の `write_events` に渡す。
`EventStart::Date(NaiveDate)` はソート比較のため `NaiveDate.and_hms_opt(0,0,0).and_utc()` に変換。

```rust
// 変更前
pub async fn handle_events(&self, calendar_id: &str, ...)

// 変更後: スライスで複数対応、順次取得 + 時間順ソート
pub async fn handle_events(&self, calendar_ids: &[String], ...)
```

### コードリファクタリング

| 変更 | 内容 |
|------|------|
| `gcal_api/client.rs` | `check_response_status()` ヘルパー抽出（5箇所の重複排除）、`local_timezone()` ヘルパー抽出 |
| `cli_mapper.rs` | `rustfmt` によるテストモジュールのインデント統一 |
| `main.rs` | `resolve_calendar_from_args()` ヘルパー抽出（Add/Update の重複4行を共通化） |

### テストカバレッジ改善

94.00% → 96.35%（239テスト）に向上。主な追加:
- `parser/duration.rs`: エラーパステスト追加 → 100%
- `parser/datetime.rs`: 曜日分岐・エラーパステスト追加 → 97.77%
- `auth/flow.rs`: CSRF ミスマッチテスト追加（モック実装使用）→ 60.95%
- `ports.rs` / `config.rs`: ユニットテスト追加

---

## v0.6.1 変更内容

### イベント終了時間・現在時刻マーカー・進行中インジケーター表示

`gcal events` の出力を改良し、現在時刻との相対関係が一目でわかるようになった。

```
2026-03-10 (Tue)
  —— 現在 (09:16) ——
  10:00-14:30  留学生オープンキャンパス  [3048561@2026-03-10]
  17:30-21:00  送別会 [3097054@2026-03-10]

# 進行中イベントがある場合
2026-03-10 (Tue)
> 09:10-09:30  サンプル [3097457@2026-03-10]
  10:00-14:30  留学生オープンキャンパス  [3048561@2026-03-10]
  17:30-21:00  送別会 [3097054@2026-03-10]
```

#### 変更ファイルと内容

| ファイル | 変更内容 |
|---------|---------|
| `src/domain.rs` | `EventSummary` に `end: Option<EventStart>` フィールド追加 |
| `src/gcal_api/models.rs` | `EventEntry` に `end: Option<EventStartTime>` フィールド追加 |
| `src/gcal_api/client.rs` | API レスポンスから終了時間をパースして `EventSummary.end` に設定 |
| `src/output.rs` | `write_events` に終了時間・現在時刻マーカー・進行中マーカー追加、`pad_time_display` の幅 5→11 |

#### 表示ルール

- 時刻付きイベント: `HH:MM-HH:MM` 形式（終了時間がない場合は `HH:MM` のみ）
- 終日イベント: `終日` （表示幅 11 にパディング）
- `—— 現在 (HH:MM) ——`: 本日の最初の未来イベントの前に挿入（進行中イベントがない場合のみ）
- `> ` プレフィックス: `start <= 現在 < end` のイベントに付与（進行中イベントがある場合は時刻マーカー非表示）

### シェル補完スクリプト生成コマンド

```bash
# bash: ~/.bashrc に追加
eval "$(gcal shell bash)"

# zsh: ~/.zshrc に追加
eval "$(gcal shell zsh)"
```

`clap_complete` クレートを利用して bash / zsh 向けの補完スクリプトを動的生成する。
サブコマンド名・オプション名がタブ補完の対象となる。

#### 変更ファイルと内容

| ファイル | 変更内容 |
|---------|---------|
| `Cargo.toml` | `clap_complete = "4"` を依存関係に追加 |
| `src/cli.rs` | `Commands::Shell { shell }` サブコマンドを追加 |
| `src/main.rs` | `Commands::Shell` ハンドラを追加（`clap_complete::generate` 呼び出し） |

### calendar_display_name の伝達

`resolve_calendar()` がカレンダー ID と表示名のタプル `(String, String)` を返すよう変更。
`NewEvent` / `UpdateEvent` の `calendar_display_name` にユーザー入力のエイリアス名を設定し、
dry-run やログ出力でカレンダー名を人間が読みやすい形で表示できるようにした。

| ファイル | 変更内容 |
|---------|---------|
| `src/cli_mapper.rs` | `AddCommandInput` / `UpdateCommandInput` に `calendar_display_name` フィールド追加 |
| `src/main.rs` | `resolve_calendar()` の戻り値を `(id, display_name)` タプルに変更 |

### Rust 1.94.0 対応・依存クレート更新

- Rust 1.94.0 でのビルド・テスト通過を確認
- エディション: `2024`（変更なし、既に最新）
- 23パッケージを最新互換バージョンに更新（`tokio 1.50.0`, `rustls 0.23.37`, `chrono 0.4.44` 等）
- 不要な `windows-sys` 関連クレートを削除

---

## v0.7.0 変更内容: LLM による自然文対応の強化

### 概要

CRUD 操作すべてを自然言語で実行できるようにする。

| 操作 | v0.6.1 まで | v0.7.0 |
|------|------------|--------|
| 作成 | `gcal add --ai "<prompt>"` | `gcal add -p "<prompt>"` (--ai は継続) |
| 更新 | `gcal update <id> --ai "<prompt>"` | `gcal update <id> -p "<prompt>"` |
| 削除 | `gcal delete <id>` (LLM なし) | `gcal delete -p "<prompt>"` (ID 省略可) |
| 統合 | なし | `gcal events -p "<prompt>"` (操作種別も LLM が判断) |
| 表示 | `gcal events [--date/--from/--to]` | `gcal events -p "来週の予定を見せて"` (日付範囲も自然文) |

---

### CLI 変更設計

#### `--prompt / -p` の追加

`AiArgs` に `--prompt / -p` を追加し、`--ai` の後継とする。
`--ai` は後方互換のため残す（内部で `prompt` に統合）。

```rust
pub struct AiArgs {
    /// 自然言語プロンプト（--ai の後継）
    #[arg(short = 'p', long, conflicts_with = "ai")]
    pub prompt: Option<String>,

    /// 自然言語プロンプト（後方互換、--prompt と排他）
    #[arg(long, conflicts_with = "prompt")]
    pub ai: Option<String>,

    // ... 既存フィールド
}
```

`CliMapper` 側で `prompt.or(ai)` として統合し、既存ロジックはそのまま使用する。

#### `gcal delete` の変更

`event_id` を `Option<String>` に変更し、`--prompt/-p` との排他グループで
「どちらか一方が必須」を表現する。

```rust
Delete {
    /// イベント ID（--prompt と排他、どちらか必須）
    #[arg(group = "target")]
    event_id: Option<String>,

    /// 自然言語でイベントを特定して削除（--ai と同義）
    #[arg(short = 'p', long, group = "target")]
    prompt: Option<String>,

    /// 後方互換 (--prompt と排他)
    #[arg(long, conflicts_with = "prompt")]
    ai: Option<String>,

    /// 確認をスキップ
    #[arg(short = 'f', long)]
    force: bool,

    /// カレンダー ID（デフォルト: primary）
    #[arg(long, default_value = "primary")]
    calendar: String,
}
```

`clap` の `ArgGroup` で `required = true` を指定し、
`event_id` または `prompt` のどちらかを必須とする。

#### `gcal events --prompt / -p` の追加

既存の `Events` コマンドに `-p/--prompt` を追加する。
他の操作系オプションとは排他にし、`-p` 指定時は CRUD ディスパッチモードに入る。

```rust
Events {
    // ... 既存フィールド

    /// 自然言語で CRUD 操作を実行（他の events オプションと排他）
    #[arg(short = 'p', long,
          conflicts_with_all = ["calendar", "calendars", "date", "from", "to", "days", "ids"])]
    prompt: Option<String>,

    // AI サーバー設定（--prompt 使用時）
    #[arg(long, requires = "prompt")]
    ai_url: Option<String>,
    #[arg(long, requires = "prompt")]
    ai_model: Option<String>,
    #[arg(short = 'y', long, requires = "prompt")]
    yes: bool,
}
```

---

### LLM スキーマ設計 (2段階方式)

`gcal events -p` のフローは **2段階** に分ける。
操作判定と引数抽出を分離することで、既存の `AiEventParameters` を再利用できる。

#### 第1段階: 操作ルータ (`AiOperationIntent`)

```rust
#[derive(Debug, Deserialize)]
pub struct AiOperationIntent {
    /// "add" | "update" | "delete" | "show"
    pub operation: String,
    /// update/delete/show 時のイベント特定・絞り込みヒント
    pub target: Option<AiEventTarget>,
}

#[derive(Debug, Deserialize)]
pub struct AiEventTarget {
    /// イベントタイトルのキーワード（部分一致検索に使用）
    pub title_hint: Option<String>,
    /// 対象イベントの日付ヒント（既存 parser で実日時に変換）
    pub date_hint: Option<String>,
    /// カレンダーエイリアス（未指定なら CLI 引数 or デフォルト）
    pub calendar: Option<String>,
}
```

システムプロンプト（第1段階）:
```
操作種別と対象イベントのヒントを JSON で返す。
{ "operation": "add"|"update"|"delete"|"show",
  "target": { "title_hint": "...", "date_hint": "...", "calendar": "..." } | null }
add の場合 target は null でよい。
```

#### 第2段階: 操作別パラメータ抽出

| 操作 | 第2段階 |
|------|---------|
| `add` | 既存 `parse_prompt()` → `AiEventParameters` |
| `update` | 対象イベント特定後、既存 `parse_prompt()` → `AiEventParameters` (変更分のみ) |
| `delete` | 対象イベント特定のみ、追加抽出なし |

---

### イベント特定フロー (update / delete)

`AiEventTarget` を受け取った後、Rust 側で決定論的に絞り込む。
LLM に候補選択は委ねない（誤動作リスクを避けるため）。

```
1. カレンダーの決定
   CLI --calendar > LLM target.calendar > config default_calendars > "primary"

2. 検索範囲の決定
   target.date_hint を既存 parser で実日時範囲に変換
   未指定の場合: 今日 -7日 〜 今日 +30日

3. イベント取得
   CalendarClient::list_events() でその範囲のイベントを取得

4. Rust 側での絞り込み（優先順）
   (a) タイトル完全一致
   (b) タイトル部分一致
   (c) 同日 + 開始時刻近傍

5. 候補数に応じた処理
   0件: エラー「該当するイベントが見つかりません」
   1件: dry-run 表示 → y/N 確認 → 実行
   複数件: 番号付きリスト表示 → 番号選択 → y/N 確認 → 実行
```

delete は曖昧な場合は必ず確認を求める（`--force` でも確認省略不可）。

---

### モジュール変更計画

| ファイル | 変更内容 |
|---------|---------|
| `src/ai/types.rs` | `AiOperationIntent`, `AiEventTarget` 構造体を追加 |
| `src/ai/client.rs` | `parse_operation_intent()` メソッドを追加（第1段階用） |
| `src/cli.rs` | `AiArgs` に `--prompt/-p` 追加、`Delete` に `event_id: Option` + ArgGroup 追加、`Events` に `--prompt/-p` 追加 |
| `src/cli_mapper.rs` | `prompt.or(ai)` で統合、`DeleteCommandInput` 追加 |
| `src/app.rs` | `handle_prompt_dispatch()` を追加（events -p のディスパッチ）、`handle_delete_by_prompt()` を追加 |
| `src/main.rs` | `Events`/`Delete` の新ハンドラを接続 |
| `src/domain.rs` | `EventSummary` に `location: Option<String>` 追加（イベント特定精度向上のため） |
| `src/gcal_api/models.rs` | `EventEntry` から `location` を `EventSummary` に伝搬 |

---

### 後方互換性の保証

| 既存コマンド | v0.7.0 での扱い |
|------------|----------------|
| `gcal add --ai "<prompt>"` | そのまま動作（`--ai` を `--prompt` に内部統合） |
| `gcal update <id> --ai "<prompt>"` | そのまま動作 |
| `gcal delete <id>` | そのまま動作（`event_id` が `Option` になるが ArgGroup で保証） |
| `gcal events` (一覧表示) | そのまま動作（`-p` 未指定時は既存フロー） |

---

### ユーザー体験イメージ

```bash
# 統合ディスパッチ: 追加
$ gcal events -p "明日の14時から1時間、会議室Aでチームミーティング"
[登録予定のイベント]
  タイトル: チームミーティング
  開始:     2026-03-11 14:00
  終了:     2026-03-11 15:00
  場所:     会議室A
登録しますか? [y/N]: y
登録しました。

# 統合ディスパッチ: 更新
$ gcal events -p "明日のチームミーティングを16時に変更して"
対象イベントを検索中...
  1. 2026-03-11 14:00-15:00  チームミーティング
このイベントを更新しますか? [y/N]: y
更新しました。

# 統合ディスパッチ: 削除
$ gcal events -p "明日のチームミーティングを削除して"
対象イベントを検索中...
  1. 2026-03-11 16:00-17:00  チームミーティング
このイベントを削除しますか? [y/N]: y
削除しました。

# 個別コマンドでの --prompt 使用
$ gcal add -p "来週月曜の午前中に歯医者"
$ gcal delete -p "来週月曜の歯医者"
$ gcal update abc123 -p "場所を〇〇クリニックに変更"
```

---

