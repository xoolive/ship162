#![doc = include_str!("../../../readme.md")]

mod sources;
mod state;
mod table;
mod tui;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{KeyCode, KeyModifiers};
use serde::Deserialize;
use state::AppState;
use std::{path::PathBuf, sync::Arc};
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use sources::tcp::TcpSource;

use crate::sources::rtlsdr::RtlSdrSource;
use crate::tui::{Event, EventHandler};

#[derive(Default, Deserialize, Parser)]
#[command(
    name = "ship162",
    about = "A lightweight AIS receiver and viewer using RTL-SDR and TCP sources"
)]
struct Options {
    /// Output file to write received messages in JSON format
    #[arg(short, long)]
    output: Option<String>,

    /// List the sources of data
    sources: Vec<sources::Source>,

    /// logging file, use "-" for stdout (only in non-interactive mode)
    #[arg(short, long, value_name = "FILE")]
    log_file: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file
    dotenv::dotenv().ok();

    let mut options = Options::default();

    let mut cfg_path = match std::env::var("XDG_CONFIG_HOME") {
        Ok(xdg_config) => expanduser(PathBuf::from(xdg_config)),
        Err(_) => dirs::config_dir().unwrap_or_default(),
    };
    cfg_path.push("ship162");
    cfg_path.push("config.toml");

    if cfg_path.exists() {
        let string = fs::read_to_string(cfg_path).await.ok().unwrap();
        options = toml::from_str(&string).unwrap();
    }

    if let Ok(config_file) = std::env::var("SHIP162_CONFIG") {
        let path = expanduser(PathBuf::from(config_file));
        let string = fs::read_to_string(path)
            .await
            .expect("Configuration file not found");
        options = toml::from_str(&string).unwrap();
    }

    let mut cli_options = Options::parse();
    if cli_options.output.is_some() {
        options.output = cli_options.output;
    }
    if cli_options.log_file.is_some() {
        options.log_file = cli_options.log_file;
    }
    options.sources.append(&mut cli_options.sources);

    // example: RUST_LOG=rs1090=DEBUG
    let env_filter = EnvFilter::from_default_env();

    let subscriber = tracing_subscriber::registry().with(env_filter);
    match options.log_file.as_deref() {
        Some("-") /*if !cli_options.interactive*/ => {
            // when it's interactive, logs will disrupt the display
            subscriber.with(fmt::layer().pretty()).init();
        }
        Some(log_file) if log_file != "-" => {
            let file = std::fs::File::create(log_file)
                .unwrap_or_else(|_| panic!("fail to create log file: {log_file}"));
            let file_layer = fmt::layer().with_writer(file).with_ansi(false);
            subscriber.with(file_layer).init();
        }
        _ => {
            subscriber.init(); // no logging
        }
    }
    // Initialize terminal
    let mut terminal = tui::init()?;

    // Create application state wrapped in Arc<Mutex<>> for sharing
    let state = Arc::new(Mutex::new(AppState::new()));

    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    // Add a shutdown signal channel
    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    let mut src_handles = vec![];
    for source in options.sources {
        let tcp_clone = tx.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        let handle = match source.address {
            sources::Address::Tcp(address) => tokio::spawn(async move {
                match address {
                    sources::AddressPath::Short(addr) => {
                        let parts: Vec<&str> = addr.split(':').collect();
                        if parts.len() != 2 {
                            eprintln!("Invalid TCP address format: {}", addr);
                            return;
                        }
                        let host = parts[0].to_string();
                        let port = parts[1].parse::<u16>().unwrap_or_else(|_| {
                            eprintln!("Invalid port number in address: {}", addr);
                            0
                        });
                        if port == 0 {
                            return;
                        }
                        let source = TcpSource::new(tcp_clone, host, port);
                        tokio::select! {
                            result = source.run() => {
                                if let Err(e) = result {
                                    eprintln!("TCP source error: {}", e);
                                }
                            }
                            _ = shutdown_rx.recv() => {
                                // Silent shutdown
                            }
                        }
                    }
                    sources::AddressPath::Long(addr) => {
                        let host = addr.host;
                        let port = addr.port;
                        let source = TcpSource::new(tcp_clone, host, port);
                        tokio::select! {
                            result = source.run() => {
                                if let Err(e) = result {
                                    eprintln!("TCP source error: {}", e);
                                }
                            }
                            _ = shutdown_rx.recv() => {
                                // Silent shutdown
                            }
                        }
                    }
                }
            }),
            sources::Address::Rtlsdr(_) => {
                let rtl_clone = tx.clone();
                tokio::spawn(async move {
                    let source = RtlSdrSource::new(rtl_clone, Default::default());
                    tokio::select! {
                        result = source.run() => {
                            if let Err(e) = result {
                                eprintln!("RTL-SDR source error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            // Silent shutdown
                        }
                    }
                })
            }
        };
        src_handles.push(handle);
    }

    // Create event handler
    let size = terminal.size()?;
    let mut events = EventHandler::new(size.width);

    let terminal_state = state.clone();

    // Main application loop
    let ui_handle = tokio::spawn(async move {
        loop {
            // Lock state for rendering
            let state_guard = terminal_state.lock().await;
            terminal.draw(|frame| table::render(frame, &state_guard))?;
            drop(state_guard); // Release lock before waiting for events

            match events.next().await? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        break;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        let max_visible = terminal.size()?.height.saturating_sub(5) as usize;
                        let mut state_guard = terminal_state.lock().await;
                        state_guard.scroll_down(max_visible);
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        let mut state_guard = terminal_state.lock().await;
                        state_guard.scroll_up();
                    }
                    _ => {}
                },
                Event::Tick(_width) => {
                    // State is updated by the TCP source task
                }
                Event::Error => {
                    break;
                }
            }
        }
        // Restore terminal
        tui::restore()
    });

    let mut file = if let Some(output_path) = options.output {
        let output_path = expanduser(PathBuf::from(output_path));
        Some(
            fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(output_path)
                .await?,
        )
    } else {
        None
    };

    // Use tokio::select! to handle both message processing and UI completion
    tokio::select! {
        _ = async {
            while let Some(message) = rx.recv().await {
                let state_guard = state.lock().await;
                sources::process_sentence(state_guard, &message).await;
                if let Ok(json) = serde_json::to_string(&message) {
                    if let Some(file) = &mut file {
                        let _ = file.write_all(json.as_bytes()).await;
                        let _ = file.write_all(b"\n").await;
                    }
                }
            }
        } => {},
        _ = ui_handle => {
            // UI has exited, send shutdown signal
            let _ = shutdown_tx.send(());
        }
    }

    // Wait for all source tasks to finish (with timeout)
    let timeout = tokio::time::Duration::from_secs(2);
    for handle in src_handles {
        let _ = tokio::time::timeout(timeout, handle).await;
    }

    Ok(())
}

fn expanduser(path: PathBuf) -> PathBuf {
    // Check if the path starts with "~"
    if let Some(stripped) = path.to_str().and_then(|p| p.strip_prefix("~")) {
        if let Some(home_dir) = dirs::home_dir() {
            // Join the home directory with the rest of the path
            return home_dir.join(stripped.trim_start_matches('/'));
        }
    }
    path
}
