use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::common::ShipType;
use super::converters::*;

/// Custom reader for optional spare field in Part A
fn read_optional_spare<R: std::io::Read + std::io::Seek>(
    reader: &mut Reader<R>,
) -> Result<Option<u8>, DekuError> {
    // Try to read 8 bits for spare field
    match u8::from_reader_with_ctx(reader, deku::ctx::BitSize(8)) {
        Ok(spare) => Ok(Some(spare)),
        Err(DekuError::Incomplete(_)) => Ok(None), // No more data available (160-bit message)
        Err(e) => Err(e),                          // Other errors
    }
}

/// AIS Static Data Report (Type 24) - Part A
///
/// Contains vessel name information.
/// Can be 160 bits (without spare) or 168 bits (with spare).
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct StaticDataReportPartA {
    /// Message type (always 24 for this message)
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

    /// Part number (always 0 for Part A)
    #[deku(bits = "2")]
    pub partno: u8,

    /// Ship name (20 6-bit characters)
    #[deku(
        bits = "120",
        map = "|x: u128| -> Result<_, DekuError> { Ok(from_sixbit_ascii_120(x, 20)) }"
    )]
    pub shipname: String,

    /// Spare bits (should be zero) - optional for Part A (only present in 168-bit version)
    #[deku(reader = "read_optional_spare(deku::reader)")]
    pub spare_1: Option<u8>,
}

/// AIS Static Data Report (Type 24) - Part B
///
/// Contains vessel type, dimensions, and equipment information.
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct StaticDataReportPartB {
    /// Message type (always 24 for this message)
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

    /// Part number (always 1 for Part B)
    #[deku(bits = "2")]
    pub partno: u8,

    /// Ship and cargo classification
    #[deku(
        bits = "8",
        map = "|x: u8| -> Result<_, DekuError> { Ok(ShipType::from_bits(x)) }"
    )]
    pub ship_type: ShipType,

    /// Vendor ID (3 6-bit characters)
    #[deku(
        bits = "18",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_sixbit_ascii_18(x, 3)) }"
    )]
    pub vendorid: String,

    /// Unit model code
    #[deku(bits = "4")]
    pub model: u8,

    /// Serial number
    #[deku(bits = "20")]
    pub serial: u32,

    /// Call sign (7 6-bit characters)
    #[deku(
        bits = "42",
        map = "|x: u64| -> Result<_, DekuError> { Ok(from_sixbit_ascii_42(x, 7)) }"
    )]
    pub callsign: String,

    /// Distance from reference point to bow in meters
    #[deku(bits = "9")]
    pub to_bow: u16,

    /// Distance from reference point to stern in meters
    #[deku(bits = "9")]
    pub to_stern: u16,

    /// Distance from reference point to port in meters
    #[deku(bits = "6")]
    pub to_port: u8,

    /// Distance from reference point to starboard in meters
    #[deku(bits = "6")]
    pub to_starboard: u8,

    /// Spare bits (should be zero)
    #[deku(bits = "6", assert_eq = "0")]
    #[serde(skip)]
    pub spare_1: u8,
}

/// AIS Static Data Report (Type 24)
///
/// This message is used by Class B equipment to associate dynamic and
/// voyage data with the vessel. It has two parts:
/// - Part A (partno = 0): Contains vessel name
/// - Part B (partno = 1): Contains vessel type, dimensions, and equipment info
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_24_static_data_report
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StaticDataReport {
    /// Part A (partno = 0) - vessel name
    PartA(StaticDataReportPartA),
    /// Part B (partno = 1) - vessel type and dimensions
    PartB(StaticDataReportPartB),
}

impl StaticDataReport {
    /// Parse Type 24 message from binary data, determining variant based on partno field (bits 38-39)
    pub fn from_bytes(data: (&[u8], usize)) -> Result<((&[u8], usize), Self), DekuError> {
        let (bytes, _bit_offset) = data;

        // Extract the partno field manually from bits 38-39
        // Bit 38 is in byte 4, bit 6; Bit 39 is in byte 4, bit 7
        if bytes.len() < 5 {
            return Err(DekuError::Incomplete(deku::error::NeedSize::new(5 * 8)));
        }

        let partno = bytes[4] & 0x03;

        match partno {
            0 => {
                // Part A (partno = 0)
                let (remaining, part_a_msg) = StaticDataReportPartA::from_bytes(data)?;
                Ok((remaining, StaticDataReport::PartA(part_a_msg)))
            }
            1 => {
                // Part B (partno = 1)
                let (remaining, part_b_msg) = StaticDataReportPartB::from_bytes(data)?;
                Ok((remaining, StaticDataReport::PartB(part_b_msg)))
            }
            _ => {
                // Invalid partno value - raise an error
                Err(DekuError::Parse(
                    format!("Invalid partno value: {}", partno).into(),
                ))
            }
        }
    }

    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        match self {
            StaticDataReport::PartA(msg) => {
                let mut result = serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "partno": msg.partno,
                    "shipname": msg.shipname,
                });

                // Only include spare_1 if it exists and we want to show it
                if let Some(spare) = msg.spare_1 {
                    result["spare_1"] = serde_json::Value::from(spare);
                }

                result
            }
            StaticDataReport::PartB(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "partno": msg.partno,
                    "ship_type": msg.ship_type as u8,
                    "vendorid": msg.vendorid,
                    "model": msg.model,
                    "serial": msg.serial,
                    "callsign": msg.callsign,
                    "to_bow": msg.to_bow,
                    "to_stern": msg.to_stern,
                    "to_port": msg.to_port,
                    "to_starboard": msg.to_starboard,
                })
            }
        }
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Get the message type (always 24)
    pub fn msg_type(&self) -> u8 {
        match self {
            StaticDataReport::PartA(msg) => msg.msg_type,
            StaticDataReport::PartB(msg) => msg.msg_type,
        }
    }

    /// Get the MMSI
    pub fn mmsi(&self) -> u32 {
        match self {
            StaticDataReport::PartA(msg) => msg.mmsi,
            StaticDataReport::PartB(msg) => msg.mmsi,
        }
    }
}

impl DekuReader<'_, ()> for StaticDataReport {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::nmea::NmeaAisMessage;

    fn decode(sentence: &str) -> StaticDataReport {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = StaticDataReport::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_24() {
        let msg = decode("!AIVDM,1,1,,A,H52KMeDU653hhhi0000000000000,0*1A");

        // Verify it's the Part B variant
        if let StaticDataReport::PartB(msg) = msg {
            assert_eq!(msg.msg_type, 24);
            assert_eq!(msg.mmsi, 338091445);
            assert_eq!(msg.partno, 1);
            assert_eq!(msg.ship_type, ShipType::PleasureCraft);
            assert_eq!(msg.vendorid, "FEC");
            assert_eq!(msg.callsign, "");
            assert_eq!(msg.to_bow, 0);
            assert_eq!(msg.to_stern, 0);
            assert_eq!(msg.to_port, 0);
            assert_eq!(msg.to_starboard, 0);
            assert_eq!(msg.spare_1, 0);
        } else {
            panic!("Expected Part B variant");
        }
    }

    #[test]
    fn test_msg_type_24_with_160_bits() {
        let msg = decode("!AIVDO,1,1,,A,H1mg=5@EP4m0hF1<PU000000000,2*77");

        // Verify it's the Part A variant
        if let StaticDataReport::PartA(msg) = msg {
            assert_eq!(msg.msg_type, 24);
            assert_eq!(msg.partno, 0);
            assert_eq!(msg.mmsi, 123456789);
        } else {
            panic!("Expected Part A variant");
        }
    }

    #[test]
    fn test_msg_type_24_with_168_bits() {
        let msg = decode("!AIVDO,1,1,,A,H1mg=5@EP4m0hF1<PU0000000000,0*45");

        // Verify it's the Part A variant
        if let StaticDataReport::PartA(msg) = msg {
            assert_eq!(msg.msg_type, 24);
            assert_eq!(msg.partno, 0);
            assert_eq!(msg.mmsi, 123456789);
        } else {
            panic!("Expected Part A variant");
        }
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,A,H52KMeDU653hhhi0000000000000,0*1A");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: StaticDataReport = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.msg_type(), deserialized.msg_type());
        assert_eq!(msg.mmsi(), deserialized.mmsi());
    }
}
