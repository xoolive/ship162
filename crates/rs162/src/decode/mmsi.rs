use super::ais::Message;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Country {
    pub country: String,
    pub flag: String,
    #[serde(rename = "iso-3166-1")]
    pub iso_3166_1: String,
}

impl Default for Country {
    fn default() -> Self {
        Self {
            country: "Unknown".to_string(),
            flag: "🏳️".to_string(),
            iso_3166_1: "XX".to_string(),
        }
    }
}

pub static DEFAULT_COUNTRY: Lazy<Country> = Lazy::new(Country::default);

/**
 * The first digit of an MMSI depends on the kind of station to which it is
 * assigned. So, MMSIs starting with any number between 2 to 7 refer to ship
 * stations. The initial digit of a ship station determines the vessel's area of
 * origin:
 *
 * 2 = Europe
 * 3 = North America, Central America and Caribbean
 * 4 = Asia
 * 5 = Oceania
 * 6 = Africa
 * 7 = South America
 *
 * The next 2 digits make each MMSI country-specific. This is how the vessel's
 * flag can be identified!
 *
 * For example:
 *
 * 247 = Italy
 * 205 = Belgium (notice that due to the fact that these countries
 * are European, both their MMSIs begin with the number 2)
 *
 * So, the first 3 digits of any MMSI number are indicative of the respective
 * vessel's flag. These 3 digits are also called Maritime Identification Digits
 * (MID).
 */
// Source: ITU Table of Maritime Identification Digits
// https://en.wikipedia.org/wiki/Maritime_identification_digits
const MID_JSON: &str = include_str!("../../data/country_mid.json");
pub static COUNTRY_MID: Lazy<HashMap<String, Country>> =
    Lazy::new(|| serde_json::from_str(MID_JSON).unwrap());

/// Validate and interpret a Maritime Mobile Service Identity (MMSI)
/// Based on ITU-R M.585 and USCG “MMSI Format” document:
/// https://www.navcen.uscg.gov/sites/default/files/pdf/MMSI/MMSI_Format.pdf
fn country_from_mid(mid: u16) -> &'static Country {
    if let Some(country) = COUNTRY_MID.get(&mid.to_string()) {
        country
    } else {
        &DEFAULT_COUNTRY
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MmsiType {
    #[serde(rename = "Coast Station")]
    CoastStation,
    #[serde(rename = "Group of Ships")]
    GroupOfShips,
    #[serde(rename = "SAR Aircraft")]
    SarAircraft,
    #[serde(rename = "AIS AtoN")]
    AisAton,
    #[serde(rename = "AIS SART/MOB/EPIRB")]
    AisSartMobEpirb,
    #[serde(rename = "Ship")]
    StandardShipStation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MmsiInfo {
    pub mmsi: String,
    pub mmsi_type: MmsiType,
    #[serde(flatten)]
    pub country: Country,
}

impl MmsiInfo {
    pub fn new(mmsi: String, mmsi_type: MmsiType, country: Country) -> Self {
        Self {
            mmsi,
            mmsi_type,
            country,
        }
    }

    pub fn from_message(msg: &Message) -> Result<Self, String> {
        let mmsi = match msg {
            Message::PositionReport1(msg) => msg.mmsi,
            Message::PositionReport2(msg) => msg.mmsi,
            Message::PositionReport3(msg) => msg.mmsi,
            Message::BaseStationTimeReport(msg) => msg.mmsi,
            Message::StaticAndVoyageData(msg) => msg.mmsi,
            Message::BinaryAddressedMessage(msg) => msg.mmsi,
            Message::BinaryAcknowledge(msg) => msg.mmsi,
            Message::BinaryBroadcastMessage(msg) => match msg {
                super::ais::type8::BinaryBroadcastMessage::Default(m) => m.mmsi,
                super::ais::type8::BinaryBroadcastMessage::Inland(m) => m.mmsi,
            },
            Message::SarAircraftPositionReport(msg) => msg.mmsi,
            Message::UtcDateInquiry(msg) => msg.mmsi,
            Message::BaseStationTimeReport11(msg) => msg.mmsi,
            Message::AddressedSafetyMessage(msg) => msg.mmsi,
            Message::SafetyBroadcastMessage(msg) => msg.mmsi,
            Message::Interrogation(msg) => msg.mmsi,
            Message::AssignmentModeCommand(msg) => match msg {
                super::ais::type16::AssignmentModeCommand::Single(m) => m.mmsi,
                super::ais::type16::AssignmentModeCommand::Double(m) => m.mmsi,
            },
            Message::DgnssBroadcastMessage(msg) => msg.mmsi,
            Message::ClassBPositionReport(msg) => msg.mmsi,
            Message::ExtendedClassBPositionReport(msg) => msg.mmsi,
            Message::DataLinkManagementMessage(msg) => msg.mmsi,
            Message::AidToNavigationReport(msg) => msg.mmsi,
            Message::ChannelManagement(msg) => match msg {
                super::ais::type22::ChannelManagement::Broadcast(m) => m.mmsi,
                super::ais::type22::ChannelManagement::Addressed(m) => m.mmsi,
            },
            Message::GroupAssignmentCommand(msg) => msg.mmsi,
            Message::StaticDataReport(msg) => match msg {
                super::ais::type24::StaticDataReport::PartA(m) => m.mmsi,
                super::ais::type24::StaticDataReport::PartB(m) => m.mmsi,
            },
            Message::SingleSlotBinaryMessage(msg) => match msg {
                super::ais::type25::SingleSlotBinaryMessage::AddressedStructured(m) => m.mmsi,
                super::ais::type25::SingleSlotBinaryMessage::BroadcastStructured(m) => m.mmsi,
                super::ais::type25::SingleSlotBinaryMessage::AddressedUnstructured(m) => m.mmsi,
                super::ais::type25::SingleSlotBinaryMessage::BroadcastUnstructured(m) => m.mmsi,
            },
            Message::MultipleSlotBinaryMessage(msg) => match msg {
                super::ais::type26::MultipleSlotBinaryMessage::AddressedStructured(m) => m.mmsi,
                super::ais::type26::MultipleSlotBinaryMessage::BroadcastStructured(m) => m.mmsi,
                super::ais::type26::MultipleSlotBinaryMessage::AddressedUnstructured(m) => m.mmsi,
                super::ais::type26::MultipleSlotBinaryMessage::BroadcastUnstructured(m) => m.mmsi,
            },
            Message::LongRangeAisBroadcastMessage(msg) => msg.mmsi,
        };
        Self::from_mmsi(mmsi)
    }

    pub fn from_mmsi(mmsi_num: u32) -> Result<Self, String> {
        let mmsi_str = format!("{:09}", mmsi_num);

        // Extract prefixes
        let prefix3: u16 = mmsi_str[0..3].parse().unwrap_or(0);

        // FORMAT 1 — Coast Station: 00MIDXXXX
        if mmsi_str.starts_with("00") {
            let mid: u16 = mmsi_str[2..5].parse().unwrap_or(0);
            if (200..800).contains(&mid) {
                return Ok(MmsiInfo::new(
                    mmsi_str,
                    MmsiType::CoastStation,
                    country_from_mid(mid).clone(),
                ));
            } else {
                return Err(format!("Invalid MID ({}) for coast station", mid));
            }
        }

        // FORMAT 2 — Group of Ships: 0MIDXXXXX
        if mmsi_str.starts_with('0') && !mmsi_str.starts_with("00") {
            let mid: u16 = mmsi_str[1..4].parse().unwrap_or(0);
            if (200..800).contains(&mid) {
                return Ok(MmsiInfo::new(
                    mmsi_str,
                    MmsiType::GroupOfShips,
                    country_from_mid(mid).clone(),
                ));
            } else {
                return Err(format!("Invalid MID ({}) for group", mid));
            }
        }

        // FORMAT 3 — SAR Aircraft: 111MIDXXX
        if mmsi_str.starts_with("111") {
            let mid: u16 = mmsi_str[3..6].parse().unwrap_or(0);
            if (200..800).contains(&mid) {
                return Ok(MmsiInfo::new(
                    mmsi_str,
                    MmsiType::SarAircraft,
                    country_from_mid(mid).clone(),
                ));
            } else {
                return Err(format!("Invalid MID ({}) for SAR aircraft", mid));
            }
        }

        // FORMAT 4 — AIS AtoN (99MID1XXX / 99MID6XXX)
        if mmsi_str.starts_with("99") {
            let mid: u16 = mmsi_str[2..5].parse().unwrap_or(0);
            let country = country_from_mid(mid);
            if country.country == "Unknown" {
                return Err(format!("Invalid MID ({}) for AtoN", mid));
            }
            return Ok(MmsiInfo::new(mmsi_str, MmsiType::AisAton, country.clone()));
        }

        // FORMAT 5 — AIS SART / MOB / EPIRB (970–979XXXX)
        if (970..980).contains(&prefix3) {
            return Ok(MmsiInfo::new(
                mmsi_str,
                MmsiType::AisSartMobEpirb,
                DEFAULT_COUNTRY.clone(),
            ));
        }

        // FORMAT 6 — Standard Ship Station (MIDXXXXXX)
        let mid: u16 = prefix3;
        if (200..800).contains(&mid) {
            return Ok(MmsiInfo::new(
                mmsi_str,
                MmsiType::StandardShipStation,
                country_from_mid(mid).clone(),
            ));
        }

        Err(format!(
            "Invalid or unrecognized MMSI (prefix = {:?})",
            prefix3
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_mmsi_various_formats() {
        if MmsiInfo::from_mmsi(0).is_ok() {
            panic!("Expected Err for MMSI 0, got Ok");
        }
        if let Ok(info) = MmsiInfo::from_mmsi(227456789u32) {
            assert_eq!(info.country.country, "France");
            assert_eq!(info.mmsi_type, MmsiType::StandardShipStation);
        } else {
            panic!("Expected Ok for MMSI 227456789, got Err");
        }
        if let Ok(info) = MmsiInfo::from_mmsi(2270001u32) {
            assert_eq!(info.country.country, "France");
            assert_eq!(info.mmsi_type, MmsiType::CoastStation);
        } else {
            panic!("Expected Ok for MMSI 2270001, got Err");
        }
        if let Ok(info) = MmsiInfo::from_mmsi(111227001u32) {
            assert_eq!(info.country.country, "France");
            assert_eq!(info.mmsi_type, MmsiType::SarAircraft);
        } else {
            panic!("Expected Ok for MMSI 111227001, got Err");
        }
        if let Ok(info) = MmsiInfo::from_mmsi(992270001u32) {
            assert_eq!(info.country.country, "France");
            assert_eq!(info.mmsi_type, MmsiType::AisAton);
        } else {
            panic!("Expected Ok for MMSI 992270001, got Err");
        }
        if let Ok(info) = MmsiInfo::from_mmsi(974123456u32) {
            assert_eq!(info.country.country, "Unknown");
            assert_eq!(info.mmsi_type, MmsiType::AisSartMobEpirb);
        } else {
            panic!("Expected Ok for MMSI 974123456, got Err");
        }
        if let Ok(info) = MmsiInfo::from_mmsi(22745600u32) {
            assert_eq!(info.country.country, "France");
            assert_eq!(info.mmsi_type, MmsiType::GroupOfShips);
        } else {
            panic!("Expected Ok for MMSI 22745600, got Err");
        }

        if let Ok(info) = MmsiInfo::from_mmsi(338123456u32) {
            assert_eq!(info.country.country, "United States");
            assert_eq!(info.mmsi_type, MmsiType::StandardShipStation);
        } else {
            panic!("Expected Ok for MMSI 338123456, got Err");
        }
        if MmsiInfo::from_mmsi(123456789u32).is_err() {
            // Invalid MID
        } else {
            panic!("Expected Err for MMSI 123456789, got Ok");
        }
        if MmsiInfo::from_mmsi(999999999u32).is_err() {
            // Out of range
        } else {
            panic!("Expected Err for MMSI 999999999, got Ok");
        }
        if let Ok(info) = MmsiInfo::from_mmsi(200000000u32) {
            assert_eq!(info.mmsi_type, MmsiType::StandardShipStation);
            // Lowest valid MID
        } else {
            panic!("Expected Ok for MMSI 200000000, got Err");
        }
        if let Ok(info) = MmsiInfo::from_mmsi(799999999u32) {
            assert_eq!(info.mmsi_type, MmsiType::StandardShipStation);
            // Highest valid MID
        } else {
            panic!("Expected Ok for MMSI 799999999, got Err");
        }
        if let Ok(info) = MmsiInfo::from_mmsi(970000001u32) {
            // AIS SART/MOB/EPIRB lower bound
            assert_eq!(info.mmsi_type, MmsiType::AisSartMobEpirb);
        } else {
            panic!("Expected Ok for MMSI 970000001, got Err");
        }
        if let Ok(info) = MmsiInfo::from_mmsi(979999999u32) {
            // AIS SART/MOB/EPIRB upper bound
            assert_eq!(info.mmsi_type, MmsiType::AisSartMobEpirb);
        } else {
            panic!("Expected Ok for MMSI 979999999, got Err");
        }
        if MmsiInfo::from_mmsi(800000000u32).is_err() {
            // Just outside valid MID
        } else {
            panic!("Expected Err for MMSI 800000000, got Ok");
        }
        if MmsiInfo::from_mmsi(199999999u32).is_err() {
            // Just below valid MID
        } else {
            panic!("Expected Err for MMSI 199999999, got Ok");
        }
    }
}
