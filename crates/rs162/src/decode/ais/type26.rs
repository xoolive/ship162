use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

// Custom reader for remaining binary data (excluding radio field at the end)
fn read_remaining_data_before_radio<R: std::io::Read + std::io::Seek>(
    reader: &mut Reader<R>,
) -> Result<Vec<u8>, DekuError> {
    let mut data = Vec::new();

    // Read all remaining data except what we need for the radio field
    // We'll read until we can't read anymore, then remove the last few bytes
    // that should contain the radio field
    while let Ok(byte) = u8::from_reader_with_ctx(reader, ()) {
        data.push(byte);
    }

    // The radio field is 20 bits (2.5 bytes) at the end
    // We need to be conservative and remove enough bytes to ensure
    // the radio field can be read properly
    if data.len() >= 3 {
        // Remove the last 3 bytes which should contain the radio field
        data.truncate(data.len() - 3);
    } else {
        // If we have less than 3 bytes, clear the data field
        data.clear();
    }

    Ok(data)
}

/// AIS Multiple Slot Binary Message (Type 26) - Addressed Structured
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct MultipleSlotBinaryAddressedStructured {
    /// Message type (always 26)
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

    /// Addressed flag (always 1 for this variant)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub addressed: bool,

    /// Structured flag (always 1 for this variant)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub structured: bool,

    /// Destination MMSI
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub dest_mmsi: u32,

    /// Application ID
    #[deku(bits = "16")]
    pub app_id: u16,

    /// Binary data (remaining bits except radio)
    #[deku(reader = "read_remaining_data_before_radio(deku::reader)")]
    pub data: Vec<u8>,

    /// Radio status
    #[deku(bits = "20")]
    pub radio: u32,
}

/// AIS Multiple Slot Binary Message (Type 26) - Broadcast Structured
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct MultipleSlotBinaryBroadcastStructured {
    /// Message type (always 26)
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

    /// Addressed flag (always 0 for this variant)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub addressed: bool,

    /// Structured flag (always 1 for this variant)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub structured: bool,

    /// Application ID
    #[deku(bits = "16")]
    pub app_id: u16,

    /// Binary data (remaining bits except radio)
    #[deku(reader = "read_remaining_data_before_radio(deku::reader)")]
    pub data: Vec<u8>,

    /// Radio status
    #[deku(bits = "20")]
    pub radio: u32,
}

/// AIS Multiple Slot Binary Message (Type 26) - Addressed Unstructured
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct MultipleSlotBinaryAddressedUnstructured {
    /// Message type (always 26)
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

    /// Addressed flag (always 1 for this variant)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub addressed: bool,

    /// Structured flag (always 0 for this variant)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub structured: bool,

    /// Destination MMSI
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub dest_mmsi: u32,

    /// Application ID
    #[deku(bits = "16")]
    pub app_id: u16,

    /// Binary data (remaining bits except radio)
    #[deku(reader = "read_remaining_data_before_radio(deku::reader)")]
    pub data: Vec<u8>,

    /// Radio status
    #[deku(bits = "20")]
    pub radio: u32,
}

/// AIS Multiple Slot Binary Message (Type 26) - Broadcast Unstructured
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct MultipleSlotBinaryBroadcastUnstructured {
    /// Message type (always 26)
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

    /// Addressed flag (always 0 for this variant)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub addressed: bool,

    /// Structured flag (always 0 for this variant)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub structured: bool,

    /// Binary data (remaining bits except radio)
    #[deku(reader = "read_remaining_data_before_radio(deku::reader)")]
    pub data: Vec<u8>,

    /// Radio status
    #[deku(bits = "20")]
    pub radio: u32,
}

/// AIS Multiple Slot Binary Message (Type 26)
///
/// This message is used for multiple-slot binary data transmission.
/// It has four variants based on addressed and structured flags.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_26_multiple_slot_binary_message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MultipleSlotBinaryMessage {
    /// Addressed structured message
    AddressedStructured(MultipleSlotBinaryAddressedStructured),
    /// Broadcast structured message
    BroadcastStructured(MultipleSlotBinaryBroadcastStructured),
    /// Addressed unstructured message
    AddressedUnstructured(MultipleSlotBinaryAddressedUnstructured),
    /// Broadcast unstructured message
    BroadcastUnstructured(MultipleSlotBinaryBroadcastUnstructured),
}

impl MultipleSlotBinaryMessage {
    fn from_bytes(data: (&[u8], usize)) -> Result<((&[u8], usize), Self), DekuError> {
        let (bytes, _bit_offset) = data;

        // Extract addressed and structured flags from bits 38-39
        if bytes.len() < 5 {
            return Err(DekuError::Incomplete(deku::error::NeedSize::new(5 * 8)));
        }

        let flags = bytes[4] & 0x03;
        let addressed = (flags & 0x01) != 0; // bit 38
        let structured = (flags & 0x02) != 0; // bit 39

        match (addressed, structured) {
            (true, true) => {
                let (remaining, msg) = MultipleSlotBinaryAddressedStructured::from_bytes(data)?;
                Ok((
                    remaining,
                    MultipleSlotBinaryMessage::AddressedStructured(msg),
                ))
            }
            (false, true) => {
                let (remaining, msg) = MultipleSlotBinaryBroadcastStructured::from_bytes(data)?;
                Ok((
                    remaining,
                    MultipleSlotBinaryMessage::BroadcastStructured(msg),
                ))
            }
            (true, false) => {
                let (remaining, msg) = MultipleSlotBinaryAddressedUnstructured::from_bytes(data)?;
                Ok((
                    remaining,
                    MultipleSlotBinaryMessage::AddressedUnstructured(msg),
                ))
            }
            (false, false) => {
                let (remaining, msg) = MultipleSlotBinaryBroadcastUnstructured::from_bytes(data)?;
                Ok((
                    remaining,
                    MultipleSlotBinaryMessage::BroadcastUnstructured(msg),
                ))
            }
        }
    }

    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        match self {
            MultipleSlotBinaryMessage::AddressedStructured(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "addressed": msg.addressed,
                    "structured": msg.structured,
                    "dest_mmsi": msg.dest_mmsi,
                    "app_id": msg.app_id,
                    "data": msg.data,
                    "radio": msg.radio,
                })
            }
            MultipleSlotBinaryMessage::BroadcastStructured(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "addressed": msg.addressed,
                    "structured": msg.structured,
                    "app_id": msg.app_id,
                    "data": msg.data,
                    "radio": msg.radio,
                })
            }
            MultipleSlotBinaryMessage::AddressedUnstructured(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "addressed": msg.addressed,
                    "structured": msg.structured,
                    "dest_mmsi": msg.dest_mmsi,
                    "app_id": msg.app_id,
                    "data": msg.data,
                    "radio": msg.radio,
                })
            }
            MultipleSlotBinaryMessage::BroadcastUnstructured(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "addressed": msg.addressed,
                    "structured": msg.structured,
                    "data": msg.data,
                    "radio": msg.radio,
                })
            }
        }
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Get the message type (always 26)
    pub fn msg_type(&self) -> u8 {
        26
    }

    /// Get the MMSI
    pub fn mmsi(&self) -> u32 {
        match self {
            MultipleSlotBinaryMessage::AddressedStructured(msg) => msg.mmsi,
            MultipleSlotBinaryMessage::BroadcastStructured(msg) => msg.mmsi,
            MultipleSlotBinaryMessage::AddressedUnstructured(msg) => msg.mmsi,
            MultipleSlotBinaryMessage::BroadcastUnstructured(msg) => msg.mmsi,
        }
    }
}

impl DekuReader<'_, ()> for MultipleSlotBinaryMessage {
    fn from_reader_with_ctx<R: std::io::Read + std::io::Seek>(
        reader: &mut Reader<R>,
        _ctx: (),
    ) -> Result<Self, DekuError> {
        // Parse the message to determine variant based on addressed/structured flags
        let mut data = Vec::new();

        // Read remaining data using deku's byte reading methods
        while let Ok(byte) = u8::from_reader_with_ctx(reader, ()) {
            data.push(byte);
        }

        // Parse the message to determine variant based on length
        let (_, msg) = Self::from_bytes((&data, 0))?;
        Ok(msg)
    }
}
/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::nmea::NmeaAisMessage;

    fn decode(sentence: &str) -> MultipleSlotBinaryMessage {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = MultipleSlotBinaryMessage::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_26_a() {
        let msg = decode("!AIVDM,1,1,,A,JB3R0GO7p>vQL8tjw0b5hqpd0706kh9d3lR2vbl0400,2*40");
        let dict = msg.asdict();

        assert_eq!(dict["msg_type"], 26);
        assert_eq!(dict["addressed"], true);
        assert_eq!(dict["structured"], true);
        assert_eq!(dict["dest_mmsi"], 838351848);
    }

    #[test]
    fn test_msg_type_26_b() {
        let msg = decode("!AIVDM,1,1,,A,J0@00@370>t0Lh3P0000200H:2rN92,4*14");
        let dict = msg.asdict();

        assert_eq!(dict["msg_type"], 26);
        assert_eq!(dict["addressed"], false);
        assert_eq!(dict["structured"], false);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,JB3R0GO7p>vQL8tjw0b5hqpd0706kh9d3lR2vbl0400,2*40");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: MultipleSlotBinaryMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.msg_type(), deserialized.msg_type());
        assert_eq!(msg.mmsi(), deserialized.mmsi());
    }
}
 */
