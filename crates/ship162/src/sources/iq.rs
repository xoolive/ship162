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
                if let Some(ais_msg) = msg.decode() {
                    let sentence = super::TimedMessage {
                        timestamp: msg.timestamp,
                        signal_level: Some(msg.signal_level),
                        message: ais_msg,
                        mmsi_info: None,
                    };
                    self.tx.send(sentence).await?;
                }
            }
        }
        println!("Stream returned None, exiting loop"); // Debug log

        Ok(())
    }
}
