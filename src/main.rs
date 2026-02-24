use chrono::Local;
use clap::Parser;

use gcal::app::App;
use gcal::auth::callback::{LoopbackReceiver, ManualReceiver};
use gcal::auth::flow::run_init;
use gcal::auth::provider::RefreshingTokenProvider;
use gcal::cli::{Cli, Commands};
use gcal::config::{Config, FileTokenStore};
use gcal::cli_mapper::CliMapper;

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

            let store = FileTokenStore::new(config_path.clone());

            if manual {
                let receiver = ManualReceiver::new(std::io::BufReader::new(std::io::stdin()));
                println!("認証後にリダイレクトされた URL を貼り付けてください:");
                run_init(&SystemBrowserOpener, &receiver, &store, &config_path, client_id, client_secret).await?;
            } else {
                let receiver = LoopbackReceiver::bind()?;
                run_init(&SystemBrowserOpener, &receiver, &store, &config_path, client_id, client_secret).await?;
            }
        }

        Commands::Calendars => {
            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_calendars(&mut out).await?;
        }

        Commands::Add { title, date, start, end, calendar, repeat, every, on, until, count, recur, reminder, reminders, location, .. } => {
            let today = Local::now().date_naive();
            let event = CliMapper::map_add_command(
                title, date, start, end, calendar, repeat, every, on, until, count, recur, reminder, reminders, location, today,
                None, // AI 統合は次ステップ
            )?;
            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_add_event(event, &mut out).await?;
        }

        Commands::Update { event_id, title, date, start, end, calendar, clear_repeat, clear_reminders, clear_location, repeat, every, on, until, count, recur, reminder, reminders, location, .. } => {
            let today = Local::now().date_naive();
            let event = CliMapper::map_update_command(
                event_id, title, date, start, end, calendar, clear_repeat, clear_reminders, clear_location, repeat, every, on, until, count, recur, reminder, reminders, location, today
            )?;
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

fn prompt(message: &str) -> Result<String, GcalError> {
    use std::io::Write;
    print!("{message}");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).map_err(GcalError::IoError)?;
    Ok(input.trim().to_string())
}
