use anyhow::Result;
#[cfg(feature = "ssh")]
use rs162::sources::nmea_ts::AsyncTimestampedNmeaSource;
use rs162::sources::nmea_ts::AsyncTimestampedNmeaTcpSource;
use tokio::sync::mpsc::Sender;
use tracing::info;

#[cfg(feature = "ssh")]
use rs162::sources::ssh::TunnelledTcp;

use crate::sources::TimedMessage;

pub struct TcpSource {
    tx: Sender<TimedMessage>,
    host: String,
    port: u16,
    #[cfg(feature = "ssh")]
    jump: Option<String>,
}

impl TcpSource {
    pub fn new(tx: Sender<TimedMessage>, host: String, port: u16) -> Self {
        Self {
            tx,
            host,
            port,
            #[cfg(feature = "ssh")]
            jump: None,
        }
    }

    #[cfg(feature = "ssh")]
    pub fn with_jump(tx: Sender<TimedMessage>, host: String, port: u16, jump: String) -> Self {
        Self {
            tx,
            host,
            port,
            jump: Some(jump),
        }
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            match self.connect_and_process().await {
                Ok(_) => {
                    tracing::warn!("Connection closed, reconnecting...");
                }
                Err(e) => {
                    tracing::warn!("Error: {}, reconnecting in 5 seconds...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn connect_and_process(&self) -> Result<()> {
        #[cfg(feature = "ssh")]
        if let Some(jump) = &self.jump {
            // SSH tunnelled connection
            info!(
                "Connecting to {}:{} via SSH tunnel through {}",
                self.host, self.port, jump
            );
            let tunnel = TunnelledTcp {
                address: self.host.clone(),
                port: self.port,
                jump: jump.clone(),
            };
            let tunnel_reader = tunnel
                .connect()
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let buf_reader = tokio::io::BufReader::new(tunnel_reader);
            let mut source = AsyncTimestampedNmeaSource::from_reader(buf_reader);

            info!("Connected to {}:{} via tunnel", self.host, self.port);

            while let Some(line) = source.next().await {
                if let Ok(sentence) = line {
                    if let Some(message) = sentence.decode() {
                        let sentence = super::TimedMessage {
                            timestamp: sentence.timestamp,
                            signal_level: None,
                            message,
                            mmsi_info: None,
                            nmea_sentences: vec![],
                        };
                        self.tx.send(sentence).await?;
                    }
                }
            }

            return Ok(());
        }

        // Regular TCP connection
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
                        nmea_sentences: vec![],
                    };
                    self.tx.send(sentence).await?;
                }
            }
        }

        Ok(())
    }
}
