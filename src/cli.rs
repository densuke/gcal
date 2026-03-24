use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "gcal", version, about = "Google Calendar CLI tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// 設定ファイルのパスを指定する (デフォルト: ~/.config/gcal/config.toml)
    #[arg(short, long, global = true)]
    pub config: Option<std::path::PathBuf>,

    /// 詳細出力を有効にする (設定ファイルのパスなどを表示)
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// 読み込まれた設定内容を表示する (機密情報はマスクされます)
    #[arg(long, global = true)]
    pub show_config: bool,
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
    /// 自然言語プロンプト（--ai の後継）
    #[arg(short = 'p', long, conflicts_with = "ai")]
    pub prompt: Option<String>,
    /// 自然言語プロンプト（後方互換、--prompt と排他）
    #[arg(long, conflicts_with = "prompt")]
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
    /// --prompt/--ai 使用時の確認プロンプトをスキップして即時実行
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
    /// 直近のイベントを表示、または -p で CRUD 操作を自然言語で実行
    Events {
        /// 対象カレンダーの ID またはエイリアス（--calendars と排他）
        #[arg(long, conflicts_with_all = ["calendars", "prompt"])]
        calendar: Option<String>,
        /// カンマ区切りで複数カレンダーを指定（--calendar と排他）
        /// 例: --calendars 仕事,個人
        #[arg(long, conflicts_with_all = ["calendar", "prompt"])]
        calendars: Option<String>,
        /// 取得する日数（--date / --from / --to / --prompt と同時指定不可）
        #[arg(long, conflicts_with_all = ["date", "from", "to", "prompt"])]
        days: Option<u64>,
        /// イベント ID を表示する
        #[arg(long, conflicts_with = "prompt")]
        ids: bool,
        /// 日付・期間を自然言語で指定（--days / --from / --to / --prompt と同時指定不可）
        #[arg(long, conflicts_with_all = ["days", "from", "to", "prompt"])]
        date: Option<String>,
        /// 開始日を自然言語で指定（--date / --days / --prompt と同時指定不可）
        #[arg(long, conflicts_with_all = ["date", "days", "prompt"])]
        from: Option<String>,
        /// 終了日を自然言語で指定（--date / --days / --prompt と同時指定不可）
        #[arg(long, conflicts_with_all = ["date", "days", "prompt"])]
        to: Option<String>,
        /// 自然言語で CRUD 操作を実行（他の表示系オプションと排他）
        #[arg(short = 'p', long,
              conflicts_with_all = ["calendar", "calendars", "days", "ids", "date", "from", "to"])]
        prompt: Option<String>,
        /// AI サーバーの URL（--prompt 使用時のオーバーライド）
        #[arg(long, requires = "prompt")]
        ai_url: Option<String>,
        /// AI モデル名（--prompt 使用時のオーバーライド）
        #[arg(long, requires = "prompt")]
        ai_model: Option<String>,
        /// 確認プロンプトをスキップ（--prompt 使用時）
        #[arg(short = 'y', long, requires = "prompt")]
        yes: bool,
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
    #[command(group = clap::ArgGroup::new("target").required(true).args(["event_id", "prompt", "ai"]))]
    Delete {
        /// イベント ID（--prompt/--ai と排他、どちらか必須）
        #[arg(group = "target")]
        event_id: Option<String>,
        /// 自然言語でイベントを特定して削除
        #[arg(short = 'p', long, group = "target")]
        prompt: Option<String>,
        /// 自然言語プロンプト（後方互換、--prompt と排他）
        #[arg(long, conflicts_with = "prompt", group = "target")]
        ai: Option<String>,
        /// 確認をスキップして削除
        #[arg(short = 'f', long)]
        force: bool,
        /// カレンダーID（デフォルト: primary）
        #[arg(long, default_value = "primary")]
        calendar: String,
    },
    /// シェル補完スクリプトを標準出力に出力する
    ///
    /// 使い方: eval "$(gcal shell bash)" を ~/.bashrc に追加
    #[command(name = "shell")]
    Shell {
        /// 対象シェル (bash または zsh)
        #[arg(value_parser = ["bash", "zsh"])]
        shell: String,
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
            "gcal",
            "add",
            "Test Event",
            "--repeat",
            "weekly",
            "--every",
            "2",
            "--on",
            "mon,wed",
            "--until",
            "2026-12-31",
        ])
        .unwrap();
        if let Some(Commands::Add { recurrence, .. }) = cli.command {
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
            "gcal",
            "add",
            "Test Event",
            "--reminder",
            "popup:10m",
            "--reminder",
            "email:1d",
        ])
        .unwrap();
        if let Some(Commands::Add { reminder_args, .. }) = cli.command {
            assert_eq!(
                reminder_args.reminder,
                Some(vec!["popup:10m".to_string(), "email:1d".to_string()])
            );
            assert_eq!(reminder_args.reminders, None);
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_add_reminders_preset() {
        let cli =
            Cli::try_parse_from(["gcal", "add", "Test Event", "--reminders", "default"]).unwrap();
        if let Some(Commands::Add { reminder_args, .. }) = cli.command {
            assert_eq!(reminder_args.reminder, None);
            assert_eq!(reminder_args.reminders.as_deref(), Some("default"));
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_update_clear_flags() {
        let cli = Cli::try_parse_from([
            "gcal",
            "update",
            "evt_id",
            "--clear-repeat",
            "--clear-reminders",
            "--clear-location",
        ])
        .unwrap();
        if let Some(Commands::Update {
            clear_repeat,
            clear_reminders,
            clear_location,
            ..
        }) = cli.command
        {
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
        if let Some(Commands::Add { ai_args, .. }) = cli.command {
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
        if let Some(Commands::Add { ai_args, .. }) = cli.command {
            assert!(ai_args.yes);
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_add_prompt_short_flag() {
        let cli = Cli::try_parse_from(["gcal", "add", "-p", "明日の会議"]).unwrap();
        if let Some(Commands::Add { ai_args, .. }) = cli.command {
            assert_eq!(ai_args.prompt.as_deref(), Some("明日の会議"));
            assert_eq!(ai_args.ai, None);
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_add_prompt_long_flag() {
        let cli = Cli::try_parse_from(["gcal", "add", "--prompt", "明日の会議"]).unwrap();
        if let Some(Commands::Add { ai_args, .. }) = cli.command {
            assert_eq!(ai_args.prompt.as_deref(), Some("明日の会議"));
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_add_prompt_and_ai_are_exclusive() {
        let result = Cli::try_parse_from(["gcal", "add", "--prompt", "test", "--ai", "test"]);
        assert!(result.is_err(), "--prompt と --ai は排他のはず");
    }

    #[test]
    fn test_cli_update_prompt_flag() {
        let cli = Cli::try_parse_from(["gcal", "update", "evt_id", "-p", "場所を変更"]).unwrap();
        if let Some(Commands::Update { ai_args, .. }) = cli.command {
            assert_eq!(ai_args.prompt.as_deref(), Some("場所を変更"));
        } else {
            panic!("Expected Update command");
        }
    }

    #[test]
    fn test_cli_add_ai_still_works_for_compat() {
        // --ai は後方互換のため引き続き動作すること
        let cli = Cli::try_parse_from(["gcal", "add", "--ai", "明日の会議"]).unwrap();
        if let Some(Commands::Add { ai_args, .. }) = cli.command {
            assert_eq!(ai_args.ai.as_deref(), Some("明日の会議"));
            assert_eq!(ai_args.prompt, None);
        } else {
            panic!("Expected Add command");
        }
    }

    #[test]
    fn test_cli_delete_force() {
        let cli = Cli::try_parse_from(["gcal", "delete", "evt_id", "-f"]).unwrap();
        if let Some(Commands::Delete {
            force, calendar, ..
        }) = cli.command
        {
            assert!(force);
            assert_eq!(calendar, "primary");
        } else {
            panic!("Expected Delete command");
        }
    }

    #[test]
    fn test_cli_delete_by_id() {
        let cli = Cli::try_parse_from(["gcal", "delete", "evt_abc123"]).unwrap();
        if let Some(Commands::Delete {
            event_id, prompt, ..
        }) = cli.command
        {
            assert_eq!(event_id, Some("evt_abc123".to_string()));
            assert_eq!(prompt, None);
        } else {
            panic!("Expected Delete command");
        }
    }

    #[test]
    fn test_cli_delete_by_prompt() {
        let cli = Cli::try_parse_from(["gcal", "delete", "-p", "明日の会議を削除"]).unwrap();
        if let Some(Commands::Delete {
            event_id, prompt, ..
        }) = cli.command
        {
            assert_eq!(event_id, None);
            assert_eq!(prompt.as_deref(), Some("明日の会議を削除"));
        } else {
            panic!("Expected Delete command");
        }
    }

    #[test]
    fn test_cli_delete_requires_id_or_prompt() {
        // どちらも指定しない場合はエラー
        let result = Cli::try_parse_from(["gcal", "delete"]);
        assert!(
            result.is_err(),
            "event_id か --prompt のどちらかが必須のはず"
        );
    }

    #[test]
    fn test_cli_delete_id_and_prompt_are_exclusive() {
        let result = Cli::try_parse_from(["gcal", "delete", "evt_id", "-p", "会議を削除"]);
        assert!(result.is_err(), "event_id と --prompt は排他のはず");
    }

    #[test]
    fn test_cli_events_prompt_flag() {
        let cli = Cli::try_parse_from(["gcal", "events", "-p", "明日の会議を削除して"]).unwrap();
        if let Some(Commands::Events { prompt, .. }) = cli.command {
            assert_eq!(prompt.as_deref(), Some("明日の会議を削除して"));
        } else {
            panic!("Expected Events command");
        }
    }

    #[test]
    fn test_cli_events_prompt_conflicts_with_date() {
        let result = Cli::try_parse_from(["gcal", "events", "-p", "test", "--date", "今日"]);
        assert!(result.is_err(), "--prompt と --date は排他のはず");
    }

    #[test]
    fn test_cli_events_prompt_conflicts_with_days() {
        let result = Cli::try_parse_from(["gcal", "events", "-p", "test", "--days", "7"]);
        assert!(result.is_err(), "--prompt と --days は排他のはず");
    }

    #[test]
    fn test_cli_events_date_conflicts_with_days() {
        let result = Cli::try_parse_from(["gcal", "events", "--date", "今日", "--days", "7"]);
        assert!(result.is_err(), "date と days は排他のはず");
    }

    #[test]
    fn test_cli_calendars_alias_subcommand() {
        let cli = Cli::try_parse_from([
            "gcal",
            "calendars",
            "alias",
            "仕事",
            "abc@group.calendar.google.com",
        ])
        .unwrap();
        if let Some(Commands::Calendars {
            sub: Some(CalendarSubcommands::Alias { name, calendar_id }),
        }) = cli.command
        {
            assert_eq!(name, "仕事");
            assert_eq!(calendar_id, "abc@group.calendar.google.com");
        } else {
            panic!("Expected Calendars::Alias");
        }
    }

    #[test]
    fn test_cli_add_date_conflicts_with_start() {
        let result = Cli::try_parse_from([
            "gcal",
            "add",
            "MTG",
            "--date",
            "今日 14:00",
            "--start",
            "今日 14:00",
        ]);
        assert!(result.is_err(), "date と start は排他のはず");
    }
}
