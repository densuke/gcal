use chrono::{DateTime, Duration, Local, NaiveDate, TimeZone, Utc};
use clap::Parser;

use gcal::app::App;
use gcal::auth::callback::{LoopbackReceiver, ManualReceiver};
use gcal::auth::flow::run_init;
use gcal::auth::provider::RefreshingTokenProvider;
use gcal::cli::{Cli, Commands};
use gcal::config::{Config, FileTokenStore};
use gcal::date_parser::{parse_datetime_expr, parse_datetime_range_expr, parse_end_expr, resolve_event_range};
use gcal::domain::{NewEvent, UpdateEvent};
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

        Commands::Add { title, date, start, end, calendar } => {
            let today = Local::now().date_naive();
            let (start_dt, end_dt) = if let Some(d) = date {
                parse_datetime_range_expr(&d, today)?
            } else {
                let s = start.ok_or_else(|| {
                    GcalError::ConfigError(
                        "--date か --start のいずれかを指定してください".to_string(),
                    )
                })?;
                let start_dt = parse_datetime_expr(&s, today)?;
                let end_dt = match end {
                    Some(e) => parse_end_expr(&e, start_dt, today)?,
                    None => start_dt + Duration::hours(1),
                };
                (start_dt, end_dt)
            };
            let event = NewEvent {
                summary: title,
                calendar_id: calendar,
                start: start_dt,
                end: end_dt,
            };
            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_add_event(event, &mut out).await?;
        }

        Commands::Update { event_id, title, date, start, end, calendar } => {
            // --title / --start・--end / --date のいずれも指定されていない場合はエラー
            if title.is_none() && start.is_none() && date.is_none() {
                return Err(GcalError::ConfigError(
                    "--title / --start・--end / --date のいずれかを指定してください".to_string(),
                ));
            }
            let today = Local::now().date_naive();
            let (start_dt, end_dt) = if let Some(d) = date {
                let (s, e) = parse_datetime_range_expr(&d, today)?;
                (Some(s), Some(e))
            } else {
                match (start, end) {
                    (Some(s), Some(e)) => {
                        let start_dt = parse_datetime_expr(&s, today)?;
                        let end_dt = parse_end_expr(&e, start_dt, today)?;
                        (Some(start_dt), Some(end_dt))
                    }
                    _ => (None, None),
                }
            };
            let event = UpdateEvent {
                event_id,
                calendar_id: calendar,
                title,
                start: start_dt,
                end: end_dt,
            };
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
            let range = resolve_event_range(
                date.as_deref(),
                from.as_deref(),
                to.as_deref(),
                days,
                today,
            )?;

            let time_min = naive_date_to_utc_start(range.from)?;
            let time_max = naive_date_to_utc_end(range.to)?;

            let app = build_app(&config_path)?;
            let mut out = std::io::stdout();
            app.handle_events(&calendar, time_min, time_max, ids, &mut out).await?;
        }
    }

    Ok(())
}

/// NaiveDate の 0:00:00 をローカルタイムとして UTC に変換
fn naive_date_to_utc_start(date: NaiveDate) -> Result<DateTime<Utc>, GcalError> {
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).expect("0:00:00 は常に有効"))
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(|| GcalError::ConfigError("ローカル時刻の変換に失敗しました".to_string()))
}

/// NaiveDate の 23:59:59 をローカルタイムとして UTC に変換
fn naive_date_to_utc_end(date: NaiveDate) -> Result<DateTime<Utc>, GcalError> {
    Local
        .from_local_datetime(&date.and_hms_opt(23, 59, 59).expect("23:59:59 は常に有効"))
        .single()
        .map(|dt| dt.with_timezone(&Utc))
        .ok_or_else(|| GcalError::ConfigError("ローカル時刻の変換に失敗しました".to_string()))
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
