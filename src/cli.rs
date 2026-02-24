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

        /// 取得する日数（--date と同時指定不可）
        #[arg(long, conflicts_with = "date")]
        days: Option<u64>,

        /// 日付・期間を自然言語で指定（--days と同時指定不可）
        /// 例: 今日, 明日, 来週, 今月, 3/19, 3月19日, 2026/3/19, 3日後, 2週間後
        #[arg(long, conflicts_with = "days")]
        date: Option<String>,
    },
}
