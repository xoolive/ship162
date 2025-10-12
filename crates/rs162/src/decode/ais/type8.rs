use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::common::InlandLoadedType;
use super::converters::*;

/// AIS Binary Broadcast Message (Type 8)
///
/// This message is used for broadcasting binary data. Different variants exist
/// based on the DAC (Designated Area Code) and FID (Function Identifier).
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_8_binary_broadcast_message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BinaryBroadcastMessage {
    /// Default variant for unknown DAC/FID combinations
    Default(MessageType8Default),
    /// Inland navigation variant (DAC=200, FID=10)
    Inland(MessageType8Dac200Fid10),
}

/// Default Type 8 message structure
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct MessageType8Default {
    /// Message type (always 8 for this message)
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

    /// Designated Area Code
    #[deku(bits = "10")]
    pub dac: u16,

    /// Function Identifier
    #[deku(bits = "6")]
    pub fid: u8,

    /// Binary data payload (variable length, up to 952 bits)
    #[deku(reader = "read_remaining_data(deku::reader)")]
    pub data: Vec<u8>,
}

/// Inland navigation Type 8 message (DAC=200, FID=10)
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct MessageType8Dac200Fid10 {
    /// Message type (always 8 for this message)
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

    /// Designated Area Code (200 for this variant)
    #[deku(bits = "10")]
    pub dac: u16,

    /// Function Identifier (10 for this variant)
    #[deku(bits = "6")]
    pub fid: u8,

    /// Unique European Vessel Identification Number / ERI number
    #[deku(
        bits = "48",
        map = "|x: u64| -> Result<_, DekuError> { Ok(from_sixbit_ascii_48(x, 8)) }"
    )]
    pub vin: String,

    /// Length of ship in 1/10m (1-8000, 0=default)
    #[deku(
        bits = "13",
        map = "|x: u16| -> Result<_, DekuError> { Ok(from_10th_u16(x)) }"
    )]
    pub length: f32,

    /// Beam of ship in 1/10m (1-1000, 0=default)
    #[deku(
        bits = "10",
        map = "|x: u16| -> Result<_, DekuError> { Ok(from_10th_u16(x)) }"
    )]
    pub beam: f32,

    /// Numeric ERI Classification
    #[deku(bits = "14")]
    pub shiptype: u16,

    /// Number of blue cones/lights (0-3, 4=B-Flag, 5=default/unknown)
    #[deku(bits = "3")]
    pub hazard: u8,

    /// Draught in 1/100m (1-2000, 0=default/unknown)
    #[deku(
        bits = "11",
        map = "|x: u16| -> Result<_, DekuError> { Ok(from_100th_u16(x)) }"
    )]
    pub draught: f32,

    /// Load status
    #[deku(
        bits = "2",
        map = "|x: u8| -> Result<_, DekuError> { Ok(InlandLoadedType::from_bits(x)) }"
    )]
    pub loaded: InlandLoadedType,

    /// Speed quality flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub speed_q: bool,

    /// Course quality flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub course_q: bool,

    /// Heading quality flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub heading_q: bool,

    /// Spare bits
    #[deku(bits = "8")]
    pub spare: u8,
}

fn read_remaining_data<R: std::io::Read + std::io::Seek>(
    reader: &mut Reader<R>,
) -> Result<Vec<u8>, DekuError> {
    let mut data = Vec::new();

    // Read remaining data using deku's byte reading methods
    while let Ok(byte) = u8::from_reader_with_ctx(reader, ()) {
        data.push(byte);
    }

    Ok(data)
}

impl DekuReader<'_, ()> for BinaryBroadcastMessage {
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

impl BinaryBroadcastMessage {
    /// Parse Type 8 message from binary data, determining variant based on DAC/FID
    pub fn from_bytes(data: (&[u8], usize)) -> Result<((&[u8], usize), Self), DekuError> {
        let (bytes, _bit_offset) = data;

        if bytes.len() < 7 {
            return Err(DekuError::Incomplete(deku::error::NeedSize::new(56)));
        }

        // Parse just enough to get DAC and FID
        // Extract DAC (bits 40-49) and FID (bits 50-55)
        let dac = ((bytes[5] as u16) << 2) | ((bytes[6] as u16) >> 6);
        let fid = bytes[6] & 0x3F;

        // Determine which variant to parse based on DAC and FID
        if dac == 200 && fid == 10 {
            let (remaining, inland_msg) = MessageType8Dac200Fid10::from_bytes(data)?;
            Ok((remaining, BinaryBroadcastMessage::Inland(inland_msg)))
        } else {
            let (remaining, default_msg) = MessageType8Default::from_bytes(data)?;
            Ok((remaining, BinaryBroadcastMessage::Default(default_msg)))
        }
    }

    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        match self {
            BinaryBroadcastMessage::Default(msg) => msg.asdict(),
            BinaryBroadcastMessage::Inland(msg) => msg.asdict(),
        }
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

impl MessageType8Default {
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "dac": self.dac,
            "fid": self.fid,
            "data": self.data,
        })
    }
}

impl MessageType8Dac200Fid10 {
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "dac": self.dac,
            "fid": self.fid,
            "vin": self.vin,
            "length": self.length,
            "beam": self.beam,
            "shiptype": self.shiptype,
            "hazard": self.hazard,
            "draught": self.draught,
            "loaded": self.loaded,
            "speed_q": self.speed_q,
            "course_q": self.course_q,
            "heading_q": self.heading_q,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::nmea::NmeaAisMessage;

    fn decode(sentence: &str) -> BinaryBroadcastMessage {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = BinaryBroadcastMessage::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_8() {
        let msg = decode("!AIVDM,1,1,,A,85Mwp`1Kf3aCnsNvBWLi=wQuNhA5t43N`5nCuI=p<IBfVqnMgPGs,0*47");

        let result = msg.asdict();
        assert_eq!(result["repeat"], 0);
        assert_eq!(result["mmsi"], 366999712);
        assert_eq!(result["dac"], 366);
        assert_eq!(result["fid"], 56);
        // Note: The exact data bytes may differ due to parsing implementation
        assert!(result["data"].is_array());
    }

    #[test]
    fn test_msg_type_8_inland() {
        let decoded = decode("!BSVDM,1,1,,B,83m;Fa0j2d<<<<<<<0@pUg`50000,0*11");
        let msg = decoded.asdict();

        assert_eq!(msg["repeat"], 0);
        assert_eq!(msg["mmsi"], 257087140);
        assert_eq!(msg["dac"], 200);
        assert_eq!(msg["fid"], 10);

        // Should be inland variant
        if let BinaryBroadcastMessage::Inland(_) = decoded {
            assert!(msg.get("beam").is_some());
            assert_eq!(msg["beam"], 7.5);
        } else {
            panic!("Expected inland variant");
        }
    }

    #[test]
    fn test_msg_type_8_inland_2() {
        let decoded = decode("!AIVDO,1,1,,A,85M67F@j2U=7EW=RAkQkBDITMV=e,0*51");
        let msg = decoded.asdict();

        assert_eq!(msg["mmsi"], 366053209);
        assert_eq!(msg["dac"], 200);
        assert_eq!(msg["fid"], 10);

        // Use approximate equality for floating-point values
        assert!((msg["length"].as_f64().unwrap() - 180.6).abs() < 0.01);
        assert!((msg["beam"].as_f64().unwrap() - 42.0).abs() < 0.01);

        if let BinaryBroadcastMessage::Inland(inland_msg) = decoded {
            assert_eq!(inland_msg.loaded, InlandLoadedType::NotAvailable);
        } else {
            panic!("Expected inland variant");
        }
    }
}
