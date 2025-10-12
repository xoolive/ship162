use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

fn read_remaining_text<R: std::io::Read + std::io::Seek>(
    reader: &mut Reader<R>,
) -> Result<String, DekuError> {
    let mut data = Vec::new();

    // Read remaining data using deku's byte reading methods
    while let Ok(byte) = u8::from_reader_with_ctx(reader, ()) {
        data.push(byte);
    }

    // Convert bytes to 6-bit ASCII text
    let mut result = String::new();
    let mut bit_buffer = 0u64;
    let mut bit_count = 0;

    for byte in data {
        bit_buffer = (bit_buffer << 8) | (byte as u64);
        bit_count += 8;

        while bit_count >= 6 {
            let char_bits = (bit_buffer >> (bit_count - 6)) & 0x3F;
            bit_count -= 6;

            let ch = match char_bits as u8 {
                0 => '@',                                 // null/padding
                1..=31 => (char_bits as u8 + 64) as char, // A-Z[\]^_ (add 64: gives 65-95)
                32..=63 => char_bits as u8 as char, // space through ? (use as-is: gives 32-63)
                _ => '@',
            };

            if ch != '@' {
                result.push(ch);
            }
        }
    }

    Ok(result.trim().to_string())
}

/// AIS Addressed Safety-Related Message (Type 12)
///
/// This message is used to send safety-related text messages to a specific station.
/// It contains addressing information and a variable-length text payload.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_12_addressed_safety_related_message
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct AddressedSafetyMessage {
    /// Message type (always 12 for this message)
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
    #[deku(bits = "1", assert_eq = "0")]
    #[serde(skip)]
    pub spare_1: u8,

    /// Safety-related text message (variable length, up to 936 bits)
    #[deku(reader = "read_remaining_text(deku::reader)")]
    pub text: String,
}

impl AddressedSafetyMessage {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "seqno": self.seqno,
            "dest_mmsi": self.dest_mmsi,
            "retransmit": self.retransmit,
            "text": self.text,
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

    fn decode(sentence: &str) -> AddressedSafetyMessage {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = AddressedSafetyMessage::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_12_a() {
        let msg = decode("!AIVDM,1,1,,A,<5?SIj1;GbD07??4,0*38");

        assert_eq!(msg.msg_type, 12);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.mmsi, 351853000);
        assert_eq!(msg.seqno, 0);
        assert_eq!(msg.dest_mmsi, 316123456);
        assert!(!msg.retransmit);
        assert_eq!(msg.text, "GOOD");
    }

    #[test]
    fn test_msg_type_12_b() {
        let msg = decode("!AIVDM,1,1,,A,<42Lati0W:Ov=C7P6B?=Pjoihhjhqq0,2*2B");

        assert_eq!(msg.msg_type, 12);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.mmsi, 271002099);
        assert_eq!(msg.seqno, 0);
        assert_eq!(msg.dest_mmsi, 271002111);
        assert!(msg.retransmit);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,<5?SIj1;GbD07??4,0*38");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: AddressedSafetyMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_asdict_compatibility() {
        let msg = decode("!AIVDM,1,1,,A,<5?SIj1;GbD07??4,0*38");
        let dict = msg.asdict();

        assert_eq!(dict["msg_type"], 12);
        assert_eq!(dict["repeat"], 0);
        assert_eq!(dict["mmsi"], 351853000);
        assert_eq!(dict["dest_mmsi"], 316123456);
        assert_eq!(dict["text"], "GOOD");
    }

    #[test]
    fn test_msg_type_12_fields() {
        let msg = decode("!AIVDM,1,1,,A,<5?SIj1;GbD07??4,0*38");

        assert_eq!(msg.msg_type, 12);
        assert_eq!(msg.repeat, 0);
        assert!(msg.mmsi > 0);
        assert!(msg.dest_mmsi > 0);
        assert!(!msg.text.is_empty());

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }
}
