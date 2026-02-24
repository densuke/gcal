use chrono::Local;
use clap::Parser;

use gcal::ai::client::OllamaClient;
use gcal::ai::types::AiEventParameters;
use gcal::output::{write_new_event_dry_run, write_update_event_dry_run};
use gcal::app::App;
use gcal::auth::callback::{LoopbackReceiver, ManualReceiver};
use gcal::auth::flow::run_init;
use gcal::auth::provider::RefreshingTokenProvider;
use gcal::cli::{Cli, Commands};
use gcal::cli_mapper::CliMapper;
use gcal::config::{AiConfig, Config, FileTokenStore};
use gcal::error::GcalError;
use gcal::gcal_api::client::GoogleCalendarClient;
use gcal::ports::{SystemBrowserOpener, SystemClock};

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("エラー: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), GcalError> {
    let cli = Cli::parse();
    let config_path = Config::default_path()?;

    match cli.command {
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

        Commands::Calendars => {
            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_calendars(&mut out).await?;
        }

        Commands::Add { title, date, start, end, calendar, repeat, every, on, until, count, recur, reminder, reminders, location, ai, ai_url, ai_model, dry_run, .. } => {
            let today = Local::now().date_naive();
            let ai_params = resolve_ai_params(ai, ai_url, ai_model, &config_path).await?;
            let event = CliMapper::map_add_command(
                title, date, start, end, calendar, repeat, every, on, until, count, recur, reminder, reminders, location, today,
                ai_params,
            )?;
            if dry_run {
                let mut out = std::io::stdout();
                write_new_event_dry_run(&event, &mut out)?;
                return Ok(());
            }
            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_add_event(event, &mut out).await?;
        }

        Commands::Update { event_id, title, date, start, end, calendar, clear_repeat, clear_reminders, clear_location, repeat, every, on, until, count, recur, reminder, reminders, location, ai, ai_url, ai_model, dry_run, .. } => {
            let today = Local::now().date_naive();
            let ai_params = resolve_ai_params(ai, ai_url, ai_model, &config_path).await?;
            let event = CliMapper::map_update_command(
                event_id, title, date, start, end, calendar, clear_repeat, clear_reminders, clear_location, repeat, every, on, until, count, recur, reminder, reminders, location, today,
                ai_params,
            )?;
            if dry_run {
                let mut out = std::io::stdout();
                write_update_event_dry_run(&event, &mut out)?;
                return Ok(());
            }
            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_update_event(event, &mut out).await?;
        }

        Commands::Delete { event_id, force, calendar } => {
            if !force {
                let answer = prompt(&format!(
                    "イベント (ID: {}) を削除しますか? [y/N]: ",
                    event_id
                ))?;
                if answer.to_lowercase() != "y" {
                    println!("キャンセルしました");
                    return Ok(());
                }
            }
            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_delete_event(&calendar, &event_id, &mut out).await?;
        }

        Commands::Events { calendar, days, date, from, to, ids } => {
            let today = Local::now().date_naive();
            let (time_min, time_max) = CliMapper::map_events_command(
                date, from, to, days.map(|x| x as u64), today
            )?;

            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_events(&calendar, time_min, time_max, ids, &mut out).await?;
        }
    }

    Ok(())
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
/// config が存在しない場合はデフォルト値（localhost:11434 / llama3）を使用。
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
