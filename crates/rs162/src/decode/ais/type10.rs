use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

/// AIS UTC/Date Inquiry (Type 10)
///
/// This message is used to request UTC and date information from a station.
/// It's a simple query message that contains the MMSI of the requesting station
/// and the MMSI of the target station.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_10_utc_date_inquiry
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct UtcDateInquiry {
    /// Message type (always 10 for this message)
    #[deku(bits = "6")]
    pub msg_type: u8,

    /// Repeat indicator (0-3)
    #[deku(bits = "2")]
    pub repeat: u8,

    /// Maritime Mobile Service Identity (9 digits) - requesting station
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub mmsi: u32,

    /// Spare bits (should be zero)
    #[deku(bits = "2")]
    pub spare_1: u8,

    /// Destination MMSI - target station for the inquiry
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub dest_mmsi: u32,

    /// Spare bits (should be zero)
    #[deku(bits = "2")]
    pub spare_2: u8,
}

impl UtcDateInquiry {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "dest_mmsi": self.dest_mmsi,
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

    fn decode(sentence: &str) -> UtcDateInquiry {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = UtcDateInquiry::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_10_a() {
        let msg = decode("!AIVDM,1,1,,B,:5MlU41GMK6@,0*6C");

        assert_eq!(msg.dest_mmsi, 366832740);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.msg_type, 10);
    }

    #[test]
    fn test_msg_type_10_b() {
        let msg = decode("!AIVDM,1,1,,B,:6TMCD1GOS60,0*5B");

        assert_eq!(msg.dest_mmsi, 366972000);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.msg_type, 10);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,B,:5MlU41GMK6@,0*6C");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: UtcDateInquiry = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_asdict_compatibility() {
        let msg = decode("!AIVDM,1,1,,B,:5MlU41GMK6@,0*6C");
        let dict = msg.asdict();

        assert_eq!(dict["msg_type"], 10);
        assert_eq!(dict["repeat"], 0);
        assert_eq!(dict["dest_mmsi"], 366832740);
        assert!(dict.get("mmsi").is_some());
    }

    #[test]
    fn test_msg_type_10_fields() {
        let msg = decode("!AIVDM,1,1,,B,:5MlU41GMK6@,0*6C");

        assert_eq!(msg.msg_type, 10);
        assert_eq!(msg.repeat, 0);
        assert!(msg.mmsi > 0);
        assert!(msg.dest_mmsi > 0);

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }
}
