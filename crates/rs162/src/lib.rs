//! RS162 - NMEA AIS Message Parser and Decoder
//!
//! This library provides functionality to parse NMEA AIVDM/AIVDO messages
//! and convert them to binary u8 data, then decode them to structured AIS messages.

pub mod decode;
pub mod dsp;
pub mod sources;

pub mod prelude {
    pub use crate::decode::ais::Message;
    pub use crate::decode::ais::{
        AddressedSafetyMessage, AidToNavigationReport, AssignmentModeCommand,
        BaseStationTimeReport, BinaryAcknowledge, BinaryAddressedMessage, BinaryBroadcastMessage,
        ChannelManagement, ClassBPositionReport, DataLinkManagementMessage, DgnssBroadcastMessage,
        EpfdType, ExtendedClassBPositionReport, GroupAssignmentCommand, InlandLoadedType,
        Interrogation, LongRangeAisBroadcastMessage, ManeuverIndicator, MultipleSlotBinaryMessage,
        NavAid, NavigationStatus, PositionReport, SafetyBroadcastMessage,
        SarAircraftPositionReport, ShipType, SingleSlotBinaryMessage, StaticDataReport,
        StationIntervals, StationType, TransmitMode, TurnRate, UtcDateInquiry,
    };
    pub use crate::decode::nmea::{MessageAssembler, NmeaAisMessage, NmeaError};
    pub use deku::DekuRead;
}
