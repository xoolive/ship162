use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::common::NavigationStatus;
use super::converters::*;

/// AIS Long Range AIS Broadcast Message (Type 27)
///
/// This message is used for long-range AIS broadcast in areas where
/// there are many vessels and spectrum is tight. It provides reduced
/// position data with lower accuracy but extended range.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_27_long_range_ais_broadcast_message
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct LongRangeAisBroadcastMessage {
    /// Message type (always 27 for this message)
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

    /// Position accuracy flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub accuracy: bool,

    /// RAIM flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub raim: bool,

    /// Navigation status
    #[deku(
        bits = "4",
        map = "|x: u8| -> Result<_, DekuError> { Ok(NavigationStatus::from_bits(x)) }"
    )]
    pub status: NavigationStatus,

    /// Longitude in 1/600 minutes (signed)
    #[deku(
        bits = "18",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_longitude_600(x)) }"
    )]
    pub longitude: f64,

    /// Latitude in 1/600 minutes (signed)
    #[deku(
        bits = "17",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_latitude_600(x)) }"
    )]
    pub latitude: f64,

    /// Speed over ground in knots
    #[deku(bits = "6")]
    pub speed: u8,

    /// Course over ground in degrees
    #[deku(bits = "9")]
    pub course: u16,

    /// GNSS position status flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub gnss: bool,

    /// Spare bit (should be zero)
    #[deku(bits = "1", assert_eq = "0")]
    #[serde(skip)]
    pub spare_1: u8,
}

impl LongRangeAisBroadcastMessage {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "accuracy": self.accuracy,
            "raim": self.raim,
            "status": self.status as u8,
            "longitude": self.longitude,
            "latitude": self.latitude,
            "speed": self.speed,
            "course": self.course,
            "gnss": self.gnss,
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

    fn decode(sentence: &str) -> LongRangeAisBroadcastMessage {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = LongRangeAisBroadcastMessage::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_27_signed() {
        let msg = decode("!AIVDO,1,1,,A,K01;FQh?PbtE3P00,0*75");

        assert_eq!(msg.mmsi, 1234567);
        assert!((msg.longitude - (-13.368333)).abs() < 0.000001);
        assert!((msg.latitude - (-50.121667)).abs() < 0.000001);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDO,1,1,,A,K01;FQh?PbtE3P00,0*75");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: LongRangeAisBroadcastMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }
}
