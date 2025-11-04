use anyhow::Result;
use rs162::sources::nmea_ts::AsyncTimestampedNmeaTcpSource;
use tokio::sync::mpsc::Sender;
use tracing::info;

use crate::sources::TimedMessage;

pub struct TcpSource {
    tx: Sender<TimedMessage>,
    host: String,
    port: u16,
}

impl TcpSource {
    pub fn new(tx: Sender<TimedMessage>, host: String, port: u16) -> Self {
        Self { tx, host, port }
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            match self.connect_and_process().await {
                Ok(_) => {
                    eprintln!("Connection closed, reconnecting...");
                }
                Err(e) => {
                    eprintln!("Error: {}, reconnecting in 5 seconds...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn connect_and_process(&self) -> Result<()> {
        let mut source =
            AsyncTimestampedNmeaTcpSource::new(&format!("{}:{}", self.host, self.port)).await?;

        info!("Connected to {}:{}", self.host, self.port);

        while let Some(line) = source.next().await {
            if let Ok(sentence) = line {
                if let Some(message) = sentence.decode() {
                    let sentence = super::TimedMessage {
                        timestamp: sentence.timestamp,
                        signal_level: None,
                        message,
                        mmsi_info: None,
                    };
                    self.tx.send(sentence).await?;
                }
            }
        }

        Ok(())
    }
}
