use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

// Custom reader for remaining binary data
fn read_remaining_data<R: std::io::Read + std::io::Seek>(
    reader: &mut Reader<R>,
) -> Result<Vec<u8>, DekuError> {
    let mut data = Vec::new();

    // Read remaining data until end of stream
    while let Ok(byte) = u8::from_reader_with_ctx(reader, ()) {
        data.push(byte);
    }

    Ok(data)
}

/// AIS Single Slot Binary Message (Type 25) - Addressed Structured
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct SingleSlotBinaryAddressedStructured {
    /// Message type (always 25)
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

    /// Binary data (remaining bits)
    #[deku(reader = "read_remaining_data(deku::reader)")]
    pub data: Vec<u8>,
}

/// AIS Single Slot Binary Message (Type 25) - Broadcast Structured
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct SingleSlotBinaryBroadcastStructured {
    /// Message type (always 25)
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

    /// Binary data (remaining bits)
    #[deku(reader = "read_remaining_data(deku::reader)")]
    pub data: Vec<u8>,
}

/// AIS Single Slot Binary Message (Type 25) - Addressed Unstructured
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct SingleSlotBinaryAddressedUnstructured {
    /// Message type (always 25)
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

    /// Binary data (remaining bits)
    #[deku(reader = "read_remaining_data(deku::reader)")]
    pub data: Vec<u8>,
}

/// AIS Single Slot Binary Message (Type 25) - Broadcast Unstructured
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct SingleSlotBinaryBroadcastUnstructured {
    /// Message type (always 25)
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

    /// Binary data (remaining bits)
    #[deku(reader = "read_remaining_data(deku::reader)")]
    pub data: Vec<u8>,
}

/// AIS Single Slot Binary Message (Type 25)
///
/// This message is used for single-slot binary data transmission.
/// It has four variants based on addressed and structured flags.
///
/// Reference: <https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_25_single_slot_binary_message>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SingleSlotBinaryMessage {
    /// Addressed structured message
    AddressedStructured(SingleSlotBinaryAddressedStructured),
    /// Broadcast structured message
    BroadcastStructured(SingleSlotBinaryBroadcastStructured),
    /// Addressed unstructured message
    AddressedUnstructured(SingleSlotBinaryAddressedUnstructured),
    /// Broadcast unstructured message
    BroadcastUnstructured(SingleSlotBinaryBroadcastUnstructured),
}

impl SingleSlotBinaryMessage {
    fn from_bytes(data: (&[u8], usize)) -> Result<((&[u8], usize), Self), DekuError> {
        let (bytes, _bit_offset) = data;

        // Extract addressed and structured flags from bits 38-39
        if bytes.len() < 5 {
            return Err(DekuError::Incomplete(deku::error::NeedSize::new(5 * 8)));
        }

        let flags = (bytes[4] >> 6) & 0x03;
        let addressed = (flags & 0x02) != 0; // bit 38
        let structured = (flags & 0x01) != 0; // bit 39

        match (addressed, structured) {
            (true, true) => {
                let (remaining, msg) = SingleSlotBinaryAddressedStructured::from_bytes(data)?;
                Ok((remaining, SingleSlotBinaryMessage::AddressedStructured(msg)))
            }
            (false, true) => {
                let (remaining, msg) = SingleSlotBinaryBroadcastStructured::from_bytes(data)?;
                Ok((remaining, SingleSlotBinaryMessage::BroadcastStructured(msg)))
            }
            (true, false) => {
                let (remaining, msg) = SingleSlotBinaryAddressedUnstructured::from_bytes(data)?;
                Ok((
                    remaining,
                    SingleSlotBinaryMessage::AddressedUnstructured(msg),
                ))
            }
            (false, false) => {
                let (remaining, msg) = SingleSlotBinaryBroadcastUnstructured::from_bytes(data)?;
                Ok((
                    remaining,
                    SingleSlotBinaryMessage::BroadcastUnstructured(msg),
                ))
            }
        }
    }
}

impl DekuReader<'_, ()> for SingleSlotBinaryMessage {
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

impl SingleSlotBinaryMessage {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        match self {
            SingleSlotBinaryMessage::AddressedStructured(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "addressed": msg.addressed,
                    "structured": msg.structured,
                    "dest_mmsi": msg.dest_mmsi,
                    "app_id": msg.app_id,
                    "data": msg.data,
                })
            }
            SingleSlotBinaryMessage::BroadcastStructured(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "addressed": msg.addressed,
                    "structured": msg.structured,
                    "app_id": msg.app_id,
                    "data": msg.data,
                })
            }
            SingleSlotBinaryMessage::AddressedUnstructured(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "addressed": msg.addressed,
                    "structured": msg.structured,
                    "dest_mmsi": msg.dest_mmsi,
                    "data": msg.data,
                })
            }
            SingleSlotBinaryMessage::BroadcastUnstructured(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "addressed": msg.addressed,
                    "structured": msg.structured,
                    "data": msg.data,
                })
            }
        }
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Get the message type (always 25)
    pub fn msg_type(&self) -> u8 {
        25
    }

    /// Get the MMSI
    pub fn mmsi(&self) -> u32 {
        match self {
            SingleSlotBinaryMessage::AddressedStructured(msg) => msg.mmsi,
            SingleSlotBinaryMessage::BroadcastStructured(msg) => msg.mmsi,
            SingleSlotBinaryMessage::AddressedUnstructured(msg) => msg.mmsi,
            SingleSlotBinaryMessage::BroadcastUnstructured(msg) => msg.mmsi,
        }
    }
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::nmea::NmeaAisMessage;

    fn decode(sentence: &str) -> SingleSlotBinaryMessage {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = SingleSlotBinaryMessage::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_25_a() {
        let msg = decode("!AIVDM,1,1,,A,I6SWo?8P00a3PKpEKEVj0?vNP<65,0*73");
        let dict = msg.asdict();

        assert_eq!(dict["msg_type"], 25);
        assert_eq!(dict["addressed"], true);
        assert_eq!(dict["structured"], false);
        assert_eq!(dict["dest_mmsi"], 134218384);
    }

    #[test]
    fn test_msg_type_25_b() {
        let msg = decode("!AIVDO,1,1,,A,I6SWo?<P00a00;Cwwwwwwwwwww0,0*4A");
        let dict = msg.asdict();

        assert_eq!(dict["msg_type"], 25);
        assert_eq!(dict["addressed"], true);
        assert_eq!(dict["structured"], true);
        assert_eq!(dict["dest_mmsi"], 134218384);
        assert_eq!(dict["mmsi"], 440006460);
        assert_eq!(dict["repeat"], 0);
        assert_eq!(dict["app_id"], 45);
    }

    #[test]
    fn test_msg_type_25_c() {
        let msg = decode("!AIVDO,1,1,,A,I6SWo?8P00a0003wwwwwwwwwwww0,0*35");
        let dict = msg.asdict();

        assert_eq!(dict["msg_type"], 25);
        assert_eq!(dict["addressed"], true);
        assert_eq!(dict["structured"], false);
        assert_eq!(dict["dest_mmsi"], 134218384);
        assert_eq!(dict["mmsi"], 440006460);
        assert_eq!(dict["repeat"], 0);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,I6SWo?8P00a3PKpEKEVj0?vNP<65,0*73");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: SingleSlotBinaryMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.msg_type(), deserialized.msg_type());
        assert_eq!(msg.mmsi(), deserialized.mmsi());
    }
}
 */
