use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::common::EpfdType;
use super::converters::*;

/// AIS Base Station Report (Type 4) / UTC Date Response (Type 11)
///
/// This message is transmitted by base stations to provide position and time information.
/// Type 4 is a periodic base station report, while Type 11 is a response to a UTC/Date inquiry.
/// Both message types use identical formats.
///
/// It includes UTC time, position coordinates, and information about the electronic
/// position fixing device (EPFD) being used.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_4_base_station_report
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct BaseStationTimeReport {
    /// Message type (4 for base station report, 11 for UTC/date response)
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

    /// Year (1-9999, 0 = N/A)
    #[deku(bits = "14")]
    pub year: u16,

    /// Month (1-12, 0 = N/A)
    #[deku(bits = "4")]
    pub month: u8,

    /// Day (1-31, 0 = N/A)
    #[deku(bits = "5")]
    pub day: u8,

    /// Hour (0-23, 24 = N/A)
    #[deku(bits = "5")]
    pub hour: u8,

    /// Minute (0-59, 60 = N/A)
    #[deku(bits = "6")]
    pub minute: u8,

    /// Second (0-59, 60 = N/A)
    #[deku(bits = "6")]
    pub second: u8,

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

    /// Electronic Position Fixing Device type
    #[deku(
        bits = "4",
        map = "|x: u8| -> Result<_, DekuError> { Ok(EpfdType::from_bits(x)) }"
    )]
    pub epfd: EpfdType,

    /// Spare bits (should be zero)
    #[deku(bits = "10", assert_eq = "0")]
    #[serde(skip)]
    pub spare_1: u16,

    /// RAIM flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub raim: bool,

    /// Radio status
    #[deku(bits = "19")]
    pub radio: u32,
}

impl BaseStationTimeReport {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "year": self.year,
            "month": self.month,
            "day": self.day,
            "hour": self.hour,
            "minute": self.minute,
            "second": self.second,
            "accuracy": self.accuracy,
            "lon": self.lon,
            "lat": self.lat,
            "epfd": self.epfd as u8,
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

    fn decode(sentence: &str) -> BaseStationTimeReport {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = BaseStationTimeReport::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_4_a() {
        let msg = decode("!AIVDM,1,1,,A,403OviQuMGCqWrRO9>E6fE700@GO,0*4D");

        assert!((msg.lon - (-76.352362)).abs() < 0.000001);
        assert!((msg.lat - 36.883767).abs() < 0.000001);
        assert!(msg.accuracy);
        assert_eq!(msg.year, 2007);
        assert_eq!(msg.month, 5);
        assert_eq!(msg.day, 14);
        assert_eq!(msg.minute, 57);
        assert_eq!(msg.second, 39);
    }

    #[test]
    fn test_msg_type_4_b() {
        let msg = decode("!AIVDM,1,1,,B,403OtVAv>lba;o?Ia`E`4G?02H6k,0*44");

        assert!((msg.lon - (-122.4648)).abs() < 0.0001);
        assert!((msg.lat - 37.7943).abs() < 0.0001);
        assert_eq!(msg.mmsi, 3669145);
        assert!(msg.accuracy);
        assert_eq!(msg.year, 2019);
        assert_eq!(msg.month, 11);
        assert_eq!(msg.day, 9);
        assert_eq!(msg.hour, 10);
        assert_eq!(msg.minute, 41);
        assert_eq!(msg.second, 11);
        assert_eq!(msg.epfd, EpfdType::InternalGnss);
        assert_eq!(msg.epfd as u8, 15);
    }

    #[test]
    fn test_msg_type_11() {
        let msg = decode("!AIVDM,1,1,,B,;4R33:1uUK2F`q?mOt@@GoQ00000,0*5D");

        assert!((msg.lon - (-94.4077)).abs() < 0.0001);
        assert!((msg.lat - 28.4091).abs() < 0.0001);
        assert!(msg.accuracy);
        assert_eq!(msg.msg_type, 11);
        assert_eq!(msg.year, 2009);
        assert_eq!(msg.month, 5);
        assert_eq!(msg.day, 22);
        assert_eq!(msg.hour, 2);
        assert_eq!(msg.minute, 22);
        assert_eq!(msg.second, 40);
    }

    #[test]
    fn test_msg_type_4_and_11_compatibility() {
        // Test that both Type 4 and Type 11 can be decoded with the same structure
        let type4_msg = decode("!AIVDM,1,1,,A,403OviQuMGCqWrRO9>E6fE700@GO,0*4D");
        let type11_msg = decode("!AIVDM,1,1,,B,;4R33:1uUK2F`q?mOt@@GoQ00000,0*5D");

        assert_eq!(type4_msg.msg_type, 4);
        assert_eq!(type11_msg.msg_type, 11);

        // Both should have valid timestamps
        assert!(type4_msg.year > 0);
        assert!(type11_msg.year > 0);

        // Both should serialize successfully
        assert!(type4_msg.to_json().is_ok());
        assert!(type11_msg.to_json().is_ok());
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,403OviQuMGCqWrRO9>E6fE700@GO,0*4D");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: BaseStationTimeReport = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_msg_type_4_fields() {
        let msg = decode("!AIVDM,1,1,,A,403OviQuMGCqWrRO9>E6fE700@GO,0*4D");

        assert_eq!(msg.msg_type, 4);
        assert_eq!(msg.repeat, 0);
        assert!(msg.mmsi > 0);

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }
}
