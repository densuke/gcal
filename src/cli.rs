use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gcal", version, about = "Google Calendar CLI tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// 繰り返し設定（Add / Update で共通）
#[derive(Args, Debug, Default)]
pub struct RecurrenceArgs {
    /// 繰り返し設定: daily, weekly, monthly, yearly
    #[arg(long, value_parser = ["daily", "weekly", "monthly", "yearly"])]
    pub repeat: Option<String>,
    /// 繰り返しの間隔
    #[arg(long, requires = "repeat")]
    pub every: Option<u32>,
    /// 繰り返しの曜日指定 (カンマ区切り)
    #[arg(long, requires = "repeat")]
    pub on: Option<String>,
    /// 繰り返しの終了期日 (YYYY-MM-DD or タイムスタンプ)
    #[arg(long, requires = "repeat", conflicts_with = "count")]
    pub until: Option<String>,
    /// 繰り返しの回数
    #[arg(long, requires = "repeat", conflicts_with = "until")]
    pub count: Option<u32>,
    /// RRULE生指定 (複数可)
    #[arg(long, conflicts_with = "repeat")]
    pub recur: Option<Vec<String>>,
}

/// リマインダー設定（Add / Update で共通）
#[derive(Args, Debug, Default)]
pub struct ReminderArgs {
    /// リマインダー通知 (複数可) 例: popup:10m, email:1d
    #[arg(long)]
    pub reminder: Option<Vec<String>>,
    /// リマインダーのプリセット: default または none
    #[arg(long, conflicts_with = "reminder", value_parser = ["default", "none"])]
    pub reminders: Option<String>,
}

/// AI・実行制御フラグ（Add / Update で共通）
#[derive(Args, Debug, Default)]
pub struct AiArgs {
    /// AIに渡す自然言語プロンプト（例: "明日の14時から会議室Aでミーティング"）
    #[arg(long)]
    pub ai: Option<String>,
    /// AI サーバーのベースURL（設定をオーバーライド）
    #[arg(long)]
    pub ai_url: Option<String>,
    /// AI への問い合わせ時に使用するモデル（設定をオーバーライド）
    #[arg(long)]
    pub ai_model: Option<String>,
    /// カレンダーへの書き込みを行わず、実行予定の内容を表示して終了
    #[arg(long)]
    pub dry_run: bool,
    /// --ai 使用時の確認プロンプトをスキップして即時実行
    #[arg(short = 'y', long)]
    pub yes: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// OAuth2 認証情報の初期設定
    Init {
        /// 手動入力モード（SSH 環境などブラウザが使えない場合）
        #[arg(long)]
        manual: bool,
    },
    /// カレンダーの操作（引数なしで一覧表示）
    Calendars {
        #[command(subcommand)]
        sub: Option<CalendarSubcommands>,
    },
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
        #[arg(long, conflicts_with_all = ["days", "from", "to"])]
        date: Option<String>,
        /// 開始日を自然言語で指定（--date / --days と同時指定不可）
        #[arg(long, conflicts_with_all = ["date", "days"])]
        from: Option<String>,
        /// 終了日を自然言語で指定（--date / --days と同時指定不可）
        #[arg(long, conflicts_with_all = ["date", "days"])]
        to: Option<String>,
    },
    /// 既存の予定を更新（--title / --start・--end / --date のうち少なくとも1つ必須）
    Update {
        /// イベント ID
        event_id: String,
        /// 新しいタイトル
        #[arg(long)]
        title: Option<String>,
        /// 新しい開始日時（--end と同時指定必須、--date と排他）
        #[arg(long, requires = "end", conflicts_with = "date")]
        start: Option<String>,
        /// 新しい終了日時（--start と同時指定必須、--date と排他）
        /// 相対指定可: "+1h", "+30m", "+1h30m"
        #[arg(long, requires = "start", conflicts_with = "date")]
        end: Option<String>,
        /// 開始〜終了を一括指定（--start / --end と排他）
        #[arg(long, conflicts_with_all = ["start", "end"])]
        date: Option<String>,
        /// 繰り返し指定をクリア
        #[arg(long)]
        clear_repeat: bool,
        /// 通知をクリア
        #[arg(long)]
        clear_reminders: bool,
        /// 場所をクリア
        #[arg(long)]
        clear_location: bool,
        /// 場所を更新します
        #[arg(long)]
        location: Option<String>,
        /// カレンダーID またはエイリアス（例: 仕事, 個人）
        #[arg(long)]
        calendar: Option<String>,
        #[command(flatten)]
        recurrence: RecurrenceArgs,
        #[command(flatten)]
        reminder_args: ReminderArgs,
        #[command(flatten)]
        ai_args: AiArgs,
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
        /// 予定名（--ai を指定する場合は省略可）
        title: Option<String>,
        /// 開始〜終了を一括指定（--start / --end と排他）
        #[arg(long, conflicts_with_all = ["start", "end"])]
        date: Option<String>,
        /// 開始日時（--date と排他、例: "今日 14:00", "3/19 10:00"）
        #[arg(long, conflicts_with = "date")]
        start: Option<String>,
        /// 終了日時（省略時: 開始 +1時間、相対指定可: "+1h", "+30m"、--date と排他）
        #[arg(long, conflicts_with = "date")]
        end: Option<String>,
        /// 場所を設定します（例: 東京タワー, 会議室A）
        #[arg(long)]
        location: Option<String>,
        /// カレンダーID またはエイリアス（例: 仕事, 個人）
        #[arg(long)]
        calendar: Option<String>,
        #[command(flatten)]
        recurrence: RecurrenceArgs,
        #[command(flatten)]
        reminder_args: ReminderArgs,
        #[command(flatten)]
        ai_args: AiArgs,
    },
}

#[derive(Subcommand)]
pub enum CalendarSubcommands {
    /// カレンダーエイリアスを追加または更新（例: gcal calendars alias 仕事 <ID>）
    Alias {
        /// エイリアス名（例: 仕事, 個人, g）
        name: String,
        /// Google カレンダー ID
        calendar_id: String,
    },
    /// 設定済みエイリアス一覧を表示
    Aliases,
    /// エイリアスを削除（例: gcal calendars unalias 仕事）
    Unalias {
        /// エイリアス名
        name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_add_repeat_args() {
        let cli = Cli::try_parse_from([
            "gcal", "add", "Test Event",
            "--repeat", "weekly", "--every", "2", "--on", "mon,wed", "--until", "2026-12-31",
        ]).unwrap();
        if let Commands::Add { recurrence, .. } = cli.command {
            assert_eq!(recurrence.repeat.as_deref(), Some("weekly"));
            assert_eq!(recurrence.every, Some(2));
            assert_eq!(recurrence.on.as_deref(), Some("mon,wed"));
            assert_eq!(recurrence.until.as_deref(), Some("2026-12-31"));
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_add_reminder_args() {
        let cli = Cli::try_parse_from([
            "gcal", "add", "Test Event",
            "--reminder", "popup:10m", "--reminder", "email:1d",
        ]).unwrap();
        if let Commands::Add { reminder_args, .. } = cli.command {
            assert_eq!(reminder_args.reminder, Some(vec!["popup:10m".to_string(), "email:1d".to_string()]));
            assert_eq!(reminder_args.reminders, None);
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_add_reminders_preset() {
        let cli = Cli::try_parse_from(["gcal", "add", "Test Event", "--reminders", "default"]).unwrap();
        if let Commands::Add { reminder_args, .. } = cli.command {
            assert_eq!(reminder_args.reminder, None);
            assert_eq!(reminder_args.reminders.as_deref(), Some("default"));
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_update_clear_flags() {
        let cli = Cli::try_parse_from([
            "gcal", "update", "evt_id",
            "--clear-repeat", "--clear-reminders", "--clear-location",
        ]).unwrap();
        if let Commands::Update { clear_repeat, clear_reminders, clear_location, .. } = cli.command {
            assert!(clear_repeat);
            assert!(clear_reminders);
            assert!(clear_location);
        } else {
            panic!("Expected Update command");
        }
    }

    #[test]
    fn test_cli_add_ai_args() {
        let cli = Cli::try_parse_from(["gcal", "add", "--ai", "明日の会議", "--dry-run"]).unwrap();
        if let Commands::Add { ai_args, .. } = cli.command {
            assert_eq!(ai_args.ai.as_deref(), Some("明日の会議"));
            assert!(ai_args.dry_run);
            assert!(!ai_args.yes);
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_add_yes_flag() {
        let cli = Cli::try_parse_from(["gcal", "add", "--ai", "MTG", "-y"]).unwrap();
        if let Commands::Add { ai_args, .. } = cli.command {
            assert!(ai_args.yes);
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_delete_force() {
        let cli = Cli::try_parse_from(["gcal", "delete", "evt_id", "-f"]).unwrap();
        if let Commands::Delete { force, calendar, .. } = cli.command {
            assert!(force);
            assert_eq!(calendar, "primary");
        } else {
            panic!("Expected Delete command");
        }
    }

    #[test]
    fn test_cli_events_date_conflicts_with_days() {
        let result = Cli::try_parse_from(["gcal", "events", "--date", "今日", "--days", "7"]);
        assert!(result.is_err(), "date と days は排他のはず");
    }

    #[test]
    fn test_cli_calendars_alias_subcommand() {
        let cli = Cli::try_parse_from([
            "gcal", "calendars", "alias", "仕事", "abc@group.calendar.google.com",
        ]).unwrap();
        if let Commands::Calendars { sub: Some(CalendarSubcommands::Alias { name, calendar_id }) } = cli.command {
            assert_eq!(name, "仕事");
            assert_eq!(calendar_id, "abc@group.calendar.google.com");
        } else {
            panic!("Expected Calendars::Alias");
        }
    }

    #[test]
    fn test_cli_add_date_conflicts_with_start() {
        let result = Cli::try_parse_from([
            "gcal", "add", "MTG", "--date", "今日 14:00", "--start", "今日 14:00",
        ]);
        assert!(result.is_err(), "date と start は排他のはず");
    }
}
