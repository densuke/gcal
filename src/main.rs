use clap::Parser;

use gcal::app::App;
use gcal::auth::callback::{LoopbackReceiver, ManualReceiver};
use gcal::auth::flow::run_init;
use gcal::auth::provider::RefreshingTokenProvider;
use gcal::cli::{Cli, Commands};
use gcal::config::{Config, FileTokenStore};
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
            // client_id と client_secret を入力させる
            let client_id = prompt("Google OAuth2 Client ID: ")?;
            let client_secret = rpassword::prompt_password("Google OAuth2 Client Secret: ")
                .map_err(|e| GcalError::IoError(e))?;

            let store = FileTokenStore::new(config_path.clone());

            if manual {
                let receiver = ManualReceiver::new(std::io::BufReader::new(std::io::stdin()));
                println!("認証後にリダイレクトされた URL を貼り付けてください:");
                run_init(
                    &SystemBrowserOpener,
                    &receiver,
                    &store,
                    &config_path,
                    client_id,
                    client_secret,
                )
                .await?;
            } else {
                let receiver = LoopbackReceiver::bind()?;
                run_init(
                    &SystemBrowserOpener,
                    &receiver,
                    &store,
                    &config_path,
                    client_id,
                    client_secret,
                )
                .await?;
            }
        }

        Commands::Calendars => {
            let (client_id, client_secret) = load_credentials(&config_path)?;
            let store = FileTokenStore::new(config_path);
            let token_provider = RefreshingTokenProvider::new(
                store,
                SystemClock,
                client_id,
                client_secret,
            );
            let http_client = GoogleCalendarClient::new(reqwest::Client::new(), token_provider);
            let app = App {
                calendar_client: http_client,
                clock: SystemClock,
            };

            let mut out = std::io::stdout();
            app.handle_calendars(&mut out).await?;
        }

        Commands::Events { calendar, days } => {
            let (client_id, client_secret) = load_credentials(&config_path)?;
            let store = FileTokenStore::new(config_path);
            let token_provider = RefreshingTokenProvider::new(
                store,
                SystemClock,
                client_id,
                client_secret,
            );
            let http_client = GoogleCalendarClient::new(reqwest::Client::new(), token_provider);
            let app = App {
                calendar_client: http_client,
                clock: SystemClock,
            };

            let mut out = std::io::stdout();
            app.handle_events(&calendar, days, &mut out).await?;
        }
    }

    Ok(())
}

fn load_credentials(config_path: &std::path::Path) -> Result<(String, String), GcalError> {
    let config = Config::load(config_path)?;
    Ok((
        config.credentials.client_id,
        config.credentials.client_secret,
    ))
}

fn prompt(message: &str) -> Result<String, GcalError> {
    use std::io::Write;
    print!("{message}");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(GcalError::IoError)?;
    Ok(input.trim().to_string())
}
