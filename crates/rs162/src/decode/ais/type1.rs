use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::common::{ManeuverIndicator, NavigationStatus};
use super::converters::*;

/// AIS Vessel position report using SOTDMA (Self-Organizing Time Division Multiple Access)
///
/// This message structure is used for AIS message types 1, 2, and 3, which are all
/// Class A position reports with identical formats. The only difference between them
/// is the message type field, which affects transmission timing and priority.
///
/// - Type 1: Scheduled position report
/// - Type 2: Assigned scheduled position report  
/// - Type 3: Special position report, response to interrogation
///
/// Reference: <https://gpsd.gitlab.io/gpsd/AIVDM.html#_types_1_2_and_3_position_report_class_a>
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct PositionReport {
    /// Message type (1, 2, or 3 for this message structure)
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

    /// Navigation status
    #[deku(
        bits = "4",
        map = "|x: u8| -> Result<_, DekuError> { Ok(NavigationStatus::from_bits(x)) }"
    )]
    pub status: NavigationStatus,

    /// Rate of turn (signed)
    #[deku(
        bits = "8",
        map = "|x: u8| -> Result<_, DekuError> { Ok(from_turn(x)) }"
    )]
    pub turn: f32,

    /// Speed over ground in knots
    #[deku(
        bits = "10",
        map = "|x: u16| -> Result<_, DekuError> { Ok(from_speed(x)) }"
    )]
    pub speed: f32,

    /// Position accuracy flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub accuracy: bool,

    /// Longitude in degrees (signed)
    #[deku(
        bits = "28",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_longitude(x)) }"
    )]
    pub longitude: f64,

    /// Latitude in degrees (signed)  
    #[deku(
        bits = "27",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_latitude(x)) }"
    )]
    pub latitude: f64,

    /// Course over ground in degrees
    #[deku(
        bits = "12",
        map = "|x: u16| -> Result<_, DekuError> { Ok(from_course(x)) }"
    )]
    pub course: f32,

    /// True heading in degrees
    #[deku(bits = "9")]
    pub heading: u16,

    /// Time stamp (0-59 seconds)
    #[deku(bits = "6")]
    pub second: u8,

    /// Maneuver indicator
    #[deku(
        bits = "2",
        map = "|x: u8| -> Result<_, DekuError> { Ok(ManeuverIndicator::from_bits(x)) }"
    )]
    pub maneuver: ManeuverIndicator,

    /// Spare bits (should be zero)
    #[deku(bits = "3")]
    #[serde(skip)]
    pub spare_1: u8,

    /// RAIM flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub raim: bool,

    /// Radio status
    #[deku(bits = "19")]
    pub radio: u32,
}

impl PositionReport {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "status": self.status,
            "turn": self.turn,
            "speed": self.speed,
            "accuracy": self.accuracy,
            "longitude": self.longitude,
            "latitude": self.latitude,
            "course": self.course,
            "heading": self.heading,
            "second": self.second,
            "maneuver": self.maneuver,
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
    use crate::prelude::*;

    fn decode(sentence: &str) -> PositionReport {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = PositionReport::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_1_a() {
        let result = decode("!AIVDM,1,1,,B,15M67FC000G?ufbE`FepT@3n00Sa,0*5C");

        assert_eq!(result.msg_type, 1);
        assert_eq!(result.repeat, 0);
        assert_eq!(result.mmsi, 366053209);
        assert_eq!(result.status, NavigationStatus::RestrictedManoeuverability);
        assert_eq!(result.turn, 0.0);
        assert_eq!(result.speed, 0.0);
        assert!(!result.accuracy);
        assert!((result.longitude - (-122.341618)).abs() < 0.000001);
        assert!((result.latitude - 37.802118).abs() < 0.000001);
        assert!((result.course - 219.3).abs() < 0.1);
        assert_eq!(result.heading, 1);
        assert_eq!(result.second, 59);
        assert_eq!(result.maneuver, ManeuverIndicator::NotAvailable);
        assert!(!result.raim);
        assert_eq!(result.radio, 2281);
    }

    #[test]
    fn test_msg_type_1_b() {
        let msg = decode("!AIVDM,1,1,,A,15NPOOPP00o?b=bE`UNv4?w428D;,0*24");

        assert_eq!(msg.msg_type, 1);
        assert_eq!(msg.mmsi, 367533950);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.status, NavigationStatus::UnderWayUsingEngine);
        assert_eq!(msg.turn, TurnRate::NO_TI_DEFAULT);
        assert_eq!(msg.speed, 0.0);
        assert!(msg.accuracy);
        assert!((msg.latitude - 37.8084).abs() < 0.0001);
        assert!((msg.longitude - (-122.4082)).abs() < 0.0001);
        assert_eq!(msg.course, 360.0);
        assert_eq!(msg.heading, 511);
        assert_eq!(msg.second, 34);
        assert_eq!(msg.maneuver, ManeuverIndicator::NotAvailable);
        assert!(msg.raim);
    }

    #[test]
    fn test_msg_type_1_c() {
        let msg = decode("!AIVDM,1,1,,B,181:Kjh01ewHFRPDK1s3IRcn06sd,0*08");

        assert_eq!(msg.course, 87.0);
        assert_eq!(msg.mmsi, 538090443);
        assert_eq!(msg.speed, 10.9);
        assert_eq!(msg.turn, 0.0);

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }

    #[test]
    fn test_decode_pos_1_2_3() {
        // Message of type 1 (originally type 0 in Python test, but payload indicates type 1)
        let msg = decode("!AIVDM,1,1,,B,0S9edj0P03PecbBN`ja@0?w42cFC,0*7C");

        assert_eq!(msg.repeat, 2);
        assert_eq!(msg.mmsi, 211512520);
        assert_eq!(msg.turn, TurnRate::NO_TI_DEFAULT);
        assert_eq!(msg.speed, 0.3);
        assert!((msg.latitude - 53.5427).abs() < 0.0001);
        assert!((msg.longitude - 9.9794).abs() < 0.0001);
        assert!((msg.course - 0.0).abs() < 0.1);

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,B,15M67FC000G?ufbE`FepT@3n00Sa,0*5C");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let expected_json = r#"{"msg_type":1,"repeat":0,"mmsi":366053209,"status":"RestrictedManoeuverability","turn":0.0,"speed":0.0,"accuracy":false,"longitude":-122.34161833333333,"latitude":37.80211833333333,"course":219.3,"heading":1,"second":59,"maneuver":"N/A","raim":false,"radio":2281}"#;
        assert_eq!(json, expected_json);
    }

    #[test]
    fn test_msg_type_3() {
        let msg = decode("!AIVDM,1,1,,A,35NSH95001G?wopE`beasVk@0E5:,0*6F");

        assert_eq!(msg.msg_type, 3);
        assert_eq!(msg.mmsi, 367581220);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.status, NavigationStatus::Moored);
        assert_eq!(msg.turn, 0.0);
        assert_eq!(msg.speed, 0.1);
        assert!(!msg.accuracy);
        assert!((msg.latitude - 37.8107).abs() < 0.0001);
        assert!((msg.longitude - (-122.3343)).abs() < 0.0001);
        assert!((msg.course - 254.2).abs() < 0.1);
        assert_eq!(msg.heading, 217);
        assert_eq!(msg.second, 40);
        assert_eq!(msg.maneuver, ManeuverIndicator::NotAvailable);
        assert!(!msg.raim);

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }
}
