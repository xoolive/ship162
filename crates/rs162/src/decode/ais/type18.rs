use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

/// AIS Standard Class B CS Position Report (Type 18)
///
/// This message is used by Class B shipborne mobile equipment to report position,
/// course, and speed information. It's similar to Type 1-3 messages but for Class B equipment.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_18_standard_class_b_cs_position_report
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct ClassBPositionReport {
    /// Message type (always 18 for this message)
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

    /// Reserved for regional applications
    #[deku(bits = "8")]
    pub reserved_1: u8,

    /// Speed over ground in 0.1 knots
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
    pub lon: f64,

    /// Latitude in degrees (signed)
    #[deku(
        bits = "27",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_latitude(x)) }"
    )]
    pub lat: f64,

    /// Course over ground in 0.1 degrees
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

    /// Reserved for regional applications
    #[deku(bits = "2")]
    pub reserved_2: u8,

    /// CS unit flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub cs: bool,

    /// Display flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub display: bool,

    /// DSC flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub dsc: bool,

    /// Band flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub band: bool,

    /// Message 22 flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub msg22: bool,

    /// Assigned flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub assigned: bool,

    /// RAIM flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub raim: bool,

    /// Radio status
    #[deku(bits = "20")]
    pub radio: u32,
}

impl ClassBPositionReport {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "reserved_1": self.reserved_1,
            "speed": self.speed,
            "accuracy": self.accuracy,
            "lon": self.lon,
            "lat": self.lat,
            "course": self.course,
            "heading": self.heading,
            "second": self.second,
            "reserved_2": self.reserved_2,
            "cs": self.cs,
            "display": self.display,
            "dsc": self.dsc,
            "band": self.band,
            "msg22": self.msg22,
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

    fn decode(sentence: &str) -> ClassBPositionReport {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = ClassBPositionReport::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_18() {
        let msg = decode("!AIVDM,1,1,,A,B5NJ;PP005l4ot5Isbl03wsUkP06,0*76");

        assert_eq!(msg.msg_type, 18);
        assert_eq!(msg.mmsi, 367430530);
        assert_eq!(msg.speed, 0.0);
        assert!(!msg.accuracy);
        assert!((msg.lat - 37.79).abs() < 0.01);
        assert!((msg.lon - (-122.27)).abs() < 0.01);
        assert_eq!(msg.course, 0.0);
        assert_eq!(msg.heading, 511);
        assert_eq!(msg.second, 55);
        assert_eq!(msg.reserved_2, 0);
        assert!(msg.cs);
        assert!(!msg.display);
        assert!(msg.dsc);
        assert!(msg.band);
        assert!(msg.msg22);
        assert!(!msg.assigned);
        assert!(!msg.raim);
    }

    #[test]
    fn test_msg_type_18_speed() {
        let msg = decode("!AIVDO,1,1,,A,B5NJ;PP2aUl4ot5Isbl6GwsUkP06,0*35");

        assert!((msg.speed - 67.8).abs() < 0.1);
        assert!((msg.course - 10.1).abs() < 0.1);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,B5NJ;PP005l4ot5Isbl03wsUkP06,0*76");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: ClassBPositionReport = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }
}
