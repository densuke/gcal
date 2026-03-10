use chrono::Local;
use clap::Parser;
use clap_complete::{generate, Shell};

use gcal::ai::client::OllamaClient;
use gcal::ai::types::{AiEventParameters, AiEventTarget};
use gcal::output::{write_new_event_dry_run, write_update_event_dry_run};
use gcal::alias_handler::{handle_list_aliases, handle_remove_alias, handle_set_alias};
use gcal::app::App;
use gcal::auth::callback::{LoopbackReceiver, ManualReceiver};
use gcal::auth::flow::run_init;
use gcal::auth::provider::RefreshingTokenProvider;
use gcal::cli::{CalendarSubcommands, Cli, Commands};
use gcal::cli_mapper::{AddCommandInput, UpdateCommandInput, CliMapper};
use gcal::config::{AiConfig, Config, FileTokenStore};
use gcal::domain::EventSummary;
use gcal::error::GcalError;
use gcal::event_selector;
use gcal::gcal_api::client::GoogleCalendarClient;
use gcal::ports::{SystemBrowserOpener, SystemClock};
use gcal::prompt_flow;

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("エラー: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), GcalError> {
    let cli = Cli::parse();
    let mut load_info = Vec::new();
    let mut config = Config::default();

    // 読み込み候補パスのリスト（優先度の低い順 = 後のファイルが上書きする）
    let mut candidate_paths = Vec::new();

    // 1. ホームディレクトリ直下: ~/.gcal.toml
    if let Some(home) = dirs::home_dir() {
        candidate_paths.push((format!("[Home]    "), home.join(".gcal.toml")));
    }
    // 2. XDG準拠: ~/.config/gcal/config.toml
    if let Some(home) = dirs::home_dir() {
        candidate_paths.push((format!("[XDG]     "), home.join(".config").join("gcal").join("config.toml")));
    }
    // 3. OS依存の標準の場所: (macOSなら ~/Library/Application Support/gcal/config.toml)
    if let Some(os_config) = dirs::config_dir() {
        candidate_paths.push((format!("[OS-Spec] "), os_config.join("gcal").join("config.toml")));
    }
    // 4. カレントディレクトリ: ./.gcal.toml
    candidate_paths.push((format!("[Current] "), std::path::PathBuf::from(".gcal.toml")));

    // 5. CLI引数で指定されたパス（あれば最強）
    if let Some(ref path) = cli.config {
        candidate_paths.push((format!("[Explicit]"), path.clone()));
    }

    // 重複するパスを除去（OS-Spec と XDG が同じ場合があるため）
    let mut seen = std::collections::HashSet::new();
    let unique_candidates: Vec<_> = candidate_paths
        .into_iter()
        .filter(|(_, p)| seen.insert(p.clone()))
        .collect();

    // ファイルを順に読み込んでマージ
    let mut last_loaded_path = None;
    let mut priority = 1;
    for (label, path) in unique_candidates {
        if path.exists() {
            match Config::load(&path) {
                Ok(other) => {
                    config.merge(other);
                    load_info.push(format!("{}. {} {:?} (読み込み完了/上書き)", priority, label, path));
                    last_loaded_path = Some(path.clone());
                    priority += 1;
                }
                Err(e) => {
                    load_info.push(format!("{}. {} {:?} (エラー: {})", priority, label, path, e));
                    priority += 1;
                }
            }
        } else if cli.verbose || cli.show_config {
            // --verbose か --show-config の時は「存在しなかった」ことも出すと親切
            load_info.push(format!("{}. {} {:?} (存在しません)", priority, label, path));
            priority += 1;
        }
    }

    // 最後に書き込み等で使う「メインのパス」を決定
    // CLI引数 > 最後に読み込んだパス > デフォルトパス
    let config_path = cli.config.clone()
        .or(last_loaded_path)
        .unwrap_or_else(|| Config::default_path().unwrap_or_else(|_| std::path::PathBuf::from("config.toml")));

    if cli.verbose || cli.show_config {
        for info in &load_info {
            eprintln!("Info: {}", info);
        }
        eprintln!("Info: 最終的な設定ファイル (保存先): {:?}", config_path);
    }

    if cli.show_config {
        println!("\n{}", config.display_config());
        return Ok(());
    }

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            return Ok(());
        }
    };

    match command {
        Commands::Init { manual } => {
            let (client_id, client_secret) = resolve_credentials(&config_path)?;
            let ai_config = resolve_ai_config(&config_path)?;

            let store = FileTokenStore::new(config_path.clone());

            if manual {
                let receiver = ManualReceiver::new(std::io::BufReader::new(std::io::stdin()));
                println!("認証後にリダイレクトされた URL を貼り付けてください:");
                run_init(&SystemBrowserOpener, &receiver, &store, &config_path, client_id, client_secret, ai_config).await?;
            } else {
                let receiver = LoopbackReceiver::bind()?;
                run_init(&SystemBrowserOpener, &receiver, &store, &config_path, client_id, client_secret, ai_config).await?;
            }
        }

        Commands::Calendars { sub } => {
            match sub {
                None => {
                    let app = build_app(&config_path)?;
                    let mut out = std::io::stdout();
                    app.handle_calendars(&mut out).await?;
                }
                Some(CalendarSubcommands::Alias { name, calendar_id }) => {
                    let mut out = std::io::stdout();
                    handle_set_alias(&config_path, &name, &calendar_id, &mut out)?;
                }
                Some(CalendarSubcommands::Aliases) => {
                    let mut out = std::io::stdout();
                    handle_list_aliases(&config_path, &mut out)?;
                }
                Some(CalendarSubcommands::Unalias { name }) => {
                    let mut out = std::io::stdout();
                    handle_remove_alias(&config_path, &name, &mut out)?;
                }
            }
        }

        Commands::Add { title, date, start, end, location, calendar, recurrence, reminder_args, ai_args } => {
            let today = Local::now().date_naive();
            let ai_params = resolve_ai_params(ai_args.prompt.or(ai_args.ai), ai_args.ai_url, ai_args.ai_model, &config_path).await?;
            let used_ai = ai_params.is_some();
            let (calendar_id, calendar_display_name) = resolve_calendar_from_args(&config_path, calendar, ai_params.as_ref());

            let event = CliMapper::map_add_command(AddCommandInput {
                title, date, start, end, calendar: calendar_id, calendar_display_name, location, recurrence, reminder_args, today, ai_params
            })?;
            // AI 使用時は登録内容を表示して確認を求める（--yes でスキップ）
            if used_ai && !ai_args.yes {
                let mut out = std::io::stdout();
                write_new_event_dry_run(&event, &mut out)?;
                if !confirm_or_cancel("この内容で登録しますか? [y/N]: ")? {
                    return Ok(());
                }
            }
            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_add_event(event, &mut out).await?;
        }

        Commands::Update { event_id, title, date, start, end, calendar, clear_repeat, clear_reminders, clear_location, location, recurrence, reminder_args, ai_args } => {
            let today = Local::now().date_naive();
            let ai_params = resolve_ai_params(ai_args.prompt.or(ai_args.ai), ai_args.ai_url, ai_args.ai_model, &config_path).await?;
            let used_ai = ai_params.is_some();
            let (calendar_id, calendar_display_name) = resolve_calendar_from_args(&config_path, calendar, ai_params.as_ref());

            let event = CliMapper::map_update_command(UpdateCommandInput {
                event_id, calendar: calendar_id, calendar_display_name, title, date, start, end, clear_repeat, clear_reminders, clear_location, location, recurrence, reminder_args, today, ai_params
            })?;
            // AI 使用時は更新内容を表示して確認を求める（--yes でスキップ）
            if used_ai && !ai_args.yes {
                let mut out = std::io::stdout();
                write_update_event_dry_run(&event, &mut out)?;
                if !confirm_or_cancel("この内容で更新しますか? [y/N]: ")? {
                    return Ok(());
                }
            }
            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_update_event(event, &mut out).await?;
        }

        Commands::Delete { event_id, prompt, ai, force, calendar } => {
            if let Some(prompt_str) = prompt.or(ai) {
                // 自然文フロー: 候補イベントを特定して削除
                dispatch_prompt_delete(prompt_str, force, &config_path).await?;
            } else {
                // ID 直接指定フロー（従来通り）
                let (calendar_id, _) = resolve_calendar(&config_path, &calendar);
                let id = event_id.expect("event_id は ArgGroup で保証");
                if !force {
                    if !confirm_or_cancel(&format!(
                        "イベント (ID: {}) を削除しますか? [y/N]: ",
                        id
                    ))? {
                        return Ok(());
                    }
                }
                let app = build_app(&config_path)?;
                let mut out = std::io::stdout();
                app.handle_delete_event(&calendar_id, &id, &mut out).await?;
            }
        }

        Commands::Events { calendar, calendars, days, date, from, to, ids, prompt, ai_url, ai_model, yes } => {
            if let Some(prompt_str) = prompt {
                // CRUD ディスパッチモード: --prompt/-p で操作種別を判断してルーティング
                dispatch_prompt_events(
                    prompt_str, ai_url, ai_model, yes,
                    calendar, calendars,
                    &config_path,
                ).await?;
            } else {
                // 既存の表示モード
                let config = Config::load(&config_path).unwrap_or_default();
                let calendar_ids = config.resolve_event_calendars(
                    calendar.as_deref(),
                    calendars.as_deref(),
                );
                let today = Local::now().date_naive();
                let (time_min, time_max) = CliMapper::map_events_command(
                    date, from, to, days.map(|x| x as u64), today
                )?;

                let app = build_app(&config_path)?;
                let mut out = std::io::stdout();
                app.handle_events(&calendar_ids, time_min, time_max, ids, &mut out).await?;
            }
        }

        Commands::Shell { shell } => {
            use clap::CommandFactory;
            let mut cmd = Cli::command();
            let mut out = std::io::stdout();
            match shell.as_str() {
                "bash" => generate(Shell::Bash, &mut cmd, "gcal", &mut out),
                "zsh" => generate(Shell::Zsh, &mut cmd, "gcal", &mut out),
                _ => unreachable!(),
            }
        }
    }

    Ok(())
}

/// events -p ディスパッチモードのエントリポイント。
/// LLM で操作種別（add/update/delete）を判断してルーティングする。
/// 複数候補の場合は番号選択 UI を表示（インタラクティブ部分のみ main.rs に残す）。
async fn dispatch_prompt_events(
    prompt_str: String,
    ai_url: Option<String>,
    ai_model: Option<String>,
    yes: bool,
    calendar: Option<String>,
    calendars: Option<String>,
    config_path: &std::path::Path,
) -> Result<(), GcalError> {
    let ai_config = Config::load(config_path).map(|c| c.ai).unwrap_or_default();
    let base_url = ai_url.as_deref().unwrap_or(&ai_config.base_url).to_string();
    let model = ai_model.as_deref().unwrap_or(&ai_config.model).to_string();
    let ai_client = OllamaClient::new(base_url, model);

    let intent = ai_client.parse_operation_intent(&prompt_str).await?;
    let today = Local::now().date_naive();
    let mut out = std::io::stdout();

    match intent.operation.as_str() {
        "add" => {
            let ai_params = ai_client.parse_prompt(&prompt_str).await?;
            let (calendar_id, calendar_display_name) =
                resolve_calendar_from_args(config_path, calendar, Some(&ai_params));
            let event = CliMapper::map_add_command(AddCommandInput {
                calendar: calendar_id,
                calendar_display_name,
                today,
                ai_params: Some(ai_params),
                ..Default::default()
            })?;
            if !yes {
                write_new_event_dry_run(&event, &mut out)?;
                if !confirm_or_cancel("この内容で登録しますか? [y/N]: ")? {
                    return Ok(());
                }
            }
            let app = build_app(config_path)?;
            app.handle_add_event(event, &mut out).await?;
        }

        "show" => {
            let target = intent.target.unwrap_or(AiEventTarget {
                title_hint: None,
                date_hint: None,
                calendar: None,
            });
            let config = Config::load(config_path).unwrap_or_default();
            let calendar_ids = config.resolve_event_calendars(
                calendar.as_deref(),
                calendars.as_deref(),
            );
            let (time_min, time_max) = prompt_flow::search_range(target.date_hint.as_deref(), today)?;
            let app = build_app(config_path)?;
            app.handle_events(&calendar_ids, time_min, time_max, false, &mut out).await?;
        }

        op @ ("delete" | "update") => {
            let target = intent.target.unwrap_or(AiEventTarget {
                title_hint: None,
                date_hint: None,
                calendar: None,
            });
            let config = Config::load(config_path).unwrap_or_default();
            let calendar_ids = config.resolve_event_calendars(
                calendar.as_deref(),
                calendars.as_deref(),
            );
            let (time_min, time_max) = prompt_flow::search_range(target.date_hint.as_deref(), today)?;

            let app = build_app(config_path)?;
            let all_events = prompt_flow::fetch_events(
                &app.calendar_client, &calendar_ids, time_min, time_max,
            ).await?;
            let summaries: Vec<EventSummary> = all_events.iter().map(|(_, e)| e.clone()).collect();
            let matched = event_selector::filter_by_target(&summaries, &target, today);

            if matched.is_empty() {
                println!("候補イベントが見つかりませんでした");
                return Ok(());
            }
            let selected_idx = if matched.len() == 1 {
                matched[0]
            } else {
                let list = prompt_flow::format_candidate_list(&all_events, &matched);
                print!("{list}");
                select_candidate_index(&matched)?
            };

            if op == "delete" {
                let (cal_id, event) = &all_events[selected_idx];
                if !yes {
                    if !confirm_or_cancel(&format!("「{}」を削除しますか? [y/N]: ", event.summary))? {
                        return Ok(());
                    }
                }
                app.handle_delete_event(cal_id, &event.id, &mut out).await?;
            } else {
                let (cal_id, selected) = &all_events[selected_idx];
                let ai_params = ai_client.parse_prompt(&prompt_str).await?;
                let update_event = CliMapper::map_update_command(UpdateCommandInput {
                    event_id: selected.id.clone(),
                    calendar: cal_id.clone(),
                    calendar_display_name: cal_id.clone(),
                    today,
                    ai_params: Some(ai_params),
                    ..Default::default()
                })?;
                if !yes {
                    write_update_event_dry_run(&update_event, &mut out)?;
                    if !confirm_or_cancel("この内容で更新しますか? [y/N]: ")? {
                        return Ok(());
                    }
                }
                app.handle_update_event(update_event, &mut out).await?;
            }
        }

        other => {
            return Err(GcalError::ConfigError(format!(
                "不明な操作種別: '{}' (add/update/delete のいずれかが必要)",
                other
            )));
        }
    }
    Ok(())
}

/// delete -p/-ai フローのエントリポイント。
/// 自然文でイベントを特定して削除する。
async fn dispatch_prompt_delete(
    prompt_str: String,
    force: bool,
    config_path: &std::path::Path,
) -> Result<(), GcalError> {
    let ai_config = Config::load(config_path).map(|c| c.ai).unwrap_or_default();
    let ai_client = OllamaClient::new(ai_config.base_url, ai_config.model);

    let intent = ai_client.parse_operation_intent(&prompt_str).await?;
    let target = intent.target.unwrap_or(AiEventTarget {
        title_hint: None,
        date_hint: None,
        calendar: None,
    });

    let today = Local::now().date_naive();
    let config = Config::load(config_path).unwrap_or_default();
    let calendar_ids = config.resolve_event_calendars(None, None);
    let (time_min, time_max) = prompt_flow::search_range(target.date_hint.as_deref(), today)?;

    let app = build_app(config_path)?;
    let all_events = prompt_flow::fetch_events(
        &app.calendar_client, &calendar_ids, time_min, time_max,
    ).await?;
    let summaries: Vec<EventSummary> = all_events.iter().map(|(_, e)| e.clone()).collect();
    let matched = event_selector::filter_by_target(&summaries, &target, today);

    if matched.is_empty() {
        println!("候補イベントが見つかりませんでした");
        return Ok(());
    }
    let selected_idx = if matched.len() == 1 {
        matched[0]
    } else {
        let list = prompt_flow::format_candidate_list(&all_events, &matched);
        print!("{list}");
        select_candidate_index(&matched)?
    };
    let (cal_id, event) = &all_events[selected_idx];
    let mut out = std::io::stdout();
    if !force {
        if !confirm_or_cancel(&format!("「{}」を削除しますか? [y/N]: ", event.summary))? {
            return Ok(());
        }
    }
    app.handle_delete_event(cal_id, &event.id, &mut out).await?;
    Ok(())
}

/// 候補番号をユーザーから受け取り matched[n-1] を返す。
fn select_candidate_index(matched: &[usize]) -> Result<usize, GcalError> {
    let answer = prompt(&format!("番号を選択してください (1-{}): ", matched.len()))?;
    let n: usize = answer.trim().parse().map_err(|_| {
        GcalError::ConfigError(format!("無効な番号です: '{}'", answer.trim()))
    })?;
    if n < 1 || n > matched.len() {
        return Err(GcalError::ConfigError(format!(
            "1 から {} の番号を入力してください",
            matched.len()
        )));
    }
    Ok(matched[n - 1])
}

/// CLI 引数と AI パラメータからカレンダー ID を解決する。
/// 優先順位: CLI 引数 > AI 出力 > "primary"、その後エイリアス解決
fn resolve_calendar_from_args(
    config_path: &std::path::Path,
    calendar: Option<String>,
    ai_params: Option<&AiEventParameters>,
) -> (String, String) {
    let raw = calendar
        .or_else(|| ai_params.and_then(|p| p.calendar.clone()))
        .unwrap_or_else(|| "primary".to_string());
    resolve_calendar(config_path, &raw)
}

fn resolve_calendar(config_path: &std::path::Path, input: &str) -> (String, String) {
    let config = Config::load(config_path).unwrap_or_default();
    let resolved = config.resolve_calendar_id(input);
    if resolved == input && !input.contains('@') && input != "primary" && !config.calendars.is_empty() {
        // TODO: 将来的には「unknown alias → 作成後に別カレンダーへ移動」機能を追加
        eprintln!("警告: 未知のカレンダーエイリアス '{}' → primary を使用します", input);
        eprintln!("      `gcal calendars aliases` でエイリアス一覧を確認できます");
        return ("primary".to_string(), "primary".to_string());
    }
    (resolved, input.to_string())
}

/// API クライアントと App を組み立てる
fn build_app(
    config_path: &std::path::Path,
) -> Result<App<GoogleCalendarClient<RefreshingTokenProvider<FileTokenStore, SystemClock>>>, GcalError> {
    let (client_id, client_secret) = load_credentials(config_path)?;
    let store = FileTokenStore::new(config_path.to_path_buf());
    let token_provider = RefreshingTokenProvider::new(store, SystemClock, client_id, client_secret);
    let http_client = GoogleCalendarClient::new(reqwest::Client::new(), token_provider);
    Ok(App { calendar_client: http_client })
}

fn load_credentials(config_path: &std::path::Path) -> Result<(String, String), GcalError> {
    let config = Config::load(config_path)?;
    Ok((config.credentials.client_id, config.credentials.client_secret))
}

/// `gcal init` 時に使う認証情報を決定する。
/// 既存の設定ファイルに client_id が残っていれば表示して再利用を選択できる。
fn resolve_credentials(config_path: &std::path::Path) -> Result<(String, String), GcalError> {
    if let Ok(config) = Config::load(config_path) {
        if !config.credentials.client_id.is_empty() {
            println!("既存の認証情報が見つかりました:");
            println!("  Client ID: {}", config.credentials.client_id);
            let answer = prompt("既存の認証情報を使いますか? [Y/n]: ")?;
            if answer.is_empty() || answer.to_lowercase() == "y" {
                return Ok((config.credentials.client_id, config.credentials.client_secret));
            }
        }
    }
    let client_id = prompt("Google OAuth2 Client ID: ")?;
    let client_secret = rpassword::prompt_password("Google OAuth2 Client Secret: ")
        .map_err(GcalError::IoError)?;
    Ok((client_id, client_secret))
}

/// `gcal init` 時に AI 設定をプロンプトで確認・設定する
///
/// 既存の設定があればその値を、なければデフォルト値を角括弧内に表示し、
/// Enter のみで確定（変更不要な場合はそのまま Enter）。
fn resolve_ai_config(config_path: &std::path::Path) -> Result<AiConfig, GcalError> {
    let existing = Config::load(config_path).map(|c| c.ai).unwrap_or_default();

    println!("\nAI設定 (Ollama):");

    let base_url_input = prompt(&format!("  サーバーURL [{}]: ", existing.base_url))?;
    let base_url = if base_url_input.is_empty() { existing.base_url } else { base_url_input };

    let model_input = prompt(&format!("  使用モデル  [{}]: ", existing.model))?;
    let model = if model_input.is_empty() { existing.model } else { model_input };

    Ok(AiConfig { base_url, model, enabled: existing.enabled })
}

/// `--ai` フラグが指定されていれば Ollama に問い合わせて AiEventParameters を返す
///
/// `--ai-url` / `--ai-model` は config の値を上書きする。
/// config が存在しない場合はデフォルト値（localhost:11434 / gemma3:4b）を使用。
async fn resolve_ai_params(
    ai_prompt: Option<String>,
    ai_url: Option<String>,
    ai_model: Option<String>,
    config_path: &std::path::Path,
) -> Result<Option<AiEventParameters>, GcalError> {
    let Some(prompt_str) = ai_prompt else {
        return Ok(None);
    };
    let ai_config = Config::load(config_path)
        .map(|c| c.ai)
        .unwrap_or_default();
    let base_url = ai_url.unwrap_or(ai_config.base_url);
    let model = ai_model.unwrap_or(ai_config.model);
    let client = OllamaClient::new(base_url, model);
    let params = client.parse_prompt(&prompt_str).await?;
    Ok(Some(params))
}

fn prompt(message: &str) -> Result<String, GcalError> {
    use std::io::Write;
    print!("{message}");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).map_err(GcalError::IoError)?;
    Ok(input.trim().to_string())
}

/// y/N プロンプトを表示し、y なら true、それ以外は "キャンセルしました" を表示して false を返す
fn confirm_or_cancel(message: &str) -> Result<bool, GcalError> {
    let answer = prompt(message)?;
    if answer.to_lowercase() != "y" {
        println!("キャンセルしました");
        return Ok(false);
    }
    Ok(true)
}
