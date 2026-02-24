use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gcal", version, about = "Google Calendar CLI tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// OAuth2 認証情報の初期設定
    Init {
        /// 手動入力モード（SSH 環境などブラウザが使えない場合）
        #[arg(long)]
        manual: bool,
    },
    /// カレンダーの一覧を表示
    Calendars,
    /// 直近のイベントを表示
    Events {
        /// 対象カレンダーの ID（デフォルト: primary）
        #[arg(long, default_value = "primary")]
        calendar: String,

        /// 取得する日数（--date / --from / --to と同時指定不可）
        #[arg(long, conflicts_with_all = ["date", "from", "to"])]
        days: Option<u64>,

        /// イベント ID を表示する
        #[arg(long)]
        ids: bool,

        /// 日付・期間を自然言語で指定（--days / --from / --to と同時指定不可）
        /// 例: 今日, 明日, 来週, 今月, 3/19, 3月19日, 3日後
        #[arg(long, conflicts_with_all = ["days", "from", "to"])]
        date: Option<String>,

        /// 開始日を自然言語で指定（--date / --days と同時指定不可）
        /// 例: 3/19, 今日, 来週月曜
        #[arg(long, conflicts_with_all = ["date", "days"])]
        from: Option<String>,

        /// 終了日を自然言語で指定（--date / --days と同時指定不可）
        /// 例: 3/25, 来週, 今月末
        #[arg(long, conflicts_with_all = ["date", "days"])]
        to: Option<String>,
    },
    /// 既存の予定を更新（--title / --start / --end のうち少なくとも1つ必須）
    Update {
        /// イベント ID
        event_id: String,
        /// 新しいタイトル
        #[arg(long)]
        title: Option<String>,
        /// 新しい開始日時（--end と同時指定必須）
        #[arg(long, requires = "end")]
        start: Option<String>,
        /// 新しい終了日時（--start と同時指定必須）
        #[arg(long, requires = "start")]
        end: Option<String>,
        /// カレンダーID（デフォルト: primary）
        #[arg(long, default_value = "primary")]
        calendar: String,
    },
    /// 既存の予定を削除
    Delete {
        /// イベント ID
        event_id: String,
        /// 確認をスキップして削除
        #[arg(short = 'f', long)]
        force: bool,
        /// カレンダーID（デフォルト: primary）
        #[arg(long, default_value = "primary")]
        calendar: String,
    },
    /// Google Calendar に予定を登録
    Add {
        /// 予定名
        title: String,
        /// 開始日時（例: "今日 14:00", "3/19 10:00"）
        #[arg(long)]
        start: String,
        /// 終了日時（省略時: 開始 +1時間）
        #[arg(long)]
        end: Option<String>,
        /// カレンダーID（デフォルト: primary）
        #[arg(long, default_value = "primary")]
        calendar: String,
    },
}
