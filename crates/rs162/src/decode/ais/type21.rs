use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::common::{EpfdType, NavAid, Timestamp};
use super::converters::*;

// Custom reader for optional name extension field
fn read_optional_name_ext<R: std::io::Read + std::io::Seek>(
    reader: &mut Reader<R>,
) -> Result<Option<String>, DekuError> {
    // Try to read 84 bits for name extension
    match u128::from_reader_with_ctx(reader, deku::ctx::BitSize(84)) {
        Ok(raw_name_ext) => Ok(from_sixbit_ascii_optional(raw_name_ext, 14)),
        Err(DekuError::Incomplete(_)) => Ok(None), // No more data available
        Err(e) => Err(e),                          // Other errors
    }
}

/// AIS Aid-to-Navigation Report (Type 21)
///
/// This message is used to report the position and status of aids to navigation
/// such as lighthouses, buoys, and other marine navigation aids.
///
/// Message length varies between 272 and 360 bits depending on the presence
/// and size of the Name Extension field.
///
/// Reference: <https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_21_aid_to_navigation_report>
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct AidToNavigationReport {
    /// Message type (always 21 for this message)
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

    /// Type of aid to navigation
    #[deku(
        bits = "5",
        map = "|x: u8| -> Result<_, DekuError> { Ok(NavAid::from_bits(x)) }"
    )]
    pub aid_type: NavAid,

    /// Name of the aid to navigation (20 6-bit characters)
    #[deku(
        bits = "120",
        map = "|x: u128| -> Result<_, DekuError> { Ok(from_sixbit_ascii_120(x, 20)) }"
    )]
    pub name: String,

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

    /// Time stamp
    #[deku(
        bits = "6",
        map = "|x: u8| -> Result<_, DekuError> { Ok(Timestamp::from_bits(x)) }"
    )]
    pub second: Timestamp,

    /// Off position indicator
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub off_position: bool,

    /// Reserved for regional use
    #[deku(bits = "8")]
    pub reserved_1: u8,

    /// RAIM flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub raim: bool,

    /// Virtual aid flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub virtual_aid: bool,

    /// Assigned flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub assigned: bool,

    /// Spare bit (should be zero)
    #[deku(bits = "1")]
    #[serde(skip)]
    pub spare_1: u8,

    /// Name extension (14 6-bit characters, if any) - optional, only present if message > 272 bits
    #[deku(reader = "read_optional_name_ext(deku::reader)")]
    pub name_ext: Option<String>,
}

impl AidToNavigationReport {
    /// Get the full name (name + name_ext if present)
    pub fn full_name(&self) -> String {
        match &self.name_ext {
            Some(ext) if !ext.is_empty() => format!("{}{}", self.name, ext),
            _ => self.name.clone(),
        }
    }

    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "aid_type": self.aid_type as u8,
            "name": self.name,
            "accuracy": self.accuracy,
            "longitude": self.longitude,
            "latitude": self.latitude,
            "to_bow": self.to_bow,
            "to_stern": self.to_stern,
            "to_port": self.to_port,
            "to_starboard": self.to_starboard,
            "epfd": self.epfd as u8,
            "second": self.second,
            "off_position": self.off_position,
            "reserved_1": self.reserved_1,
            "raim": self.raim,
            "virtual_aid": self.virtual_aid,
            "assigned": self.assigned,
            "name_ext": self.name_ext,
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

    fn decode(sentence1: &str, sentence2: &str) -> AidToNavigationReport {
        let nmea_msg1 = NmeaAisMessage::parse(sentence1).unwrap();
        let nmea_msg2 = NmeaAisMessage::parse(sentence2).unwrap();

        let messages = vec![nmea_msg1, nmea_msg2];
        let binary_data = MessageAssembler::assemble_from_iterable(messages).unwrap();

        let (_, msg) = AidToNavigationReport::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_21() {
        let msg = decode(
            "!AIVDM,2,1,7,B,E4eHJhPR37q0000000000000000KUOSc=rq4h00000a,0*4A",
            "!AIVDM,2,2,7,B,@20,4*54",
        );

        assert_eq!(msg.msg_type, 21);
        assert_eq!(msg.mmsi, 316021442);
        assert_eq!(msg.aid_type, NavAid::ReferencePoint);
        assert_eq!(msg.name, "DFO2");
        assert!(msg.accuracy);
        assert!((msg.latitude.unwrap() - 48.65457).abs() < 0.00001);
        assert!((msg.longitude.unwrap() - (-123.429155)).abs() < 0.000001);
        assert_eq!(msg.to_bow, 0);
        assert_eq!(msg.to_stern, 0);
        assert_eq!(msg.to_port, 0);
        assert_eq!(msg.to_starboard, 0);
        assert!(msg.off_position);
        assert_eq!(msg.reserved_1, 0);
        assert!(msg.raim);
        assert!(!msg.virtual_aid);
        assert!(!msg.assigned);
        assert_eq!(msg.name_ext, None);
        assert_eq!(msg.epfd, EpfdType::Gps);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode(
            "!AIVDM,2,1,7,B,E4eHJhPR37q0000000000000000KUOSc=rq4h00000a,0*4A",
            "!AIVDM,2,2,7,B,@20,4*54",
        );

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: AidToNavigationReport = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_full_name() {
        let msg = decode(
            "!AIVDM,2,1,7,B,E4eHJhPR37q0000000000000000KUOSc=rq4h00000a,0*4A",
            "!AIVDM,2,2,7,B,@20,4*54",
        );

        assert_eq!(msg.full_name(), "DFO2");
    }
}
