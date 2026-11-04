//! Websocket source for AIS messages from [aisstream.io](https://aisstream.io)
//!
//! aisstream.io relays decoded AIS messages from a worldwide network of
//! terrestrial receivers as JSON, over a websocket. A free API key is needed.
//! This module subscribes to one or more bounding boxes, and converts the
//! position and static reports into the same [`Message`] values as the NMEA
//! decoder.
//!
//! The connection negotiates `permessage-deflate` compression, which aisstream.io
//! asks clients to use, and goes through an HTTP proxy when `https_proxy` is
//! set (see [`crate::sources::proxy`]).
//!
//! ```no_run
//! use rs162::sources::aisstream::{AisStreamReceiver, BoundingBox, Subscription};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let api_key = std::env::var("AISSTREAM_API_KEY")?;
//!     // The English Channel
//!     let channel = BoundingBox::new([48.5, -6.0], [52.0, 3.0]);
//!     let subscription = Subscription::new(api_key, vec![channel]);
//!     let mut receiver = AisStreamReceiver::connect(&subscription).await?;
//!     while let Some(message) = receiver.next().await {
//!         println!("{:?}", message?);
//!     }
//!     Ok(())
//! }
//! ```

use std::sync::Arc;
use std::time::Duration;

use futures::SinkExt;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::{client::TlsStream, rustls, TlsConnector};
use tracing::{debug, info};
use url::Url;
use yawc::{Frame, OpCode, Options, WebSocket};

use crate::decode::ais::{
    common::{EpfdType, ManeuverIndicator, NavigationStatus, ShipType, Timestamp},
    converters::from_turn,
    type24::{StaticDataReport, StaticDataReportPartA, StaticDataReportPartB},
    ClassBPositionReport, ExtendedClassBPositionReport, PositionReport, StaticAndVoyageData,
};
use crate::prelude::Message;
use crate::sources::proxy;

/// The aisstream.io websocket endpoint
pub const AISSTREAM_URL: &str = "wss://stream.aisstream.io/v0/stream";

/// The server closes connections that do not subscribe within 3 seconds
const SUBSCRIBE_TIMEOUT: Duration = Duration::from_secs(3);
/// Bound for the TCP, TLS and websocket handshakes. The server sometimes leaves an
/// upgrade unanswered; a short timeout lets the reconnect policy try again sooner.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

/// Errors raised by the aisstream.io source
#[derive(Debug, thiserror::Error)]
pub enum AisStreamError {
    /// Network or TLS error
    #[error("aisstream.io connection error: {0}")]
    Io(#[from] std::io::Error),
    /// Websocket protocol error
    #[error("aisstream.io websocket error: {0}")]
    WebSocket(#[from] yawc::WebSocketError),
    /// The handshake did not complete in time
    #[error("aisstream.io handshake timed out")]
    Timeout,
    /// The server reported an error, for example an invalid API key
    #[error("aisstream.io error: {0}")]
    Server(String),
    /// A message could not be decoded
    #[error("invalid aisstream.io message: {0}")]
    Json(#[from] serde_json::Error),
}

/// A latitude/longitude rectangle, given by two opposite corners
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BoundingBox(pub [[f64; 2]; 2]);

impl BoundingBox {
    /// A box from two `[latitude, longitude]` corners
    #[must_use]
    pub fn new(corner: [f64; 2], opposite: [f64; 2]) -> Self {
        Self([corner, opposite])
    }

    /// The whole world
    #[must_use]
    pub fn world() -> Self {
        Self::new([-90.0, -180.0], [90.0, 180.0])
    }
}

/// A subscription request
///
/// The API key is never printed by the `Debug` implementation.
#[derive(Clone, PartialEq, Serialize)]
pub struct Subscription {
    #[serde(rename = "APIKey")]
    api_key: String,
    /// Areas of interest; messages outside all boxes are not sent
    #[serde(rename = "BoundingBoxes")]
    pub bounding_boxes: Vec<BoundingBox>,
    /// Optional MMSI filter (at most 200 entries)
    #[serde(rename = "FiltersShipMMSI", skip_serializing_if = "Vec::is_empty")]
    pub mmsi: Vec<String>,
    /// Optional message type filter, for example `PositionReport`
    #[serde(rename = "FilterMessageTypes", skip_serializing_if = "Vec::is_empty")]
    pub message_types: Vec<String>,
}

impl std::fmt::Debug for Subscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscription")
            .field("api_key", &"***")
            .field("bounding_boxes", &self.bounding_boxes)
            .field("mmsi", &self.mmsi)
            .field("message_types", &self.message_types)
            .finish()
    }
}

impl Subscription {
    /// A subscription to all message types within `bounding_boxes`
    #[must_use]
    pub fn new(api_key: impl Into<String>, bounding_boxes: Vec<BoundingBox>) -> Self {
        Self {
            api_key: api_key.into(),
            bounding_boxes,
            mmsi: vec![],
            message_types: vec![],
        }
    }
}

/// A decoded AIS message with its reception time
#[derive(Debug, Clone)]
pub struct AisStreamMessage {
    /// Reception time, in seconds since the Unix epoch
    pub timestamp: f64,
    /// The AIS message
    pub message: Message,
}

/// A connection to aisstream.io
pub struct AisStreamReceiver {
    websocket: WebSocket<TlsStream<TcpStream>>,
    compression: Option<bool>,
}

impl AisStreamReceiver {
    /// Connect to [`AISSTREAM_URL`] and send the subscription
    pub async fn connect(subscription: &Subscription) -> Result<Self, AisStreamError> {
        Self::connect_to(AISSTREAM_URL, subscription).await
    }

    /// Connect to a custom `wss://` endpoint and send the subscription
    pub async fn connect_to(
        url: &str,
        subscription: &Subscription,
    ) -> Result<Self, AisStreamError> {
        let url = Url::parse(url).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string())
        })?;
        let host = url
            .host_str()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing host"))?
            .to_string();
        let port = url.port_or_known_default().unwrap_or(443);

        let mut websocket = tokio::time::timeout(CONNECT_TIMEOUT, async {
            let tcp = proxy::connect(&host, port, true).await?;
            tcp.set_nodelay(true)?;
            debug!(host, port, "aisstream.io TCP connected");
            let server_name = ServerName::try_from(host.clone()).map_err(|error| {
                std::io::Error::new(std::io::ErrorKind::InvalidInput, error.to_string())
            })?;
            let tls = tls_connector().connect(server_name, tcp).await?;
            debug!("aisstream.io TLS established");
            let options = Options::default()
                .with_balanced_compression()
                .with_max_payload_read(4 * 1024 * 1024);
            let websocket = WebSocket::handshake(url.clone(), tls, options).await?;
            debug!("aisstream.io websocket upgraded");
            Ok::<_, AisStreamError>(websocket)
        })
        .await
        .map_err(|_| AisStreamError::Timeout)??;

        let request = serde_json::to_string(subscription)?;
        tokio::time::timeout(SUBSCRIBE_TIMEOUT, websocket.send(Frame::text(request)))
            .await
            .map_err(|_| AisStreamError::Timeout)??;
        info!(
            boxes = subscription.bounding_boxes.len(),
            "subscribed to aisstream.io"
        );
        Ok(Self {
            websocket,
            compression: None,
        })
    }

    /// Whether the server confirmed `permessage-deflate`, once the confirmation arrived
    #[must_use]
    pub fn compression_enabled(&self) -> Option<bool> {
        self.compression
    }

    /// The next AIS message, or `None` when the server closed the connection
    pub async fn next(&mut self) -> Option<Result<AisStreamMessage, AisStreamError>> {
        loop {
            let frame = match self.websocket.next_frame().await {
                Ok(frame) => frame,
                Err(error) => return Some(Err(error.into())),
            };
            match frame.opcode() {
                OpCode::Text | OpCode::Binary => {}
                OpCode::Close => return None,
                _ => continue,
            }
            match decode(frame.payload()) {
                Ok(Decoded::Message(message)) => return Some(Ok(message)),
                Ok(Decoded::Confirmation { compression }) => {
                    info!(compression, "aisstream.io subscription confirmed");
                    self.compression = Some(compression);
                }
                Ok(Decoded::Ignored(message_type)) => {
                    debug!(message_type, "ignoring aisstream.io message");
                }
                Err(error) => return Some(Err(error)),
            }
        }
    }
}

fn tls_connector() -> TlsConnector {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = rustls::crypto::CryptoProvider::get_default()
        .cloned()
        .unwrap_or_else(|| Arc::new(rustls::crypto::ring::default_provider()));
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring supports the default TLS versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    TlsConnector::from(Arc::new(config))
}

/// The outcome of decoding one websocket message
#[derive(Debug)]
pub enum Decoded {
    /// A supported AIS message
    Message(AisStreamMessage),
    /// The subscription confirmation, telling whether compression is enabled
    Confirmation {
        /// Whether `permessage-deflate` was negotiated
        compression: bool,
    },
    /// A valid message of a type this source does not convert
    Ignored(String),
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "MessageType")]
    message_type: Option<String>,
    #[serde(rename = "MetaData")]
    metadata: Option<MetaData>,
    #[serde(rename = "Message")]
    message: Option<serde_json::Value>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct MetaData {
    time_utc: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Dimension {
    a: u16,
    b: u16,
    c: u8,
    d: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JsonPositionReport {
    #[serde(rename = "MessageID")]
    message_id: u8,
    repeat_indicator: u8,
    #[serde(rename = "UserID")]
    user_id: u32,
    navigational_status: u8,
    rate_of_turn: i16,
    sog: f32,
    position_accuracy: bool,
    longitude: f64,
    latitude: f64,
    cog: f32,
    true_heading: u16,
    timestamp: u8,
    special_manoeuvre_indicator: u8,
    raim: bool,
    communication_state: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JsonClassBPositionReport {
    repeat_indicator: u8,
    #[serde(rename = "UserID")]
    user_id: u32,
    sog: f32,
    position_accuracy: bool,
    longitude: f64,
    latitude: f64,
    cog: f32,
    true_heading: u16,
    timestamp: u8,
    class_b_unit: bool,
    class_b_display: bool,
    class_b_dsc: bool,
    class_b_band: bool,
    class_b_msg22: bool,
    assigned_mode: bool,
    raim: bool,
    communication_state: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JsonExtendedClassBPositionReport {
    repeat_indicator: u8,
    #[serde(rename = "UserID")]
    user_id: u32,
    sog: f32,
    position_accuracy: bool,
    longitude: f64,
    latitude: f64,
    cog: f32,
    true_heading: u16,
    timestamp: u8,
    name: String,
    #[serde(rename = "Type")]
    ship_type: u8,
    dimension: Dimension,
    fix_type: u8,
    raim: bool,
    dte: bool,
    assigned_mode: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Eta {
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JsonShipStaticData {
    repeat_indicator: u8,
    #[serde(rename = "UserID")]
    user_id: u32,
    ais_version: u8,
    imo_number: u32,
    call_sign: String,
    name: String,
    #[serde(rename = "Type")]
    ship_type: u8,
    dimension: Dimension,
    fix_type: u8,
    eta: Eta,
    maximum_static_draught: f32,
    destination: String,
    dte: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JsonStaticDataReport {
    repeat_indicator: u8,
    #[serde(rename = "UserID")]
    user_id: u32,
    /// false for part A (name), true for part B
    part_number: bool,
    report_a: JsonReportA,
    report_b: JsonReportB,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JsonReportA {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct JsonReportB {
    ship_type: u8,
    #[serde(rename = "VendorIDName")]
    vendor_id_name: String,
    #[serde(rename = "VenderIDModel")]
    vendor_id_model: u8,
    #[serde(rename = "VenderIDSerial")]
    vendor_id_serial: u32,
    call_sign: String,
    dimension: Dimension,
}

/// Decode one aisstream.io websocket message
pub fn decode(payload: &[u8]) -> Result<Decoded, AisStreamError> {
    let envelope: Envelope = serde_json::from_slice(payload)?;
    if let Some(error) = envelope.error {
        return Err(AisStreamError::Server(error));
    }
    let message_type = envelope.message_type.unwrap_or_default();
    let Some(mut body) = envelope.message else {
        return Ok(Decoded::Ignored(message_type));
    };
    if message_type == "SubscriptionConfirmation" {
        let compression = body
            .get("CompressionEnabled")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        return Ok(Decoded::Confirmation { compression });
    }
    let timestamp = envelope
        .metadata
        .and_then(|metadata| metadata.time_utc)
        .and_then(|time| parse_time_utc(&time))
        .unwrap_or_else(now);
    let inner = body
        .get_mut(&message_type)
        .map(serde_json::Value::take)
        .unwrap_or(serde_json::Value::Null);

    let message = match message_type.as_str() {
        "PositionReport" => {
            let report: JsonPositionReport = serde_json::from_value(inner)?;
            let position = PositionReport {
                msg_type: report.message_id,
                repeat: report.repeat_indicator,
                mmsi: report.user_id,
                status: NavigationStatus::from_bits(report.navigational_status),
                turn: from_turn(report.rate_of_turn as i8 as u8),
                speed: speed(report.sog),
                accuracy: report.position_accuracy,
                longitude: longitude(report.longitude),
                latitude: latitude(report.latitude),
                course: course(report.cog),
                heading: heading(report.true_heading),
                second: Timestamp::from_bits(report.timestamp),
                maneuver: ManeuverIndicator::from_bits(report.special_manoeuvre_indicator),
                spare_1: 0,
                raim: report.raim,
                radio: report.communication_state,
            };
            match report.message_id {
                2 => Message::PositionReport2(position),
                3 => Message::PositionReport3(position),
                _ => Message::PositionReport1(PositionReport {
                    msg_type: 1,
                    ..position
                }),
            }
        }
        "StandardClassBPositionReport" => {
            let report: JsonClassBPositionReport = serde_json::from_value(inner)?;
            Message::ClassBPositionReport(ClassBPositionReport {
                msg_type: 18,
                repeat: report.repeat_indicator,
                mmsi: report.user_id,
                reserved_1: 0,
                speed: speed(report.sog),
                accuracy: report.position_accuracy,
                longitude: longitude(report.longitude),
                latitude: latitude(report.latitude),
                course: course(report.cog),
                heading: heading(report.true_heading),
                second: Timestamp::from_bits(report.timestamp),
                reserved_2: 0,
                cs: report.class_b_unit,
                display: report.class_b_display,
                dsc: report.class_b_dsc,
                band: report.class_b_band,
                msg22: report.class_b_msg22,
                assigned: report.assigned_mode,
                raim: report.raim,
                radio: report.communication_state,
            })
        }
        "ExtendedClassBPositionReport" => {
            let report: JsonExtendedClassBPositionReport = serde_json::from_value(inner)?;
            Message::ExtendedClassBPositionReport(ExtendedClassBPositionReport {
                msg_type: 19,
                repeat: report.repeat_indicator,
                mmsi: report.user_id,
                reserved_1: 0,
                speed: speed(report.sog),
                accuracy: report.position_accuracy,
                longitude: longitude(report.longitude),
                latitude: latitude(report.latitude),
                course: course(report.cog),
                heading: heading(report.true_heading),
                second: Timestamp::from_bits(report.timestamp),
                reserved_2: 0,
                shipname: text(&report.name),
                ship_type: ShipType::from_bits(report.ship_type),
                to_bow: report.dimension.a,
                to_stern: report.dimension.b,
                to_port: report.dimension.c,
                to_starboard: report.dimension.d,
                epfd: EpfdType::from_bits(report.fix_type),
                raim: report.raim,
                dte: report.dte,
                assigned: report.assigned_mode,
                spare_1: 0,
            })
        }
        "ShipStaticData" => {
            let data: JsonShipStaticData = serde_json::from_value(inner)?;
            Message::StaticAndVoyageData(StaticAndVoyageData {
                msg_type: 5,
                repeat: data.repeat_indicator,
                mmsi: data.user_id,
                ais_version: data.ais_version,
                imo: (data.imo_number != 0).then_some(data.imo_number),
                callsign: text(&data.call_sign),
                shipname: text(&data.name),
                ship_type: ShipType::from_bits(data.ship_type),
                to_bow: data.dimension.a,
                to_stern: data.dimension.b,
                to_port: data.dimension.c,
                to_starboard: data.dimension.d,
                epfd: EpfdType::from_bits(data.fix_type),
                month: (data.eta.month != 0).then_some(data.eta.month),
                day: (data.eta.day != 0).then_some(data.eta.day),
                hour: (data.eta.hour != 24).then_some(data.eta.hour),
                minute: (data.eta.minute != 60).then_some(data.eta.minute),
                draught: (data.maximum_static_draught > 0.0).then_some(data.maximum_static_draught),
                destination: text(&data.destination),
                dte: data.dte,
                spare_1: 0,
            })
        }
        "StaticDataReport" => {
            let report: JsonStaticDataReport = serde_json::from_value(inner)?;
            let part = if report.part_number {
                StaticDataReport::PartB(StaticDataReportPartB {
                    msg_type: 24,
                    repeat: report.repeat_indicator,
                    mmsi: report.user_id,
                    partno: 1,
                    ship_type: ShipType::from_bits(report.report_b.ship_type),
                    vendorid: text(&report.report_b.vendor_id_name),
                    model: report.report_b.vendor_id_model,
                    serial: report.report_b.vendor_id_serial,
                    callsign: text(&report.report_b.call_sign),
                    to_bow: report.report_b.dimension.a,
                    to_stern: report.report_b.dimension.b,
                    to_port: report.report_b.dimension.c,
                    to_starboard: report.report_b.dimension.d,
                    spare_1: 0,
                })
            } else {
                StaticDataReport::PartA(StaticDataReportPartA {
                    msg_type: 24,
                    repeat: report.repeat_indicator,
                    mmsi: report.user_id,
                    partno: 0,
                    shipname: text(&report.report_a.name),
                    spare_1: None,
                })
            };
            Message::StaticDataReport(part)
        }
        _ => return Ok(Decoded::Ignored(message_type)),
    };
    Ok(Decoded::Message(AisStreamMessage { timestamp, message }))
}

/// Speed over ground in knots; 102.3 means not available
fn speed(sog: f32) -> Option<f32> {
    (sog < 102.25).then_some(sog)
}

/// Course over ground in degrees; 360 means not available
fn course(cog: f32) -> Option<f32> {
    (cog < 360.0).then_some(cog)
}

/// True heading in degrees; 511 means not available
fn heading(true_heading: u16) -> Option<u16> {
    (true_heading < 360).then_some(true_heading)
}

/// Longitude in degrees; 181 means not available
fn longitude(value: f64) -> Option<f64> {
    (value.abs() <= 180.0).then_some(value)
}

/// Latitude in degrees; 91 means not available
fn latitude(value: f64) -> Option<f64> {
    (value.abs() <= 90.0).then_some(value)
}

/// AIS text without the `@` padding and trailing spaces
fn text(value: &str) -> String {
    value.trim_end_matches(['@', ' ']).to_string()
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default()
}

/// Parse `2026-11-03 07:51:21.143580457 +0000 UTC` into seconds since the epoch
fn parse_time_utc(value: &str) -> Option<f64> {
    let mut parts = value.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    let offset = parts.next().unwrap_or("+0000");

    let mut date = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let (clock, fraction) = time.split_once('.').unwrap_or((time, ""));
    let mut clock = clock.split(':').map(str::parse::<i64>);
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    let fraction = if fraction.is_empty() {
        0.0
    } else {
        format!("0.{fraction}").parse::<f64>().ok()?
    };
    let offset_seconds = if offset.len() == 5 {
        let sign = if offset.starts_with('-') { -1 } else { 1 };
        let hours: i64 = offset[1..3].parse().ok()?;
        let minutes: i64 = offset[3..5].parse().ok()?;
        sign * (hours * 3600 + minutes * 60)
    } else {
        0
    };
    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + hour * 3600 + minute * 60 + second - offset_seconds;
    Some(seconds as f64 + fraction)
}

/// Days since 1970-01-01 in the proleptic Gregorian calendar (H. Hinnant)
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(payload: &str) -> AisStreamMessage {
        match decode(payload.as_bytes()).unwrap() {
            Decoded::Message(message) => message,
            other => panic!("expected a message, got {other:?}"),
        }
    }

    #[test]
    fn parses_aisstream_time() {
        let time = parse_time_utc("2026-11-03 07:51:21.143580457 +0000 UTC").unwrap();
        assert!((time - 1_793_692_281.143_580_5).abs() < 1e-6);
        assert_eq!(parse_time_utc("1970-01-01 00:00:00 +0000 UTC"), Some(0.0));
        assert_eq!(parse_time_utc("1970-01-01 01:00:00 +0100 CET"), Some(0.0));
        assert_eq!(parse_time_utc("garbage"), None);
    }

    #[test]
    fn serializes_subscription_without_leaking_key() {
        let mut subscription =
            Subscription::new("secret", vec![BoundingBox::new([48.5, -6.0], [52.0, 3.0])]);
        let json = serde_json::to_string(&subscription).unwrap();
        assert_eq!(
            json,
            r#"{"APIKey":"secret","BoundingBoxes":[[[48.5,-6.0],[52.0,3.0]]]}"#
        );
        subscription.message_types = vec!["PositionReport".into()];
        let json = serde_json::to_string(&subscription).unwrap();
        assert!(json.ends_with(r#""FilterMessageTypes":["PositionReport"]}"#));
        assert!(!format!("{subscription:?}").contains("secret"));
    }

    #[test]
    fn decodes_confirmation_and_errors() {
        let confirmation = r#"{"Message": {"CompressionEnabled": true}, "MessageType": "SubscriptionConfirmation"}"#;
        assert!(matches!(
            decode(confirmation.as_bytes()).unwrap(),
            Decoded::Confirmation { compression: true }
        ));
        let error = decode(br#"{"error": "Api Key Is Not Valid"}"#).unwrap_err();
        assert!(
            matches!(error, AisStreamError::Server(message) if message == "Api Key Is Not Valid")
        );
        let ignored = r#"{"MetaData": {"MMSI": 2, "time_utc": "2026-11-03 07:51:21 +0000 UTC"}, "MessageType": "BaseStationReport", "Message": {"BaseStationReport": {}}}"#;
        assert!(
            matches!(decode(ignored.as_bytes()).unwrap(), Decoded::Ignored(kind) if kind == "BaseStationReport")
        );
    }

    #[test]
    fn decodes_position_report() {
        let decoded = message(
            r#"{"MetaData": {"MMSI": 308094000, "MMSI_String": 308094000, "ShipName": "COMMODORE CLIPPER   ", "latitude": 49.47929, "longitude": 0.13547, "time_utc": "2026-11-03 07:51:21.404817638 +0000 UTC"}, "MessageType": "PositionReport", "Message": {"PositionReport": {"MessageID": 3, "RepeatIndicator": 0, "UserID": 308094000, "Valid": true, "NavigationalStatus": 5, "RateOfTurn": -128, "Sog": 12.3, "PositionAccuracy": false, "Longitude": 0.13547, "Latitude": 49.479288333333336, "Cog": 32.1, "TrueHeading": 511, "Timestamp": 21, "SpecialManoeuvreIndicator": 0, "Spare": 0, "Raim": false, "CommunicationState": 84938}}}"#,
        );
        assert!((decoded.timestamp - 1_793_692_281.404_817_6).abs() < 1e-6);
        let Message::PositionReport3(report) = decoded.message else {
            panic!("expected a type 3 position report");
        };
        assert_eq!(report.mmsi, 308094000);
        assert_eq!(report.status, NavigationStatus::Moored);
        assert_eq!(report.turn, None);
        assert_eq!(report.speed, Some(12.3));
        assert_eq!(report.course, Some(32.1));
        assert_eq!(report.heading, None);
        assert_eq!(report.latitude, Some(49.479288333333336));
    }

    #[test]
    fn decodes_class_b_position_report() {
        let decoded = message(
            r#"{"MetaData": {"MMSI": 232052272, "time_utc": "2026-11-03 07:51:21.143580457 +0000 UTC"}, "MessageType": "StandardClassBPositionReport", "Message": {"StandardClassBPositionReport": {"AssignedMode": false, "ClassBBand": true, "ClassBDisplay": false, "ClassBDsc": true, "ClassBMsg22": true, "ClassBUnit": true, "Cog": 360, "CommunicationState": 393222, "CommunicationStateIsItdma": true, "Latitude": 50.70399666666666, "Longitude": -1.50295, "MessageID": 18, "PositionAccuracy": true, "Raim": true, "RepeatIndicator": 0, "Sog": 102.3, "Spare1": 0, "Spare2": 0, "Timestamp": 21, "TrueHeading": 511, "UserID": 232052272, "Valid": true}}}"#,
        );
        let Message::ClassBPositionReport(report) = decoded.message else {
            panic!("expected a class B position report");
        };
        assert_eq!(report.mmsi, 232052272);
        assert_eq!(report.longitude, Some(-1.50295));
        assert_eq!(
            (report.speed, report.course, report.heading),
            (None, None, None)
        );
        assert!(report.cs && report.dsc && report.msg22);
    }

    #[test]
    fn decodes_extended_class_b_position_report() {
        let decoded = message(
            r#"{"MetaData": {"MMSI": 244000000, "time_utc": "2026-11-03 07:51:21 +0000 UTC"}, "MessageType": "ExtendedClassBPositionReport", "Message": {"ExtendedClassBPositionReport": {"AssignedMode": false, "Cog": 90.5, "Dimension": {"A": 10, "B": 5, "C": 2, "D": 2}, "Dte": true, "FixType": 1, "Latitude": 52.1, "Longitude": 4.2, "MessageID": 19, "Name": "SAILOR@@@@", "PositionAccuracy": false, "Raim": false, "RepeatIndicator": 0, "Sog": 5.5, "Spare1": 0, "Spare2": 0, "Spare3": 0, "Timestamp": 30, "TrueHeading": 92, "Type": 36, "UserID": 244000000, "Valid": true}}}"#,
        );
        let Message::ExtendedClassBPositionReport(report) = decoded.message else {
            panic!("expected an extended class B position report");
        };
        assert_eq!(report.shipname, "SAILOR");
        assert_eq!(report.heading, Some(92));
        assert_eq!((report.to_bow, report.to_starboard), (10, 2));
    }

    #[test]
    fn decodes_ship_static_data() {
        let decoded = message(
            r#"{"MetaData": {"MMSI": 311358000, "time_utc": "2026-11-03 07:51:22.106873248 +0000 UTC"}, "MessageType": "ShipStaticData", "Message": {"ShipStaticData": {"AisVersion": 2, "CallSign": "C6WU2", "Destination": "GB SOU", "Dimension": {"A": 169, "B": 21, "C": 15, "D": 14}, "Dte": false, "Eta": {"Day": 1, "Hour": 21, "Minute": 0, "Month": 11}, "FixType": 3, "ImoNumber": 9346823, "MaximumStaticDraught": 9.1, "MessageID": 5, "Name": "POMORZE", "RepeatIndicator": 0, "Spare": false, "Type": 70, "UserID": 311358000, "Valid": true}}}"#,
        );
        let Message::StaticAndVoyageData(data) = decoded.message else {
            panic!("expected static and voyage data");
        };
        assert_eq!(data.mmsi, 311358000);
        assert_eq!(data.imo, Some(9346823));
        assert_eq!(
            (data.shipname.as_str(), data.callsign.as_str()),
            ("POMORZE", "C6WU2")
        );
        assert_eq!(data.destination, "GB SOU");
        assert_eq!(
            (data.month, data.day, data.hour, data.minute),
            (Some(11), Some(1), Some(21), Some(0))
        );
        assert_eq!(data.draught, Some(9.1));
        assert_eq!(data.to_bow, 169);
    }

    #[test]
    fn decodes_static_data_report_parts() {
        let part_b = message(
            r#"{"MetaData": {"MMSI": 205227330, "time_utc": "2026-11-03 07:51:23.390020667 +0000 UTC"}, "MessageType": "StaticDataReport", "Message": {"StaticDataReport": {"MessageID": 24, "PartNumber": true, "RepeatIndicator": 0, "ReportA": {"Name": "", "Valid": false}, "ReportB": {"CallSign": "OP2273", "Dimension": {"A": 11, "B": 2, "C": 1, "D": 3}, "FixType": 0, "ShipType": 37, "Spare": 0, "Valid": true, "VenderIDModel": 1, "VenderIDSerial": 32811, "VendorIDName": "DGY"}, "Reserved": 0, "UserID": 205227330, "Valid": true}}}"#,
        );
        let Message::StaticDataReport(StaticDataReport::PartB(report)) = part_b.message else {
            panic!("expected a part B static data report");
        };
        assert_eq!(
            (report.callsign.as_str(), report.vendorid.as_str()),
            ("OP2273", "DGY")
        );
        assert_eq!(report.to_bow, 11);

        let part_a = message(
            r#"{"MetaData": {"MMSI": 205227330, "time_utc": "2026-11-03 07:51:23 +0000 UTC"}, "MessageType": "StaticDataReport", "Message": {"StaticDataReport": {"MessageID": 24, "PartNumber": false, "RepeatIndicator": 0, "ReportA": {"Name": "LITTLE BOAT", "Valid": true}, "ReportB": {"CallSign": "", "Dimension": {"A": 0, "B": 0, "C": 0, "D": 0}, "FixType": 0, "ShipType": 0, "Spare": 0, "Valid": false, "VenderIDModel": 0, "VenderIDSerial": 0, "VendorIDName": ""}, "Reserved": 0, "UserID": 205227330, "Valid": true}}}"#,
        );
        let Message::StaticDataReport(StaticDataReport::PartA(report)) = part_a.message else {
            panic!("expected a part A static data report");
        };
        assert_eq!(report.shipname, "LITTLE BOAT");
    }
}
