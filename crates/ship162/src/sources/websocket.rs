use anyhow::Result;
use futures::StreamExt;
use tokio::sync::mpsc::Sender;
use tokio_tungstenite::connect_async;
use tracing::info;

use rs162::decode::nmea::{MessageAssembler, NmeaAisMessage};
use rs162::prelude::Message;

use crate::sources::TimedMessage;

pub struct WsSource {
    tx: Sender<TimedMessage>,
    url: String,
}

impl WsSource {
    pub fn new(tx: Sender<TimedMessage>, url: String) -> Self {
        Self { tx, url }
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            match self.connect_and_process().await {
                Ok(_) => {
                    eprintln!("WebSocket connection closed, reconnecting...");
                }
                Err(e) => {
                    eprintln!("WebSocket error: {}, reconnecting in 5 seconds...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn connect_and_process(&self) -> Result<()> {
        let (ws_stream, _) = connect_async(&self.url).await?;
        info!("Connected to WebSocket: {}", self.url);

        let (_write, mut read) = ws_stream.split();
        let mut assembler = MessageAssembler::new();

        while let Some(msg) = read.next().await {
            let msg = msg?;
            let text = match msg {
                tokio_tungstenite::tungstenite::Message::Text(t) => t.to_string(),
                _ => continue,
            };

            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // Try timestamped format (\s:...\!AIVDM,...) then plain NMEA
                let (timestamp, nmea_part) = if let Some(idx) = line.find("\\!") {
                    let ts = parse_timestamp_from_tag(line).unwrap_or_else(now);
                    let nmea = &line[idx + 1..];
                    (ts, nmea.to_string())
                } else if line.starts_with('!') {
                    (now(), line.to_string())
                } else {
                    continue;
                };

                let nmea_msg = match NmeaAisMessage::parse(&nmea_part) {
                    Ok(msg) => msg,
                    Err(_) => continue,
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
