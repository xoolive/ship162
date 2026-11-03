use anyhow::Result;
use rs162::sources::mqtt::{MqttConnectionStatus, MqttReceiver};
use tokio::sync::mpsc::Sender;

use crate::{
    sources::{MqttSettings, TimedMessage},
    status::{ReconnectingSource, SourceRuntime, SourceStatus},
};

pub struct MqttSource {
    tx: Sender<TimedMessage>,
    settings: MqttSettings,
    client_id: String,
    runtime: SourceRuntime,
}

impl MqttSource {
    pub fn new(tx: Sender<TimedMessage>, settings: MqttSettings, runtime: SourceRuntime) -> Self {
        let client_id = settings.client_id.clone().unwrap_or_else(random_client_id);
        Self {
            tx,
            settings,
            client_id,
            runtime,
        }
    }
}

/// Brokers drop a client when another connects with its ID: make it unique
fn random_client_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or_default();
    format!("ship162-{:08x}", nanos ^ std::process::id().rotate_left(16))
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
        let mut source =
            MqttReceiver::with_config(&self.client_id, self.settings.url(), self.settings.topic())
                .await?;
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
                    self.tx.send(TimedMessage::received(
                        &self.runtime,
                        message.timestamp,
                        message.message,
                        None,
                    )).await?;
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
