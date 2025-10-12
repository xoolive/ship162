use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::common::{EpfdType, ShipType};
use super::converters::*;

/// AIS Static and Voyage Related Data (Type 5)
///
/// This message contains static information about a vessel including ship name,
/// call sign, dimensions, destination, and voyage-related data. It's transmitted
/// as a multi-part message due to its large size.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_5_static_and_voyage_related_data
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct StaticAndVoyageData {
    /// Message type (always 5 for this message)
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

    /// AIS version (0-3)
    #[deku(bits = "2")]
    pub ais_version: u8,

    /// IMO number (30 bits)
    #[deku(bits = "30")]
    pub imo: u32,

    /// Call sign (7 characters, 6-bit ASCII)
    #[deku(
        bits = "42",
        map = "|x: u64| -> Result<_, DekuError> { Ok(from_sixbit_ascii(x, 7)) }"
    )]
    pub callsign: String,

    /// Vessel name (20 characters, 6-bit ASCII)
    #[deku(
        bits = "120",
        map = "|x: u128| -> Result<_, DekuError> { Ok(from_sixbit_ascii_120(x, 20)) }"
    )]
    pub shipname: String,

    /// Ship and cargo type
    #[deku(
        bits = "8",
        map = "|x: u8| -> Result<_, DekuError> { Ok(ShipType::from_bits(x)) }"
    )]
    pub ship_type: ShipType,

    /// Distance from reference point to bow (meters)
    #[deku(bits = "9")]
    pub to_bow: u16,

    /// Distance from reference point to stern (meters)
    #[deku(bits = "9")]
    pub to_stern: u16,

    /// Distance from reference point to port side (meters)
    #[deku(bits = "6")]
    pub to_port: u8,

    /// Distance from reference point to starboard side (meters)
    #[deku(bits = "6")]
    pub to_starboard: u8,

    /// Electronic Position Fixing Device type
    #[deku(
        bits = "4",
        map = "|x: u8| -> Result<_, DekuError> { Ok(EpfdType::from_bits(x)) }"
    )]
    pub epfd: EpfdType,

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

    /// Maximum present static draught (1/10 meters)
    #[deku(
        bits = "8",
        map = "|x: u8| -> Result<_, DekuError> { Ok(from_10th_u8(x)) }"
    )]
    pub draught: f32,

    /// Destination (20 characters, 6-bit ASCII)
    #[deku(
        bits = "120",
        map = "|x: u128| -> Result<_, DekuError> { Ok(from_sixbit_ascii_120(x, 20)) }"
    )]
    pub destination: String,

    /// Data Terminal Equipment (DTE) ready flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub dte: bool,

    /// Spare bit (should be zero)
    #[deku(bits = "1", assert_eq = "0")]
    #[serde(skip)]
    pub spare_1: u8,
}

impl StaticAndVoyageData {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "ais_version": self.ais_version,
            "imo": self.imo,
            "callsign": self.callsign,
            "shipname": self.shipname,
            "ship_type": self.ship_type,
            "to_bow": self.to_bow,
            "to_stern": self.to_stern,
            "to_port": self.to_port,
            "to_starboard": self.to_starboard,
            "epfd": self.epfd,
            "month": self.month,
            "day": self.day,
            "hour": self.hour,
            "minute": self.minute,
            "draught": self.draught,
            "destination": self.destination,
            "dte": self.dte,
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
    use crate::decode::nmea::{MessageAssembler, NmeaAisMessage};

    fn decode(sentence1: &str, sentence2: &str) -> StaticAndVoyageData {
        let nmea_msg1 = NmeaAisMessage::parse(sentence1).unwrap();
        let nmea_msg2 = NmeaAisMessage::parse(sentence2).unwrap();

        let messages = vec![nmea_msg1, nmea_msg2];
        let binary_data = MessageAssembler::assemble_from_iterable(messages).unwrap();

        let (_, msg) = StaticAndVoyageData::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_5() {
        let msg = decode(
            "!AIVDM,2,1,1,A,55?MbV02;H;s<HtKR20EHE:0@T4@Dn2222222216L961O5Gf0NSQEp6ClRp8,0*1C",
            "!AIVDM,2,2,1,A,88888888880,2*25",
        );

        assert_eq!(msg.callsign, "3FOF8");
        assert_eq!(msg.shipname, "EVER DIADEM");
        assert_eq!(msg.ship_type, ShipType::Cargo);
        assert_eq!(msg.to_bow, 225);
        assert_eq!(msg.to_stern, 70);
        assert_eq!(msg.to_port, 1);
        assert_eq!(msg.to_starboard, 31);
        assert!((msg.draught - 12.2).abs() < 0.1);
        assert_eq!(msg.destination, "NEW YORK");
        assert!(!msg.dte);
        assert_eq!(msg.epfd, EpfdType::Gps);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode(
            "!AIVDM,2,1,1,A,55?MbV02;H;s<HtKR20EHE:0@T4@Dn2222222216L961O5Gf0NSQEp6ClRp8,0*1C",
            "!AIVDM,2,2,1,A,88888888880,2*25",
        );

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: StaticAndVoyageData = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_msg_type_5_fields() {
        let msg = decode(
            "!AIVDM,2,1,1,A,55?MbV02;H;s<HtKR20EHE:0@T4@Dn2222222216L961O5Gf0NSQEp6ClRp8,0*1C",
            "!AIVDM,2,2,1,A,88888888880,2*25",
        );

        assert_eq!(msg.msg_type, 5);
        assert_eq!(msg.repeat, 0);
        assert!(msg.mmsi > 0);
        assert!(!msg.callsign.is_empty());
        assert!(!msg.shipname.is_empty());

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }
}
