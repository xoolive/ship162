//! MQTT source for AIS messages from Digitraffic Finland
//!
//! This module provides real-time AIS message reception from the Finnish Digitraffic
//! maritime traffic MQTT service (meri.digitraffic.fi).
//!
//! Example usage:
//! ```no_run
//! use rs162::sources::mqtt::MqttReceiver;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut receiver = MqttReceiver::new("MyApp/1.0").await?;
//!
//!     while let Some(result) = receiver.next().await {
//!         match result {
//!             Ok(msg) => {
//!                 println!("Received AIS message: {:?}", msg);
//!             }
//!             Err(e) => {
//!                 eprintln!("Error: {}", e);
//!             }
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```

use paho_mqtt as mqtt;
use serde::{Deserialize, Serialize};
use serde_json;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{
    decode::ais::{common::Timestamp, StaticAndVoyageData},
    prelude::Message,
};

const MQTT_BROKER: &str = "wss://meri.digitraffic.fi/mqtt"; // Use WebSocket Secure URI
const MQTT_TOPIC: &str = "vessels-v2/#";
const MQTT_QOS: i32 = 1;
const CHANNEL_BUFFER_SIZE: usize = 100;

/// JSON structure for position reports (vessels-v2/<mmsi>/location)
#[derive(Debug, Clone, Deserialize, Serialize)]
struct PositionReport {
    time: u64,
    #[serde(rename = "sog")]
    speed_over_ground: f32,
    #[serde(rename = "cog")]
    course_over_ground: f32,
    #[serde(rename = "navStat")]
    navigation_status: u8,
    #[serde(rename = "rot")]
    rate_of_turn: i16,
    #[serde(rename = "posAcc")]
    position_accuracy: bool,
    /// Receiver autonomous integrity monitoring (RAIM)
    raim: bool,
    heading: u16,
    #[serde(rename = "lon")]
    longitude: f64,
    #[serde(rename = "lat")]
    latitude: f64,
}

/// JSON structure for metadata (vessels-v2/<mmsi>/metadata)
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Metadata {
    timestamp: u64,
    destination: String,
    name: String,
    draught: u16,
    eta: u32,
    #[serde(rename = "posType")]
    pos_type: u8,
    #[serde(rename = "refA")]
    ref_a: u16,
    #[serde(rename = "refB")]
    ref_b: u16,
    #[serde(rename = "refC")]
    ref_c: u8,
    #[serde(rename = "refD")]
    ref_d: u8,
    #[serde(rename = "callSign")]
    callsign: String,
    imo: u32,
    #[serde(rename = "type")]
    ship_type: u8,
}

/// Represents a decoded AIS message received from MQTT
#[derive(Debug, Clone)]
struct MqttAisMessage {
    mmsi: u32,
    message_type: MqttMessageType,
}

#[derive(Debug, Clone)]
enum MqttMessageType {
    Position(PositionReport),
    Metadata(Metadata),
}

#[derive(Debug, Clone)]
pub struct MqttMessage {
    /// Timestamp in seconds since epoch
    pub timestamp: u64,
    /// AIS message (ported from the received JSON payload)
    pub message: Message,
}

impl MqttAisMessage {
    /// Parse MQTT message from topic and JSON payload
    fn from_mqtt(topic: String, payload: String) -> Result<Self, MqttError> {
        // Extract MMSI from topic: vessels-v2/<mmsi>/location or vessels-v2/<mmsi>/metadata
        let parts: Vec<&str> = topic.split('/').collect();
        if parts.len() != 3 || parts[0] != "vessels-v2" {
            return Err(MqttError::Channel(format!(
                "Invalid topic format: {}",
                topic
            )));
        }

        let mmsi = parts[1]
            .parse::<u32>()
            .map_err(|_| MqttError::Channel(format!("Invalid MMSI in topic: {}", parts[1])))?;

        let message_type = match parts[2] {
            "location" => {
                let position: PositionReport = serde_json::from_str(&payload).map_err(|e| {
                    MqttError::Channel(format!("Failed to parse position JSON: {}", e))
                })?;
                MqttMessageType::Position(position)
            }
            "metadata" => {
                let metadata: Metadata = serde_json::from_str(&payload).map_err(|e| {
                    MqttError::Channel(format!("Failed to parse metadata JSON: {}", e))
                })?;
                MqttMessageType::Metadata(metadata)
            }
            _ => {
                return Err(MqttError::Channel(format!(
                    "Unknown message type: {}",
                    parts[2]
                )));
            }
        };

        Ok(Self { mmsi, message_type })
    }

    /// Convert to standard AIS Message enum
    pub fn to_timed_message(&self) -> MqttMessage {
        use crate::prelude::*;

        match &self.message_type {
            MqttMessageType::Position(pos) => {
                // Convert to PositionReportClassA (Message Type 1, 2, or 3)
                MqttMessage {
                    timestamp: pos.time,
                    message: Message::PositionReport1(PositionReport {
                        mmsi: self.mmsi,
                        longitude: Some(pos.longitude),
                        latitude: Some(pos.latitude),
                        raim: pos.raim,
                        msg_type: 1,
                        repeat: 0,
                        status: NavigationStatus::from_bits(pos.navigation_status),
                        turn: Some(pos.rate_of_turn as f32),
                        speed: Some(pos.speed_over_ground),
                        accuracy: pos.position_accuracy,
                        course: Some(pos.course_over_ground),
                        heading: if pos.heading != 511 {
                            Some(pos.heading)
                        } else {
                            None
                        },
                        second: Timestamp::NotAvailable,
                        maneuver: ManeuverIndicator::NotAvailable,
                        spare_1: 0,
                        radio: 0,
                    }),
                }
            }
            MqttMessageType::Metadata(meta) => {
                // Convert to StaticAndVoyageRelatedData (Message Type 5)
                // Estimated time of arrival; MMDDHHMM UTC
                // Bits 19-16: month; 1-12; 0 = not available = default
                // Bits 15-11: day; 1-31; 0 = not available = default
                // Bits 10-6: hour; 0-23; 24 = not available = default
                // Bits 5-0: minute; 0-59; 60 = not available = default
                let month = (meta.eta >> 16) as u8;
                let day = ((meta.eta >> 11) & 0x1F) as u8;
                let hour = ((meta.eta >> 6) & 0x1F) as u8;
                let minute = (meta.eta & 0x3F) as u8;
                MqttMessage {
                    timestamp: meta.timestamp / 1000,
                    message: Message::StaticAndVoyageData(StaticAndVoyageData {
                        ais_version: 0,
                        msg_type: 5,
                        mmsi: self.mmsi,
                        repeat: 0,
                        callsign: meta.callsign.clone(),
                        shipname: meta.name.clone(),
                        ship_type: ShipType::from_bits(meta.ship_type),
                        destination: meta.destination.clone(),
                        imo: Some(meta.imo),
                        to_bow: meta.ref_a,
                        to_stern: meta.ref_b,
                        to_port: meta.ref_c,
                        to_starboard: meta.ref_d,
                        dte: false,
                        epfd: EpfdType::from_bits(meta.pos_type),
                        month: if month == 0 { None } else { Some(month) },
                        day: if day == 0 { None } else { Some(day) },
                        hour: if hour == 24 { None } else { Some(hour) },
                        minute: if minute == 60 { None } else { Some(minute) },
                        draught: if meta.draught == 0 {
                            None
                        } else {
                            Some(meta.draught as f32 / 10.0)
                        },
                        spare_1: 0,
                    }),
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum MqttError {
    Connection(mqtt::Error),
    Channel(String),
}

impl std::fmt::Display for MqttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MqttError::Connection(e) => write!(f, "MQTT connection error: {}", e),
            MqttError::Channel(s) => write!(f, "Channel error: {}", s),
        }
    }
}

impl std::error::Error for MqttError {}

impl From<mqtt::Error> for MqttError {
    fn from(e: mqtt::Error) -> Self {
        MqttError::Connection(e)
    }
}

/// Asynchronous MQTT receiver for AIS messages
pub struct MqttReceiver {
    rx: mpsc::Receiver<Result<MqttMessage, MqttError>>,
    client: mqtt::AsyncClient,
}

impl MqttReceiver {
    /// Create a new MQTT receiver with the specified application name
    pub async fn new(client_id: &str) -> Result<Self, MqttError> {
        Self::with_config(client_id, MQTT_BROKER, MQTT_TOPIC).await
    }

    /// Create a new MQTT receiver with custom broker and topic
    pub async fn with_config(
        client_id: &str,
        broker: &str,
        topic: &str,
    ) -> Result<Self, MqttError> {
        info!("Creating MQTT client with ID: {}", client_id);

        // Use WebSocket transport
        let create_opts = mqtt::CreateOptionsBuilder::new()
            .server_uri(broker)
            .client_id(client_id)
            .persistence(None)
            .finalize();

        let client = mqtt::AsyncClient::new(create_opts)?;

        // Set up connection options with TLS
        let ssl_opts = mqtt::SslOptionsBuilder::new().finalize();

        let conn_opts = mqtt::ConnectOptionsBuilder::new()
            .ssl_options(ssl_opts)
            .keep_alive_interval(Duration::from_secs(20))
            .clean_session(true)
            .automatic_reconnect(Duration::from_secs(1), Duration::from_secs(30))
            .finalize();

        // Create channel for messages
        let (tx, rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);

        // Set up message callback
        let tx_clone = tx.clone();

        client.set_message_callback(move |_cli, msg_opt| {
            if let Some(msg) = msg_opt {
                let topic = msg.topic().to_string();
                let payload = msg.payload_str().to_string();

                match MqttAisMessage::from_mqtt(topic.clone(), payload) {
                    Ok(mqtt_msg) => {
                        if let Err(e) = tx_clone.try_send(Ok(mqtt_msg.to_timed_message())) {
                            warn!("Failed to send message to channel: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse MQTT message from topic {}: {}", topic, e);
                    }
                }
            }
        });

        // Set up connection callback
        let topic_sub = topic.to_string();
        client.set_connected_callback(move |cli| {
            info!("Connected to MQTT broker, subscribing to {}", topic_sub);
            let _sub_token = cli.subscribe(&topic_sub, MQTT_QOS);
        });

        // Set up disconnection callback
        client.set_connection_lost_callback(|_cli| {
            warn!("Connection to MQTT broker lost");
        });

        // Connect to broker
        info!("Connecting to MQTT broker at {}", broker);
        client.connect(conn_opts).await?;

        Ok(Self { rx, client })
    }

    /// Get the next AIS message from the MQTT stream
    pub async fn next(&mut self) -> Option<Result<MqttMessage, MqttError>> {
        self.rx.recv().await
    }

    /// Disconnect from the MQTT broker
    pub async fn disconnect(self) -> Result<(), MqttError> {
        info!("Disconnecting from MQTT broker");
        self.client.disconnect(None).await?;
        Ok(())
    }

    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }
}

impl Drop for MqttReceiver {
    fn drop(&mut self) {
        if self.is_connected() {
            info!("MqttReceiver dropped, disconnecting");
            // Note: Can't await in Drop, so we just disconnect synchronously
            std::mem::drop(self.client.disconnect(None));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_mqtt_connection() {
        let receiver = MqttReceiver::new("ship162").await;
        assert!(receiver.is_ok());

        if let Ok(mut rx) = receiver {
            assert!(rx.is_connected());

            // Try to receive a few messages
            let mut count = 0;
            while let Some(Ok(msg)) = rx.next().await {
                println!("Received message from MMSI: {:?}", msg);

                count += 1;
                if count >= 10 {
                    break;
                }
            }

            let _ = rx.disconnect().await;
        }
    }

    #[test]
    fn test_parse_position_json() {
        let json = r#"{
            "time": 1763764197,
            "sog": 13.2,
            "cog": 231.3,
            "navStat": 0,
            "rot": 0,
            "posAcc": true,
            "raim": false,
            "heading": 230,
            "lon": 26.275163,
            "lat": 60.150685
        }"#;

        let topic = "vessels-v2/230703000/location".to_string();
        let msg = MqttAisMessage::from_mqtt(topic, json.to_string());
        assert!(msg.is_ok());

        let msg = msg.unwrap();
        assert_eq!(msg.mmsi, 230703000);

        if let MqttMessageType::Position(pos) = msg.message_type {
            assert_eq!(pos.speed_over_ground, 13.2);
            assert_eq!(pos.latitude, 60.150685);
        } else {
            panic!("Expected Position message type");
        }
    }

    #[test]
    fn test_parse_metadata_json() {
        let json = r#"{
            "timestamp": 1763764197190,
            "destination": "FINLI<>FILAN<>SEKPS",
            "name": "FINNSIRIUS",
            "draught": 68,
            "eta": 766287,
            "posType": 3,
            "refA": 65,
            "refB": 170,
            "refC": 10,
            "refD": 24,
            "callSign": "OJUK",
            "imo": 9902419,
            "type": 60
        }"#;

        let topic = "vessels-v2/253361000/metadata".to_string();
        let msg = MqttAisMessage::from_mqtt(topic, json.to_string());
        assert!(msg.is_ok());

        let msg = msg.unwrap();
        assert_eq!(msg.mmsi, 253361000);

        if let MqttMessageType::Metadata(meta) = msg.message_type {
            assert_eq!(meta.name, "FINNSIRIUS");
            assert_eq!(meta.imo, 9902419);
        } else {
            panic!("Expected Metadata message type");
        }
    }
}
