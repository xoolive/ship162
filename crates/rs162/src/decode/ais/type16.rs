use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

/// AIS Assignment Mode Command (Type 16) - Single Station
///
/// Used to assign slot numbers and transmission intervals to a single station.
/// Message length: 96 bits (92 bits + 4 spare bits)
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct AssignmentCommandSingle {
    /// Message type (always 16 for this message)
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

    /// First station MMSI to assign
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub mmsi1: u32,

    /// Slot offset for first station
    #[deku(bits = "12")]
    pub offset1: u16,

    /// Slot increment for first station
    #[deku(bits = "10")]
    pub increment1: u16,

    /// Spare bits (should be zero)
    #[deku(bits = "4")]
    #[serde(skip)]
    pub spare_2: u8,
}

/// AIS Assignment Mode Command (Type 16) - Two Stations
///
/// Used to assign slot numbers and transmission intervals to two stations.
/// Message length: 144 bits
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct AssignmentCommandDouble {
    /// Message type (always 16 for this message)
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

    /// First station MMSI to assign
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub mmsi1: u32,

    /// Slot offset for first station
    #[deku(bits = "12")]
    pub offset1: u16,

    /// Slot increment for first station
    #[deku(bits = "10")]
    pub increment1: u16,

    /// Second station MMSI to assign
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub mmsi2: u32,

    /// Slot offset for second station
    #[deku(bits = "12")]
    pub offset2: u16,

    /// Slot increment for second station
    #[deku(bits = "10")]
    pub increment2: u16,
}

/// AIS Assignment Mode Command (Type 16)
///
/// This message is used by base stations to assign transmission schedules to other stations.
/// It has two variants based on message length:
/// - 96 bits: Single station assignment
/// - 144 bits: Two station assignment
///
/// Reference: <https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_16_assignment_mode_command>
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssignmentModeCommand {
    /// Single station assignment (96 bits)
    Single(AssignmentCommandSingle),
    /// Two station assignment (144 bits)
    Double(AssignmentCommandDouble),
}

impl AssignmentModeCommand {
    /// Parse Type 16 message from binary data, determining variant based on length
    pub fn from_bytes(data: (&[u8], usize)) -> Result<((&[u8], usize), Self), DekuError> {
        let (bytes, _bit_offset) = data;
        let total_bits = bytes.len() * 8;

        if total_bits > 96 {
            // Long message (144 bits) - two stations
            let (remaining, double_msg) = AssignmentCommandDouble::from_bytes(data)?;
            Ok((remaining, AssignmentModeCommand::Double(double_msg)))
        } else {
            // Short message (96 bits) - single station
            let (remaining, single_msg) = AssignmentCommandSingle::from_bytes(data)?;
            Ok((remaining, AssignmentModeCommand::Single(single_msg)))
        }
    }

    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        match self {
            AssignmentModeCommand::Single(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "mmsi1": msg.mmsi1,
                    "offset1": msg.offset1,
                    "increment1": msg.increment1,
                })
            }
            AssignmentModeCommand::Double(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "mmsi1": msg.mmsi1,
                    "offset1": msg.offset1,
                    "increment1": msg.increment1,
                    "mmsi2": msg.mmsi2,
                    "offset2": msg.offset2,
                    "increment2": msg.increment2,
                })
            }
        }
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Get the message type (always 16)
    pub fn msg_type(&self) -> u8 {
        match self {
            AssignmentModeCommand::Single(msg) => msg.msg_type,
            AssignmentModeCommand::Double(msg) => msg.msg_type,
        }
    }

    /// Get the MMSI
    pub fn mmsi(&self) -> u32 {
        match self {
            AssignmentModeCommand::Single(msg) => msg.mmsi,
            AssignmentModeCommand::Double(msg) => msg.mmsi,
        }
    }
}

impl DekuReader<'_, ()> for AssignmentModeCommand {
    fn from_reader_with_ctx<R: std::io::Read + std::io::Seek>(
        reader: &mut Reader<R>,
        _ctx: (),
    ) -> Result<Self, DekuError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::nmea::NmeaAisMessage;

    fn decode(sentence: &str) -> AssignmentModeCommand {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = AssignmentModeCommand::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_16_short() {
        let msg = decode("!AIVDM,1,1,,A,@01uEO@mMk7P<P00,0*18");

        // Verify it's the single variant
        if let AssignmentModeCommand::Single(msg) = msg {
            assert_eq!(msg.msg_type, 16);
            assert_eq!(msg.repeat, 0);
            assert_eq!(msg.mmsi, 2053501);
            assert_eq!(msg.mmsi1, 224251000);
            assert_eq!(msg.offset1, 200);
            assert_eq!(msg.increment1, 0);
        } else {
            panic!("Expected single station variant");
        }
    }

    #[test]
    fn test_msg_type_16_long() {
        let msg = decode("!AIVDO,1,1,,A,@@07Ql@01Qat005h0gN<@00e,0*46");

        // Verify it's the double variant
        if let AssignmentModeCommand::Double(msg) = msg {
            assert_eq!(msg.msg_type, 16);
            assert_eq!(msg.repeat, 1);
            assert_eq!(msg.mmsi, 123345);
            assert_eq!(msg.mmsi1, 99999);
            assert_eq!(msg.offset1, 0);
            assert_eq!(msg.increment1, 23);
            assert_eq!(msg.mmsi2, 777777);
            assert_eq!(msg.offset2, 0);
            assert_eq!(msg.increment2, 45);
        } else {
            panic!("Expected double station variant");
        }
    }

    #[test]
    fn test_msg_type_16_types() {
        let short = decode("!AIVDM,1,1,,A,@01uEO@mMk7P<P00,0*18");
        let long = decode("!AIVDO,1,1,,A,@@07Ql@01Qat005h0gN<@00e,0*46");

        // Ensure each message has the expected variant
        assert!(matches!(short, AssignmentModeCommand::Single(_)));
        assert!(matches!(long, AssignmentModeCommand::Double(_)));

        // Both instances have a MMSI
        assert_eq!(short.mmsi(), 2053501);
        assert_eq!(long.mmsi(), 123345);

        // Test specific field access
        if let AssignmentModeCommand::Single(single_msg) = short {
            assert_eq!(single_msg.mmsi1, 224251000);
        }

        if let AssignmentModeCommand::Double(double_msg) = long {
            assert_eq!(double_msg.mmsi1, 99999);
            assert_eq!(double_msg.mmsi2, 777777);
        }
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,@01uEO@mMk7P<P00,0*18");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: AssignmentModeCommand = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.msg_type(), deserialized.msg_type());
        assert_eq!(msg.mmsi(), deserialized.mmsi());
    }

    #[test]
    fn test_asdict_compatibility() {
        let msg = decode("!AIVDM,1,1,,A,@01uEO@mMk7P<P00,0*18");
        let dict = msg.asdict();

        assert_eq!(dict["msg_type"], 16);
        assert_eq!(dict["repeat"], 0);
        assert_eq!(dict["mmsi"], 2053501);
        assert_eq!(dict["mmsi1"], 224251000);
    }
}
