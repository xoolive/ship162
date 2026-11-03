//! MQTT source for AIS messages from Digitraffic Finland
//!
//! This module provides real-time AIS message reception from the Finnish Digitraffic
//! maritime traffic MQTT service (meri.digitraffic.fi), with the pure-Rust rumqttc
//! client over secure websockets. Connections go through the HTTP proxy from the
//! environment (`https_proxy`, `no_proxy`; see [`crate::sources::proxy`]).
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

use std::sync::Arc;
use std::time::Duration;

use rumqttc::{
    AsyncClient, ConnectionError, Event, EventLoop, MqttOptions, Packet, Proxy, ProxyAuth,
    ProxyType, QoS, TlsConfiguration, Transport,
};
use serde::{Deserialize, Serialize};
use serde_json;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::{
    decode::ais::{common::Timestamp, StaticAndVoyageData},
    prelude::Message,
};

const MQTT_BROKER: &str = "wss://meri.digitraffic.fi/mqtt"; // Use WebSocket Secure URI
const MQTT_TOPIC: &str = "vessels-v2/#";
const MQTT_QOS: QoS = QoS::AtLeastOnce;
const CHANNEL_BUFFER_SIZE: usize = 100;
/// Digitraffic asks clients to identify themselves with this header
const DIGITRAFFIC_USER: &str = "ship162";
const KEEP_ALIVE: Duration = Duration::from_secs(20);
const RECONNECT_MIN: Duration = Duration::from_secs(1);
const RECONNECT_MAX: Duration = Duration::from_secs(30);

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
    pub timestamp: f64,
    /// AIS message (ported from the received JSON payload)
    pub message: Message,
}

impl MqttAisMessage {
    /// Parse AIS data, ignoring the service's non-AIS status topic.
    fn from_mqtt(topic: String, payload: String) -> Result<Option<Self>, MqttError> {
        // vessels-v2/# includes this heartbeat/control topic. It is neither AIS
        // nor a transport connection event, and its payload need not be AIS JSON.
        if topic == "vessels-v2/status" {
            return Ok(None);
        }
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

        Ok(Some(Self { mmsi, message_type }))
    }

    /// Convert to standard AIS Message enum
    pub fn to_timed_message(&self) -> MqttMessage {
        use crate::prelude::*;

        match &self.message_type {
            MqttMessageType::Position(pos) => {
                // Convert to PositionReportClassA (Message Type 1, 2, or 3)
                MqttMessage {
                    timestamp: pos.time as f64,
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
                    timestamp: meta.timestamp as f64 / 1000.0,
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
    /// The broker could not be reached, or the MQTT connection failed
    Connection(String),
    /// The broker address is not a supported URI
    Address(String),
    Channel(String),
}

impl std::fmt::Display for MqttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MqttError::Connection(e) => write!(f, "MQTT connection error: {}", e),
            MqttError::Address(e) => write!(f, "invalid MQTT broker address: {}", e),
            MqttError::Channel(s) => write!(f, "Channel error: {}", s),
        }
    }
}

impl std::error::Error for MqttError {}

impl From<ConnectionError> for MqttError {
    fn from(e: ConnectionError) -> Self {
        MqttError::Connection(e.to_string())
    }
}

/// MQTT transport state, independent of whether AIS messages are arriving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MqttConnectionStatus {
    /// The initial connection is being established.
    Connecting,
    /// The MQTT transport is connected.
    Connected,
    /// The transport was lost; the client is reconnecting automatically.
    Reconnecting,
}

/// Broker transport, parsed from `tcp://host:port`, `mqtt://host:port`, `ws://…` or `wss://…`
#[derive(Debug, Clone, PartialEq, Eq)]
struct Broker {
    host: String,
    port: u16,
    /// Websocket URL, None for plain TCP
    websocket: Option<String>,
    secure: bool,
}

impl Broker {
    fn parse(uri: &str) -> Result<Self, MqttError> {
        let (scheme, rest) = uri
            .split_once("://")
            .ok_or_else(|| MqttError::Address(format!("{uri}: expected scheme://host")))?;
        let (secure, websocket, default_port) = match scheme {
            "tcp" | "mqtt" => (false, false, 1883),
            "ws" => (false, true, 80),
            "wss" => (true, true, 443),
            _ => {
                return Err(MqttError::Address(format!(
                    "{uri}: unsupported scheme `{scheme}`"
                )))
            }
        };
        let authority = rest.split('/').next().unwrap_or_default();
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) if !host.contains(':') || host.ends_with(']') => (
                host,
                port.parse()
                    .map_err(|_| MqttError::Address(format!("{uri}: invalid port")))?,
            ),
            _ => (authority, default_port),
        };
        if host.is_empty() {
            return Err(MqttError::Address(format!("{uri}: missing host")));
        }
        Ok(Self {
            host: host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string(),
            port,
            websocket: websocket.then(|| uri.to_string()),
            secure,
        })
    }

    fn options(&self, client_id: &str) -> MqttOptions {
        let address = self.websocket.clone().unwrap_or_else(|| self.host.clone());
        let mut options = MqttOptions::new(client_id, address, self.port);
        options.set_keep_alive(KEEP_ALIVE).set_clean_session(true);
        match (&self.websocket, self.secure) {
            (Some(_), true) => {
                options.set_transport(Transport::wss_with_config(tls_configuration()));
            }
            (Some(_), false) => {
                options.set_transport(Transport::Ws);
            }
            _ => {}
        }
        if self.websocket.is_some() {
            options.set_request_modifier(|mut request: http::Request<()>| async move {
                if let Ok(value) = DIGITRAFFIC_USER.parse() {
                    request.headers_mut().insert("Digitraffic-User", value);
                }
                request
            });
        }
        // Websocket brokers go through the HTTP proxy from the environment, if any
        match crate::sources::proxy::ProxySettings::from_env().proxy_for(&self.host, self.secure) {
            Ok(Some(proxy)) => {
                info!(
                    proxy.host,
                    proxy.port, "connecting to the MQTT broker through an HTTP proxy"
                );
                let auth = match proxy.credentials() {
                    Some((username, password)) => ProxyAuth::Basic {
                        username: username.to_string(),
                        password: password.to_string(),
                    },
                    None => ProxyAuth::None,
                };
                options.set_proxy(Proxy {
                    ty: ProxyType::Http,
                    auth,
                    addr: proxy.host.clone(),
                    port: proxy.port,
                });
            }
            Ok(None) => {}
            Err(error) => warn!("ignoring invalid proxy setting: {}", error),
        }
        options
    }
}

/// TLS with the bundled Mozilla roots and the ring provider: no system certificates or C toolchain needed
fn tls_configuration() -> TlsConfiguration {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("ring supports the default TLS versions")
    .with_root_certificates(roots)
    .with_no_client_auth();
    TlsConfiguration::Rustls(Arc::new(config))
}

/// Asynchronous MQTT receiver for AIS messages
pub struct MqttReceiver {
    rx: mpsc::Receiver<Result<MqttMessage, MqttError>>,
    client: AsyncClient,
    connection_status: watch::Receiver<MqttConnectionStatus>,
    task: JoinHandle<()>,
}

impl MqttReceiver {
    /// Create a new MQTT receiver with the specified application name
    pub async fn new(client_id: &str) -> Result<Self, MqttError> {
        Self::with_config(client_id, MQTT_BROKER, MQTT_TOPIC).await
    }

    /// Create a new MQTT receiver with custom broker and topic
    ///
    /// Returns once the broker acknowledged the connection, or with the error of
    /// the first attempt. Later disconnections are retried automatically and
    /// reported through [`MqttReceiver::connection_status`].
    pub async fn with_config(
        client_id: &str,
        broker: &str,
        topic: &str,
    ) -> Result<Self, MqttError> {
        info!("Creating MQTT client with ID: {}", client_id);
        let options = Broker::parse(broker)?.options(client_id);
        let (client, mut eventloop) = AsyncClient::new(options, CHANNEL_BUFFER_SIZE);
        let (status_tx, connection_status) = watch::channel(MqttConnectionStatus::Connecting);

        // The first connection: report its failure to the caller, as a reconnecting source expects
        info!("Connecting to MQTT broker at {}", broker);
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => break,
                Ok(_) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        status_tx.send_replace(MqttConnectionStatus::Connected);
        info!("Connected to MQTT broker, subscribing to {}", topic);
        client
            .subscribe(topic, MQTT_QOS)
            .await
            .map_err(|e| MqttError::Connection(e.to_string()))?;

        let (tx, rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
        let task = tokio::spawn(run(
            eventloop,
            client.clone(),
            topic.to_string(),
            tx,
            status_tx,
        ));
        Ok(Self {
            rx,
            client,
            connection_status,
            task,
        })
    }

    /// Observe connection/reconnection changes without waiting for AIS data.
    pub fn connection_status(&self) -> watch::Receiver<MqttConnectionStatus> {
        self.connection_status.clone()
    }

    /// Get the next AIS message from the MQTT stream
    pub async fn next(&mut self) -> Option<Result<MqttMessage, MqttError>> {
        self.rx.recv().await
    }

    /// Disconnect from the MQTT broker
    pub async fn disconnect(self) -> Result<(), MqttError> {
        info!("Disconnecting from MQTT broker");
        self.client
            .disconnect()
            .await
            .map_err(|e| MqttError::Connection(e.to_string()))
    }

    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        *self.connection_status.borrow() == MqttConnectionStatus::Connected
    }
}

/// Drive the event loop: forward AIS messages, reconnect with a growing delay, resubscribe
async fn run(
    mut eventloop: EventLoop,
    client: AsyncClient,
    topic: String,
    tx: mpsc::Sender<Result<MqttMessage, MqttError>>,
    status_tx: watch::Sender<MqttConnectionStatus>,
) {
    let mut delay = RECONNECT_MIN;
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                delay = RECONNECT_MIN;
                status_tx.send_replace(MqttConnectionStatus::Connected);
                info!("Reconnected to MQTT broker, subscribing to {}", topic);
                // A clean session forgets subscriptions; subscribe from a task, as the
                // request queue is only drained by this loop
                let client = client.clone();
                let topic = topic.clone();
                tokio::spawn(async move {
                    if let Err(e) = client.subscribe(topic, MQTT_QOS).await {
                        warn!("MQTT subscription failed: {}", e);
                    }
                });
            }
            Ok(Event::Incoming(Packet::Publish(publish))) => {
                let payload = String::from_utf8_lossy(&publish.payload).into_owned();
                match MqttAisMessage::from_mqtt(publish.topic.clone(), payload) {
                    Ok(Some(message)) => {
                        if let Err(e) = tx.try_send(Ok(message.to_timed_message())) {
                            if tx.is_closed() {
                                return; // the receiver was dropped
                            }
                            warn!("Failed to send message to channel: {}", e);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => warn!(
                        "Failed to parse MQTT message from topic {}: {}",
                        publish.topic, e
                    ),
                }
            }
            Ok(event) => debug!(?event, "MQTT event"),
            Err(ConnectionError::RequestsDone) => return, // client disconnected
            Err(error) => {
                if tx.is_closed() {
                    return;
                }
                if *status_tx.borrow() != MqttConnectionStatus::Reconnecting {
                    warn!("Connection to MQTT broker lost: {}", error);
                }
                status_tx.send_replace(MqttConnectionStatus::Reconnecting);
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(RECONNECT_MAX);
            }
        }
    }
}

impl Drop for MqttReceiver {
    fn drop(&mut self) {
        // Stop the event loop; the broker sees the connection close
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connection_callbacks_report_idle_connection_and_loss() {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
            sync::oneshot,
            time::timeout,
        };

        // A minimal local MQTT broker: acknowledge CONNECT, then close on request.
        // No external Digitraffic connection or AIS message is needed.
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let broker_url = format!("tcp://{}", listener.local_addr().unwrap());
        let (close_tx, close_rx) = oneshot::channel();
        let broker = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            assert_eq!(socket.read_u8().await.unwrap() >> 4, 1); // CONNECT
            let mut remaining = 0_usize;
            let mut multiplier = 1;
            loop {
                let byte = socket.read_u8().await.unwrap();
                remaining += usize::from(byte & 127) * multiplier;
                assert!(remaining < 4096);
                if byte & 128 == 0 {
                    break;
                }
                multiplier *= 128;
            }
            let mut payload = vec![0; remaining];
            socket.read_exact(&mut payload).await.unwrap();
            socket.write_all(&[0x20, 0x02, 0x00, 0x00]).await.unwrap(); // CONNACK
            close_rx.await.unwrap();
        });
        let receiver = timeout(
            Duration::from_secs(5),
            MqttReceiver::with_config("ship162-lifecycle-test", &broker_url, "test/#"),
        )
        .await
        .unwrap()
        .unwrap();
        let mut status = receiver.connection_status();
        timeout(
            Duration::from_secs(5),
            status.wait_for(|status| *status == MqttConnectionStatus::Connected),
        )
        .await
        .unwrap()
        .unwrap();
        close_tx.send(()).unwrap();
        timeout(
            Duration::from_secs(5),
            status.wait_for(|status| *status == MqttConnectionStatus::Reconnecting),
        )
        .await
        .unwrap()
        .unwrap();
        broker.await.unwrap();
    }

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
    fn parses_broker_addresses() {
        let broker = Broker::parse("wss://meri.digitraffic.fi/mqtt").unwrap();
        assert_eq!(
            (broker.host.as_str(), broker.port, broker.secure),
            ("meri.digitraffic.fi", 443, true)
        );
        assert_eq!(
            broker.websocket.as_deref(),
            Some("wss://meri.digitraffic.fi/mqtt")
        );
        let broker = Broker::parse("tcp://127.0.0.1:1884").unwrap();
        assert_eq!(
            (broker.host.as_str(), broker.port, broker.websocket),
            ("127.0.0.1", 1884, None)
        );
        assert_eq!(Broker::parse("mqtt://broker").unwrap().port, 1883);
        assert_eq!(Broker::parse("ws://[::1]:8080/mqtt").unwrap().host, "::1");
        assert!(Broker::parse("broker:1883").is_err());
        assert!(Broker::parse("ssl://broker:8883").is_err());
    }

    #[tokio::test]
    async fn first_connection_failure_is_returned() {
        // Nothing listens on this port
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let result = MqttReceiver::with_config(
            "ship162-refused",
            &format!("tcp://127.0.0.1:{port}"),
            "test/#",
        )
        .await;
        assert!(matches!(result, Err(MqttError::Connection(_))));
    }

    #[test]
    fn service_status_messages_are_not_ais_parse_failures() {
        for payload in ["online", "", r#"{"status":"ok"}"#] {
            assert!(
                MqttAisMessage::from_mqtt("vessels-v2/status".into(), payload.into())
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn malformed_ais_data_and_unknown_topics_still_return_errors() {
        for topic in [
            "vessels-v2/230703000/location",
            "vessels-v2/230703000/metadata",
            "vessels-v2/not-an-mmsi/location",
            "vessels-v2/230703000/unknown",
            "vessels-v2/status/unknown",
            "unexpected/status",
        ] {
            assert!(
                MqttAisMessage::from_mqtt(topic.into(), "{}".into()).is_err(),
                "silently ignored {topic}"
            );
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

        let msg = msg.unwrap().unwrap();
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

        let msg = msg.unwrap().unwrap();
        assert_eq!(msg.mmsi, 253361000);

        if let MqttMessageType::Metadata(meta) = msg.message_type {
            assert_eq!(meta.name, "FINNSIRIUS");
            assert_eq!(meta.imo, 9902419);
        } else {
            panic!("Expected Metadata message type");
        }
    }
}
