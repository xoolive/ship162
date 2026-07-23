use anyhow::Result;
#[cfg(feature = "ssh")]
use rs162::sources::nmea_ts::AsyncTimestampedNmeaSource;
use rs162::sources::nmea_ts::AsyncTimestampedNmeaTcpSource;
use tokio::sync::mpsc::Sender;

#[cfg(feature = "ssh")]
use rs162::sources::ssh::TunnelledTcp;

use crate::{
    sources::TimedMessage,
    status::{ReconnectingSource, SourceRuntime, SourceStatus},
};

pub struct TcpSource {
    tx: Sender<TimedMessage>,
    runtime: SourceRuntime,
    host: String,
    port: u16,
    #[cfg(feature = "ssh")]
    jump: Option<String>,
}

impl TcpSource {
    pub fn new(tx: Sender<TimedMessage>, runtime: SourceRuntime, host: String, port: u16) -> Self {
        Self {
            tx,
            runtime,
            host,
            port,
            #[cfg(feature = "ssh")]
            jump: None,
        }
    }

    #[cfg(feature = "ssh")]
    pub fn with_jump(
        tx: Sender<TimedMessage>,
        runtime: SourceRuntime,
        host: String,
        port: u16,
        jump: String,
    ) -> Self {
        Self {
            tx,
            runtime,
            host,
            port,
            jump: Some(jump),
        }
    }
}

impl ReconnectingSource for TcpSource {
    fn runtime(&self) -> &SourceRuntime {
        &self.runtime
    }

    async fn connect_once(&self) -> Result<()> {
        #[cfg(feature = "ssh")]
        if let Some(jump) = &self.jump {
            let tunnel = TunnelledTcp {
                address: self.host.clone(),
                port: self.port,
                jump: jump.clone(),
            };
            let tunnel_reader = tunnel
                .connect()
                .await
                .map_err(|error| anyhow::anyhow!("{error}"))?;
            tracing::info!(
                host = self.host.as_str(),
                port = self.port,
                jump,
                "ssh tunnel established"
            );
            let buf_reader = tokio::io::BufReader::new(tunnel_reader);
            let mut source = AsyncTimestampedNmeaSource::from_reader(buf_reader);

            self.runtime.report(SourceStatus::Connected);
            while let Some(line) = source.next().await {
                let Ok(sentence) = line else {
                    continue;
                };
                let Some(message) = sentence.decode() else {
                    continue;
                };
                self.tx
                    .send(super::TimedMessage {
                        timestamp: sentence.timestamp,
                        signal_level: None,
                        message,
                        mmsi_info: None,
                        nmea_sentences: vec![],
                    })
                    .await?;
            }
            return Ok(());
        }

        let mut source =
            AsyncTimestampedNmeaTcpSource::new(&format!("{}:{}", self.host, self.port)).await?;

        self.runtime.report(SourceStatus::Connected);
        while let Some(line) = source.next().await {
            let Ok(sentence) = line else {
                continue;
            };
            let Some(message) = sentence.decode() else {
                continue;
            };
            self.tx
                .send(super::TimedMessage {
                    timestamp: sentence.timestamp,
                    signal_level: None,
                    message,
                    mmsi_info: None,
                    nmea_sentences: vec![],
                })
                .await?;
        }

        Ok(())
    }
}
