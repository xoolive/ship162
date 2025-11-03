use rs162::{
    decode::ais::type24,
    prelude::{Message, MmsiInfo},
    sources::nmea_ts::TimestampedMessage,
};
use tokio::sync::MutexGuard;

use crate::state::AppState;

pub mod tcp;

async fn process_sentence(mut state: MutexGuard<'_, AppState>, sentence: TimestampedMessage) {
    if let Some(message) = sentence.decode() {
        let vessel = if let Ok(mmsi_info) = MmsiInfo::from_message(&message) {
            state.update_vessel(mmsi_info)
        } else {
            // TODO
            return;
        };
        match message {
            Message::PositionReport1(msg) => {
                vessel.latitude = Some(msg.latitude);
                vessel.longitude = Some(msg.longitude);
                vessel.speed = Some(msg.speed);
                vessel.turn = Some(msg.turn);
                vessel.course = Some(msg.course);
                vessel.heading = Some(msg.heading);
                vessel.status = Some(msg.status);
                vessel.last_update = sentence.timestamp;
            }
            Message::PositionReport2(msg) => {
                vessel.latitude = Some(msg.latitude);
                vessel.longitude = Some(msg.longitude);
                vessel.speed = Some(msg.speed);
                vessel.turn = Some(msg.turn);
                vessel.course = Some(msg.course);
                vessel.heading = Some(msg.heading);
                vessel.status = Some(msg.status);
                vessel.last_update = sentence.timestamp;
            }
            Message::PositionReport3(msg) => {
                vessel.latitude = Some(msg.latitude);
                vessel.longitude = Some(msg.longitude);
                vessel.speed = Some(msg.speed);
                vessel.turn = Some(msg.turn);
                vessel.course = Some(msg.course);
                vessel.heading = Some(msg.heading);
                vessel.status = Some(msg.status);
                vessel.last_update = sentence.timestamp;
            }
            Message::BaseStationTimeReport(msg) => {
                vessel.latitude = Some(msg.latitude);
                vessel.longitude = Some(msg.longitude);
                vessel.last_update = sentence.timestamp;
            }
            Message::StaticAndVoyageData(msg) => {
                vessel.destination = Some(msg.destination);
                vessel.ship_name = Some(msg.shipname);
                vessel.callsign = Some(msg.callsign);
                vessel.to_bow = Some(msg.to_bow);
                vessel.to_stern = Some(msg.to_stern);
                vessel.to_port = Some(msg.to_port);
                vessel.to_starboard = Some(msg.to_starboard);
                vessel.ship_type = Some(msg.ship_type);
                vessel.last_update = sentence.timestamp;
            }
            Message::SarAircraftPositionReport(msg) => {
                vessel.latitude = Some(msg.latitude);
                vessel.longitude = Some(msg.longitude);
                vessel.speed = Some(msg.speed as f32);
                vessel.course = Some(msg.course);
                vessel.last_update = sentence.timestamp;
            }
            Message::DgnssBroadcastMessage(msg) => {
                vessel.latitude = Some(msg.latitude);
                vessel.longitude = Some(msg.longitude);
                vessel.last_update = sentence.timestamp;
            }
            Message::ClassBPositionReport(msg) => {
                vessel.latitude = Some(msg.latitude);
                vessel.longitude = Some(msg.longitude);
                vessel.speed = Some(msg.speed);
                vessel.course = Some(msg.course);
                vessel.heading = Some(msg.heading);
                vessel.last_update = sentence.timestamp;
            }
            Message::ExtendedClassBPositionReport(msg) => {
                vessel.latitude = Some(msg.latitude);
                vessel.longitude = Some(msg.longitude);
                vessel.speed = Some(msg.speed);
                vessel.course = Some(msg.course);
                vessel.heading = Some(msg.heading);
                vessel.ship_name = Some(msg.shipname.clone());
                vessel.ship_type = Some(msg.ship_type);
                vessel.to_bow = Some(msg.to_bow);
                vessel.to_stern = Some(msg.to_stern);
                vessel.to_port = Some(msg.to_port);
                vessel.to_starboard = Some(msg.to_starboard);
                vessel.last_update = sentence.timestamp;
            }
            Message::AidToNavigationReport(msg) => {
                vessel.latitude = Some(msg.latitude);
                vessel.longitude = Some(msg.longitude);
                vessel.to_bow = Some(msg.to_bow);
                vessel.to_stern = Some(msg.to_stern);
                vessel.to_port = Some(msg.to_port);
                vessel.to_starboard = Some(msg.to_starboard);
                vessel.last_update = sentence.timestamp;
            }
            Message::StaticDataReport(type24::StaticDataReport::PartA(msg)) => {
                vessel.ship_name = Some(msg.shipname);
                vessel.last_update = sentence.timestamp;
            }
            Message::StaticDataReport(type24::StaticDataReport::PartB(msg)) => {
                vessel.ship_type = Some(msg.ship_type);
                vessel.callsign = Some(msg.callsign);
                vessel.to_bow = Some(msg.to_bow);
                vessel.to_stern = Some(msg.to_stern);
                vessel.to_port = Some(msg.to_port);
                vessel.to_starboard = Some(msg.to_starboard);
                vessel.last_update = sentence.timestamp;
            }
            Message::LongRangeAisBroadcastMessage(msg) => {
                vessel.latitude = Some(msg.latitude);
                vessel.longitude = Some(msg.longitude);
                vessel.speed = Some(msg.speed as f32);
                vessel.course = Some(msg.course as f32);
                vessel.last_update = sentence.timestamp;
            }
            _ => { /* Ignore other message types for now */ }
        }
    }
}
