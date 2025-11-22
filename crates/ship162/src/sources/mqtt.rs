use rs162::sources::mqtt::MqttReceiver;
use tokio::sync::mpsc::Sender;
use tracing::info;

use crate::sources::TimedMessage;

pub struct MqttSource {
    tx: Sender<TimedMessage>,
    broker_url: String,
}

impl MqttSource {
    pub fn new(tx: Sender<TimedMessage>, broker_url: String) -> Self {
        Self { tx, broker_url }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
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

    async fn connect_and_process(&self) -> anyhow::Result<()> {
        let mut source = MqttReceiver::new(&self.broker_url).await?;

        info!("Connected to MQTT broker at {}", self.broker_url);
        while let Some(msg) = source.next().await {
            if let Ok(msg) = msg {
                let sentence = super::TimedMessage {
                    timestamp: msg.timestamp,
                    signal_level: None,
                    message: msg.message,
                    mmsi_info: None,
                };
                self.tx.send(sentence).await?;
            }
        }
        Ok(())
    }
}
