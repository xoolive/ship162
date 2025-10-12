use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

// Custom reader for optional MMSI fields that returns None if no data available
fn read_optional_mmsi<R: std::io::Read + std::io::Seek>(
    reader: &mut Reader<R>,
) -> Result<Option<u32>, DekuError> {
    // Try to read 30 bits for MMSI
    match u32::from_reader_with_ctx(reader, deku::ctx::BitSize(30)) {
        Ok(raw_mmsi) => Ok(Some(from_mmsi(raw_mmsi))),
        Err(DekuError::Incomplete(_)) => Ok(None), // No more data available
        Err(e) => Err(e),                          // Other errors
    }
}

// Custom reader for optional sequence number that returns None if no data available
fn read_optional_seq<R: std::io::Read + std::io::Seek>(
    reader: &mut Reader<R>,
) -> Result<Option<u8>, DekuError> {
    // Try to read 2 bits for sequence number
    match u8::from_reader_with_ctx(reader, deku::ctx::BitSize(2)) {
        Ok(seq) => Ok(Some(seq)),
        Err(DekuError::Incomplete(_)) => Ok(None), // No more data available
        Err(e) => Err(e),                          // Other errors
    }
}

/// AIS Binary Acknowledge (Type 7) / Safety-Related Acknowledgement (Type 13)
///
/// This message is used to acknowledge receipt of a binary addressed message (Type 7)
/// or a safety-related message (Type 13). Both message types use identical formats.
/// It can acknowledge up to 4 different messages by including the MMSI and
/// sequence number of each message being acknowledged.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_7_binary_acknowledge
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct BinaryAcknowledge {
    /// Message type (7 for binary acknowledge, 13 for safety-related acknowledge)
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

    /// Spare bits (should be zero)
    #[deku(bits = "2")]
    pub spare_1: u8,

    /// First MMSI being acknowledged
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub mmsi1: u32,

    /// Sequence number for first MMSI
    #[deku(bits = "2")]
    pub mmsiseq1: u8,

    /// Second MMSI being acknowledged (optional)
    #[deku(reader = "read_optional_mmsi(deku::reader)")]
    pub mmsi2: Option<u32>,

    /// Sequence number for second MMSI (optional)
    #[deku(reader = "read_optional_seq(deku::reader)")]
    pub mmsiseq2: Option<u8>,

    /// Third MMSI being acknowledged (optional)
    #[deku(reader = "read_optional_mmsi(deku::reader)")]
    pub mmsi3: Option<u32>,

    /// Sequence number for third MMSI (optional)
    #[deku(reader = "read_optional_seq(deku::reader)")]
    pub mmsiseq3: Option<u8>,

    /// Fourth MMSI being acknowledged (optional)
    #[deku(reader = "read_optional_mmsi(deku::reader)")]
    pub mmsi4: Option<u32>,

    /// Sequence number for fourth MMSI (optional)
    #[deku(reader = "read_optional_seq(deku::reader)")]
    pub mmsiseq4: Option<u8>,
}

impl BinaryAcknowledge {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "mmsi1": self.mmsi1,
            "mmsiseq1": self.mmsiseq1,
            "mmsi2": self.mmsi2,
            "mmsiseq2": self.mmsiseq2,
            "mmsi3": self.mmsi3,
            "mmsiseq3": self.mmsiseq3,
            "mmsi4": self.mmsi4,
            "mmsiseq4": self.mmsiseq4,
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

    fn decode(sentence: &str) -> BinaryAcknowledge {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = BinaryAcknowledge::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_7() {
        let msg = decode("!AIVDM,1,1,,A,702R5`hwCjq8,0*6B");

        assert_eq!(msg.mmsi, 2655651);
        assert_eq!(msg.msg_type, 7);
        assert_eq!(msg.mmsi1, 265538450);
        assert_eq!(msg.mmsiseq1, 0);
        assert_eq!(msg.mmsi2, None);
        assert_eq!(msg.mmsiseq2, None);
        assert_eq!(msg.mmsi3, None);
        assert_eq!(msg.mmsiseq3, None);
        assert_eq!(msg.mmsi4, None);
        assert_eq!(msg.mmsiseq4, None);
    }

    #[test]
    fn test_msg_type_13() {
        let msg = decode("!AIVDM,1,1,,A,=39UOj0jFs9R,0*65");

        assert_eq!(msg.msg_type, 13);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.mmsi, 211378120);
        assert_eq!(msg.mmsi1, 211217560);
        assert_eq!(msg.mmsi2, None);
        assert_eq!(msg.mmsi3, None);
        assert_eq!(msg.mmsi4, None);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,702R5`hwCjq8,0*6B");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: BinaryAcknowledge = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_msg_type_7_fields() {
        let msg = decode("!AIVDM,1,1,,A,702R5`hwCjq8,0*6B");

        assert_eq!(msg.msg_type, 7);
        assert_eq!(msg.repeat, 0);
        assert!(msg.mmsi > 0);

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }

    #[test]
    fn test_msg_type_7_and_13_compatibility() {
        // Test that both Type 7 and Type 13 can be decoded with the same structure
        let type7_msg = decode("!AIVDM,1,1,,A,702R5`hwCjq8,0*6B");
        let type13_msg = decode("!AIVDM,1,1,,A,=39UOj0jFs9R,0*65");

        assert_eq!(type7_msg.msg_type, 7);
        assert_eq!(type13_msg.msg_type, 13);

        // Both should have valid MMSI fields
        assert!(type7_msg.mmsi > 0);
        assert!(type13_msg.mmsi > 0);

        // Both should serialize successfully
        assert!(type7_msg.to_json().is_ok());
        assert!(type13_msg.to_json().is_ok());
    }
}
