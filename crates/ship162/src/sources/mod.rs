use std::str::FromStr;

#[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
use desperado::Gain;
use rs162::{
    decode::ais::type24,
    prelude::{Message, MmsiInfo},
};
use serde::{Deserialize, Serialize};
use tokio::sync::MutexGuard;
use tracing::error;
use url::Url;

use crate::state::AppState;

pub mod iq;
#[cfg(feature = "mqtt")]
pub mod mqtt;
pub mod tcp;

// AIS-specific constants for SDR devices
#[cfg(any(feature = "rtlsdr", feature = "soapy"))]
pub const AIS_RTLSDR_GAIN: f64 = 49.6; // Maximum gain recommended for 162 MHz

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddressStruct {
    pub host: String,
    pub port: u16,
    pub jump: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AddressPath {
    Short(String),
    Long(AddressStruct),
}

/// Structured RTL-SDR device configuration for TOML
#[cfg(feature = "rtlsdr")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RtlSdrPath {
    #[serde(flatten)]
    pub config: RtlSdrDeviceConfig,
}

/// RTL-SDR device configuration fields
#[cfg(feature = "rtlsdr")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RtlSdrDeviceConfig {
    /// Device index (0, 1, 2, ...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<usize>,
    /// Serial number filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// Manufacturer filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// Product filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<String>,
    /// Optional sample rate in Hz (e.g., 1536000 for 1.536 MS/s)
    /// If not specified, defaults to 288 kHz
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
}

/// Helper struct for deserializing SoapySDR configuration from TOML
#[cfg(feature = "soapy")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoapyPath {
    /// SoapySDR driver arguments (e.g., "driver=rtlsdr" or "driver=airspy")
    pub args: String,
    /// Optional sample rate in Hz (e.g., 3000000 for Airspy Mini at 3 MS/s)
    /// If not specified, defaults to 288 kHz
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
}

/// Structured Airspy device configuration for TOML
#[cfg(feature = "airspy")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AirspyPath {
    #[serde(flatten)]
    pub config: AirspyDeviceConfig,
}

/// Airspy device configuration fields
#[cfg(feature = "airspy")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirspyDeviceConfig {
    /// Device index (0, 1, 2, ...)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<usize>,
    /// Serial number selector (e.g. "0x35AC63DC2D8C7A4F")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// Optional sample rate in Hz (e.g., 3000000 for Airspy Mini)
    /// If not specified, defaults to 6 MS/s
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Address {
    /// Address to a TCP feed (e.g. `tcp://` defaults to the Norwegian AIS server, otherwise `tcp://ais.example.com:1234`)
    Tcp(AddressPath),
    /// Address to the Finnish Digitraffic MQTT broker (e.g. `mqtt://` defaults to `ship162` client ID, otherwise `mqtt://my_client_id`)
    #[cfg(feature = "mqtt")]
    Mqtt(String),
    /// An RTL-SDR device, e.g. `rtlsdr://` or with structured config: `rtlsdr = { device = 0 }`
    #[cfg(feature = "rtlsdr")]
    Rtlsdr(RtlSdrPath),
    /// A SoapySDR device, e.g. `soapy://driver=rtlsdr` or with structured config: `soapy = "driver=rtlsdr"`
    #[cfg(feature = "soapy")]
    Soapy(SoapyPath),
    /// An Airspy device, e.g. `airspy://` or with structured config: `airspy = { device = 0 }`
    #[cfg(feature = "airspy")]
    Airspy(AirspyPath),
    /// An IQ file source
    IqFile(String),
}

/**
 * Describe sources of raw AIS data.
 *
 * Several sensors can be behind a single source of data.
 */
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    /// The address to the raw AIS data feed
    #[serde(flatten)]
    pub address: Address,
    /// Gain setting for SDR devices (RTL-SDR/Soapy default: 49.6, Airspy default: auto)
    #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
    pub gain: Option<Gain>,
    #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
    pub sample_rate: Option<f64>,
    /// Enable bias-tee to power external LNA (RTL-SDR/SoapySDR/Airspy, default: false)
    #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
    pub bias_tee: Option<bool>,
}

// Custom deserializer to ensure proper validation
impl<'de> Deserialize<'de> for Source {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SourceHelper {
            #[serde(flatten)]
            address: Address,
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
            gain: Option<Gain>,
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
            sample_rate: Option<f64>,
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
            bias_tee: Option<bool>,
        }

        let helper = SourceHelper::deserialize(deserializer)?;

        Ok(Source {
            address: helper.address,
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
            gain: helper.gain,
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
            sample_rate: helper.sample_rate,
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
            bias_tee: helper.bias_tee,
        })
    }
}

#[cfg(feature = "airspy")]
pub fn parse_airspy_serial(value: &str) -> Result<u64, String> {
    if let Some(hex) = value.strip_prefix("0x") {
        return u64::from_str_radix(hex, 16)
            .map_err(|_| format!("Invalid Airspy serial (hex): '{value}'"));
    }

    value
        .parse::<u64>()
        .or_else(|_| u64::from_str_radix(value, 16))
        .map_err(|_| format!("Invalid Airspy serial: '{value}'"))
}

impl FromStr for Source {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let default_tcp = Url::parse("tcp://").unwrap();

        let url = default_tcp.join(s).map_err(|e| e.to_string())?;

        let address = match url.scheme() {
            "tcp" => Address::Tcp(AddressPath::Short(format!(
                "{}:{}",
                url.host_str().unwrap_or("153.44.253.27"),
                url.port_or_known_default().unwrap_or(5631),
            ))),
            #[cfg(feature = "mqtt")]
            "mqtt" => Address::Mqtt(url.host_str().unwrap_or("ship162").to_string()),
            #[cfg(not(feature = "mqtt"))]
            "mqtt" => {
                return Err("MQTT support is not enabled. Compile with --features mqtt".to_string())
            }
            #[cfg(feature = "rtlsdr")]
            "rtlsdr" => {
                // Parse rtlsdr:// URL - default to device 0
                let config = RtlSdrDeviceConfig {
                    device: Some(0),
                    serial: None,
                    manufacturer: None,
                    product: None,
                    sample_rate: None,
                };
                Address::Rtlsdr(RtlSdrPath { config })
            }
            #[cfg(not(feature = "rtlsdr"))]
            "rtlsdr" => {
                return Err(
                    "RTL-SDR support is not enabled. Compile with --features rtlsdr".to_string(),
                )
            }
            #[cfg(feature = "soapy")]
            "soapy" => {
                let args = url.host_str().unwrap_or("driver=rtlsdr").to_string();
                Address::Soapy(SoapyPath {
                    args,
                    sample_rate: None,
                })
            }
            #[cfg(feature = "airspy")]
            "airspy" => {
                let device_str = url.host_str().unwrap_or("");
                let config = if device_str.is_empty() {
                    AirspyDeviceConfig {
                        device: Some(0),
                        serial: None,
                        sample_rate: None,
                    }
                } else if let Ok(idx) = device_str.parse::<usize>() {
                    AirspyDeviceConfig {
                        device: Some(idx),
                        serial: None,
                        sample_rate: None,
                    }
                } else if let Some(serial) = device_str.strip_prefix("serial=") {
                    AirspyDeviceConfig {
                        device: None,
                        serial: Some(serial.to_string()),
                        sample_rate: None,
                    }
                } else {
                    eprintln!(
                        "WARNING: Unrecognized Airspy device format: '{}'\n\
                         Expected device index (0, 1, 2, ...) or 'serial=...'.\n\
                         Defaulting to device 0.",
                        device_str
                    );
                    AirspyDeviceConfig {
                        device: Some(0),
                        serial: None,
                        sample_rate: None,
                    }
                };
                Address::Airspy(AirspyPath { config })
            }
            #[cfg(not(feature = "soapy"))]
            "soapy" => {
                return Err(
                    "SoapySDR support is not enabled. Compile with --features soapy".to_string(),
                )
            }
            #[cfg(not(feature = "airspy"))]
            "airspy" => {
                return Err(
                    "Airspy support is not enabled. Compile with --features airspy".to_string(),
                )
            }

            "file" => {
                let path = url
                    .to_file_path()
                    .map_err(|_| "invalid file path".to_string())?
                    .to_string_lossy()
                    .to_string();
                Address::IqFile(path)
            }

            _ => return Err("unsupported scheme".to_string()),
        };

        let source = Source {
            address,
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
            gain: None,
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
            sample_rate: None,
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "airspy"))]
            bias_tee: None,
        };

        Ok(source)
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
    use super::*;

    #[test]
    fn test_toml_rtlsdr_device_index() {
        #[cfg(feature = "rtlsdr")]
        {
            let toml = r#"
                rtlsdr = { device = 0 }
                gain = 49.6
            "#;
            let source: Source =
                toml::from_str(toml).expect("Failed to parse RTL-SDR with device index");
            assert!(matches!(source.address, Address::Rtlsdr(_)));
            if let Address::Rtlsdr(path) = &source.address {
                assert_eq!(path.config.device, Some(0));
                assert_eq!(path.config.serial, None);
            }
            assert_eq!(source.gain, Some(Gain::Manual(49.6)));
        }
    }

    #[test]
    fn test_toml_rtlsdr_serial() {
        #[cfg(feature = "rtlsdr")]
        {
            let toml = r#"
                rtlsdr = { serial = "00000001" }
                gain = 49.6
                bias_tee = true
            "#;
            let source: Source = toml::from_str(toml).expect("Failed to parse RTL-SDR with serial");
            if let Address::Rtlsdr(path) = &source.address {
                assert_eq!(path.config.device, None);
                assert_eq!(path.config.serial, Some("00000001".to_string()));
            }
            assert_eq!(source.gain, Some(Gain::Manual(49.6)));
            assert_eq!(source.bias_tee, Some(true));
        }
    }

    #[test]
    fn test_toml_rtlsdr_all_filters() {
        #[cfg(feature = "rtlsdr")]
        {
            let toml = r#"
                rtlsdr = { 
                    serial = "00000001", 
                    manufacturer = "Realtek", 
                    product = "RTL2838UHIDIR" 
                }
            "#;
            let source: Source =
                toml::from_str(toml).expect("Failed to parse RTL-SDR with all filters");
            if let Address::Rtlsdr(path) = &source.address {
                assert_eq!(path.config.serial, Some("00000001".to_string()));
                assert_eq!(path.config.manufacturer, Some("Realtek".to_string()));
                assert_eq!(path.config.product, Some("RTL2838UHIDIR".to_string()));
            }
        }
    }

    #[test]
    fn test_toml_soapy() {
        #[cfg(feature = "soapy")]
        {
            let toml = r#"
                soapy = { args = "driver=rtlsdr" }
                gain = 49.6
                bias_tee = false
            "#;
            let source: Source = toml::from_str(toml).expect("Failed to parse SoapySDR");
            if let Address::Soapy(path) = &source.address {
                assert_eq!(path.args, "driver=rtlsdr");
                assert_eq!(path.sample_rate, None);
            }
            assert_eq!(source.gain, Some(Gain::Manual(49.6)));
            assert_eq!(source.bias_tee, Some(false));
        }
    }

    #[test]
    fn test_toml_soapy_airspy_3m() {
        #[cfg(feature = "soapy")]
        {
            let toml = r#"
                soapy = { args = "driver=airspy", sample_rate = 3000000 }
                gain = 49.6
            "#;
            let source: Source =
                toml::from_str(toml).expect("Failed to parse Airspy Mini at 3 MS/s");
            if let Address::Soapy(path) = &source.address {
                assert_eq!(path.args, "driver=airspy");
                assert_eq!(path.sample_rate, Some(3000000));
            }
            assert_eq!(source.gain, Some(Gain::Manual(49.6)));
        }
    }

    #[test]
    fn test_toml_airspy() {
        #[cfg(feature = "airspy")]
        {
            let toml = r#"
                airspy = { device = 0, sample_rate = 6000000 }
                gain = "auto"
                bias_tee = true
            "#;
            let source: Source = toml::from_str(toml).expect("Failed to parse Airspy TOML");
            if let Address::Airspy(path) = &source.address {
                assert_eq!(path.config.device, Some(0));
                assert_eq!(path.config.serial, None);
                assert_eq!(path.config.sample_rate, Some(6000000));
            }
            assert_eq!(source.gain, Some(Gain::Auto));
            assert_eq!(source.bias_tee, Some(true));
        }
    }

    #[test]
    fn test_toml_tcp() {
        let toml = r#"
            tcp = "153.44.253.27:5631"
        "#;
        let source: Source = toml::from_str(toml).expect("Failed to parse TCP source");
        assert!(matches!(source.address, Address::Tcp(_)));
    }

    #[test]
    fn test_toml_mqtt() {
        #[cfg(feature = "mqtt")]
        {
            let toml = r#"
                mqtt = "ship162_client"
            "#;
            let source: Source = toml::from_str(toml).expect("Failed to parse MQTT source");
            if let Address::Mqtt(client_id) = &source.address {
                assert_eq!(client_id, "ship162_client");
            }
        }
    }

    #[test]
    fn test_toml_iqfile() {
        let toml = r#"
            iqfile = "/path/to/recording.iq"
        "#;
        let source: Source = toml::from_str(toml).expect("Failed to parse IQ file source");
        if let Address::IqFile(path) = &source.address {
            assert_eq!(path, "/path/to/recording.iq");
        }
    }

    #[test]
    fn test_toml_unknown_field_rejected() {
        #[cfg(feature = "rtlsdr")]
        {
            let toml = r#"
                rtlsdr = { device = 0, invalid_field = "foo" }
            "#;
            let result: Result<Source, _> = toml::from_str(toml);
            assert!(result.is_err(), "Unknown field should be rejected");
        }
    }

    #[test]
    fn test_url_backward_compatibility() {
        // Test that URL string parsing still works
        #[cfg(feature = "rtlsdr")]
        {
            let source = Source::from_str("rtlsdr://").expect("Failed to parse rtlsdr:// URL");
            if let Address::Rtlsdr(path) = &source.address {
                assert_eq!(path.config.device, Some(0)); // Default device
            }
        }

        let source = Source::from_str("tcp://153.44.253.27:5631").expect("Failed to parse TCP URL");
        assert!(matches!(source.address, Address::Tcp(_)));

        #[cfg(feature = "airspy")]
        {
            let source = Source::from_str("airspy://").expect("Failed to parse airspy:// URL");
            if let Address::Airspy(path) = &source.address {
                assert_eq!(path.config.device, Some(0));
                assert_eq!(path.config.serial, None);
            }

            let source = Source::from_str("airspy://1").expect("Failed to parse airspy://1 URL");
            if let Address::Airspy(path) = &source.address {
                assert_eq!(path.config.device, Some(1));
                assert_eq!(path.config.serial, None);
            }

            let source = Source::from_str("airspy://serial=0x35AC63DC2D8C7A4F")
                .expect("Failed to parse airspy serial URL");
            if let Address::Airspy(path) = &source.address {
                assert_eq!(path.config.device, None);
                assert_eq!(path.config.serial, Some("0x35AC63DC2D8C7A4F".to_string()));
            }
        }
    }
}
