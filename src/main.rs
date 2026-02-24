use chrono::{DateTime, Local, NaiveDate, TimeZone, Utc};
use clap::Parser;

use gcal::app::App;
use gcal::auth::callback::{LoopbackReceiver, ManualReceiver};
use gcal::auth::flow::run_init;
use gcal::auth::provider::RefreshingTokenProvider;
use gcal::cli::{Cli, Commands};
use gcal::config::{Config, FileTokenStore};
use gcal::date_parser::resolve_event_range;
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
            let client_id = prompt("Google OAuth2 Client ID: ")?;
            let client_secret = rpassword::prompt_password("Google OAuth2 Client Secret: ")
                .map_err(GcalError::IoError)?;

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

        Commands::Events { calendar, days, date, from, to } => {
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
            app.handle_events(&calendar, time_min, time_max, &mut out).await?;
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

fn prompt(message: &str) -> Result<String, GcalError> {
    use std::io::Write;
    print!("{message}");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).map_err(GcalError::IoError)?;
    Ok(input.trim().to_string())
}
