use futures::StreamExt; // for .next().await
use rs162::sources::iq::AisAsyncIqSource;
use tokio::sync::mpsc::Sender;
use tracing::info;

use crate::sources::TimedMessage;

pub struct Source {
    tx: Sender<TimedMessage>,
    source: AisAsyncIqSource,
}

impl Source {
    pub fn new(tx: Sender<TimedMessage>, source: AisAsyncIqSource) -> Self {
        Self { tx, source }
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        info!("Starting source...");

        while let Some(msg_res) = self.source.next().await {
            if let Ok(msg) = msg_res {
                let encoded = msg.encode_nmea();
                if let Some(ais_msg) = encoded.decode() {
                    let sentence = super::TimedMessage {
                        timestamp: encoded.timestamp,
                        signal_level: Some(encoded.signal_level),
                        message: ais_msg,
                        mmsi_info: None,
                        nmea_sentences: encoded.nmea_sentences,
                    };
                    self.tx.send(sentence).await?;
                }
            }
        }
        tracing::info!("source stream ended");

        Ok(())
    }
}
