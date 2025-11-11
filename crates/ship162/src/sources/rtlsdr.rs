use desperado::rtlsdr::RtlSdrConfig;
use futures::StreamExt; // for .next().await
use rs162::sources::rtlsdr::AsyncRtlSdrReceiver;
use tokio::sync::mpsc::Sender;
use tracing::info;

use crate::sources::TimedMessage;

pub struct RtlSdrSource {
    tx: Sender<TimedMessage>,
    config: RtlSdrConfig,
}

impl RtlSdrSource {
    pub fn new(tx: Sender<TimedMessage>, config: RtlSdrConfig) -> Self {
        Self { tx, config }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        info!("Starting RTL-SDR source...");
        let mut receiver = AsyncRtlSdrReceiver::with_config(self.config.clone())
            .await
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;

        while let Some(msg_res) = receiver.next().await {
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
