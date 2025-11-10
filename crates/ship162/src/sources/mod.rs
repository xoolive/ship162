use std::str::FromStr;

use rs162::{
    decode::ais::type24,
    prelude::{Message, MmsiInfo},
};
use serde::{Deserialize, Serialize};
use tokio::sync::MutexGuard;
use tracing::error;
use url::Url;

use crate::state::AppState;

pub mod rtlsdr;
pub mod tcp;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddressStruct {
    pub host: String,
    pub port: u16,
    jump: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AddressPath {
    Short(String),
    Long(AddressStruct),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Address {
    /// Address to a TCP feed, like the one from Norwegian AIS Server (e.g. `tcp://ais.example.com:1234`)
    Tcp(AddressPath),
    /// A RTL-SDR dongle (require feature `rtlsdr`): the parameter can be empty, or use other specifiers, e.g. `rtlsdr://serial=00000001`
    Rtlsdr(Option<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    #[serde(flatten)]
    pub address: Address,
}

impl FromStr for Source {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let default_tcp = Url::parse("tcp://").unwrap();

        let url = default_tcp.join(s).map_err(|e| e.to_string())?;

        let address = match url.scheme() {
            "tcp" => Address::Tcp(AddressPath::Short(format!(
                "{}:{}",
                url.host_str().unwrap_or("153.44.253.27"),
                url.port_or_known_default().unwrap_or(5631),
            ))),

            "rtlsdr" => Address::Rtlsdr(url.host_str().map(|s| s.to_string())),

            _ => return Err("unsupported scheme".to_string()),
        };

        let source = Source { address };

        Ok(source)
    }
}

#[derive(Debug, Serialize)]
pub struct TimedMessage {
    pub timestamp: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signal_level: Option<f32>,
    #[serde(flatten)]
    pub message: Message,
    #[serde(flatten)]
    pub mmsi_info: Option<MmsiInfo>,
}

pub async fn process_sentence(mut state: MutexGuard<'_, AppState>, sentence: &mut TimedMessage) {
    let message = &sentence.message;
    let vessel = if let Ok(mmsi_info) = MmsiInfo::from_message(message) {
        sentence.mmsi_info = Some(mmsi_info.clone());
        state.update_vessel(mmsi_info)
    } else {
        error!("Failed to extract MMSI information: {:?}", message);
        return;
    };
    match message {
        Message::PositionReport1(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed;
            vessel.turn = msg.turn;
            vessel.course = msg.course;
            vessel.heading = msg.heading;
            vessel.status = Some(msg.status);
            vessel.last_update = sentence.timestamp;
        }
        Message::PositionReport2(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed;
            vessel.turn = msg.turn;
            vessel.course = msg.course;
            vessel.heading = msg.heading;
            vessel.status = Some(msg.status);
            vessel.last_update = sentence.timestamp;
        }
        Message::PositionReport3(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed;
            vessel.turn = msg.turn;
            vessel.course = msg.course;
            vessel.heading = msg.heading;
            vessel.status = Some(msg.status);
            vessel.last_update = sentence.timestamp;
        }
        Message::BaseStationTimeReport(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.last_update = sentence.timestamp;
        }
        Message::StaticAndVoyageData(msg) => {
            vessel.destination = Some(msg.destination.to_string());
            vessel.ship_name = Some(msg.shipname.to_string());
            vessel.callsign = Some(msg.callsign.to_string());
            vessel.to_bow = Some(msg.to_bow);
            vessel.to_stern = Some(msg.to_stern);
            vessel.to_port = Some(msg.to_port);
            vessel.to_starboard = Some(msg.to_starboard);
            vessel.ship_type = Some(msg.ship_type);
            vessel.last_update = sentence.timestamp;
        }
        Message::SarAircraftPositionReport(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed.map(|s| s as f32);
            vessel.course = msg.course;
            vessel.last_update = sentence.timestamp;
        }
        Message::DgnssBroadcastMessage(msg) => {
            vessel.latitude = Some(msg.latitude);
            vessel.longitude = Some(msg.longitude);
            vessel.last_update = sentence.timestamp;
        }
        Message::ClassBPositionReport(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed;
            vessel.course = msg.course;
            vessel.heading = msg.heading;
            vessel.last_update = sentence.timestamp;
        }
        Message::ExtendedClassBPositionReport(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed;
            vessel.course = msg.course;
            vessel.heading = msg.heading;
            vessel.ship_name = Some(msg.shipname.clone());
            vessel.ship_type = Some(msg.ship_type);
            vessel.to_bow = Some(msg.to_bow);
            vessel.to_stern = Some(msg.to_stern);
            vessel.to_port = Some(msg.to_port);
            vessel.to_starboard = Some(msg.to_starboard);
            vessel.last_update = sentence.timestamp;
        }
        Message::AidToNavigationReport(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.to_bow = Some(msg.to_bow);
            vessel.to_stern = Some(msg.to_stern);
            vessel.to_port = Some(msg.to_port);
            vessel.to_starboard = Some(msg.to_starboard);
            vessel.last_update = sentence.timestamp;
        }
        Message::StaticDataReport(type24::StaticDataReport::PartA(msg)) => {
            vessel.ship_name = Some(msg.shipname.to_string());
            vessel.last_update = sentence.timestamp;
        }
        Message::StaticDataReport(type24::StaticDataReport::PartB(msg)) => {
            vessel.ship_type = Some(msg.ship_type);
            vessel.callsign = Some(msg.callsign.to_string());
            vessel.to_bow = Some(msg.to_bow);
            vessel.to_stern = Some(msg.to_stern);
            vessel.to_port = Some(msg.to_port);
            vessel.to_starboard = Some(msg.to_starboard);
            vessel.last_update = sentence.timestamp;
        }
        Message::LongRangeAisBroadcastMessage(msg) => {
            vessel.latitude = msg.latitude;
            vessel.longitude = msg.longitude;
            vessel.speed = msg.speed.map(|s| s as f32);
            vessel.course = msg.course.map(|c| c as f32);
            vessel.last_update = sentence.timestamp;
        }
        _ => { /* Ignore other message types for now */ }
    }
}
