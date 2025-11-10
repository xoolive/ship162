//! AIS Message Parser and Decoder
//!
//! This library provides functionality to:
//! - demodulate AIS messages from I/Q samples
//! - parse NMEA AIVDM/AIVDO messages
//!
//! and to convert them to binary u8 data, then decode them to structured AIS messages.

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
        StationIntervals, StationType, TransmitMode, UtcDateInquiry,
    };
    pub use crate::decode::mmsi::{MmsiInfo, MmsiType};
    pub use crate::decode::nmea::{MessageAssembler, NmeaAisMessage, NmeaError};
    pub use deku::DekuRead;
}
