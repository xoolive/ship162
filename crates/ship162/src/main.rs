#![doc = include_str!("../readme.md")]

mod diagnostics;
mod outputs;
mod sources;
mod state;
mod status;
mod table;
mod tui;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{KeyCode, KeyModifiers};
#[cfg(any(
    feature = "rtlsdr",
    feature = "soapy",
    feature = "airspy",
    feature = "hackrf"
))]
use desperado::Gain;
#[cfg(feature = "hackrf")]
use desperado::{GainElement, GainElementName};
use redis::AsyncCommands;
use rs162::dsp::ais::AIS_SAMPLE_RATE_288K;
use serde::Deserialize;
use state::AppState;
use std::{path::PathBuf, sync::Arc};
use tokio::{
    fs,
    io::AsyncWriteExt,
    sync::{mpsc, watch, Mutex},
};
use tracing::{error, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::sources::iq::supervise;
#[cfg(feature = "mqtt")]
use crate::sources::mqtt::MqttSource;
use crate::sources::tcp::TcpSource;
use crate::sources::websocket::WsSource;
use crate::status::{ReconnectingSource, RetryPolicy, SourceEvent, SourceId, SourceRuntime};
use crate::tui::{Event, EventHandler};
use rs162::sources::AisAsyncIqSource;

#[derive(Default, Deserialize, Parser)]
#[command(
    name = "ship162",
    about = "A lightweight AIS receiver and viewer using SDR, TCP, and MQTT sources"
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

    /// Diagnostic log file. "-" writes to stdout and cannot be combined with interactive or verbose output.
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

    /// Retry interval (seconds) when publishing to Redis fails (0 disables retry)
    #[arg(long, value_name = "SECONDS")]
    #[serde(default)]
    redis_retry_interval: Option<u64>,

    /// Serve decoded AIS as NMEA on TCP (e.g., 0.0.0.0:5631)
    #[arg(long, value_name = "ADDR")]
    serve_tcp: Option<String>,

    /// Serve decoded AIS as NMEA on UDP (e.g., 0.0.0.0:5632)
    #[arg(long, value_name = "ADDR")]
    serve_udp: Option<String>,

    /// Serve decoded AIS as NMEA on WebSocket (e.g., 0.0.0.0:8080)
    #[arg(long, value_name = "ADDR")]
    serve_ws: Option<String>,

    /// Static UDP targets to send NMEA to (e.g., feedme.mode-s.org:88888)
    #[arg(long, value_name = "ADDR")]
    #[serde(default)]
    udp_targets: Vec<String>,
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
        let string = fs::read_to_string(&cfg_path)
            .await
            .with_context(|| format!("failed to read config {}", cfg_path.display()))?;
        options = toml::from_str(&string)
            .with_context(|| format!("invalid config {}", cfg_path.display()))?;
    }

    if let Ok(config_file) = std::env::var("SHIP162_CONFIG") {
        let path = expanduser(PathBuf::from(config_file));
        let string = fs::read_to_string(&path)
            .await
            .with_context(|| format!("failed to read config {}", path.display()))?;
        options = toml::from_str(&string)
            .with_context(|| format!("invalid config {}", path.display()))?;
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
    if cli_options.redis_retry_interval.is_some() {
        options.redis_retry_interval = cli_options.redis_retry_interval;
    }
    if cli_options.serve_tcp.is_some() {
        options.serve_tcp = cli_options.serve_tcp;
    }
    if cli_options.serve_udp.is_some() {
        options.serve_udp = cli_options.serve_udp;
    }
    if cli_options.serve_ws.is_some() {
        options.serve_ws = cli_options.serve_ws;
    }
    if !cli_options.udp_targets.is_empty() {
        options.udp_targets = cli_options.udp_targets;
    }
    options.sources.append(&mut cli_options.sources);

    if options.interactive && options.verbose {
        anyhow::bail!(
            "interactive mode cannot be combined with verbose JSON output because both use stdout"
        );
    }
    if options.log_file.as_deref() == Some("-") {
        if options.interactive {
            anyhow::bail!(
                r#"log_file = "-" cannot be used in interactive mode because stdout is reserved for the TUI"#
            );
        }
        if options.verbose {
            anyhow::bail!(
                r#"log_file = "-" cannot be used with verbose JSON output because stdout is reserved for JSON"#
            );
        }
    }

    validate_sources(&options.sources)?;

    // example: RUST_LOG=rs162=DEBUG
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    let diagnostics = diagnostics::new_diagnostics();
    let subscriber = tracing_subscriber::registry()
        .with(diagnostics::UiEventLayer::new(diagnostics.clone()))
        .with(env_filter);
    match options.log_file.as_deref() {
        Some("-") => subscriber.with(fmt::layer().pretty()).init(),
        Some(log_file) => {
            let file = std::fs::File::create(log_file)
                .with_context(|| format!("failed to create log file {log_file}"))?;
            subscriber
                .with(fmt::layer().with_writer(file).with_ansi(false))
                .init();
        }
        None if options.interactive => {
            // Source diagnostics are displayed by the status panel. Output
            // errors are returned after cleanup; a log file captures details.
            subscriber.init();
        }
        None => subscriber
            .with(fmt::layer().with_writer(std::io::stderr))
            .init(),
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
    let redis_retry_interval =
        tokio::time::Duration::from_secs(options.redis_retry_interval.unwrap_or(5));

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

    // Fail output setup before starting readers or taking over the terminal.
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

    // Initialize terminal if interactive mode
    let terminal = if options.interactive {
        Some(tui::init()?)
    } else {
        None
    };

    // Create application state wrapped in Arc<Mutex<>> for sharing
    // Register all configured sources before any task can connect or the UI
    // can render. A quiet/failed source must never disappear from the count.
    let mut initial_state = initial_source_state(&options.sources);
    initial_state.diagnostics = diagnostics.clone();
    let state = Arc::new(Mutex::new(initial_state));
    let (status_tx, mut status_rx) = mpsc::unbounded_channel::<SourceEvent>();
    let status_state = state.clone();
    let status_handle = tokio::spawn(async move {
        while let Some(status) = status_rx.recv().await {
            status.log();
            status_state.lock().await.set_source_status(status);
        }
    });

    let (tx, mut rx) = tokio::sync::mpsc::channel(10);
    let (quit_tx, mut quit_rx) = watch::channel(false);
    // Add a shutdown signal channel
    let (shutdown_tx, _shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    // Headless receivers need the same graceful hardware shutdown as the TUI.
    {
        let quit_tx_signal = quit_tx.clone();
        let shutdown_tx_signal = shutdown_tx.clone();

        #[cfg(unix)]
        tokio::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};

            let mut sigint =
                signal(SignalKind::interrupt()).expect("Failed to create SIGINT handler");
            let mut sigterm =
                signal(SignalKind::terminate()).expect("Failed to create SIGTERM handler");
            let mut sighup = signal(SignalKind::hangup()).expect("Failed to create SIGHUP handler");

            tokio::select! {
                _ = sigint.recv() => {},
                _ = sigterm.recv() => {},
                _ = sighup.recv() => {},
            }

            let _ = quit_tx_signal.send(true);
            let _ = shutdown_tx_signal.send(());
        });

        #[cfg(windows)]
        tokio::spawn(async move {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen for Ctrl+C");

            let _ = quit_tx_signal.send(true);
            let _ = shutdown_tx_signal.send(());
        });
    }

    // NMEA broadcast channel for output servers (TCP/UDP/WebSocket)
    let (nmea_tx, _nmea_rx) = tokio::sync::broadcast::channel::<String>(4096);

    // Spawn output servers
    if let Some(ref addr) = options.serve_tcp {
        let addr: std::net::SocketAddr = addr.parse().expect("Invalid serve_tcp address");
        let shutdown = shutdown_tx.subscribe();
        let tx = nmea_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = outputs::tcp::run_tcp_server(addr, shutdown, tx).await {
                error!("TCP output server error: {}", e);
            }
        });
    }

    // Parse static UDP targets
    let udp_targets: Vec<std::net::SocketAddr> = options
        .udp_targets
        .iter()
        .map(|t| t.parse().expect("Invalid udp_targets address"))
        .collect();

    if options.serve_udp.is_some() || !udp_targets.is_empty() {
        let addr: std::net::SocketAddr = options
            .serve_udp
            .as_deref()
            .unwrap_or("0.0.0.0:0")
            .parse()
            .expect("Invalid serve_udp address");
        let shutdown = shutdown_tx.subscribe();
        let rx = nmea_tx.subscribe();
        let targets = udp_targets;
        tokio::spawn(async move {
            if let Err(e) = outputs::udp::run_udp_server(addr, targets, shutdown, rx).await {
                error!("UDP output server error: {}", e);
            }
        });
    }

    if let Some(ref addr) = options.serve_ws {
        let addr: std::net::SocketAddr = addr.parse().expect("Invalid serve_ws address");
        let shutdown = shutdown_tx.subscribe();
        let tx = nmea_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = outputs::websocket::run_websocket_server(addr, shutdown, tx).await {
                error!("WebSocket output server error: {}", e);
            }
        });
    }

    let has_output_servers =
        options.serve_tcp.is_some() || options.serve_udp.is_some() || options.serve_ws.is_some();

    let mut src_handles = vec![];
    for (source_index, source) in options.sources.into_iter().enumerate() {
        let source_id = SourceId::from_index(source_index);
        let tcp_clone = tx.clone();
        let source_status_tx = status_tx.clone();
        let mut shutdown_rx = shutdown_tx.subscribe();
        let handle = match source {
            sources::Source::Tcp(source_config) => tokio::spawn(async move {
                let (host, port, jump) = match source_config.tcp {
                    sources::AddressPath::Short(address) => {
                        (address.host, address.port.get(), None)
                    }
                    sources::AddressPath::Long(address) => {
                        (address.host, address.port.get(), address.jump)
                    }
                };
                let runtime = SourceRuntime::new(
                    source_id,
                    format!("tcp://{host}:{port}"),
                    source_status_tx.clone(),
                    source_config.retry,
                );
                #[cfg(feature = "ssh")]
                let source = if let Some(jump) = jump {
                    TcpSource::with_jump(tcp_clone, runtime, host, port, jump)
                } else {
                    TcpSource::new(tcp_clone, runtime, host, port)
                };
                #[cfg(not(feature = "ssh"))]
                let source = {
                    let _ = jump;
                    TcpSource::new(tcp_clone, runtime, host, port)
                };

                tokio::select! {
                    _ = source.run() => {}
                    _ = shutdown_rx.recv() => {
                        // Silent shutdown
                    }
                }
            }),
            #[cfg(feature = "mqtt")]
            sources::Source::Mqtt(source_config) => {
                let mqtt_clone = tx.clone();
                tokio::spawn(async move {
                    let client_id = source_config.mqtt;
                    let runtime = SourceRuntime::new(
                        source_id,
                        format!("mqtt: {client_id}"),
                        source_status_tx,
                        RetryPolicy::default(),
                    );
                    let source = MqttSource::new(mqtt_clone, client_id, runtime);
                    tokio::select! {
                        _ = source.run() => {},
                        _ = shutdown_rx.recv() => {},
                    }
                    source.runtime().report(status::SourceStatus::Stopped);
                })
            }
            #[cfg(feature = "rtlsdr")]
            sources::Source::RtlSdr(source_config) => {
                use desperado::rtlsdr::DeviceSelector;
                let rtl_clone = tx.clone();
                let rtl_path = source_config.rtlsdr;

                // Get gain and bias_tee from source config or use defaults
                let gain = source_config
                    .gain
                    .unwrap_or(Gain::Manual(sources::AIS_RTLSDR_GAIN));
                let bias_tee = source_config.bias_tee.unwrap_or(false);

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

                // Get sample rate from source config or use default 288 kHz
                let sample_rate = source_config
                    .sample_rate
                    .map(|r| r as u32)
                    .unwrap_or(AIS_SAMPLE_RATE_288K);
                let runtime = SourceRuntime::new(
                    source_id,
                    format!("rtlsdr://{device:?}"),
                    source_status_tx,
                    RetryPolicy::default(),
                );
                tokio::spawn(supervise(
                    rtl_clone,
                    runtime,
                    shutdown_rx,
                    true,
                    move || {
                        AisAsyncIqSource::from_rtlsdr(
                            device.clone(),
                            sample_rate,
                            gain.clone(),
                            bias_tee,
                        )
                    },
                ))
            }
            #[cfg(feature = "soapy")]
            sources::Source::Soapy(source_config) => {
                let soapy_clone = tx.clone();
                let soapy_path = source_config.soapy;
                let args = soapy_path.soapy.clone();

                // Get configuration from source or use defaults
                let gain = source_config
                    .gain
                    .unwrap_or(Gain::Manual(sources::AIS_RTLSDR_GAIN));
                let bias_tee = source_config.bias_tee.unwrap_or(false);
                // Use specified sample rate or default to 288 kHz
                let sample_rate = source_config
                    .sample_rate
                    .map(|r| r as u32)
                    .unwrap_or(AIS_SAMPLE_RATE_288K);

                let runtime = SourceRuntime::new(
                    source_id,
                    format!("soapy://{args}"),
                    source_status_tx,
                    RetryPolicy::default(),
                );
                tokio::spawn(supervise(
                    soapy_clone,
                    runtime,
                    shutdown_rx,
                    true,
                    move || {
                        AisAsyncIqSource::from_soapy(&args, sample_rate, gain.clone(), bias_tee)
                    },
                ))
            }
            #[cfg(feature = "airspy")]
            sources::Source::Airspy(source_config) => {
                use desperado::airspy::DeviceSelector as AirspyDeviceSelector;

                let airspy_clone = tx.clone();
                let airspy_path = source_config.airspy;

                let config = &airspy_path.config;
                let device = if let Some(idx) = config.device {
                    AirspyDeviceSelector::Index(idx)
                } else if let Some(ref serial) = config.serial {
                    AirspyDeviceSelector::Serial(
                        sources::parse_airspy_serial(serial).map_err(anyhow::Error::msg)?,
                    )
                } else {
                    AirspyDeviceSelector::Index(0)
                };

                // Default: sensitivity gain 50 (0-100 percentage for Airspy sensitivity table)
                let gain = source_config.gain.clone().unwrap_or(Gain::Manual(50.0));
                let bias_tee = source_config.bias_tee.unwrap_or(false);
                let sample_rate = source_config
                    .sample_rate
                    .map(|r| r as u32)
                    .unwrap_or(6_000_000);
                let lna_gain = config.lna_gain;
                let mixer_gain = config.mixer_gain;
                let vga_gain = config.vga_gain;

                let runtime = SourceRuntime::new(
                    source_id,
                    format!("airspy://{device:?}"),
                    source_status_tx,
                    RetryPolicy::default(),
                );
                tokio::spawn(supervise(
                    airspy_clone,
                    runtime,
                    shutdown_rx,
                    true,
                    move || {
                        AisAsyncIqSource::from_airspy(
                            device.clone(),
                            sample_rate,
                            gain.clone(),
                            bias_tee,
                            lna_gain,
                            mixer_gain,
                            vga_gain,
                        )
                    },
                ))
            }
            #[cfg(feature = "hackrf")]
            sources::Source::HackRf(source_config) => {
                let hackrf_clone = tx.clone();
                let hackrf_path = source_config.hackrf;

                let config = &hackrf_path.config;
                let device_index = config.device.unwrap_or(0);
                let amp_enable = config.amp_enable.unwrap_or(true);
                let lna_gain = config.lna_gain;
                let vga_gain = config.vga_gain;

                // Three-way gain priority (mirrors jet1090):
                //   1. Per-element gains in hackrf = { lna_gain, vga_gain } override everything
                //   2. Source-level `gain` is used as-is
                //   3. Default: LNA=40 dB, VGA=55 dB
                let gain = if lna_gain.is_some() || vga_gain.is_some() {
                    let mut elements = Vec::new();
                    if let Some(lna) = lna_gain {
                        elements.push(GainElement {
                            name: GainElementName::Lna,
                            value_db: lna as f64,
                        });
                    }
                    if let Some(vga) = vga_gain {
                        elements.push(GainElement {
                            name: GainElementName::Vga,
                            value_db: vga as f64,
                        });
                    }
                    Gain::Elements(elements)
                } else if let Some(g) = source_config.gain.clone() {
                    g
                } else {
                    Gain::Elements(vec![
                        GainElement {
                            name: GainElementName::Lna,
                            value_db: 40.0,
                        },
                        GainElement {
                            name: GainElementName::Vga,
                            value_db: 55.0,
                        },
                    ])
                };
                let bias_tee = source_config.bias_tee.unwrap_or(false);
                let sample_rate = source_config
                    .sample_rate
                    .map(|r| r as u32)
                    .unwrap_or(AIS_SAMPLE_RATE_288K);

                let runtime = SourceRuntime::new(
                    source_id,
                    format!("hackrf://{device_index}"),
                    source_status_tx,
                    RetryPolicy::default(),
                );
                tokio::spawn(supervise(
                    hackrf_clone,
                    runtime,
                    shutdown_rx,
                    true,
                    move || {
                        AisAsyncIqSource::from_hackrf(
                            device_index,
                            sample_rate,
                            gain.clone(),
                            amp_enable,
                            bias_tee,
                        )
                    },
                ))
            }
            sources::Source::IqFile(source_config) => {
                let file_clone = tx.clone();
                let file = source_config.iqfile;
                let runtime = SourceRuntime::new(
                    source_id,
                    format!("file://{file}"),
                    source_status_tx,
                    RetryPolicy::default(),
                );
                tokio::spawn(supervise(
                    file_clone,
                    runtime,
                    shutdown_rx,
                    false,
                    move || {
                        AisAsyncIqSource::from_file(
                            file.clone(),
                            AIS_SAMPLE_RATE_288K,
                            desperado::IqFormat::Cu8,
                        )
                    },
                ))
            }
            sources::Source::WebSocket(source_config) => {
                let ws_clone = tx.clone();
                tokio::spawn(async move {
                    let ws_path = source_config.ws;
                    let retry_policy = source_config.retry;
                    let source_name = match &ws_path {
                        sources::WsPath::Short(url) => url.as_str(),
                        sources::WsPath::Long(ws) => ws.url.as_str(),
                    };
                    let source = WsSource::new(
                        ws_clone,
                        SourceRuntime::new(
                            source_id,
                            source_name,
                            source_status_tx.clone(),
                            retry_policy,
                        ),
                        ws_path,
                    );
                    tokio::select! {
                        _ = source.run() => {}
                        _ = shutdown_rx.recv() => {
                            // Silent shutdown
                        }
                    }
                })
            }
        };
        src_handles.push(handle);
    }

    // Only the source tasks own senders now: all finite sources ending closes rx.
    drop(tx);

    // Create event handler and UI task if in interactive mode
    let ui_handle = if let Some(mut terminal) = terminal {
        let size = terminal.size()?;
        let mut events = EventHandler::new(size.width);
        let terminal_state = state.clone();
        let quit_tx_ui = quit_tx.clone();

        Some(tokio::spawn(async move {
            loop {
                // Never hold the shared state lock during terminal I/O. If a
                // writer is busy, handle input first and redraw on the next tick.
                let snapshot = terminal_state.try_lock().ok().map(|state| state.clone());
                if let Some(snapshot) = snapshot {
                    terminal.draw(|frame| table::render(frame, &snapshot))?;
                }

                match events.next().await? {
                    Event::Key(key) => match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            let _ = quit_tx_ui.send(true);
                            break;
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            let _ = quit_tx_ui.send(true);
                            break;
                        }
                        KeyCode::Char('e') | KeyCode::Char('E') => {
                            let mut state_guard = terminal_state.lock().await;
                            state_guard.show_errors = !state_guard.show_errors;
                            state_guard.error_scroll = 0;
                            if state_guard.show_errors {
                                diagnostics::mark_events_read(&state_guard.diagnostics);
                            }
                        }
                        KeyCode::Esc => {
                            let mut state_guard = terminal_state.lock().await;
                            if !state_guard.show_errors {
                                let _ = quit_tx_ui.send(true);
                                break;
                            }
                            state_guard.show_errors = false;
                            diagnostics::mark_events_read(&state_guard.diagnostics);
                        }
                        KeyCode::Char('s') | KeyCode::Char('S') => {
                            let mut state_guard = terminal_state.lock().await;
                            state_guard.show_sources = !state_guard.show_sources;
                        }
                        _ => {
                            let height = terminal.size()?.height;
                            let mut state_guard = terminal_state.lock().await;
                            navigate(&mut state_guard, key.code, height);
                        }
                    },
                    Event::Tick(_width) => {
                        // State is updated by the TCP source task
                    }
                    Event::Error => {
                        return Err(std::io::Error::other("terminal input failed"));
                    }
                }
            }
            Ok::<(), std::io::Error>(())
        }))
    } else {
        None
    };

    // The same processing path serves both modes. process_sentence consumes its
    // guard, so file/Redis I/O happens without holding the application-state lock.
    let process_messages = async {
        loop {
            if *quit_rx.borrow() {
                break;
            }
            let maybe_message = tokio::select! {
                biased;
                _ = quit_rx.changed() => None,
                message = rx.recv() => message,
            };
            let Some(mut message) = maybe_message else {
                break;
            };
            sources::process_sentence(state.lock().await, &mut message).await;
            if has_output_servers {
                for nmea in &message.nmea_sentences {
                    let line = outputs::format_timestamped_nmea(message.timestamp, nmea);
                    let _ = nmea_tx.send(line);
                }
            }
            let mut json = serde_json::to_string(&message)?;
            if options.verbose {
                println!("{json}");
            }
            if let Some(file) = &mut file {
                json.push('\n');
                file.write_all(json.as_bytes()).await?;
                json.pop();
            }
            if let Some(connection) = &mut redis_connect {
                // A failed output must not make quit wait forever for Redis.
                tokio::select! {
                    biased;
                    _ = quit_rx.changed() => break,
                    _ = publish_with_retry(connection, &redis_topic, &json, redis_retry_interval) => {},
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    };
    let processing_result = if let Some(mut ui_handle) = ui_handle {
        tokio::select! {
            result = process_messages => {
                // EOF/error can end processing before the user presses q.
                ui_handle.abort();
                let _ = ui_handle.await;
                result
            },
            result = &mut ui_handle => {
                let _ = quit_tx.send(true);
                result.context("TUI task failed").and_then(|result| result.map_err(Into::into))
            }
        }
    } else {
        process_messages.await
    };

    let _ = shutdown_tx.send(());
    let restore_result = if options.interactive {
        tui::restore()
    } else {
        Ok(())
    };

    // Do not detach timed-out tasks or discard panics during shutdown.
    let timeout = tokio::time::Duration::from_secs(5);
    let mut task_errors = Vec::new();
    for mut handle in src_handles {
        match tokio::time::timeout(timeout, &mut handle).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => task_errors.push(format!("source task failed: {error}")),
            Err(_) => {
                handle.abort();
                let _ = handle.await;
                task_errors.push("source shutdown timed out".to_string());
            }
        }
    }
    drop(status_tx);
    // Every source reporter is now dropped; drain status events before deciding
    // whether EOF was successful or a finite source failed.
    let status_result = status_handle.await.context("source status task failed");
    task_errors.extend(
        state
            .lock()
            .await
            .source_statuses()
            .filter_map(SourceEvent::terminal_error),
    );
    let flush_result = if let Some(file) = &mut file {
        file.flush().await
    } else {
        Ok(())
    };
    if options.interactive {
        // Do not write into the alternate screen, and include cleanup diagnostics.
        diagnostics::print_report(&diagnostics);
    }
    restore_result?;
    status_result?;
    flush_result?;
    processing_result?;
    if !task_errors.is_empty() {
        anyhow::bail!("{}", task_errors.join("; "));
    }
    Ok(())
}

fn navigate(state: &mut AppState, key: KeyCode, height: u16) {
    match key {
        KeyCode::Char('j' | 'J') | KeyCode::Down => {
            state.scroll_down(table::visible_rows(height, state))
        }
        KeyCode::Char('k' | 'K') | KeyCode::Up => state.scroll_up(),
        KeyCode::Char('g' | 'G') | KeyCode::Home | KeyCode::PageUp => state.scroll_to_top(),
        _ => {}
    }
}

fn initial_source_state(sources: &[sources::Source]) -> AppState {
    let mut state = AppState::new();
    for (index, source) in sources.iter().enumerate() {
        state.set_source_status(SourceEvent::connecting(
            SourceId::from_index(index),
            source.label(),
        ));
    }
    state
}

// Reject configuration errors before starting any device/task. Reconnection is
// for transient source failures, not invalid values that would panic in DSP setup.
fn validate_sources(sources: &[sources::Source]) -> Result<()> {
    for source in sources {
        let rate: Option<f64> = match source {
            #[cfg(feature = "rtlsdr")]
            sources::Source::RtlSdr(config) => config.sample_rate,
            #[cfg(feature = "soapy")]
            sources::Source::Soapy(config) => config.sample_rate,
            #[cfg(feature = "hackrf")]
            sources::Source::HackRf(config) => config.sample_rate,
            #[cfg(feature = "airspy")]
            sources::Source::Airspy(config) => {
                if let Some(serial) = &config.airspy.config.serial {
                    sources::parse_airspy_serial(serial).map_err(anyhow::Error::msg)?;
                }
                config.sample_rate
            }
            _ => None,
        };
        if let Some(rate) = rate {
            anyhow::ensure!(
                rate.is_finite() && rate > 0.0 && rate <= 12_288_000.0 && rate.fract() == 0.0,
                "invalid IQ sample rate {rate}: expected integer Hz in 1..=12288000"
            );
        }
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

async fn publish_with_retry(
    connection: &mut redis::aio::MultiplexedConnection,
    topic: &str,
    payload: &str,
    retry_interval: tokio::time::Duration,
) {
    loop {
        match connection.publish::<_, _, ()>(topic, payload).await {
            Ok(()) => break,
            Err(err) => {
                if retry_interval.is_zero() {
                    warn!(error = %err, "Redis publish failed; retries disabled");
                    break;
                }
                warn!(
                    error = %err,
                    retry_seconds = retry_interval.as_secs(),
                    "Redis publish failed; retrying"
                );
                tokio::time::sleep(retry_interval).await;
            }
        }
    }
}

#[cfg(test)]
mod tests;
