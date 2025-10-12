use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

fn read_remaining_bits<R: std::io::Read + std::io::Seek>(
    reader: &mut Reader<R>,
) -> Result<Vec<u8>, DekuError> {
    let mut data = Vec::new();

    // Read remaining bits using deku's bit reading methods
    while let Ok(byte) = u8::from_reader_with_ctx(reader, ()) {
        data.push(byte)
    }

    Ok(data)
}

/// AIS Binary Addressed Message (Type 6)
///
/// This message is used to send binary data to a specific destination.
/// It contains addressing information and a variable-length data payload.
/// The data field can contain up to 920 bits of application-specific information.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_6_binary_addressed_message
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct BinaryAddressedMessage {
    /// Message type (always 6 for this message)
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

    /// Sequence number (0-3)
    #[deku(bits = "2")]
    pub seqno: u8,

    /// Destination MMSI
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub dest_mmsi: u32,

    /// Retransmit flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub retransmit: bool,

    /// Spare bit (should be zero)
    #[deku(bits = "1")]
    pub spare_1: u8,

    /// Designated Area Code
    #[deku(bits = "10")]
    pub dac: u16,

    /// Function Identifier
    #[deku(bits = "6")]
    pub fid: u8,

    /// Binary data payload (variable length, up to 920 bits)
    #[deku(reader = "read_remaining_bits(deku::reader)")]
    pub data: Vec<u8>,
}

impl BinaryAddressedMessage {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "seqno": self.seqno,
            "dest_mmsi": self.dest_mmsi,
            "retransmit": self.retransmit,
            "dac": self.dac,
            "fid": self.fid,
            "data": self.data,
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

    fn decode(sentence: &str) -> BinaryAddressedMessage {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = BinaryAddressedMessage::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_6() {
        let msg = decode("!AIVDM,1,1,,B,6B?n;be:cbapalgc;i6?Ow4,2*4A");

        assert_eq!(msg.seqno, 3);
        assert_eq!(msg.dest_mmsi, 313240222);
        assert_eq!(msg.mmsi, 150834090);
        assert_eq!(msg.dac, 669);
        assert_eq!(msg.fid, 11);
        assert!(!msg.retransmit);
        assert_eq!(msg.data, vec![0xeb, 0x2f, 0x11, 0x8f, 0x7f, 0xf1]);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,B,6B?n;be:cbapalgc;i6?Ow4,2*4A");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: BinaryAddressedMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_msg_type_6_fields() {
        let msg = decode("!AIVDM,1,1,,B,6B?n;be:cbapalgc;i6?Ow4,2*4A");

        assert_eq!(msg.msg_type, 6);
        assert_eq!(msg.repeat, 1);
        assert!(msg.mmsi > 0);
        assert!(msg.dest_mmsi > 0);
        assert!(!msg.data.is_empty());

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }
}
