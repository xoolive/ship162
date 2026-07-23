use anyhow::Result;
use futures::StreamExt;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;
use tracing::{debug, info, warn};

use rs162::decode::nmea::{MessageAssembler, NmeaAisMessage};
use rs162::prelude::Message;

use crate::sources::{TimedMessage, WsPath};

pub struct WsSource {
    tx: Sender<TimedMessage>,
    path: WsPath,
}

impl WsSource {
    pub fn new(tx: Sender<TimedMessage>, path: WsPath) -> Self {
        Self { tx, path }
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            match self.connect_and_process().await {
                Ok(_) => {
                    tracing::warn!("WebSocket connection closed, reconnecting...");
                }
                Err(e) => {
                    tracing::warn!("WebSocket error: {}, reconnecting in 5 seconds...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn connect_and_process(&self) -> Result<()> {
        match &self.path {
            WsPath::Short(url) => {
                let (ws_stream, _) = connect_async(url.as_str()).await?;
                info!("Connected to WebSocket: {}", url);
                let (_write, read) = ws_stream.split();
                self.process_stream(read).await
            }
            WsPath::Long(ws) => {
                #[cfg(feature = "ssh")]
                if let Some(jump) = &ws.jump {
                    return self.connect_tunnelled(&ws.url, jump).await;
                }
                let (ws_stream, _) = connect_async(ws.url.as_str()).await?;
                info!("Connected to WebSocket: {}", ws.url);
                let (_write, read) = ws_stream.split();
                self.process_stream(read).await
            }
        }
    }

    #[cfg(feature = "ssh")]
    async fn connect_tunnelled(&self, url: &str, jump: &str) -> Result<()> {
        use rs162::sources::ssh::TunnelledWebsocket;
        use tokio_tungstenite::client_async;
        use tokio_tungstenite::tungstenite::handshake::client::Request;
        use url::Url;

        let parsed_url = Url::parse(url)?;
        let host = parsed_url.host_str().unwrap_or("localhost").to_owned();
        let port = parsed_url.port_or_known_default().unwrap_or(80);

        let tunnelled = TunnelledWebsocket {
            address: host,
            port,
            url: url.to_owned(),
            jump: jump.to_owned(),
        };

        let stream = tunnelled
            .connect()
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        info!("SSH tunnel established to {} via jump host {}", url, jump);

        let request = Request::builder()
            .uri(url)
            .header("Host", parsed_url.host_str().unwrap_or("localhost"))
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .body(())
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let (ws_stream, _) = client_async(request, stream)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        info!("Connected to WebSocket via tunnel: {}", url);

        let (_write, read) = ws_stream.split();
        self.process_stream(read).await
    }

    async fn process_stream<S>(&self, mut read: S) -> Result<()>
    where
        S: futures::Stream<
                Item = Result<
                    tokio_tungstenite::tungstenite::Message,
                    tokio_tungstenite::tungstenite::Error,
                >,
            > + Unpin,
    {
        let mut assembler = MessageAssembler::new();

        while let Some(msg) = read.next().await {
            let msg = msg?;

            let text = match &msg {
                tokio_tungstenite::tungstenite::Message::Text(t) => t.to_string(),
                tokio_tungstenite::tungstenite::Message::Binary(b) => {
                    match String::from_utf8(b.to_vec()) {
                        Ok(s) => s,
                        Err(_) => {
                            debug!("WebSocket binary message ({} bytes), not UTF-8", b.len());
                            continue;
                        }
                    }
                }
                tokio_tungstenite::tungstenite::Message::Ping(_)
                | tokio_tungstenite::tungstenite::Message::Pong(_) => continue,
                tokio_tungstenite::tungstenite::Message::Close(_) => break,
                _ => continue,
            };

            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                debug!("WS recv: {}", &line[..line.len().min(120)]);

                // Extract timestamp and NMEA sentence from various formats:
                // 1. Timestamped: \s:123,c:1234567890*XX\!AIVDM,...
                // 2. Plain NMEA: !AIVDM,...
                // 3. Bare NMEA without !: AIVDM,...
                let (timestamp, nmea_part) = if let Some(idx) = line.find("\\!") {
                    let ts = parse_timestamp_from_tag(line).unwrap_or_else(now);
                    let nmea = &line[idx + 1..];
                    (ts, nmea.to_string())
                } else if line.starts_with('!')
                    || line.starts_with("AIVDM")
                    || line.starts_with("BSVDM")
                {
                    (now(), line.to_string())
                } else {
                    debug!("WS skip: not NMEA");
                    continue;
                };

                let nmea_msg = match NmeaAisMessage::parse(&nmea_part) {
                    Ok(msg) => msg,
                    Err(e) => {
                        debug!("WS NMEA parse error: {}", e);
                        continue;
                    }
                };

                if let Ok(Some(binary)) = assembler.add_fragment(nmea_msg) {
                    let message = Message::from_nmea_binary(&binary).ok();
                    if let Some(message) = message {
                        let sentence = TimedMessage {
                            timestamp,
                            signal_level: None,
                            message,
                            mmsi_info: None,
                            nmea_sentences: vec![],
                        };
                        self.tx.send(sentence).await?;
                    }
                }
            }
        }

        warn!("WebSocket stream ended");
        Ok(())
    }
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

/// Extract timestamp from a tag block like \s:123,c:1234567890*XX\
fn parse_timestamp_from_tag(line: &str) -> Option<f64> {
    let tag_end = line.find("\\!")?;
    let tag = &line[..tag_end];
    for field in tag.split(',') {
        let field = field.trim_start_matches('\\');
        if let Some(ts_str) = field.strip_prefix("c:") {
            let ts_str = ts_str.split('*').next()?;
            return ts_str.parse().ok();
        }
    }
    None
}
