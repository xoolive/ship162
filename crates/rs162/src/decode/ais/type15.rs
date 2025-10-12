use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

// Custom reader for optional interrogation fields that returns None if no data available
fn read_optional_mmsi<R: std::io::Read + std::io::Seek>(
    reader: &mut Reader<R>,
) -> Result<Option<u32>, DekuError> {
    // Try to read 30 bits for MMSI
    match u32::from_reader_with_ctx(reader, deku::ctx::BitSize(30)) {
        Ok(raw_mmsi) => {
            if raw_mmsi == 0 {
                Ok(None)
            } else {
                Ok(Some(from_mmsi(raw_mmsi)))
            }
        }
        Err(DekuError::Incomplete(_)) => Ok(None), // No more data available
        Err(e) => Err(e),                          // Other errors
    }
}

fn read_optional_u8<R: std::io::Read + std::io::Seek>(
    bits: usize,
) -> impl Fn(&mut Reader<R>) -> Result<Option<u8>, DekuError> {
    move |reader: &mut Reader<R>| match u8::from_reader_with_ctx(reader, deku::ctx::BitSize(bits)) {
        Ok(value) => Ok(Some(value)),
        Err(DekuError::Incomplete(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

fn read_optional_u16<R: std::io::Read + std::io::Seek>(
    bits: usize,
) -> impl Fn(&mut Reader<R>) -> Result<Option<u16>, DekuError> {
    move |reader: &mut Reader<R>| match u16::from_reader_with_ctx(reader, deku::ctx::BitSize(bits))
    {
        Ok(value) => Ok(Some(value)),
        Err(DekuError::Incomplete(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// AIS Interrogation (Type 15)
///
/// This message is used to request specific message types from other stations.
/// It can interrogate one or two stations, with up to two message type requests per station.
///
/// Reference: <https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_15_interrogation>
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct Interrogation {
    /// Message type (always 15 for this message)
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
    #[serde(skip)]
    pub spare_1: u8,

    /// First station MMSI to interrogate
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub mmsi1: u32,

    /// First message type requested from first station
    #[deku(bits = "6")]
    pub type1_1: u8,

    /// Slot offset for first message from first station
    #[deku(bits = "12")]
    pub offset1_1: u16,

    /// Spare bits (should be zero)
    #[deku(reader = "read_optional_u8(2)(deku::reader)")]
    #[serde(skip)]
    pub spare_2: Option<u8>,

    /// Second message type requested from first station (optional)
    #[deku(reader = "read_optional_u8(6)(deku::reader)")]
    pub type1_2: Option<u8>,

    /// Slot offset for second message from first station (optional)
    #[deku(reader = "read_optional_u16(12)(deku::reader)")]
    pub offset1_2: Option<u16>,

    /// Spare bits (should be zero) (optional)
    #[deku(reader = "read_optional_u8(2)(deku::reader)")]
    #[serde(skip)]
    pub spare_3: Option<u8>,

    /// Second station MMSI to interrogate (optional)
    #[deku(reader = "read_optional_mmsi(deku::reader)")]
    pub mmsi2: Option<u32>,

    /// Message type requested from second station (optional)
    #[deku(reader = "read_optional_u8(6)(deku::reader)")]
    pub type2_1: Option<u8>,

    /// Slot offset for message from second station (optional)
    #[deku(reader = "read_optional_u16(12)(deku::reader)")]
    pub offset2_1: Option<u16>,

    /// Spare bits (should be zero) (optional)
    #[deku(reader = "read_optional_u8(2)(deku::reader)")]
    #[serde(skip)]
    pub spare_4: Option<u8>,
}

impl Interrogation {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "mmsi1": self.mmsi1,
            "type1_1": self.type1_1,
            "offset1_1": self.offset1_1,
            "type1_2": self.type1_2,
            "offset1_2": self.offset1_2,
            "mmsi2": self.mmsi2,
            "type2_1": self.type2_1,
            "offset2_1": self.offset2_1,
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

    fn decode(sentence: &str) -> Interrogation {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = Interrogation::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_15_a() {
        let msg = decode("!AIVDM,1,1,,A,?5OP=l00052HD00,2*5B");

        assert_eq!(msg.msg_type, 15);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.mmsi, 368578000);
        assert_eq!(msg.offset1_1, 0);
        assert_eq!(msg.mmsi1, 5158);
        assert_eq!(msg.offset1_2, None);
        assert_eq!(msg.mmsi2, None);
    }

    #[test]
    fn test_msg_type_15_b() {
        let msg = decode("!AIVDM,1,1,,B,?h3Ovn1GP<K0<P@59a0,2*04");

        assert_eq!(msg.msg_type, 15);
        assert_eq!(msg.repeat, 3);
        assert_eq!(msg.mmsi, 3669720);
        assert_eq!(msg.mmsi1, 367014320);
        assert_eq!(msg.mmsi2, None);
        assert_eq!(msg.type1_1, 3);
        assert_eq!(msg.type1_2, Some(5));
        //assert_eq!(msg.offset1_2, Some(617));
        assert_eq!(msg.offset1_1, 516);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,?5OP=l00052HD00,2*5B");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: Interrogation = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_asdict_compatibility() {
        let msg = decode("!AIVDM,1,1,,A,?5OP=l00052HD00,2*5B");
        let dict = msg.asdict();

        assert_eq!(dict["msg_type"], 15);
        assert_eq!(dict["repeat"], 0);
        assert_eq!(dict["mmsi"], 368578000);
        assert_eq!(dict["mmsi1"], 5158);
        assert_eq!(dict["offset1_2"], serde_json::Value::Null);
        assert_eq!(dict["mmsi2"], serde_json::Value::Null);
    }

    #[test]
    fn test_msg_type_15_fields() {
        let msg = decode("!AIVDM,1,1,,A,?5OP=l00052HD00,2*5B");

        assert_eq!(msg.msg_type, 15);
        assert_eq!(msg.repeat, 0);
        assert!(msg.mmsi > 0);
        assert!(msg.mmsi1 > 0);

        // Test JSON serialization
        assert!(msg.to_json().is_ok());
    }
}
