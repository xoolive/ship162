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

/// AIS Safety-Related Broadcast Message (Type 14)
///
/// This message is used to broadcast safety-related text messages to all stations.
/// Unlike Type 12, this is a broadcast message without specific addressing.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_14_safety_related_broadcast_message
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct SafetyBroadcastMessage {
    /// Message type (always 14 for this message)
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
    #[deku(bits = "2", assert_eq = "0")]
    #[serde(skip)]
    pub spare_1: u8,

    /// Safety-related text message (variable length, up to 968 bits)
    #[deku(reader = "read_remaining_text(deku::reader)")]
    pub text: String,
}

impl SafetyBroadcastMessage {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
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

    fn decode(sentence: &str) -> SafetyBroadcastMessage {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = SafetyBroadcastMessage::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_14() {
        let msg = decode("!AIVDM,1,1,,A,>5?Per18=HB1U:1@E=B0m<L,2*51");

        assert_eq!(msg.msg_type, 14);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.mmsi, 351809000);
        assert_eq!(msg.text, "RCVD YR TEST MSG");
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,>5?Per18=HB1U:1@E=B0m<L,2*51");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: SafetyBroadcastMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_asdict_compatibility() {
        let msg = decode("!AIVDM,1,1,,A,>5?Per18=HB1U:1@E=B0m<L,2*51");
        let dict = msg.asdict();

        assert_eq!(dict["msg_type"], 14);
        assert_eq!(dict["repeat"], 0);
        assert_eq!(dict["mmsi"], 351809000);
        assert_eq!(dict["text"], "RCVD YR TEST MSG");
    }

    #[test]
    fn test_msg_type_14_fields() {
        let msg = decode("!AIVDM,1,1,,A,>5?Per18=HB1U:1@E=B0m<L,2*51");

        assert_eq!(msg.msg_type, 14);
        assert_eq!(msg.repeat, 0);
        assert!(msg.mmsi > 0);
        assert!(!msg.text.is_empty());

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }
}
