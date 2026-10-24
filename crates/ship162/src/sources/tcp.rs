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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        outputs::format_timestamped_nmea,
        status::{RetryPolicy, SourceId},
    };
    use std::time::Duration;
    use tokio::{io::AsyncWriteExt, net::TcpListener, sync::mpsc, time::timeout};

    #[tokio::test]
    async fn norwegian_style_timestamped_nmea_decodes_over_loopback_tcp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let sentence = include_str!("../../../rs162/data/ais_nmea.txt")
            .lines()
            .next()
            .unwrap();
        let packet = format_timestamped_nmea(1_760_000_000.0, sentence);
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            socket.write_all(packet.as_bytes()).await.unwrap();
        });
        let (messages_tx, mut messages_rx) = mpsc::channel(1);
        let (status_tx, mut status_rx) = mpsc::unbounded_channel();
        let runtime = SourceRuntime::new(
            SourceId::from_index(0),
            "local NMEA",
            status_tx,
            RetryPolicy::default(),
        );
        let source = TcpSource::new(messages_tx, runtime, "127.0.0.1".into(), port);
        timeout(Duration::from_secs(2), source.connect_once())
            .await
            .unwrap()
            .unwrap();
        server.await.unwrap();
        assert!(matches!(
            status_rx.try_recv().unwrap().status(),
            SourceStatus::Connected
        ));
        let message = messages_rx.try_recv().unwrap();
        assert_eq!(message.timestamp, 1_760_000_000.0);
        assert!(message.message.mmsi() > 0);
        assert!(messages_rx.try_recv().is_err());
    }
}
