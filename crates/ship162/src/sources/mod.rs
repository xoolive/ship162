use std::str::FromStr;

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
#[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "pluto"))]
pub const AIS_RTLSDR_GAIN: f64 = 49.6; // Maximum gain recommended for 162 MHz
#[cfg(feature = "pluto")]
pub const AIS_PLUTO_GAIN: f64 = 73.0; // Maximum gain for PlutoSDR

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddressStruct {
    pub host: String,
    pub port: u16,
    jump: Option<String>,
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
}

/// Helper struct for deserializing PlutoSDR configuration from TOML
#[cfg(feature = "pluto")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlutoPath {
    /// PlutoSDR URI (IP address, USB device, or full URI like "ip:192.168.2.1" or "usb:1")
    pub pluto: String,
}

/// Helper struct for deserializing SoapySDR configuration from TOML
#[cfg(feature = "soapy")]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SoapyPath {
    /// SoapySDR driver arguments (e.g., "driver=rtlsdr")
    pub soapy: String,
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
    /// An ADALM-PLUTO device, e.g. `pluto://192.168.2.1` or with structured config: `pluto = "192.168.2.1"`
    #[cfg(feature = "pluto")]
    Pluto(PlutoPath),
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
    /// Gain setting for SDR devices (RTL-SDR/Soapy default: 49.6, PlutoSDR default: 73.0)
    #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "pluto"))]
    pub gain: Option<f64>,
    /// Enable bias-tee to power external LNA (RTL-SDR and SoapySDR, default: false)
    #[cfg(any(feature = "rtlsdr", feature = "soapy"))]
    pub bias_tee: Option<bool>,
    /// Gain element for SoapySDR (default: "TUNER")
    #[cfg(feature = "soapy")]
    pub gain_element: Option<String>,
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
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "pluto"))]
            gain: Option<f64>,
            #[cfg(any(feature = "rtlsdr", feature = "soapy"))]
            bias_tee: Option<bool>,
            #[cfg(feature = "soapy")]
            gain_element: Option<String>,
        }

        let helper = SourceHelper::deserialize(deserializer)?;

        Ok(Source {
            address: helper.address,
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "pluto"))]
            gain: helper.gain,
            #[cfg(any(feature = "rtlsdr", feature = "soapy"))]
            bias_tee: helper.bias_tee,
            #[cfg(feature = "soapy")]
            gain_element: helper.gain_element,
        })
    }
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

            #[cfg(feature = "pluto")]
            "pluto" => {
                let uri = url.host_str().unwrap_or("192.168.2.1").to_string();
                Address::Pluto(PlutoPath { pluto: uri })
            }
            #[cfg(not(feature = "pluto"))]
            "pluto" => {
                return Err(
                    "Pluto SDR support is not enabled. Compile with --features pluto".to_string(),
                )
            }
            #[cfg(feature = "rtlsdr")]
            "rtlsdr" => {
                // Parse rtlsdr:// URL - default to device 0
                let config = RtlSdrDeviceConfig {
                    device: Some(0),
                    serial: None,
                    manufacturer: None,
                    product: None,
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
                Address::Soapy(SoapyPath { soapy: args })
            }
            #[cfg(not(feature = "soapy"))]
            "soapy" => {
                return Err(
                    "SoapySDR support is not enabled. Compile with --features soapy".to_string(),
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
            #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "pluto"))]
            gain: None,
            #[cfg(any(feature = "rtlsdr", feature = "soapy"))]
            bias_tee: None,
            #[cfg(feature = "soapy")]
            gain_element: None,
        };

        Ok(source)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedMessage {
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_level: Option<f32>,
    #[serde(flatten)]
    pub message: Message,
    #[serde(flatten)]
    pub mmsi_info: Option<MmsiInfo>,
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
            assert_eq!(source.gain, Some(49.6));
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
            assert_eq!(source.gain, Some(49.6));
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
    fn test_toml_pluto() {
        #[cfg(feature = "pluto")]
        {
            let toml = r#"
                pluto = "192.168.2.1"
                gain = 73.0
            "#;
            let source: Source = toml::from_str(toml).expect("Failed to parse PlutoSDR");
            if let Address::Pluto(path) = &source.address {
                assert_eq!(path.pluto, "192.168.2.1");
            }
            assert_eq!(source.gain, Some(73.0));
        }
    }

    #[test]
    fn test_toml_soapy() {
        #[cfg(feature = "soapy")]
        {
            let toml = r#"
                soapy = "driver=rtlsdr"
                gain = 49.6
                bias_tee = false
                gain_element = "TUNER"
            "#;
            let source: Source = toml::from_str(toml).expect("Failed to parse SoapySDR");
            if let Address::Soapy(path) = &source.address {
                assert_eq!(path.soapy, "driver=rtlsdr");
            }
            assert_eq!(source.gain, Some(49.6));
            assert_eq!(source.bias_tee, Some(false));
            assert_eq!(source.gain_element, Some("TUNER".to_string()));
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
        let source = Source::from_str("rtlsdr://").expect("Failed to parse rtlsdr:// URL");
        #[cfg(feature = "rtlsdr")]
        if let Address::Rtlsdr(path) = &source.address {
            assert_eq!(path.config.device, Some(0)); // Default device
        }

        let source = Source::from_str("tcp://153.44.253.27:5631").expect("Failed to parse TCP URL");
        assert!(matches!(source.address, Address::Tcp(_)));
    }
}
