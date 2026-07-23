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

use crate::{state::AppState, status::RetryPolicy};

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

#[cfg(feature = "mqtt")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MqttConfig {
    /// Legacy MQTT source value
    pub mqtt: String,
    // TODO: model broker and client ID separately without breaking the accepted config.
    // paho-mqtt manages reconnects internally so it doesn't have a retry policy
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
    #[error("invalid file URL `{0}`")]
    InvalidFileUrl(Url),
    #[error("unsupported source scheme `{0}`")]
    UnsupportedScheme(String),
}

// TODO: normalize endpoints and SDR defaults during source parsing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Source {
    Tcp(TcpConfig),
    WebSocket(WebSocketConfig),
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
            #[cfg(feature = "mqtt")]
            "mqtt" => Ok(Self::Mqtt(MqttConfig {
                mqtt: url.host_str().unwrap_or("ship162").to_string(),
            })),
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
        let source: Source = toml::from_str(r#"mqtt = "mqtt://mqtt.digitraffic.fi""#).unwrap();
        let config = unwrap_source!(source, Mqtt);
        assert_eq!(config.mqtt, "mqtt://mqtt.digitraffic.fi");
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
}
