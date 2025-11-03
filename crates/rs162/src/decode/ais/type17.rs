use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::converters::*;

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

/// AIS DGNSS Broadcast Binary Message (Type 17)
///
/// This message is used by differential reference stations to broadcast
/// differential correction information and other related data.
///
/// Reference: <https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_17_dgnss_broadcast_binary_message>
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct DgnssBroadcastMessage {
    /// Message type (always 17 for this message)
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

    /// Longitude in 1/10 minutes (signed)
    #[deku(
        bits = "18",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_10th_minutes_longitude(x)) }"
    )]
    pub longitude: f64,

    /// Latitude in 1/10 minutes (signed)
    #[deku(
        bits = "17",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_10th_minutes_latitude(x)) }"
    )]
    pub latitude: f64,

    /// Spare bits (should be zero)
    #[deku(bits = "5", assert_eq = "0")]
    #[serde(skip)]
    pub spare_2: u8,

    /// Binary data payload (variable length, up to 736 bits)
    #[deku(reader = "read_remaining_data(deku::reader)")]
    pub data: Vec<u8>,
}

impl DgnssBroadcastMessage {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "longitude": self.longitude,
            "latitude": self.latitude,
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
    use crate::decode::nmea::{MessageAssembler, NmeaAisMessage};
    fn decode_one(sentence: &str) -> DgnssBroadcastMessage {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();
        let (_, msg) = DgnssBroadcastMessage::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    fn decode_two(sentence1: &str, sentence2: &str) -> DgnssBroadcastMessage {
        let nmea_msg1 = NmeaAisMessage::parse(sentence1).unwrap();
        let nmea_msg2 = NmeaAisMessage::parse(sentence2).unwrap();

        let messages = vec![nmea_msg1, nmea_msg2];
        let binary_data = MessageAssembler::assemble_from_iterable(messages).unwrap();

        let (_, msg) = DgnssBroadcastMessage::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_17_a() {
        let msg = decode_two(
            "!AIVDM,2,1,5,A,A02VqLPA4I6C07h5Ed1h<OrsuBTTwS?r:C?w`?la<gno1RTRwSP9:BcurA8a,0*3A",
            "!AIVDM,2,2,5,A,:Oko02TSwu8<:Jbb,0*11",
        );

        assert_eq!(msg.msg_type, 17);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.mmsi, 2734450);
        assert!((msg.longitude - 1747.8).abs() < 0.1);
        assert!((msg.latitude - 3599.2).abs() < 0.1);

        // Verify data content
        assert!(!msg.data.is_empty());
    }

    #[test]
    fn test_msg_type_17_b() {
        let msg = decode_one("!AIVDM,1,1,,A,A0476BQ>J8`<h2JpH:4P0?j@2mTEw8`=DP1DEnqvj0,0*79");

        assert_eq!(msg.msg_type, 17);
        assert_eq!(msg.repeat, 0);
        assert_eq!(msg.mmsi, 4310602);
        assert!((msg.latitude - 2058.2).abs() < 0.1);
        assert!((msg.longitude - 8029.0).abs() < 0.1);

        // Verify data content
        assert!(!msg.data.is_empty());
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode_one("!AIVDM,1,1,,A,A0476BQ>J8`<h2JpH:4P0?j@2mTEw8`=DP1DEnqvj0,0*79");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: DgnssBroadcastMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }
}
