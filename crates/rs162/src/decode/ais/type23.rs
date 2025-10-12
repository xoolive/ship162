use deku::prelude::*;
use serde::{Deserialize, Serialize};

use super::common::{ShipType, StationIntervals, StationType, TransmitMode};
use super::converters::*;

/// AIS Group Assignment Command (Type 23)
///
/// This message is used to assign a reporting schedule for a given
/// geographic area. It is used to reduce channel loading by commanding
/// Class B units to reduce their reporting rate in congested areas.
///
/// Reference: https://gpsd.gitlab.io/gpsd/AIVDM.html#_type_23_group_assignment_command
#[derive(Debug, Clone, PartialEq, DekuRead, Serialize, Deserialize)]
#[deku(endian = "big")]
pub struct GroupAssignmentCommand {
    /// Message type (always 23 for this message)
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

    /// Station type
    #[deku(
        bits = "4",
        map = "|x: u8| -> Result<_, DekuError> { Ok(StationType::from_bits(x)) }"
    )]
    pub station_type: StationType,

    /// Ship and cargo classification
    #[deku(
        bits = "8",
        map = "|x: u8| -> Result<_, DekuError> { Ok(ShipType::from_bits(x)) }"
    )]
    pub ship_type: ShipType,

    /// Spare bits (should be zero)
    #[deku(bits = "22")]
    pub spare_2: u32,

    /// Transmit/receive mode
    #[deku(
        bits = "2",
        map = "|x: u8| -> Result<_, DekuError> { Ok(TransmitMode::from_bits(x)) }"
    )]
    pub txrx: TransmitMode,

    /// Reporting interval
    #[deku(
        bits = "4",
        map = "|x: u8| -> Result<_, DekuError> { Ok(StationIntervals::from_bits(x)) }"
    )]
    pub interval: StationIntervals,

    /// Quiet time (0-15 minutes)
    #[deku(bits = "4")]
    pub quiet: u8,

    /// Spare bits (should be zero)
    #[deku(bits = "6")]
    pub spare_3: u8,
}

impl GroupAssignmentCommand {
    /// Convert to a dictionary-like structure for testing compatibility
    pub fn asdict(&self) -> serde_json::Value {
        serde_json::json!({
            "msg_type": self.msg_type,
            "repeat": self.repeat,
            "mmsi": self.mmsi,
            "ne_lon": self.ne_lon,
            "ne_lat": self.ne_lat,
            "sw_lon": self.sw_lon,
            "sw_lat": self.sw_lat,
            "station_type": self.station_type as u8,
            "ship_type": self.ship_type as u8,
            "txrx": self.txrx as u8,
            "interval": self.interval as u8,
            "quiet": self.quiet,
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

    fn decode(sentence: &str) -> GroupAssignmentCommand {
        let nmea_msg = NmeaAisMessage::parse(sentence).unwrap();
        let binary_data = nmea_msg.payload_to_binary().unwrap();

        let (_, msg) = GroupAssignmentCommand::from_bytes((&binary_data, 0)).unwrap();
        msg
    }

    #[test]
    fn test_msg_type_23() {
        let msg = decode("!AIVDM,1,1,,B,G02:Kn01R`sn@291nj600000900,2*12");

        assert_eq!(msg.msg_type, 23);
        assert_eq!(msg.mmsi, 2268120);
        assert_eq!(msg.ne_lon, 157.8);
        assert_eq!(msg.ship_type, ShipType::NotAvailable);
        assert_eq!(msg.ne_lat, 3064.2);
        assert_eq!(msg.sw_lon, 109.6);
        assert_eq!(msg.sw_lat, 3040.8);
        assert_eq!(msg.station_type, StationType::Regional);
        assert_eq!(msg.txrx, TransmitMode::TxATxBRxARxB);
        assert_eq!(msg.interval as u8, 9);
        assert_eq!(msg.quiet, 0);
    }

    #[test]
    fn test_message_serialization() {
        let msg = decode("!AIVDM,1,1,,B,G02:Kn01R`sn@291nj600000900,2*12");

        // Test that we can serialize and deserialize
        let json = msg.to_json().unwrap();
        let deserialized: GroupAssignmentCommand = serde_json::from_str(&json).unwrap();

        assert_eq!(msg, deserialized);
    }
}
