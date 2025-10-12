use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

// Custom reader for optional slot management fields
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

fn read_optional_u8<R: std::io::Read + std::io::Seek>(
    bits: usize,
) -> impl Fn(&mut Reader<R>) -> Result<Option<u8>, DekuError> {
    move |reader: &mut Reader<R>| match u8::from_reader_with_ctx(reader, deku::ctx::BitSize(bits)) {
        Ok(value) => Ok(Some(value)),
        Err(DekuError::Incomplete(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// AIS Data Link Management Message (Type 20)
///
/// This message is used to pre-allocate TDMA slots within an AIS base station network.
/// It contains no navigational information, and is unlikely to be of interest unless you
/// are implementing or studying an AIS base station network.
/// Length varies from 72-160 bits depending on the number of slot reservations (1 to 4).
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_20_data_link_management_message
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct DataLinkManagementMessage {
    /// Message type (always 20 for this message)
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

    /// Slot offset 1
    #[deku(bits = "12")]
    pub offset1: u16,

    /// Number of reserved consecutive slots 1
    #[deku(bits = "4")]
    pub number1: u8,

    /// Timeout value 1 (in minutes)
    #[deku(bits = "3")]
    pub timeout1: u8,

    /// Increment value 1
    #[deku(bits = "11")]
    pub increment1: u16,

    /// Slot offset 2 (optional)
    #[deku(reader = "read_optional_u16(12)(deku::reader)")]
    pub offset2: Option<u16>,

    /// Number of reserved consecutive slots 2 (optional)
    #[deku(reader = "read_optional_u8(4)(deku::reader)")]
    pub number2: Option<u8>,

    /// Timeout value 2 (in minutes, optional)
    #[deku(reader = "read_optional_u8(3)(deku::reader)")]
    pub timeout2: Option<u8>,

    /// Increment value 2 (optional)
    #[deku(reader = "read_optional_u16(11)(deku::reader)")]
    pub increment2: Option<u16>,

    /// Slot offset 3 (optional)
    #[deku(reader = "read_optional_u16(12)(deku::reader)")]
    pub offset3: Option<u16>,

    /// Number of reserved consecutive slots 3 (optional)
    #[deku(reader = "read_optional_u8(4)(deku::reader)")]
    pub number3: Option<u8>,

    /// Timeout value 3 (in minutes, optional)
    #[deku(reader = "read_optional_u8(3)(deku::reader)")]
    pub timeout3: Option<u8>,

    /// Increment value 3 (optional)
    #[deku(reader = "read_optional_u16(11)(deku::reader)")]
    pub increment3: Option<u16>,

    /// Slot offset 4 (optional)
    #[deku(reader = "read_optional_u16(12)(deku::reader)")]
    pub offset4: Option<u16>,

    /// Number of reserved consecutive slots 4 (optional)
    #[deku(reader = "read_optional_u8(4)(deku::reader)")]
    pub number4: Option<u8>,

    /// Timeout value 4 (in minutes, optional)
    #[deku(reader = "read_optional_u8(3)(deku::reader)")]
    pub timeout4: Option<u8>,

    /// Increment value 4 (optional)
    #[deku(reader = "read_optional_u16(11)(deku::reader)")]
    pub increment4: Option<u16>,
}

impl DataLinkManagementMessage {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "spare_1": format!("{:02x}", self.spare_1).as_bytes(),
            "offset1": self.offset1,
            "number1": self.number1,
            "timeout1": self.timeout1,
            "increment1": self.increment1,
            "offset2": self.offset2.unwrap_or(0),
            "number2": self.number2.unwrap_or(0),
            "timeout2": self.timeout2.unwrap_or(0),
            "increment2": self.increment2.unwrap_or(0),
            "offset3": self.offset3.unwrap_or(0),
            "number3": self.number3.unwrap_or(0),
            "timeout3": self.timeout3.unwrap_or(0),
            "increment3": self.increment3.unwrap_or(0),
            "offset4": self.offset4.unwrap_or(0),
            "number4": self.number4.unwrap_or(0),
            "timeout4": self.timeout4.unwrap_or(0),
            "increment4": self.increment4.unwrap_or(0),
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

    fn decode(sentence: &str) -> DataLinkManagementMessage {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = DataLinkManagementMessage::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_20() {
        let msg = decode("!AIVDM,1,1,,A,D028rqP<QNfp000000000000000,2*0C");

        assert_eq!(msg.msg_type, 20);
        assert_eq!(msg.mmsi, 2243302);
        assert_eq!(msg.offset1, 200);
        assert_eq!(msg.number1, 5);
        assert_eq!(msg.timeout1, 7);
        assert_eq!(msg.increment1, 750);
        assert_eq!(msg.spare_1, 0);

        // All other values should be None for the basic 72-bit message
        assert_eq!(msg.offset2, Some(0));
        assert_eq!(msg.number2, Some(0));
        assert_eq!(msg.timeout2, Some(0));
        assert_eq!(msg.increment2, Some(0));
        assert_eq!(msg.offset3, Some(0));
        assert_eq!(msg.number3, Some(0));
        assert_eq!(msg.timeout3, Some(0));
        assert_eq!(msg.increment3, Some(0));
        assert_eq!(msg.offset4, Some(0));
        assert_eq!(msg.number4, Some(0));
        assert_eq!(msg.timeout4, Some(0));
        assert_eq!(msg.increment4, Some(0));
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,D028rqP<QNfp000000000000000,2*0C");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: DataLinkManagementMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }
}
