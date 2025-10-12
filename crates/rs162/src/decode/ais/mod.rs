//! AIS (Automatic Identification System) message decoding
//!
//! This module provides structures and functions for decoding AIS messages
//! from binary data using the `deku` library for bit-level parsing.

pub mod common;
pub mod converters;
pub mod type1;
pub mod type10;
pub mod type12;
pub mod type14;
pub mod type15;
pub mod type16;
pub mod type17;
pub mod type18;
pub mod type19;
pub mod type20;
pub mod type21;
pub mod type22;
pub mod type23;
pub mod type24;
pub mod type25;
pub mod type26;
pub mod type27;
pub mod type4;
pub mod type5;
pub mod type6;
pub mod type7;
pub mod type8;
pub mod type9;

#[cfg(test)]
mod tests;

pub use common::{
    EpfdType, InlandLoadedType, ManeuverIndicator, NavAid, NavigationStatus, ShipType,
    StationIntervals, StationType, TransmitMode, TurnRate,
};
use serde::Serialize;
pub use type1::PositionReport; // Handles Types 1, 2, and 3
pub use type10::UtcDateInquiry;
pub use type12::AddressedSafetyMessage;
pub use type14::SafetyBroadcastMessage;
pub use type15::Interrogation;
pub use type16::AssignmentModeCommand;
pub use type17::DgnssBroadcastMessage;
pub use type18::ClassBPositionReport;
pub use type19::ExtendedClassBPositionReport;
pub use type20::DataLinkManagementMessage;
pub use type21::AidToNavigationReport;
pub use type22::ChannelManagement;
pub use type23::GroupAssignmentCommand;
pub use type24::StaticDataReport;
pub use type25::SingleSlotBinaryMessage;
pub use type26::MultipleSlotBinaryMessage;
pub use type27::LongRangeAisBroadcastMessage;
pub use type4::BaseStationTimeReport; // Handles both Type 4 and Type 11 (renamed)
pub use type5::StaticAndVoyageData;
pub use type6::BinaryAddressedMessage;
pub use type7::BinaryAcknowledge;
pub use type8::BinaryBroadcastMessage; // Keeps original name (not renamed)
pub use type9::SarAircraftPositionReport;

use super::nmea::{MessageAssembler, NmeaAisMessage};
use deku::prelude::*;

use std::io::Seek;

/// General AIS message type that dispatches to specific message types based on message type field
#[derive(Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Message {
    PositionReport1(PositionReport),
    PositionReport2(PositionReport),
    PositionReport3(PositionReport),
    BaseStationTimeReport(BaseStationTimeReport),
    StaticAndVoyageData(StaticAndVoyageData),
    BinaryAddressedMessage(BinaryAddressedMessage),
    BinaryAcknowledge(BinaryAcknowledge),
    BinaryBroadcastMessage(BinaryBroadcastMessage),
    SarAircraftPositionReport(SarAircraftPositionReport),
    UtcDateInquiry(UtcDateInquiry),
    BaseStationTimeReport11(BaseStationTimeReport),
    AddressedSafetyMessage(AddressedSafetyMessage),
    SafetyBroadcastMessage(SafetyBroadcastMessage),
    Interrogation(Interrogation),
    AssignmentModeCommand(AssignmentModeCommand),
    DgnssBroadcastMessage(DgnssBroadcastMessage),
    ClassBPositionReport(ClassBPositionReport),
    ExtendedClassBPositionReport(ExtendedClassBPositionReport),
    DataLinkManagementMessage(DataLinkManagementMessage),
    AidToNavigationReport(AidToNavigationReport),
    ChannelManagement(ChannelManagement),
    GroupAssignmentCommand(GroupAssignmentCommand),
    StaticDataReport(StaticDataReport),
    SingleSlotBinaryMessage(SingleSlotBinaryMessage),
    MultipleSlotBinaryMessage(MultipleSlotBinaryMessage),
    LongRangeAisBroadcastMessage(LongRangeAisBroadcastMessage),
}

impl DekuReader<'_, ()> for Message {
    fn from_reader_with_ctx<R: std::io::Read + std::io::Seek>(
        reader: &mut Reader<R>,
        _ctx: (),
    ) -> Result<Self, DekuError> {
        // Read the first 6 bits to determine message type
        let msg_type = u8::from_reader_with_ctx(reader, deku::ctx::BitSize(6))?;

        // Reset reader position to beginning so the specific message parser can read the full message
        reader
            .seek(std::io::SeekFrom::Start(0))
            .map_err(|e| DekuError::Io(e.kind()))?;

        match msg_type {
            1 => Ok(Message::PositionReport1(
                PositionReport::from_reader_with_ctx(reader, ())?,
            )),
            2 => Ok(Message::PositionReport2(
                PositionReport::from_reader_with_ctx(reader, ())?,
            )),
            3 => Ok(Message::PositionReport3(
                PositionReport::from_reader_with_ctx(reader, ())?,
            )),
            4 => Ok(Message::BaseStationTimeReport(
                BaseStationTimeReport::from_reader_with_ctx(reader, ())?,
            )),
            5 => Ok(Message::StaticAndVoyageData(
                StaticAndVoyageData::from_reader_with_ctx(reader, ())?,
            )),
            6 => Ok(Message::BinaryAddressedMessage(
                BinaryAddressedMessage::from_reader_with_ctx(reader, ())?,
            )),
            7 => Ok(Message::BinaryAcknowledge(
                BinaryAcknowledge::from_reader_with_ctx(reader, ())?,
            )),
            8 => Ok(Message::BinaryBroadcastMessage(
                BinaryBroadcastMessage::from_reader_with_ctx(reader, ())?,
            )),
            9 => Ok(Message::SarAircraftPositionReport(
                SarAircraftPositionReport::from_reader_with_ctx(reader, ())?,
            )),
            10 => Ok(Message::UtcDateInquiry(
                UtcDateInquiry::from_reader_with_ctx(reader, ())?,
            )),
            11 => Ok(Message::BaseStationTimeReport11(
                BaseStationTimeReport::from_reader_with_ctx(reader, ())?,
            )),
            12 => Ok(Message::AddressedSafetyMessage(
                AddressedSafetyMessage::from_reader_with_ctx(reader, ())?,
            )),
            14 => Ok(Message::SafetyBroadcastMessage(
                SafetyBroadcastMessage::from_reader_with_ctx(reader, ())?,
            )),
            15 => Ok(Message::Interrogation(Interrogation::from_reader_with_ctx(
                reader,
                (),
            )?)),
            16 => Ok(Message::AssignmentModeCommand(
                AssignmentModeCommand::from_reader_with_ctx(reader, ())?,
            )),
            17 => Ok(Message::DgnssBroadcastMessage(
                DgnssBroadcastMessage::from_reader_with_ctx(reader, ())?,
            )),
            18 => Ok(Message::ClassBPositionReport(
                ClassBPositionReport::from_reader_with_ctx(reader, ())?,
            )),
            19 => Ok(Message::ExtendedClassBPositionReport(
                ExtendedClassBPositionReport::from_reader_with_ctx(reader, ())?,
            )),
            20 => Ok(Message::DataLinkManagementMessage(
                DataLinkManagementMessage::from_reader_with_ctx(reader, ())?,
            )),
            21 => Ok(Message::AidToNavigationReport(
                AidToNavigationReport::from_reader_with_ctx(reader, ())?,
            )),
            22 => Ok(Message::ChannelManagement(
                ChannelManagement::from_reader_with_ctx(reader, ())?,
            )),
            23 => Ok(Message::GroupAssignmentCommand(
                GroupAssignmentCommand::from_reader_with_ctx(reader, ())?,
            )),
            24 => Ok(Message::StaticDataReport(
                StaticDataReport::from_reader_with_ctx(reader, ())?,
            )),
            25 => Ok(Message::SingleSlotBinaryMessage(
                SingleSlotBinaryMessage::from_reader_with_ctx(reader, ())?,
            )),
            26 => Ok(Message::MultipleSlotBinaryMessage(
                MultipleSlotBinaryMessage::from_reader_with_ctx(reader, ())?,
            )),
            27 => Ok(Message::LongRangeAisBroadcastMessage(
                LongRangeAisBroadcastMessage::from_reader_with_ctx(reader, ())?,
            )),
            _ => Err(DekuError::InvalidParam(
                format!("Unknown message type: {}", msg_type).into(),
            )),
        }
    }
}

impl Message {
    /// Create a new AIS message from binary data or NMEA sentences
    ///
    /// This method parses one or more NMEA AIS sentences and converts them into a structured
    /// AIS message. It handles both single-sentence messages (like position reports) and
    /// multi-sentence messages (like static and voyage data).
    ///
    /// # Arguments
    ///
    /// * `sentences` - A slice of NMEA sentence strings. For single-sentence messages,
    ///   provide a single element. For multi-sentence messages, provide all sentences
    ///   in the correct order.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the parsed `Message` on success, or an error if:
    /// - The NMEA sentence format is invalid
    /// - The payload cannot be decoded to binary
    /// - Multi-sentence assembly fails
    /// - The binary data cannot be parsed into a valid AIS message
    ///
    /// # JSON Conversion
    ///
    /// Once you have a `Message`, you can convert it to JSON using serde:
    /// ```rust,ignore
    /// use serde_json;
    ///
    /// let message = Message::from_nmea(&sentences)?;
    /// let json_string = serde_json::to_string(&message)?;
    /// let json_value = serde_json::to_value(&message)?;
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// use rs162::decode::ais::Message;
    ///
    /// // Example with a Type 1 position report NMEA sentence
    /// let nmea_sentence = "!AIVDM,1,1,,B,15M67FC000G?ufbE`FepT@3n00Sa,0*5C";
    /// let message = Message::from_nmea(&[nmea_sentence]).unwrap();
    ///
    /// match message {
    ///     Message::PositionReport1(pos_report) => {
    ///         println!("Received position report from MMSI: {}", pos_report.mmsi);
    ///     }
    ///     _ => println!("Received other message type"),
    /// }
    ///
    /// // Example with multi-sentence Type 5 static and voyage data
    /// let sentences = [
    ///     "!AIVDM,2,1,1,A,55?MbV02;H;s<HtKR20EHE:0@T4@Dn2222222216L961O5Gf0NSQEp6ClRp8,0*1C",
    ///     "!AIVDM,2,2,1,A,88888888880,2*25",
    /// ];
    /// let message = Message::from_nmea(&sentences).unwrap();
    /// let json = serde_json::to_string(&message).unwrap();
    /// println!("JSON: {}", json);
    /// ```
    pub fn from_nmea(sentences: &[&str]) -> Result<Message, Box<dyn std::error::Error>> {
        let binary_data = if sentences.len() == 1 {
            let nmea_msg = NmeaAisMessage::parse(sentences[0])?;
            nmea_msg.payload_to_binary()?
        } else {
            let messages: Result<Vec<_>, _> = sentences
                .iter()
                .map(|&sentence| NmeaAisMessage::parse(sentence))
                .collect();
            let messages = messages?;
            MessageAssembler::assemble_from_iterable(messages)?
        };

        let cursor = std::io::Cursor::new(binary_data);
        let mut reader = Reader::new(cursor);
        Ok(Message::from_reader_with_ctx(&mut reader, ())?)
    }
}
