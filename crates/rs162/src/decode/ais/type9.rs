use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::common::Timestamp;
use super::converters::*;

/// AIS Standard SAR Aircraft Position Report (Type 9)
///
/// This message is used by search and rescue aircraft to report their position.
/// It contains essential navigation data including position, altitude, course, and speed.
///
/// Reference: <https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_9_standard_sar_aircraft_position_report>
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct SarAircraftPositionReport {
    /// Message type (always 9 for this message)
    #[deku(bits = "6")]
    pub msg_type: u8,

    /// Repeat indicator (0-3)
    #[deku(bits = "2")]
    pub repeat: u8,

    /// Maritime Mobile Service Identity (9 digits)
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub mmsi: u32,

    /// Altitude in meters (0-4094)
    #[deku(
        bits = "12",
        map = "|x: u16| -> Result<_, DekuError> { Ok(from_altitude(x)) }"
    )]
    pub alt: Option<u16>,

    /// Speed over ground in knots (0-1022)
    #[deku(
        bits = "10",
        map = "|x: u16| -> Result<_, DekuError> { Ok(from_speed_sar(x)) }"
    )]
    pub speed: Option<u16>,

    /// Position accuracy flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub accuracy: bool,

    /// Longitude in degrees (signed)
    #[deku(
        bits = "28",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_longitude(x)) }"
    )]
    pub longitude: Option<f64>,

    /// Latitude in degrees (signed)
    #[deku(
        bits = "27",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_latitude(x)) }"
    )]
    pub latitude: Option<f64>,

    /// Course over ground in degrees
    #[deku(
        bits = "12",
        map = "|x: u16| -> Result<_, DekuError> { Ok(from_course(x)) }"
    )]
    pub course: Option<f32>,

    /// Time stamp
    #[deku(
        bits = "6",
        map = "|x: u8| -> Result<_, DekuError> { Ok(Timestamp::from_bits(x)) }"
    )]
    pub second: Timestamp,

    /// Reserved for regional or local use
    #[deku(bits = "8")]
    pub reserved_1: u8,

    /// Data Terminal Equipment (DTE) ready flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub dte: bool,

    /// Spare bits (should be zero)
    #[deku(bits = "3")]
    #[serde(skip)]
    pub spare_1: u8,

    /// Assigned flag (0 = autonomous mode, 1 = assigned mode)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub assigned: bool,

    /// RAIM flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub raim: bool,

    /// Radio status (20 bits for communication state)
    #[deku(bits = "20")]
    pub radio: u32,
}

impl SarAircraftPositionReport {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "alt": self.alt,
            "speed": self.speed,
            "accuracy": self.accuracy,
            "longitude": self.longitude,
            "latitude": self.latitude,
            "course": self.course,
            "second": self.second,
            "reserved_1": self.reserved_1,
            "dte": self.dte,
            "assigned": self.assigned,
            "raim": self.raim,
            "radio": self.radio,
        })
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::nmea::NmeaAisMessage;

    fn decode(sentence: &str) -> SarAircraftPositionReport {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = SarAircraftPositionReport::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_9() {
        let msg = decode("!AIVDM,1,1,,B,91b55wi;hbOS@OdQAC062Ch2089h,0*30");

        assert_eq!(msg.msg_type, 9);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.mmsi, 111232511);
        assert_eq!(msg.alt, Some(303));
        assert_eq!(msg.speed, Some(42));
        assert!(!msg.accuracy);
        assert!((msg.longitude.unwrap() - (-6.27884)).abs() < 0.00001);
        assert!((msg.latitude.unwrap() - 58.144).abs() < 0.001);
        assert!((msg.course.unwrap() - 154.5).abs() < 0.1);
        assert_eq!(msg.second, Timestamp::Second(15));
        assert!(msg.dte);
        assert_eq!(msg.radio, 33392);
        assert!(!msg.raim);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,B,91b55wi;hbOS@OdQAC062Ch2089h,0*30");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: SarAircraftPositionReport = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_msg_type_9_fields() {
        let msg = decode("!AIVDM,1,1,,B,91b55wi;hbOS@OdQAC062Ch2089h,0*30");

        assert_eq!(msg.msg_type, 9);
        assert_eq!(msg.repeat, 0);
        assert!(msg.mmsi > 0);

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }

    #[test]
    fn test_asdict_compatibility() {
        let msg = decode("!AIVDM,1,1,,B,91b55wi;hbOS@OdQAC062Ch2089h,0*30");
        let dict = msg.asdict();

        // Verify that asdict() uses "lon" and "lat" keys for compatibility
        assert!(dict.get("longitude").is_some());
        assert!(dict.get("latitude").is_some());
        assert_eq!(dict["msg_type"], 9);
        assert_eq!(dict["mmsi"], 111232511);
        assert!((dict["longitude"].as_f64().unwrap() - (-6.27884)).abs() < 0.00001);
        assert!((dict["latitude"].as_f64().unwrap() - 58.144).abs() < 0.001);
    }
}
