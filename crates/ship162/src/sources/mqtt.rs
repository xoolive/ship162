use anyhow::Result;
use rs162::sources::mqtt::MqttReceiver;
use tokio::sync::mpsc::Sender;

use crate::sources::TimedMessage;

pub struct MqttSource {
    tx: Sender<TimedMessage>,
    broker_url: String,
}

impl MqttSource {
    pub fn new(tx: Sender<TimedMessage>, broker_url: String) -> Self {
        Self { tx, broker_url }
    }

    pub async fn run(&self) -> Result<()> {
        // TODO: this is a bug in f27b429, passing broker_url to client_id is wrong
        // rs162 should allow configuring broker, tos and topic
        let mut source = MqttReceiver::new(&self.broker_url).await?;

        while let Some(message) = source.next().await {
            let Ok(message) = message else {
                continue;
            };
            self.tx
                .send(TimedMessage {
                    timestamp: message.timestamp,
                    signal_level: None,
                    message: message.message,
                    mmsi_info: None,
                    nmea_sentences: vec![],
                })
                .await?;
        }
        Ok(())
    }
}
