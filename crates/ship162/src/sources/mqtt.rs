use anyhow::Result;
use rs162::sources::mqtt::{MqttConnectionStatus, MqttReceiver};
use tokio::sync::mpsc::Sender;

use crate::{
    sources::TimedMessage,
    status::{ReconnectingSource, SourceRuntime, SourceStatus},
};

pub struct MqttSource {
    tx: Sender<TimedMessage>,
    client_id: String,
    runtime: SourceRuntime,
}

impl MqttSource {
    pub fn new(tx: Sender<TimedMessage>, client_id: String, runtime: SourceRuntime) -> Self {
        Self {
            tx,
            client_id,
            runtime,
        }
    }
}

fn connection_status(status: MqttConnectionStatus) -> SourceStatus {
    match status {
        MqttConnectionStatus::Connecting => SourceStatus::Connecting,
        MqttConnectionStatus::Connected => SourceStatus::Connected,
        MqttConnectionStatus::Reconnecting => SourceStatus::Reconnecting {
            reason: "MQTT connection lost".into(),
        },
    }
}

impl ReconnectingSource for MqttSource {
    fn runtime(&self) -> &SourceRuntime {
        &self.runtime
    }

    async fn connect_once(&self) -> Result<()> {
        // Preserve the legacy config's client-ID meaning. Broker/topic settings
        // are a separate API change; new() still uses rs162's Digitraffic defaults.
        let mut source = MqttReceiver::new(&self.client_id).await?;
        let mut status = source.connection_status();
        self.runtime
            .report(connection_status(*status.borrow_and_update()));
        loop {
            tokio::select! {
                changed = status.changed() => {
                    changed?;
                    self.runtime.report(connection_status(*status.borrow_and_update()));
                }
                message = source.next() => {
                    let Some(message) = message else { return Ok(()) };
                    let message = message?;
                    self.tx.send(TimedMessage {
                        timestamp: message.timestamp,
                        signal_level: None,
                        message: message.message,
                        mmsi_info: None,
                        nmea_sentences: vec![],
                    }).await?;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mqtt_transport_states_are_reported_without_ais_messages() {
        assert_eq!(
            connection_status(MqttConnectionStatus::Connecting),
            SourceStatus::Connecting
        );
        assert_eq!(
            connection_status(MqttConnectionStatus::Connected),
            SourceStatus::Connected
        );
        assert!(matches!(
            connection_status(MqttConnectionStatus::Reconnecting),
            SourceStatus::Reconnecting { .. }
        ));
    }
}
