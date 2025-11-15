use deku::prelude::*;
use serde::{Deserialize, Deserializer, Serialize};

use super::converters::*;

/// AIS Channel Management (Type 22) - Addressed Mode
///
/// Used when the message is addressed to specific stations (addressed field is 1).
/// Contains two destination MMSIs instead of geographical coordinates.
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct ChannelManagementAddressed {
    /// Message type (always 22 for this message)
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

    /// Channel A number
    #[deku(bits = "12")]
    pub channel_a: u16,

    /// Channel B number
    #[deku(bits = "12")]
    pub channel_b: u16,

    /// Transmit/receive mode
    #[deku(bits = "4")]
    pub txrx: u8,

    /// High power flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub power: bool,

    /// First destination MMSI
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub dest1: u32,

    /// Empty field 1
    #[deku(bits = "5")]
    pub empty_1: u8,

    /// Second destination MMSI
    #[deku(
        bits = "30",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_mmsi(x)) }"
    )]
    pub dest2: u32,

    /// Empty field 2
    #[deku(bits = "5")]
    pub empty_2: u8,

    /// Addressed flag (always 1 for this variant)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub addressed: bool,

    /// Channel A band flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub band_a: bool,

    /// Channel B band flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub band_b: bool,

    /// Zone size
    #[deku(bits = "3")]
    pub zonesize: u8,

    /// Spare bits (should be zero)
    #[deku(bits = "23", assert_eq = "0")]
    #[serde(skip)]
    pub spare_2: u32,
}

/// AIS Channel Management (Type 22) - Broadcast Mode
///
/// Used when the message is broadcast to all stations (addressed field is 0).
/// Contains geographical coordinates defining a rectangular area.
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct ChannelManagementBroadcast {
    /// Message type (always 22 for this message)
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

    /// Channel A number
    #[deku(bits = "12")]
    pub channel_a: u16,

    /// Channel B number
    #[deku(bits = "12")]
    pub channel_b: u16,

    /// Transmit/receive mode
    #[deku(bits = "4")]
    pub txrx: u8,

    /// High power flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub power: bool,

    /// Northeast corner longitude in 0.1 minutes (signed)
    #[deku(
        bits = "18",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_10th_minutes(x, 18)) }"
    )]
    pub ne_lon: f64,

    /// Northeast corner latitude in 0.1 minutes (signed)
    #[deku(
        bits = "17",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_10th_minutes(x, 17)) }"
    )]
    pub ne_lat: f64,

    /// Southwest corner longitude in 0.1 minutes (signed)
    #[deku(
        bits = "18",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_10th_minutes(x, 18)) }"
    )]
    pub sw_lon: f64,

    /// Southwest corner latitude in 0.1 minutes (signed)
    #[deku(
        bits = "17",
        map = "|x: u32| -> Result<_, DekuError> { Ok(from_10th_minutes(x, 17)) }"
    )]
    pub sw_lat: f64,

    /// Addressed flag (always 0 for this variant)
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub addressed: bool,

    /// Channel A band flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub band_a: bool,

    /// Channel B band flag
    #[deku(bits = "1", map = "|x: u8| -> Result<_, DekuError> { Ok(x != 0) }")]
    pub band_b: bool,

    /// Zone size
    #[deku(bits = "3")]
    pub zonesize: u8,

    /// Spare bits (should be zero)
    #[deku(bits = "23", assert_eq = "0")]
    #[serde(skip)]
    pub spare_2: u32,
}

/// AIS Channel Management (Type 22)
///
/// This message is used by base stations to manage VHF channels within their
/// coverage area. The encoding differs depending on the `addressed` field:
/// - If addressed field is 0: broadcast mode with geographical coordinates
/// - If addressed field is 1: addressed mode with destination MMSIs
///
/// Reference: <https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_22_channel_management>
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ChannelManagement {
    /// Broadcast mode (addressed = 0) - with geographical area
    Broadcast(ChannelManagementBroadcast),
    /// Addressed mode (addressed = 1) - with destination MMSIs
    Addressed(ChannelManagementAddressed),
}

// NOTE: `super::Message::deserialize` relies on `msg_type`
impl<'de> Deserialize<'de> for ChannelManagement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de;
        let value = serde_json::Value::deserialize(deserializer)?;
        let addressed = value
            .get("addressed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if addressed {
            serde_json::from_value(value)
                .map(ChannelManagement::Addressed)
                .map_err(de::Error::custom)
        } else {
            serde_json::from_value(value)
                .map(ChannelManagement::Broadcast)
                .map_err(de::Error::custom)
        }
    }
}

impl ChannelManagement {
    /// Parse Type 22 message from binary data, determining variant based on addressed flag (bit 139)
    pub fn from_bytes(data: (&[u8], usize)) -> Result<((&[u8], usize), Self), DekuError> {
        let (bytes, _bit_offset) = data;

        // Check bit 139 (the addressed flag) to determine which variant to parse
        if bytes.len() < 18 {
            return Err(DekuError::Incomplete(deku::error::NeedSize::new(18 * 8)));
        }

        let addressed_bit = (bytes[17] & 0x02) != 0; // bit 139 is in byte 17, bit 1

        if addressed_bit {
            // Addressed mode (addressed = 1)
            let (remaining, addressed_msg) = ChannelManagementAddressed::from_bytes(data)?;
            Ok((remaining, ChannelManagement::Addressed(addressed_msg)))
        } else {
            // Broadcast mode (addressed = 0)
            let (remaining, broadcast_msg) = ChannelManagementBroadcast::from_bytes(data)?;
            Ok((remaining, ChannelManagement::Broadcast(broadcast_msg)))
        }
    }

    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        match self {
            ChannelManagement::Broadcast(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "channel_a": msg.channel_a,
                    "channel_b": msg.channel_b,
                    "txrx": msg.txrx,
                    "power": msg.power,
                    "ne_lon": msg.ne_lon,
                    "ne_lat": msg.ne_lat,
                    "sw_lon": msg.sw_lon,
                    "sw_lat": msg.sw_lat,
                    "addressed": msg.addressed,
                    "band_a": msg.band_a,
                    "band_b": msg.band_b,
                    "zonesize": msg.zonesize,
                })
            }
            ChannelManagement::Addressed(msg) => {
                serde_json::json!({
                    "msg_type": msg.msg_type,
                    "repeat": msg.repeat,
                    "mmsi": msg.mmsi,
                    "channel_a": msg.channel_a,
                    "channel_b": msg.channel_b,
                    "txrx": msg.txrx,
                    "power": msg.power,
                    "dest1": msg.dest1,
                    "dest2": msg.dest2,
                    "addressed": msg.addressed,
                    "band_a": msg.band_a,
                    "band_b": msg.band_b,
                    "zonesize": msg.zonesize,
                })
            }
        }
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Get the message type (always 22)
    pub fn msg_type(&self) -> u8 {
        match self {
            ChannelManagement::Broadcast(msg) => msg.msg_type,
            ChannelManagement::Addressed(msg) => msg.msg_type,
        }
    }

    /// Get the MMSI
    pub fn mmsi(&self) -> u32 {
        match self {
            ChannelManagement::Broadcast(msg) => msg.mmsi,
            ChannelManagement::Addressed(msg) => msg.mmsi,
        }
    }
}

impl DekuReader<'_, ()> for ChannelManagement {
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

    fn decode(sentence: &str) -> ChannelManagement {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = ChannelManagement::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_22_broadcast() {
        // Broadcast
        let msg = decode("!AIVDM,1,1,,B,F030p:j2N2P5aJR0r;6f3rj10000,0*11");

        // Verify it's the broadcast variant
        if let ChannelManagement::Broadcast(msg) = msg {
            assert_eq!(msg.msg_type, 22);
            assert_eq!(msg.mmsi, 3160107);
            assert_eq!(msg.channel_a, 2087);
            assert_eq!(msg.channel_b, 2088);
            assert_eq!(msg.txrx, 0);
            assert!(!msg.power);
            assert_eq!(msg.ne_lon, -7710.0);
            assert_eq!(msg.ne_lat, 3300.0);
            assert_eq!(msg.sw_lon, -8020.0);
            assert_eq!(msg.sw_lat, 3210.0);
            assert!(!msg.addressed);
            assert!(!msg.band_a);
            assert!(!msg.band_b);
            assert_eq!(msg.zonesize, 2);
            // Expected
        } else {
            panic!("Expected broadcast variant");
        }
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,B,F030p:j2N2P5aJR0r;6f3rj10000,0*11");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: ChannelManagement = serde_json::from_str(&json).unwrap();

        assert_eq!(msg.msg_type(), deserialized.msg_type());
        assert_eq!(msg.mmsi(), deserialized.mmsi());
    }
}
