#![doc = include_str!("../readme.md")]

mod sources;
mod state;
mod table;
mod tui;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{KeyCode, KeyModifiers};
use redis::AsyncCommands;
use rs162::dsp::ais::AIS_SAMPLE_RATE_288K;
use serde::Deserialize;
use state::AppState;
use std::{path::PathBuf, sync::Arc};
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::sources::iq::Source;
#[cfg(feature = "mqtt")]
use crate::sources::mqtt::MqttSource;
use crate::sources::tcp::TcpSource;
use crate::tui::{Event, EventHandler};
use rs162::sources::AisAsyncIqSource;

#[derive(Default, Deserialize, Parser)]
#[command(
    name = "ship162",
    about = "A lightweight AIS receiver and viewer using RTL-SDR and TCP sources"
)]
struct Options {
    /// Activate JSON output to stdout
    #[arg(short, long, default_value = "false")]
    #[serde(default)]
    verbose: bool,

    /// Display a table in interactive mode (not compatible with verbose)
    #[arg(short, long, default_value = "false")]
    #[serde(default)]
    interactive: bool,

    /// Output file to write received messages in JSON format
    #[arg(short, long)]
    output: Option<String>,

    /// List the sources of data
    #[serde(default)]
    sources: Vec<sources::Source>,

    /// logging file, use "-" for stdout (only in non-interactive mode)
    #[arg(short, long, value_name = "FILE")]
    log_file: Option<String>,

    /// Prevent the computer sleeping when decoding is in progress
    #[arg(long, default_value = "false")]
    #[serde(default)]
    prevent_sleep: bool,

    /// Publish messages to a Redis pubsub
    #[arg(short, long, value_name = "REDIS URL")]
    redis_url: Option<String>,

    /// Redis topic for the messages, default to "ship162"
    #[arg(long, value_name = "REDIS TOPIC")]
    redis_topic: Option<String>,
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
    if cli_options.verbose {
        options.verbose = true;
    }
    if cli_options.interactive {
        options.interactive = true;
    }
    if cli_options.output.is_some() {
        options.output = cli_options.output;
    }
    if cli_options.log_file.is_some() {
        options.log_file = cli_options.log_file;
    }
    if cli_options.prevent_sleep {
        options.prevent_sleep = cli_options.prevent_sleep;
    }
    if cli_options.redis_url.is_some() {
        options.redis_url = cli_options.redis_url;
    }
    if cli_options.redis_topic.is_some() {
        options.redis_topic = cli_options.redis_topic;
    }
    options.sources.append(&mut cli_options.sources);

    // example: RUST_LOG=rs162=DEBUG
    let env_filter = EnvFilter::from_default_env();

    let subscriber = tracing_subscriber::registry().with(env_filter);
    match options.log_file.as_deref() {
        Some("-") if !options.interactive => {
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

    let mut redis_connect = match options
        .redis_url
        .map(|url| redis::Client::open(url).unwrap())
    {
        // map is not possible because of the .await (the async context thing)
        Some(c) => Some(
            c.get_multiplexed_async_connection()
                .await
                .expect("Unable to connect to the Redis server"),
        ),
        None => None,
    };
    let redis_topic = options.redis_topic.unwrap_or("ship162".to_string());

    let _awake = match options.prevent_sleep {
        true => Some(
            keepawake::Builder::default()
                .display(false)
                .idle(true)
                .sleep(true)
                .reason("ship162 decoding in progress")
                .app_name("ship162")
                .app_reverse_domain("io.github.ship162")
                .create()?,
        ),
        false => None,
    };

    // Initialize terminal if interactive mode
    let terminal = if options.interactive {
        Some(tui::init()?)
    } else {
        None
    };

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
                        #[cfg(feature = "ssh")]
                        let source = if let Some(jump) = addr.jump {
                            TcpSource::with_jump(tcp_clone, host, port, jump)
                        } else {
                            TcpSource::new(tcp_clone, host, port)
                        };
                        #[cfg(not(feature = "ssh"))]
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
            #[cfg(feature = "mqtt")]
            sources::Address::Mqtt(broker_url) => {
                let mqtt_clone = tx.clone();
                tokio::spawn(async move {
                    let source = MqttSource::new(mqtt_clone, broker_url);
                    tokio::select! {
                        result = source.run() => {
                            if let Err(e) = result {
                                eprintln!("MQTT source error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            // Silent shutdown
                        }
                    }
                })
            }
            #[cfg(feature = "pluto")]
            sources::Address::Pluto(pluto_path) => {
                let pluto_clone = tx.clone();
                let uri = pluto_path.pluto.clone();

                // Get gain from source config or use default
                let gain = source.gain.unwrap_or(sources::AIS_PLUTO_GAIN);

                let ais_source =
                    AisAsyncIqSource::from_pluto(&uri, AIS_SAMPLE_RATE_288K, Some(gain)).await?;
                tokio::spawn(async move {
                    let mut source = Source::new(pluto_clone, ais_source);
                    tokio::select! {
                        result = source.run() => {
                            if let Err(e) = result {
                                eprintln!("PlutoSDR source error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            // Silent shutdown
                        }
                    }
                })
            }
            #[cfg(feature = "rtlsdr")]
            sources::Address::Rtlsdr(rtl_path) => {
                use desperado::rtlsdr::DeviceSelector;
                let rtl_clone = tx.clone();

                // Get gain and bias_tee from source config or use defaults
                let gain = source.gain.unwrap_or(sources::AIS_RTLSDR_GAIN);
                let bias_tee = source.bias_tee.unwrap_or(false);

                // Determine device selector based on config
                let config = &rtl_path.config;
                let device = if let Some(idx) = config.device {
                    // Device index specified
                    DeviceSelector::Index(idx)
                } else if config.serial.is_some()
                    || config.manufacturer.is_some()
                    || config.product.is_some()
                {
                    // At least one filter specified
                    DeviceSelector::Filter {
                        manufacturer: config.manufacturer.clone(),
                        product: config.product.clone(),
                        serial: config.serial.clone(),
                    }
                } else {
                    // Empty config, default to device 0
                    DeviceSelector::Index(0)
                };

                let ais_source = AisAsyncIqSource::from_rtlsdr_selector(
                    device,
                    AIS_SAMPLE_RATE_288K,
                    Some(gain),
                    bias_tee,
                )
                .await?;
                tokio::spawn(async move {
                    let mut source = Source::new(rtl_clone, ais_source);
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
            #[cfg(feature = "soapy")]
            sources::Address::Soapy(soapy_path) => {
                let soapy_clone = tx.clone();
                let args = soapy_path.soapy.clone();

                // Get configuration from source or use defaults
                let gain = source.gain.unwrap_or(sources::AIS_RTLSDR_GAIN);
                let bias_tee = source.bias_tee.unwrap_or(false);
                let gain_element = source.gain_element.as_deref().unwrap_or("TUNER");

                let ais_source = AisAsyncIqSource::from_soapy(
                    &args,
                    AIS_SAMPLE_RATE_288K,
                    Some(gain),
                    gain_element,
                    bias_tee,
                )
                .await?;
                tokio::spawn(async move {
                    let mut source = Source::new(soapy_clone, ais_source);
                    tokio::select! {
                        result = source.run() => {
                            if let Err(e) = result {
                                eprintln!("SoapySDR source error: {}", e);
                            }
                        }
                        _ = shutdown_rx.recv() => {
                            // Silent shutdown
                        }
                    }
                })
            }
            sources::Address::IqFile(file) => {
                let file_clone = tx.clone();
                let source = AisAsyncIqSource::from_file(
                    &file,
                    AIS_SAMPLE_RATE_288K,
                    desperado::IqFormat::Cu8,
                )
                .await?;
                tokio::spawn(async move {
                    let mut source = Source::new(file_clone, source);
                    tokio::select! {
                        result = source.run() => {
                            if let Err(e) = result {
                                eprintln!("IQ File source error: {}", e);
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

    // Create event handler and UI task if in interactive mode
    let ui_handle = if let Some(mut terminal) = terminal {
        let size = terminal.size()?;
        let mut events = EventHandler::new(size.width);
        let terminal_state = state.clone();

        Some(tokio::spawn(async move {
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
        }))
    } else {
        None
    };

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

    // Use tokio::select! to handle message processing and optional UI
    if let Some(ui_handle) = ui_handle {
        // Interactive mode: wait for either UI exit or message processing
        tokio::select! {
            _ = async {
                while let Some(mut message) = rx.recv().await {
                    let state_guard = state.lock().await;
                    sources::process_sentence(state_guard, &mut message).await;
                    if let Ok(json) = serde_json::to_string(&message) {
                        if let Some(file) = &mut file {
                            let _ = file.write_all(json.as_bytes()).await;
                            let _ = file.write_all(b"\n").await;
                        }
                        if let Some(c) = &mut redis_connect {
                            let _: () = c.publish(redis_topic.clone(), json).await.unwrap();
                        }
                    }
                }
            } => {},
            _ = ui_handle => {
                // UI has exited, send shutdown signal
                let _ = shutdown_tx.send(());
            }
        }
    } else {
        // Non-interactive mode: just process messages
        while let Some(mut message) = rx.recv().await {
            let state_guard = state.lock().await;
            sources::process_sentence(state_guard, &mut message).await;
            if let Ok(json) = serde_json::to_string(&message) {
                if options.verbose {
                    println!("{json}");
                }
                if let Some(file) = &mut file {
                    let _ = file.write_all(json.as_bytes()).await;
                    let _ = file.write_all(b"\n").await;
                }
                if let Some(c) = &mut redis_connect {
                    let _: () = c.publish(redis_topic.clone(), json).await.unwrap();
                }
            }
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
