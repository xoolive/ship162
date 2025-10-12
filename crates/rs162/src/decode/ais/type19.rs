use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::common::{EpfdType, ShipType};
use super::converters::*;

/// AIS Extended Class B CS Position Report (Type 19)
///
/// This message is used by Class B shipborne mobile equipment to report position,
/// course, speed, and static information including vessel name and dimensions.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_19_extended_class_b_cs_position_report
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct ExtendedClassBPositionReport {
    /// Message type (always 19 for this message)
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
    pub longitude: f64,

    /// Latitude in degrees (signed)
    #[deku(
        bits = "27",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_latitude(x)) }"
    )]
    pub latitude: f64,

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
    #[deku(bits = "4")]
    pub reserved_2: u8,

    /// Ship and cargo type name (20 6-bit characters)
    #[deku(
        bits = "120",
        map = "|x: u128| -> Result<_, DekuError> { Ok(from_sixbit_ascii_120(x, 20)) }"
    )]
    pub shipname: String,

    /// Ship and cargo classification
    #[deku(
        bits = "8",
        map = "|x: u8| -> Result<_, DekuError> { Ok(ShipType::from_bits(x)) }"
    )]
    pub ship_type: ShipType,

    /// Distance from reference point to bow in meters
    #[deku(bits = "9")]
    pub to_bow: u16,

    /// Distance from reference point to stern in meters
    #[deku(bits = "9")]
    pub to_stern: u16,

    /// Distance from reference point to port in meters
    #[deku(bits = "6")]
    pub to_port: u8,

    /// Distance from reference point to starboard in meters
    #[deku(bits = "6")]
    pub to_starboard: u8,

    /// Electronic Position Fixing Device type
    #[deku(
        bits = "4",
        map = "|x: u8| -> Result<_, DekuError> { Ok(EpfdType::from_bits(x)) }"
    )]
    pub epfd: EpfdType,

    /// RAIM flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub raim: bool,

    /// Data Terminal Equipment ready flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub dte: bool,

    /// Assigned flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub assigned: bool,

    /// Spare bits (should be zero)
    #[deku(bits = "4", assert_eq = "0")]
    #[serde(skip)]
    pub spare_1: u8,
}

impl ExtendedClassBPositionReport {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "reserved_1": self.reserved_1,
            "speed": self.speed,
            "accuracy": self.accuracy,
            "lon": self.longitude,
            "lat": self.latitude,
            "course": self.course,
            "heading": self.heading,
            "second": self.second,
            "reserved_2": self.reserved_2,
            "shipname": self.shipname,
            "ship_type": self.ship_type as u8,
            "to_bow": self.to_bow,
            "to_stern": self.to_stern,
            "to_port": self.to_port,
            "to_starboard": self.to_starboard,
            "epfd": self.epfd as u8,
            "raim": self.raim,
            "dte": self.dte,
            "assigned": self.assigned,
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

    fn decode(sentence: &str) -> ExtendedClassBPositionReport {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = ExtendedClassBPositionReport::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_19() {
        let msg = decode("!AIVDM,1,1,,B,C5N3SRgPEnJGEBT>NhWAwwo862PaLELTBJ:V00000000S0D:R220,0*0B");

        assert_eq!(msg.msg_type, 19);
        assert_eq!(msg.mmsi, 367059850);
        assert!((msg.speed - 8.7).abs() < 0.1);
        assert!(!msg.accuracy);
        assert!((msg.latitude - 29.543695).abs() < 0.000001);
        assert!((msg.longitude - (-88.810394)).abs() < 0.00001);
        assert!((msg.course - 335.9).abs() < 0.1);
        assert_eq!(msg.heading, 511);
        assert_eq!(msg.second, 46);
        assert_eq!(msg.shipname, "CAPT.J.RIMES");
        assert_eq!(msg.ship_type, ShipType::Cargo);
        assert_eq!(msg.to_bow, 5);
        assert_eq!(msg.to_stern, 21);
        assert_eq!(msg.to_port, 4);
        assert_eq!(msg.to_starboard, 4);
        assert_eq!(msg.epfd, EpfdType::Gps);
        assert!(!msg.dte);
        assert!(!msg.assigned);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,B,C5N3SRgPEnJGEBT>NhWAwwo862PaLELTBJ:V00000000S0D:R220,0*0B");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: ExtendedClassBPositionReport = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }
}
