use std::{
    fmt,
    num::{NonZeroU16, ParseIntError},
    str::FromStr,
};

#[cfg(feature = "airspy")]
pub use desperado::sdr::parse_airspy_serial;
#[cfg(feature = "soapy")]
use desperado::sdr::SoapyPath;
#[cfg(feature = "airspy")]
use desperado::sdr::{AirspyDeviceConfig, AirspyPath};
#[cfg(feature = "hackrf")]
use desperado::sdr::{HackrfDeviceConfig, HackrfPath};
#[cfg(feature = "rtlsdr")]
use desperado::sdr::{RtlSdrDeviceConfig, RtlSdrPath};
#[cfg(any(
    feature = "rtlsdr",
    feature = "soapy",
    feature = "airspy",
    feature = "hackrf"
))]
use desperado::Gain;
use rs162::{
    decode::ais::type24,
    prelude::{Message, MmsiInfo},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::MutexGuard;
use tracing::error;
use url::Url;

use crate::{
    state::AppState,
    status::{RetryPolicy, SourceId, SourceRuntime},
};

#[cfg(feature = "aisstream")]
pub mod aisstream;
mod config;
pub mod iq;
#[cfg(feature = "mqtt")]
pub mod mqtt;
pub mod tcp;
pub mod websocket;

// AIS-specific constants for SDR devices
#[cfg(any(feature = "rtlsdr", feature = "soapy"))]
pub const AIS_RTLSDR_GAIN: f64 = 49.6; // Maximum gain recommended for 162 MHz
#[derive(Debug, Error)]
pub enum TcpAddressError {
    #[error("invalid TCP address `{address}`: expected host:port")]
    TcpPortMissing { address: String },
    #[error("invalid TCP address `{address}`: host is empty")]
    TcpHostEmpty { address: String },
    #[error("invalid TCP address `{address}`: IPv6 addresses must be enclosed in brackets")]
    TcpIpv6NotBracketed { address: String },
    #[error("invalid TCP address `{address}`: port must be between 1 and 65535")]
    TcpPortInvalid {
        address: String,
        #[source]
        source: ParseIntError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcpAddress {
    pub host: String,
    pub port: NonZeroU16,
}

impl FromStr for TcpAddress {
    type Err = TcpAddressError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (host, port) =
            value
                .rsplit_once(':')
                .ok_or_else(|| TcpAddressError::TcpPortMissing {
                    address: value.to_string(),
                })?;
        if host.is_empty() {
            return Err(TcpAddressError::TcpHostEmpty {
                address: value.to_string(),
            });
        }
        if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) {
            return Err(TcpAddressError::TcpIpv6NotBracketed {
                address: value.to_string(),
            });
        }
        let port =
            port.parse::<NonZeroU16>()
                .map_err(|source| TcpAddressError::TcpPortInvalid {
                    address: value.to_string(),
                    source,
                })?;

        Ok(Self {
            host: host.to_string(),
            port,
        })
    }
}

impl fmt::Display for TcpAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.host, self.port)
    }
}

impl Serialize for TcpAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for TcpAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddressStruct {
    pub host: String,
    pub port: NonZeroU16,
    pub jump: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AddressPath {
    Short(TcpAddress),
    Long(AddressStruct),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WsStruct {
    pub url: String,
    pub jump: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WsPath {
    Short(String),
    Long(WsStruct),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TcpConfig {
    /// Address to a TCP feed, either `host:port` or a structured endpoint
    pub tcp: AddressPath,
    /// Reconnect policy. Defaults to a fixed five-second delay when omitted
    #[serde(default)]
    pub retry: RetryPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WebSocketConfig {
    /// WebSocket source URL, optionally with an SSH jump host
    pub ws: WsPath,
    /// Reconnect policy. Defaults to a fixed five-second delay when omitted
    #[serde(default)]
    pub retry: RetryPolicy,
}

/// One or several `[[latitude, longitude], [latitude, longitude]]` boxes
#[cfg(feature = "aisstream")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BoundingBoxes {
    One(rs162::sources::aisstream::BoundingBox),
    Many(Vec<rs162::sources::aisstream::BoundingBox>),
}

#[cfg(feature = "aisstream")]
impl Default for BoundingBoxes {
    fn default() -> Self {
        Self::One(rs162::sources::aisstream::BoundingBox::world())
    }
}

#[cfg(feature = "aisstream")]
impl BoundingBoxes {
    /// All boxes as a list
    #[must_use]
    pub fn to_vec(&self) -> Vec<rs162::sources::aisstream::BoundingBox> {
        match self {
            Self::One(bbox) => vec![*bbox],
            Self::Many(boxes) => boxes.clone(),
        }
    }
}

/// Environment variable holding the aisstream.io API key
#[cfg(feature = "aisstream")]
pub const AISSTREAM_API_KEY: &str = "AISSTREAM_API_KEY";

#[cfg(feature = "aisstream")]
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AisStreamSettings {
    /// Areas of interest. Defaults to the whole world
    #[serde(default)]
    pub bbox: BoundingBoxes,
    /// The API key itself (never serialised nor printed). Exclusive with `key_file`
    #[serde(default, skip_serializing)]
    pub key: Option<String>,
    /// File containing the API key. Exclusive with `key`
    pub key_file: Option<String>,
    /// Endpoint (`wss://`). Defaults to `wss://stream.aisstream.io/v0/stream`
    pub url: Option<String>,
    /// Optional MMSI filter (at most 200 entries)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mmsi: Vec<String>,
    /// Optional aisstream.io message type filter, for example `PositionReport`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub message_types: Vec<String>,
}

#[cfg(feature = "aisstream")]
impl AisStreamSettings {
    pub fn url(&self) -> &str {
        self.url
            .as_deref()
            .unwrap_or(rs162::sources::aisstream::AISSTREAM_URL)
    }

    /// Reject what cannot work, before starting: both key settings, or a non-`wss://` url
    pub fn validate(&self) -> Result<(), String> {
        if self.key.is_some() && self.key_file.is_some() {
            return Err("aisstream.io: set either `key` or `key_file`, not both".into());
        }
        let url =
            Url::parse(self.url()).map_err(|error| format!("invalid aisstream.io url: {error}"))?;
        if url.scheme() != "wss" {
            return Err(format!(
                "unsupported aisstream.io url scheme `{}`: expected wss",
                url.scheme()
            ));
        }
        Ok(())
    }
}

#[cfg(feature = "aisstream")]
impl fmt::Debug for AisStreamSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AisStreamSettings")
            .field("bbox", &self.bbox)
            .field("key", &self.key.as_ref().map(|_| "<hidden>"))
            .field("key_file", &self.key_file)
            .field("url", &self.url)
            .field("mmsi", &self.mmsi)
            .field("message_types", &self.message_types)
            .finish()
    }
}

#[cfg(feature = "aisstream")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AisStreamConfig {
    /// aisstream.io subscription settings
    pub aisstream: AisStreamSettings,
    /// Reconnect policy. Defaults to a fixed five-second delay when omitted
    #[serde(default)]
    pub retry: RetryPolicy,
}

#[cfg(feature = "mqtt")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MqttConfig {
    /// Broker, topic and client ID
    pub mqtt: MqttSettings,
}

/// An MQTT broker publishing Digitraffic-style vessel messages
///
/// In TOML, either a table, or a string with the client ID alone (the
/// original form), connecting to Digitraffic.
#[cfg(feature = "mqtt")]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MqttSettings {
    /// Broker URI (`wss://`, `ws://`, `mqtt://` or `tcp://`). Defaults to Digitraffic
    pub url: Option<String>,
    /// Topic filter. Defaults to `vessels-v2/#`
    pub topic: Option<String>,
    /// Client ID, unique per connection. Defaults to `ship162-` and a random suffix
    pub client_id: Option<String>,
}

#[cfg(feature = "mqtt")]
impl MqttSettings {
    pub fn url(&self) -> &str {
        self.url
            .as_deref()
            .unwrap_or(rs162::sources::mqtt::MQTT_BROKER)
    }

    pub fn topic(&self) -> &str {
        self.topic
            .as_deref()
            .unwrap_or(rs162::sources::mqtt::MQTT_TOPIC)
    }

    /// Reject URIs the MQTT client cannot connect to, before starting
    pub fn validate(&self) -> Result<(), String> {
        let url = Url::parse(self.url()).map_err(|error| format!("invalid MQTT url: {error}"))?;
        match url.scheme() {
            "wss" | "ws" | "mqtt" | "tcp" => Ok(()),
            scheme => Err(format!(
                "unsupported MQTT url scheme `{scheme}`: expected wss, ws, mqtt or tcp"
            )),
        }
    }
}

#[cfg(feature = "rtlsdr")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RtlSdrConfig {
    /// RTL-SDR device selector and device-specific settings
    pub rtlsdr: RtlSdrPath,
    /// Gain setting. Defaults to the recommended AIS gain when omitted
    pub gain: Option<Gain>,
    /// Sample rate in Hz. Defaults to 288 kHz when omitted
    pub sample_rate: Option<f64>,
    /// Enable the bias tee to power an external LNA. Defaults to false
    pub bias_tee: Option<bool>,
}

#[cfg(feature = "soapy")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoapyConfig {
    /// SoapySDR device arguments, for example `driver=rtlsdr`
    pub soapy: SoapyPath,
    /// Gain setting. Defaults to the recommended AIS gain when omitted
    pub gain: Option<Gain>,
    /// Sample rate in Hz. Defaults to 288 kHz when omitted
    pub sample_rate: Option<f64>,
    /// Enable the bias tee to power an external LNA. Defaults to false
    pub bias_tee: Option<bool>,
}

#[cfg(feature = "airspy")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AirspyConfig {
    /// Airspy device selector and device-specific gain settings
    pub airspy: AirspyPath,
    /// Source-level gain setting. Cannot be combined with per-element gains
    pub gain: Option<Gain>,
    /// Sample rate in Hz. Defaults to 6 MHz when omitted
    pub sample_rate: Option<f64>,
    /// Enable the bias tee to power an external LNA. Defaults to false
    pub bias_tee: Option<bool>,
}

#[cfg(feature = "airspy")]
impl<'de> Deserialize<'de> for AirspyConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            airspy: AirspyPath,
            gain: Option<Gain>,
            sample_rate: Option<f64>,
            bias_tee: Option<bool>,
        }

        let input = Input::deserialize(deserializer)?;
        if input.gain.is_some()
            && (input.airspy.config.lna_gain.is_some()
                || input.airspy.config.mixer_gain.is_some()
                || input.airspy.config.vga_gain.is_some())
        {
            return Err(serde::de::Error::custom(
                "cannot specify both `gain` and per-element airspy gains",
            ));
        }

        Ok(Self {
            airspy: input.airspy,
            gain: input.gain,
            sample_rate: input.sample_rate,
            bias_tee: input.bias_tee,
        })
    }
}

#[cfg(feature = "hackrf")]
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HackRfConfig {
    /// HackRF device selector and device-specific gain settings
    pub hackrf: HackrfPath,
    /// Source-level gain setting. Cannot be combined with per-element gains
    pub gain: Option<Gain>,
    /// Sample rate in Hz. Defaults to 288 kHz when omitted
    pub sample_rate: Option<f64>,
    /// Enable the antenna power output. Defaults to false
    pub bias_tee: Option<bool>,
}

#[cfg(feature = "hackrf")]
impl<'de> Deserialize<'de> for HackRfConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Input {
            hackrf: HackrfPath,
            gain: Option<Gain>,
            sample_rate: Option<f64>,
            bias_tee: Option<bool>,
        }

        let input = Input::deserialize(deserializer)?;
        if input.gain.is_some()
            && (input.hackrf.config.lna_gain.is_some() || input.hackrf.config.vga_gain.is_some())
        {
            return Err(serde::de::Error::custom(
                "cannot specify both `gain` and per-element hackrf gains",
            ));
        }

        Ok(Self {
            hackrf: input.hackrf,
            gain: input.gain,
            sample_rate: input.sample_rate,
            bias_tee: input.bias_tee,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IqFileConfig {
    /// Path to a CU8 IQ recording replayed at 288 kHz
    pub iqfile: String,
}

#[derive(Debug, Error)]
pub enum SourceParseError {
    #[error("invalid source `{value}`: {source}")]
    InvalidUrl {
        value: String,
        #[source]
        source: url::ParseError,
    },
    #[error(transparent)]
    TcpAddress(#[from] TcpAddressError),
    #[cfg_attr(
        all(
            feature = "mqtt",
            feature = "rtlsdr",
            feature = "soapy",
            feature = "airspy",
            feature = "hackrf"
        ),
        allow(dead_code)
    )]
    #[error("{source_name} support is not enabled; compile with --features {feature}")]
    FeatureDisabled {
        source_name: &'static str,
        feature: &'static str,
    },
    #[cfg(feature = "aisstream")]
    #[error("invalid aisstream.io bounding box `{0}`: expected lat,lon,lat,lon")]
    InvalidBoundingBox(String),
    #[cfg(any(feature = "mqtt", feature = "aisstream"))]
    #[error("{0}")]
    Invalid(String),
    #[error("invalid file URL `{0}`")]
    InvalidFileUrl(Url),
    #[error("unsupported source scheme `{0}`")]
    UnsupportedScheme(String),
}

// TODO: normalize endpoints and SDR defaults during source parsing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged, try_from = "config::RawSource")]
pub enum Source {
    Tcp(TcpConfig),
    WebSocket(WebSocketConfig),
    #[cfg(feature = "aisstream")]
    AisStream(AisStreamConfig),
    #[cfg(feature = "mqtt")]
    Mqtt(MqttConfig),
    #[cfg(feature = "rtlsdr")]
    RtlSdr(RtlSdrConfig),
    #[cfg(feature = "soapy")]
    Soapy(SoapyConfig),
    #[cfg(feature = "airspy")]
    Airspy(AirspyConfig),
    #[cfg(feature = "hackrf")]
    HackRf(HackRfConfig),
    IqFile(IqFileConfig),
}

impl Source {
    /// A startup label, registered before any asynchronous connection attempt.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Tcp(config) => match &config.tcp {
                AddressPath::Short(address) => format!("tcp://{address}"),
                AddressPath::Long(address) => format!("tcp://{}:{}", address.host, address.port),
            },
            Self::WebSocket(config) => match &config.ws {
                WsPath::Short(url) => url.clone(),
                WsPath::Long(config) => config.url.clone(),
            },
            Self::IqFile(config) => format!("file://{}", config.iqfile),
            #[cfg(feature = "aisstream")]
            Self::AisStream(_) => "aisstream.io".into(),
            #[cfg(feature = "mqtt")]
            Self::Mqtt(config) => format!("mqtt: {}", config.mqtt.url()),
            #[cfg(feature = "rtlsdr")]
            Self::RtlSdr(_) => "rtlsdr".into(),
            #[cfg(feature = "airspy")]
            Self::Airspy(_) => "airspy".into(),
            #[cfg(feature = "hackrf")]
            Self::HackRf(_) => "hackrf".into(),
            #[cfg(feature = "soapy")]
            Self::Soapy(_) => "soapy".into(),
        }
    }
}

impl FromStr for Source {
    type Err = SourceParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let default_tcp = Url::parse("tcp://").expect("static tcp base url is valid");
        let url = default_tcp
            .join(value)
            .map_err(|source| SourceParseError::InvalidUrl {
                value: value.to_string(),
                source,
            })?;

        // for simplicity we don't parse the retry logic from the source uri, just let TOML handle it
        match url.scheme() {
            "tcp" => {
                let host = url.host_str().unwrap_or("153.44.253.27");
                let port = url.port_or_known_default().unwrap_or(5631);
                let address = if host.contains(':') {
                    format!("[{host}]:{port}")
                } else {
                    format!("{host}:{port}")
                };
                Ok(Self::Tcp(TcpConfig {
                    tcp: AddressPath::Short(address.parse()?),
                    retry: RetryPolicy::default(),
                }))
            }
            #[cfg(feature = "aisstream")]
            "aisstream" => {
                let mut settings = AisStreamSettings::default();
                let mut boxes = vec![];
                for (key, value) in url.query_pairs() {
                    match key.as_ref() {
                        "bbox" => boxes.push(parse_bounding_box(&value)?),
                        "key_file" => settings.key_file = Some(value.into_owned()),
                        "url" => settings.url = Some(value.into_owned()),
                        "mmsi" => settings.mmsi.extend(value.split(',').map(str::to_string)),
                        "message_types" => settings
                            .message_types
                            .extend(value.split(',').map(str::to_string)),
                        _ => {}
                    }
                }
                if !boxes.is_empty() {
                    settings.bbox = BoundingBoxes::Many(boxes);
                }
                settings.validate().map_err(SourceParseError::Invalid)?;
                Ok(Self::AisStream(AisStreamConfig {
                    aisstream: settings,
                    retry: RetryPolicy::default(),
                }))
            }
            #[cfg(not(feature = "aisstream"))]
            "aisstream" => Err(SourceParseError::FeatureDisabled {
                source_name: "aisstream.io",
                feature: "aisstream",
            }),
            #[cfg(feature = "mqtt")]
            "mqtt" => {
                // mqtt://?url=…&topic=…&client_id=… (Digitraffic by default)
                let mut settings = MqttSettings::default();
                for (key, value) in url.query_pairs() {
                    match key.as_ref() {
                        "url" => settings.url = Some(value.into_owned()),
                        "topic" => settings.topic = Some(value.into_owned()),
                        "client_id" => settings.client_id = Some(value.into_owned()),
                        _ => {}
                    }
                }
                // The original form, mqtt://<client id>
                if settings.client_id.is_none() {
                    settings.client_id = url.host_str().map(str::to_string);
                }
                settings.validate().map_err(SourceParseError::Invalid)?;
                Ok(Self::Mqtt(MqttConfig { mqtt: settings }))
            }
            #[cfg(not(feature = "mqtt"))]
            "mqtt" => Err(SourceParseError::FeatureDisabled {
                source_name: "MQTT",
                feature: "mqtt",
            }),
            #[cfg(feature = "rtlsdr")]
            "rtlsdr" => Ok(Self::RtlSdr(RtlSdrConfig {
                rtlsdr: RtlSdrPath {
                    config: RtlSdrDeviceConfig {
                        device: Some(0),
                        serial: None,
                        manufacturer: None,
                        product: None,
                    },
                },
                gain: None,
                sample_rate: None,
                bias_tee: None,
            })),
            #[cfg(not(feature = "rtlsdr"))]
            "rtlsdr" => Err(SourceParseError::FeatureDisabled {
                source_name: "RTL-SDR",
                feature: "rtlsdr",
            }),
            #[cfg(feature = "soapy")]
            "soapy" => Ok(Self::Soapy(SoapyConfig {
                soapy: SoapyPath {
                    soapy: url.host_str().unwrap_or("driver=rtlsdr").to_string(),
                },
                gain: None,
                sample_rate: None,
                bias_tee: None,
            })),
            #[cfg(feature = "airspy")]
            "airspy" => {
                let device = url.host_str().unwrap_or("");
                let config = if device.is_empty() {
                    AirspyDeviceConfig {
                        device: Some(0),
                        serial: None,
                        lna_gain: None,
                        mixer_gain: None,
                        vga_gain: None,
                    }
                } else if let Ok(index) = device.parse::<usize>() {
                    AirspyDeviceConfig {
                        device: Some(index),
                        serial: None,
                        lna_gain: None,
                        mixer_gain: None,
                        vga_gain: None,
                    }
                } else if let Some(serial) = device.strip_prefix("serial=") {
                    AirspyDeviceConfig {
                        device: None,
                        serial: Some(serial.to_string()),
                        lna_gain: None,
                        mixer_gain: None,
                        vga_gain: None,
                    }
                } else {
                    // TODO: return a parse error instead of warning and defaulting
                    eprintln!(
                        "WARNING: Unrecognized Airspy device format: '{device}'\n\
                         Expected device index (0, 1, 2, ...) or 'serial=...'.\n\
                         Defaulting to device 0."
                    );
                    AirspyDeviceConfig {
                        device: Some(0),
                        serial: None,
                        lna_gain: None,
                        mixer_gain: None,
                        vga_gain: None,
                    }
                };

                Ok(Self::Airspy(AirspyConfig {
                    airspy: AirspyPath { config },
                    gain: None,
                    sample_rate: None,
                    bias_tee: None,
                }))
            }
            #[cfg(feature = "hackrf")]
            "hackrf" => {
                let device = url.host_str().unwrap_or("");
                let device = if device.is_empty() {
                    0
                } else if let Ok(index) = device.parse::<usize>() {
                    index
                } else {
                    // TODO: return a parse error instead of warning and defaulting
                    eprintln!(
                        "WARNING: Unrecognized HackRF device format: '{device}'\n\
                         Expected device index (0, 1, 2, ...).\n\
                         Defaulting to device 0."
                    );
                    0
                };

                Ok(Self::HackRf(HackRfConfig {
                    hackrf: HackrfPath {
                        config: HackrfDeviceConfig {
                            device: Some(device),
                            amp_enable: None,
                            lna_gain: None,
                            vga_gain: None,
                            freq_offset_hz: None,
                        },
                    },
                    gain: None,
                    sample_rate: None,
                    bias_tee: None,
                }))
            }
            #[cfg(not(feature = "soapy"))]
            "soapy" => Err(SourceParseError::FeatureDisabled {
                source_name: "SoapySDR",
                feature: "soapy",
            }),
            #[cfg(not(feature = "airspy"))]
            "airspy" => Err(SourceParseError::FeatureDisabled {
                source_name: "Airspy",
                feature: "airspy",
            }),
            #[cfg(not(feature = "hackrf"))]
            "hackrf" => Err(SourceParseError::FeatureDisabled {
                source_name: "HackRF",
                feature: "hackrf",
            }),
            "ws" | "wss" => Ok(Self::WebSocket(WebSocketConfig {
                ws: WsPath::Short(value.to_string()),
                retry: RetryPolicy::default(),
            })),
            "file" => Ok(Self::IqFile(IqFileConfig {
                iqfile: url
                    .to_file_path()
                    .map_err(|_| SourceParseError::InvalidFileUrl(url.clone()))?
                    .to_string_lossy()
                    .into_owned(),
            })),
            _ => Err(SourceParseError::UnsupportedScheme(
                url.scheme().to_string(),
            )),
        }
    }
}

/// Parse `lat,lon,lat,lon` into a bounding box
#[cfg(feature = "aisstream")]
fn parse_bounding_box(
    value: &str,
) -> Result<rs162::sources::aisstream::BoundingBox, SourceParseError> {
    let numbers = value
        .split(',')
        .map(|number| number.trim().parse::<f64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SourceParseError::InvalidBoundingBox(value.to_string()))?;
    match numbers.as_slice() {
        [lat1, lon1, lat2, lon2] => Ok(rs162::sources::aisstream::BoundingBox::new(
            [*lat1, *lon1],
            [*lat2, *lon2],
        )),
        _ => Err(SourceParseError::InvalidBoundingBox(value.to_string())),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedMessage {
    pub timestamp: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_level: Option<f32>,
    #[serde(flatten)]
    pub message: Message,
    #[serde(flatten)]
    pub mmsi_info: Option<MmsiInfo>,
    #[serde(skip)]
    pub nmea_sentences: Vec<String>,
    /// Raw AIS bits as received (NMEA or IQ sources): exact duplicates share them.
    /// Decoded feeds (MQTT, aisstream.io) have none.
    #[serde(skip)]
    pub payload: Option<Vec<u8>>,
    /// Every source that received this message, with its own timestamp
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub metadata: Vec<Reception>,
}

/// One reception of a message by a source
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reception {
    /// The source label, e.g. `tcp://153.44.253.27:5631`
    pub source: String,
    pub timestamp: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_level: Option<f32>,
    /// Identifies the configured source: two receptions from the same one are never merged
    #[serde(skip)]
    pub source_id: Option<SourceId>,
}

impl TimedMessage {
    /// A message received once, by the source behind `runtime`
    pub fn received(
        runtime: &SourceRuntime,
        timestamp: f64,
        message: Message,
        payload: Option<Vec<u8>>,
    ) -> Self {
        Self {
            timestamp,
            signal_level: None,
            message,
            mmsi_info: None,
            nmea_sentences: vec![],
            payload,
            metadata: vec![runtime.reception(timestamp, None)],
        }
    }
}

pub async fn process_sentence(mut state: MutexGuard<'_, AppState>, sentence: &mut TimedMessage) {
    let message = &sentence.message;
    let mmsi = message.mmsi();
    let vessel = if let Ok(mmsi_info) = MmsiInfo::from_mmsi(mmsi) {
        sentence.mmsi_info = Some(mmsi_info.clone());
        state.update_vessel(mmsi, mmsi_info)
    } else {
        error!("Failed to extract MMSI information: {:?}", message);
        return;
    };
    match message {
        Message::PositionReport1(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed;
            vessel.turn = msg.turn;
            vessel.course = msg.course;
            vessel.heading = msg.heading;
            vessel.status = Some(msg.status);
            vessel.last_update = sentence.timestamp;
        }
        Message::PositionReport2(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed;
            vessel.turn = msg.turn;
            vessel.course = msg.course;
            vessel.heading = msg.heading;
            vessel.status = Some(msg.status);
            vessel.last_update = sentence.timestamp;
        }
        Message::PositionReport3(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed;
            vessel.turn = msg.turn;
            vessel.course = msg.course;
            vessel.heading = msg.heading;
            vessel.status = Some(msg.status);
            vessel.last_update = sentence.timestamp;
        }
        Message::BaseStationTimeReport(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.last_update = sentence.timestamp;
        }
        Message::StaticAndVoyageData(msg) => {
            vessel.destination = Some(msg.destination.to_string());
            vessel.ship_name = Some(msg.shipname.to_string());
            vessel.callsign = Some(msg.callsign.to_string());
            vessel.to_bow = Some(msg.to_bow);
            vessel.to_stern = Some(msg.to_stern);
            vessel.to_port = Some(msg.to_port);
            vessel.to_starboard = Some(msg.to_starboard);
            vessel.ship_type = Some(msg.ship_type);
            vessel.last_update = sentence.timestamp;
        }
        Message::SarAircraftPositionReport(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed.map(|s| s as f32);
            vessel.course = msg.course;
            vessel.last_update = sentence.timestamp;
        }
        Message::DgnssBroadcastMessage(msg) => {
            vessel.latitude = Some(msg.latitude);
            vessel.longitude = Some(msg.longitude);
            vessel.last_update = sentence.timestamp;
        }
        Message::ClassBPositionReport(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed;
            vessel.course = msg.course;
            vessel.heading = msg.heading;
            vessel.last_update = sentence.timestamp;
        }
        Message::ExtendedClassBPositionReport(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed;
            vessel.course = msg.course;
            vessel.heading = msg.heading;
            vessel.ship_name = Some(msg.shipname.clone());
            vessel.ship_type = Some(msg.ship_type);
            vessel.to_bow = Some(msg.to_bow);
            vessel.to_stern = Some(msg.to_stern);
            vessel.to_port = Some(msg.to_port);
            vessel.to_starboard = Some(msg.to_starboard);
            vessel.last_update = sentence.timestamp;
        }
        Message::AidToNavigationReport(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.to_bow = Some(msg.to_bow);
            vessel.to_stern = Some(msg.to_stern);
            vessel.to_port = Some(msg.to_port);
            vessel.to_starboard = Some(msg.to_starboard);
            vessel.last_update = sentence.timestamp;
        }
        Message::StaticDataReport(type24::StaticDataReport::PartA(msg)) => {
            vessel.ship_name = Some(msg.shipname.to_string());
            vessel.last_update = sentence.timestamp;
        }
        Message::StaticDataReport(type24::StaticDataReport::PartB(msg)) => {
            vessel.ship_type = Some(msg.ship_type);
            vessel.callsign = Some(msg.callsign.to_string());
            vessel.to_bow = Some(msg.to_bow);
            vessel.to_stern = Some(msg.to_stern);
            vessel.to_port = Some(msg.to_port);
            vessel.to_starboard = Some(msg.to_starboard);
            vessel.last_update = sentence.timestamp;
        }
        Message::LongRangeAisBroadcastMessage(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed.map(|s| s as f32);
            vessel.course = msg.course.map(|c| c as f32);
            vessel.last_update = sentence.timestamp;
        }
        _ => { /* Ignore other message types for now */ }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    macro_rules! unwrap_source {
        ($source:expr, $variant:ident) => {
            match $source {
                Source::$variant(config) => config,
                source => panic!(
                    "expected {} source, got {source:?}",
                    stringify!($variant).to_lowercase()
                ),
            }
        };
    }

    #[cfg(feature = "soapy")]
    #[test]
    fn structured_soapy_args_and_rate_match_canonical_config() {
        let nested: Source = toml::from_str("soapy = { args = 'driver=airspy', sample_rate = 3000000 }\ngain = { LNA = 13, MIX = 3, VGA = 13 }").unwrap();
        let canonical: Source = toml::from_str("soapy = 'driver=airspy'\nsample_rate = 3000000\ngain = { LNA = 13, MIX = 3, VGA = 13 }").unwrap();
        assert_eq!(nested, canonical);
        let config = unwrap_source!(nested, Soapy);
        assert_eq!(config.soapy.soapy, "driver=airspy");
        assert_eq!(config.sample_rate, Some(3_000_000.0));
    }

    #[cfg(feature = "soapy")]
    #[test]
    fn structured_soapy_args_and_rate_roundtrip_without_gain() {
        // Gain has a separate upstream Serialize/Deserialize mismatch; test the
        // normalized device arguments and sample rate independently of it.
        let source: Source =
            toml::from_str("soapy = { args = 'driver=airspy', sample_rate = 3000000 }").unwrap();
        assert_eq!(
            toml::from_str::<Source>(&toml::to_string(&source).unwrap()).unwrap(),
            source
        );
    }

    #[cfg(feature = "soapy")]
    #[test]
    fn structured_soapy_rejects_unknown_settings_and_conflicting_rates() {
        let error = toml::from_str::<Source>(
            "soapy = { args = 'driver=airspy', sample_rate = 3000000 }\nsample_rate = 1000000",
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("conflicting `sample_rate` and `soapy.sample_rate`"),
            "{error}"
        );
        let error =
            toml::from_str::<Source>("soapy = { args = 'driver=airspy', samplerate = 3000000 }")
                .unwrap_err()
                .to_string();
        assert!(error.contains("invalid `soapy` device settings"), "{error}");
        assert!(error.contains("samplerate"), "{error}");
    }

    #[cfg(not(feature = "soapy"))]
    #[test]
    fn structured_soapy_without_feature_is_explicitly_disabled() {
        let error =
            toml::from_str::<Source>("soapy = { args = 'driver=airspy', sample_rate = 3000000 }")
                .unwrap_err()
                .to_string();
        assert!(error.contains("SoapySDR support is not enabled"), "{error}");
        assert!(error.contains("--features soapy"), "{error}");
    }

    #[cfg(feature = "rtlsdr")]
    #[test]
    fn nested_rtlsdr_sample_rate_matches_source_level_and_roundtrips() {
        let nested: Source =
            toml::from_str("rtlsdr = { device = 0, sample_rate = 2304000 }").unwrap();
        let canonical: Source =
            toml::from_str("rtlsdr = { device = 0 }\nsample_rate = 2304000").unwrap();
        assert_eq!(nested, canonical);
        let encoded = toml::to_string(&nested).unwrap();
        assert_eq!(toml::from_str::<Source>(&encoded).unwrap(), canonical);
        let config = unwrap_source!(nested, RtlSdr);
        assert_eq!(config.sample_rate, Some(2_304_000.0));
        assert_eq!(config.rtlsdr.config.device, Some(0));
    }

    #[cfg(feature = "rtlsdr")]
    #[test]
    fn duplicate_rtlsdr_rates_must_agree() {
        let matching: Source = toml::from_str(
            "rtlsdr = { device = 0, sample_rate = 2304000 }\nsample_rate = 2304000.0",
        )
        .unwrap();
        assert_eq!(
            unwrap_source!(matching, RtlSdr).sample_rate,
            Some(2_304_000.0)
        );
        let error = toml::from_str::<Source>(
            "rtlsdr = { device = 0, sample_rate = 2304000 }\nsample_rate = 288000",
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("conflicting `sample_rate` and `rtlsdr.sample_rate`"),
            "{error}"
        );
    }

    #[cfg(feature = "rtlsdr")]
    #[test]
    fn rtlsdr_errors_keep_backend_and_field_context() {
        for input in [
            "rtlsdr = { device = 0, sample_rte = 2304000 }",
            "rtlsdr = { device = 0, sample_rate = 'fast' }",
        ] {
            let error = toml::from_str::<Source>(input).unwrap_err().to_string();
            assert!(error.contains("rtlsdr"), "{error}");
            assert!(error.contains("sample_"), "{error}");
            assert!(!error.contains("untagged enum"), "{error}");
        }
    }

    #[cfg(not(feature = "mqtt"))]
    #[test]
    fn mqtt_disabled_error_names_the_feature_in_toml() {
        let error = toml::from_str::<Source>("mqtt = 'mqtt://mqtt.digitraffic.fi'")
            .unwrap_err()
            .to_string();
        assert!(error.contains("MQTT support is not enabled"), "{error}");
        assert!(error.contains("--features mqtt"), "{error}");
        assert!(!error.contains("untagged enum"), "{error}");
    }

    #[test]
    fn ambiguous_source_kinds_and_unknown_tcp_fields_are_explicit_errors() {
        let error = toml::from_str::<Source>("tcp = 'localhost:5631'\nws = 'ws://localhost:8000'")
            .unwrap_err()
            .to_string();
        assert!(error.contains("exactly one"), "{error}");
        let error = toml::from_str::<Source>("tcp = 'localhost:5631'\nretrry = 5")
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid `tcp` source"), "{error}");
        assert!(error.contains("retrry"), "{error}");
    }

    #[cfg(feature = "rtlsdr")]
    #[test]
    fn test_toml_rtlsdr_device_index() {
        let source: Source = toml::from_str(
            r#"
                rtlsdr = { device = 0 }
                gain = 49.6
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, RtlSdr);
        assert_eq!(config.rtlsdr.config.device, Some(0));
        assert_eq!(config.rtlsdr.config.serial, None);
        assert_eq!(config.gain, Some(Gain::Manual(49.6)));
    }

    #[cfg(feature = "rtlsdr")]
    #[test]
    fn test_toml_rtlsdr_serial() {
        let source: Source = toml::from_str(
            r#"
                rtlsdr = { serial = "00000001" }
                gain = 49.6
                bias_tee = true
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, RtlSdr);
        assert_eq!(config.rtlsdr.config.device, None);
        assert_eq!(config.rtlsdr.config.serial.as_deref(), Some("00000001"));
        assert_eq!(config.gain, Some(Gain::Manual(49.6)));
        assert_eq!(config.bias_tee, Some(true));
    }

    #[cfg(feature = "rtlsdr")]
    #[test]
    fn test_toml_rtlsdr_all_filters() {
        let source: Source = toml::from_str(
            r#"
                rtlsdr = {
                    serial = "00000001",
                    manufacturer = "Realtek",
                    product = "RTL2838UHIDIR"
                }
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, RtlSdr);
        assert_eq!(config.rtlsdr.config.serial.as_deref(), Some("00000001"));
        assert_eq!(
            config.rtlsdr.config.manufacturer.as_deref(),
            Some("Realtek")
        );
        assert_eq!(
            config.rtlsdr.config.product.as_deref(),
            Some("RTL2838UHIDIR")
        );
    }

    #[cfg(feature = "rtlsdr")]
    #[test]
    fn test_toml_rtlsdr_with_sample_rate() {
        let source: Source = toml::from_str(
            r#"
                rtlsdr = { device = 0 }
                sample_rate = 1536000
                gain = 49.6
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, RtlSdr);
        assert_eq!(config.sample_rate, Some(1_536_000.0));
    }

    #[cfg(feature = "soapy")]
    #[test]
    fn test_toml_soapy() {
        let source: Source = toml::from_str(
            r#"
                soapy = "driver=rtlsdr"
                gain = 49.6
                bias_tee = false
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, Soapy);
        assert_eq!(config.soapy.soapy, "driver=rtlsdr");
        assert_eq!(config.gain, Some(Gain::Manual(49.6)));
        assert_eq!(config.bias_tee, Some(false));
    }

    #[cfg(feature = "soapy")]
    #[test]
    fn test_toml_soapy_with_sample_rate() {
        let source: Source = toml::from_str(
            r#"
                soapy = "driver=airspy"
                sample_rate = 3000000
                gain = 49.6
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, Soapy);
        assert_eq!(config.soapy.soapy, "driver=airspy");
        assert_eq!(config.sample_rate, Some(3_000_000.0));
        assert_eq!(config.gain, Some(Gain::Manual(49.6)));
    }

    #[cfg(feature = "airspy")]
    #[test]
    fn test_toml_airspy() {
        let source: Source = toml::from_str(
            r#"
                airspy = { device = 0 }
                gain = "auto"
                bias_tee = true
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, Airspy);
        assert_eq!(config.airspy.config.device, Some(0));
        assert_eq!(config.airspy.config.serial, None);
        assert_eq!(config.gain, Some(Gain::Auto));
        assert_eq!(config.bias_tee, Some(true));
    }

    #[cfg(feature = "airspy")]
    #[test]
    fn test_toml_airspy_with_sample_rate() {
        let source: Source = toml::from_str(
            r#"
                airspy = { device = 0 }
                sample_rate = 6000000
                gain = "auto"
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, Airspy);
        assert_eq!(config.sample_rate, Some(6_000_000.0));
    }

    #[cfg(feature = "hackrf")]
    #[test]
    fn test_toml_hackrf() {
        let source: Source = toml::from_str(
            r#"
                hackrf = { device = 0 }
                gain = "auto"
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, HackRf);
        assert_eq!(config.hackrf.config.device, Some(0));
        assert_eq!(config.hackrf.config.amp_enable, None);
        assert_eq!(config.gain, Some(Gain::Auto));
    }

    #[cfg(feature = "hackrf")]
    #[test]
    fn test_toml_hackrf_with_amp() {
        let source: Source = toml::from_str(
            r#"
                hackrf = { device = 0, amp_enable = true }
                bias_tee = false
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, HackRf);
        assert_eq!(config.hackrf.config.device, Some(0));
        assert_eq!(config.hackrf.config.amp_enable, Some(true));
        assert_eq!(config.bias_tee, Some(false));
    }

    #[test]
    fn test_toml_tcp() {
        let source: Source = toml::from_str(
            r#"
                tcp = "153.44.253.27:5631"
                retry = { strategy = "fixed", delay_seconds = 2.5 }
            "#,
        )
        .unwrap();
        let config = unwrap_source!(&source, Tcp);
        let AddressPath::Short(address) = &config.tcp else {
            panic!("expected short TCP address");
        };
        assert_eq!(address.host, "153.44.253.27");
        assert_eq!(address.port.get(), 5631);
        let RetryPolicy::Fixed { delay } = config.retry;
        assert_eq!(delay.duration(), Duration::from_millis(2500));

        let serialized = toml::to_string(&source).unwrap();
        let roundtrip: Source = toml::from_str(&serialized).unwrap();
        assert_eq!(source, roundtrip);
    }

    #[cfg(feature = "mqtt")]
    #[test]
    fn test_toml_mqtt() {
        // The original form: the client ID alone, connecting to Digitraffic
        let source: Source = toml::from_str(r#"mqtt = "ship162-test""#).unwrap();
        let config = unwrap_source!(source, Mqtt);
        assert_eq!(config.mqtt.client_id.as_deref(), Some("ship162-test"));
        assert_eq!(config.mqtt.url(), rs162::sources::mqtt::MQTT_BROKER);
        assert_eq!(config.mqtt.topic(), "vessels-v2/#");

        let source: Source = toml::from_str(
            r#"mqtt = { url = "mqtt://localhost:1883", topic = "vessels-v2/+/location" }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let config = unwrap_source!(source, Mqtt);
        assert_eq!(config.mqtt.url(), "mqtt://localhost:1883");
        assert_eq!(config.mqtt.topic(), "vessels-v2/+/location");
        assert_eq!(config.mqtt.client_id, None);
        assert_eq!(
            source_label(r#"mqtt = { url = "mqtt://localhost:1883" }"#),
            "mqtt: mqtt://localhost:1883"
        );

        let error = toml::from_str::<Source>(r#"mqtt = { url = "https://example.org" }"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsupported MQTT url scheme"), "{error}");

        let source: Source = "mqtt://?url=wss://example.org/mqtt&client_id=me"
            .parse()
            .unwrap();
        let config = unwrap_source!(source, Mqtt);
        assert_eq!(config.mqtt.url(), "wss://example.org/mqtt");
        assert_eq!(config.mqtt.client_id.as_deref(), Some("me"));
    }

    #[cfg(feature = "mqtt")]
    fn source_label(toml: &str) -> String {
        toml::from_str::<Source>(toml).unwrap().label()
    }

    #[test]
    fn test_invalid_tcp_address() {
        assert!(matches!(
            "localhost".parse::<TcpAddress>(),
            Err(TcpAddressError::TcpPortMissing { .. })
        ));
        assert!(matches!(
            "localhost:0".parse::<TcpAddress>(),
            Err(TcpAddressError::TcpPortInvalid { .. })
        ));
        assert!(matches!(
            "localhost:not-a-port".parse::<TcpAddress>(),
            Err(TcpAddressError::TcpPortInvalid { .. })
        ));
        assert!(matches!(
            "::1:5631".parse::<TcpAddress>(),
            Err(TcpAddressError::TcpIpv6NotBracketed { .. })
        ));

        assert!(toml::from_str::<Source>("tcp = { host = \"localhost\", port = 0 }").is_err());
        assert!(matches!(
            Source::from_str("tcp://localhost:0"),
            Err(SourceParseError::TcpAddress(
                TcpAddressError::TcpPortInvalid { .. }
            ))
        ));
        assert!(matches!(
            Source::from_str("ftp://example.com/source"),
            Err(SourceParseError::UnsupportedScheme(scheme)) if scheme == "ftp"
        ));
    }

    #[test]
    fn test_toml_websocket() {
        let source: Source = toml::from_str(
            r#"
                ws = "ws://localhost:9876"
                retry = { strategy = "fixed", delay_seconds = 3 }
            "#,
        )
        .unwrap();
        let config = unwrap_source!(source, WebSocket);
        let RetryPolicy::Fixed { delay } = config.retry;
        assert_eq!(delay.duration(), Duration::from_secs(3));
    }

    #[test]
    fn test_toml_iqfile() {
        let source: Source = toml::from_str(r#"iqfile = "/path/to/recording.iq""#).unwrap();
        let config = unwrap_source!(source, IqFile);
        assert_eq!(config.iqfile, "/path/to/recording.iq");
    }

    #[cfg(feature = "rtlsdr")]
    #[test]
    fn test_toml_unknown_field_rejected() {
        let result: Result<Source, _> = toml::from_str(
            r#"
                rtlsdr = { device = 0 }
                invalid_field = "foo"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_url_backward_compatibility() {
        #[cfg(feature = "rtlsdr")]
        {
            let source = Source::from_str("rtlsdr://").unwrap();
            let config = unwrap_source!(source, RtlSdr);
            assert_eq!(config.rtlsdr.config.device, Some(0));
        }

        let source = Source::from_str("tcp://153.44.253.27:5631").unwrap();
        let config = unwrap_source!(source, Tcp);
        let RetryPolicy::Fixed { delay } = config.retry;
        assert_eq!(delay.duration(), Duration::from_secs(5));

        #[cfg(feature = "airspy")]
        {
            let source = Source::from_str("airspy://").unwrap();
            let config = unwrap_source!(source, Airspy);
            assert_eq!(config.airspy.config.device, Some(0));
            assert_eq!(config.airspy.config.serial, None);

            let source = Source::from_str("airspy://1").unwrap();
            let config = unwrap_source!(source, Airspy);
            assert_eq!(config.airspy.config.device, Some(1));

            let source = Source::from_str("airspy://serial=0x35AC63DC2D8C7A4F").unwrap();
            let config = unwrap_source!(source, Airspy);
            assert_eq!(config.airspy.config.device, None);
            assert_eq!(
                config.airspy.config.serial.as_deref(),
                Some("0x35AC63DC2D8C7A4F")
            );
        }

        #[cfg(feature = "hackrf")]
        {
            let source = Source::from_str("hackrf://").unwrap();
            let config = unwrap_source!(source, HackRf);
            assert_eq!(config.hackrf.config.device, Some(0));

            let source = Source::from_str("hackrf://1").unwrap();
            let config = unwrap_source!(source, HackRf);
            assert_eq!(config.hackrf.config.device, Some(1));
        }
    }

    #[cfg(feature = "aisstream")]
    #[test]
    fn parses_aisstream_toml() {
        use rs162::sources::aisstream::BoundingBox;

        let source: Source = toml::from_str(
            r#"aisstream = { bbox = [[48.5, -6.0], [52.0, 3.0]], key_file = "~/key" }
            retry = { strategy = "fixed", delay_seconds = 10 }"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let config = unwrap_source!(source, AisStream);
        assert_eq!(
            config.aisstream.bbox.to_vec(),
            vec![BoundingBox::new([48.5, -6.0], [52.0, 3.0])]
        );
        assert_eq!(config.aisstream.key_file.as_deref(), Some("~/key"));

        let source: Source = toml::from_str(
            r#"aisstream = { bbox = [[[48, -6], [52, 3]], [[54, 7], [58, 13]]], message_types = ["PositionReport"] }"#,
        )
        .unwrap();
        let config = unwrap_source!(source, AisStream);
        assert_eq!(config.aisstream.bbox.to_vec().len(), 2);
        assert_eq!(config.aisstream.message_types, vec!["PositionReport"]);

        let source: Source = toml::from_str("aisstream = {}").unwrap();
        let config = unwrap_source!(source, AisStream);
        assert_eq!(config.aisstream.bbox.to_vec(), vec![BoundingBox::world()]);

        let error = toml::from_str::<Source>(r#"aisstream = { api_key = "secret" }"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown field `api_key`"), "{error}");
    }

    #[cfg(feature = "aisstream")]
    #[test]
    fn parses_aisstream_key_table() {
        #[derive(Deserialize)]
        struct Sources {
            sources: Vec<Source>,
        }
        let parsed: Sources = toml::from_str(
            r#"
            [[sources]]
            [sources.aisstream]
            bbox = [[48.5, -6.0], [52.0, 3.0]]
            key = "secret"
            "#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let config = unwrap_source!(parsed.sources.into_iter().next().unwrap(), AisStream);
        assert_eq!(config.aisstream.key.as_deref(), Some("secret"));
        assert!(!format!("{config:?}").contains("secret"));

        let error = toml::from_str::<Sources>(
            r#"
            [[sources]]
            [sources.aisstream]
            key = "secret"
            key_file = "~/key"
            "#,
        )
        .err()
        .expect("key and key_file are exclusive")
        .to_string();
        assert!(error.contains("not both"), "{error}");
        assert!(!error.contains("secret"), "{error}");

        let parsed: Sources = toml::from_str(
            r#"
            [[sources]]
            [sources.aisstream]
            url = "wss://relay.example.org/v0/stream"
            "#,
        )
        .unwrap();
        let config = unwrap_source!(parsed.sources.into_iter().next().unwrap(), AisStream);
        assert_eq!(config.aisstream.url(), "wss://relay.example.org/v0/stream");
        assert_eq!(
            AisStreamSettings::default().url(),
            "wss://stream.aisstream.io/v0/stream"
        );
        let error = toml::from_str::<Source>(r#"aisstream = { url = "ws://insecure" }"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("expected wss"), "{error}");
    }

    #[cfg(feature = "aisstream")]
    #[test]
    fn parses_aisstream_url() {
        use rs162::sources::aisstream::BoundingBox;

        let source: Source = "aisstream://?bbox=48.5,-6,52,3&bbox=54,7,58,13&key_file=/tmp/key"
            .parse()
            .unwrap();
        let config = unwrap_source!(source, AisStream);
        assert_eq!(
            config.aisstream.bbox.to_vec(),
            vec![
                BoundingBox::new([48.5, -6.0], [52.0, 3.0]),
                BoundingBox::new([54.0, 7.0], [58.0, 13.0])
            ]
        );
        assert_eq!(config.aisstream.key_file.as_deref(), Some("/tmp/key"));
        assert_eq!(
            Source::from_str("aisstream://").unwrap().label(),
            "aisstream.io"
        );
        assert!(matches!(
            Source::from_str("aisstream://?bbox=1,2,3"),
            Err(SourceParseError::InvalidBoundingBox(_))
        ));
    }
}
