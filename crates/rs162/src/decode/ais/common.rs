use std::fmt::Display;

use serde::{Deserialize, Serialize};
/// Navigation status values for AIS messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NavigationStatus {
    #[serde(rename = "Under way using engine")]
    UnderWayUsingEngine = 0,
    #[serde(rename = "At anchor")]
    AtAnchor = 1,
    #[serde(rename = "Not under command")]
    NotUnderCommand = 2,
    #[serde(rename = "Restricted manoeuverability")]
    RestrictedManoeuverability = 3,
    #[serde(rename = "Constrained by her draught")]
    ConstrainedByHerDraught = 4,
    Moored = 5,
    Aground = 6,
    #[serde(rename = "Engaged in fishing")]
    EngagedInFishing = 7,
    #[serde(rename = "Under way sailing")]
    UnderWaySailing = 8,
    ReservedForFutureAmendment9 = 9,
    ReservedForFutureAmendment10 = 10,
    #[serde(rename = "Power-driven vessel towing astern")]
    PowerDrivenVesselTowingAstern = 11,
    #[serde(rename = "Power-driven vessel pushing ahead")]
    PowerDrivenVesselPushingAhead = 12,
    ReservedForFutureUse13 = 13,
    #[serde(rename = "AIS-SART is active")]
    AisSartIsActive = 14,
    Undefined = 15,
}

impl NavigationStatus {
    pub fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::UnderWayUsingEngine,
            1 => Self::AtAnchor,
            2 => Self::NotUnderCommand,
            3 => Self::RestrictedManoeuverability,
            4 => Self::ConstrainedByHerDraught,
            5 => Self::Moored,
            6 => Self::Aground,
            7 => Self::EngagedInFishing,
            8 => Self::UnderWaySailing,
            9 => Self::ReservedForFutureAmendment9,
            10 => Self::ReservedForFutureAmendment10,
            11 => Self::PowerDrivenVesselTowingAstern,
            12 => Self::PowerDrivenVesselPushingAhead,
            13 => Self::ReservedForFutureUse13,
            14 => Self::AisSartIsActive,
            _ => Self::Undefined,
        }
    }
    pub fn to_bits(self) -> u8 {
        self as u8
    }
}

impl Display for NavigationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            NavigationStatus::UnderWayUsingEngine => "Under way",
            NavigationStatus::AtAnchor => "At anchor",
            NavigationStatus::NotUnderCommand => "No command",
            NavigationStatus::RestrictedManoeuverability => "Restricted",
            NavigationStatus::ConstrainedByHerDraught => "Constrained",
            NavigationStatus::Moored => "Moored",
            NavigationStatus::Aground => "Aground",
            NavigationStatus::EngagedInFishing => "Fishing",
            NavigationStatus::UnderWaySailing => "Sailing",
            NavigationStatus::ReservedForFutureAmendment9 => "Reserved",
            NavigationStatus::ReservedForFutureAmendment10 => "Reserved",
            NavigationStatus::PowerDrivenVesselTowingAstern => "Towing astern",
            NavigationStatus::PowerDrivenVesselPushingAhead => "Pushing ahead",
            NavigationStatus::ReservedForFutureUse13 => "Reserved",
            NavigationStatus::AisSartIsActive => "AIS-SART",
            NavigationStatus::Undefined => "Undefined",
        };
        write!(f, "{}", description)
    }
}

impl ManeuverIndicator {}

/// Maneuver indicator values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManeuverIndicator {
    #[serde(rename = "N/A")]
    NotAvailable = 0,
    #[serde(rename = "No special maneuver")]
    NoSpecialManeuver = 1,
    #[serde(rename = "Special maneuver")]
    SpecialManeuver = 2,
    Undefined = 3,
}

impl ManeuverIndicator {
    pub fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::NotAvailable,
            1 => Self::NoSpecialManeuver,
            2 => Self::SpecialManeuver,
            _ => Self::Undefined,
        }
    }
    pub fn to_bits(self) -> u8 {
        self as u8
    }
}

/// Turn rate constants
pub struct TurnRate;

impl TurnRate {
    /// No turn information available
    pub const NO_TI_DEFAULT: f32 = -128.0;
    /// Turn information not available
    pub const NOT_AVAILABLE: f32 = -128.0;
    /// No turn
    pub const NO_TURN: f32 = 0.0;
}

/// Electronic Position Fixing Device (EPFD) types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpfdType {
    Undefined = 0,
    #[serde(rename = "GPS")]
    Gps = 1,
    #[serde(rename = "GLONASS")]
    Glonass = 2,
    #[serde(rename = "Combined GPS/GLONASS")]
    CombinedGpsGlonass = 3,
    #[serde(rename = "Loran-C")]
    LoranC = 4,
    Chayka = 5,
    #[serde(rename = "Integrated Navigation System")]
    IntegratedNavigationSystem = 6,
    Surveyed = 7,
    Galileo = 8,
    #[serde(rename = "Internal GNSS")]
    InternalGnss = 15,
}

impl EpfdType {
    pub fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::Undefined,
            1 => Self::Gps,
            2 => Self::Glonass,
            3 => Self::CombinedGpsGlonass,
            4 => Self::LoranC,
            5 => Self::Chayka,
            6 => Self::IntegratedNavigationSystem,
            7 => Self::Surveyed,
            8 => Self::Galileo,
            15 => Self::InternalGnss,
            _ => Self::Undefined,
        }
    }
}

/// Ship and Cargo Type values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShipType {
    #[serde(rename = "N/A")]
    NotAvailable = 0,
    // Reserved values 1-19
    Reserved = 1,
    // WIG types 21-29
    #[serde(rename = "Wing in Ground")]
    WingInGround = 20,
    Fishing = 30,
    Towing = 31,
    /// Towing: length exceeds 200m or breadth exceeds 25m
    #[serde(rename = "Towing, large")]
    TowingLarge = 32,
    #[serde(rename = "Dredging or underwater operations")]
    DredgingOrUnderwaterOps = 33,
    #[serde(rename = "Diving operations")]
    DivingOps = 34,
    #[serde(rename = "Military operations")]
    MilitaryOps = 35,
    Sailing = 36,
    #[serde(rename = "Pleasure craft")]
    PleasureCraft = 37,
    // Reserved 38-39
    #[serde(rename = "High Speed Craft")]
    HighSpeedCraft = 40,
    #[serde(rename = "Pilot Vessel")]
    PilotVessel = 50,
    #[serde(rename = "Search and Rescue")]
    SearchAndRescue = 51,
    Tug = 52,
    #[serde(rename = "Port Tender")]
    PortTender = 53,
    #[serde(rename = "Anti-Pollution Equipment")]
    AntiPollutionEquipment = 54,
    #[serde(rename = "Law Enforcement")]
    LawEnforcement = 55,
    Spare = 56,
    #[serde(rename = "Medical Transport")]
    MedicalTransport = 58,
    /// Non-combatant ship according to RR Resolution No. 18
    #[serde(rename = "Non-combatant Ship")]
    NoncombatantShip = 59,
    Passenger = 60,
    // More passenger types 61-69
    Cargo = 70,
    // More cargo types 71-79
    Tanker = 80,
    // More tanker types 81-89
    Other = 90,
    // More other types 91-99
}

impl Display for ShipType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            ShipType::NotAvailable => "N/A",
            ShipType::Reserved => "Reserved",
            ShipType::WingInGround => "Wing in Ground",
            ShipType::Fishing => "Fishing",
            ShipType::Towing => "Towing",
            ShipType::TowingLarge => "Towing, large",
            ShipType::DredgingOrUnderwaterOps => "Dredging or underwater operations",
            ShipType::DivingOps => "Diving operations",
            ShipType::MilitaryOps => "Military operations",
            ShipType::Sailing => "Sailing",
            ShipType::PleasureCraft => "Pleasure craft",
            ShipType::HighSpeedCraft => "High Speed Craft",
            ShipType::PilotVessel => "Pilot Vessel",
            ShipType::SearchAndRescue => "Search and Rescue",
            ShipType::Tug => "Tug",
            ShipType::PortTender => "Port Tender",
            ShipType::AntiPollutionEquipment => "Anti-Pollution Equipment",
            ShipType::LawEnforcement => "Law Enforcement",
            ShipType::Spare => "Spare",
            ShipType::MedicalTransport => "Medical Transport",
            ShipType::NoncombatantShip => "Non-combatant Ship",
            ShipType::Passenger => "Passenger",
            ShipType::Cargo => "Cargo",
            ShipType::Tanker => "Tanker",
            ShipType::Other => "Other",
        };
        write!(f, "{}", description)
    }
}

impl ShipType {
    pub fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::NotAvailable,
            1..=19 => Self::Reserved,
            20..=29 => Self::WingInGround,
            30 => Self::Fishing,
            31 => Self::Towing,
            32 => Self::TowingLarge,
            33 => Self::DredgingOrUnderwaterOps,
            34 => Self::DivingOps,
            35 => Self::MilitaryOps,
            36 => Self::Sailing,
            37 => Self::PleasureCraft,
            38..=39 => Self::Reserved,
            40..=49 => Self::HighSpeedCraft,
            50 => Self::PilotVessel,
            51 => Self::SearchAndRescue,
            52 => Self::Tug,
            53 => Self::PortTender,
            54 => Self::AntiPollutionEquipment,
            55 => Self::LawEnforcement,
            56..=57 => Self::Spare,
            58 => Self::MedicalTransport,
            59 => Self::NoncombatantShip,
            60..=69 => Self::Passenger,
            70..=79 => Self::Cargo,
            80..=89 => Self::Tanker,
            90..=99 => Self::Other,
            _ => Self::NotAvailable,
        }
    }
}

/// Inland navigation load status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlandLoadedType {
    NotAvailable = 0,
    Loaded = 1,
    Unloaded = 2,
    Reserved = 3,
}

impl InlandLoadedType {
    pub fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::NotAvailable,
            1 => Self::Loaded,
            2 => Self::Unloaded,
            _ => Self::Reserved,
        }
    }
}

/// Navigation Aid Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum NavAid {
    /// Default, Type of AtoN not specified
    #[serde(rename = "Default")]
    NotSpecified = 0,
    /// Reference point
    #[serde(rename = "Reference point")]
    ReferencePoint = 1,
    /// RACON (radar transponder marking a navigation hazard)
    #[serde(rename = "RACON")]
    Racon = 2,
    /// Fixed structure off shore, such as oil platforms, wind farms
    #[serde(rename = "Fixed structure")]
    FixedStructure = 3,
    /// Spare, Reserved for future use
    Spare = 4,
    /// Light, without sectors
    Light = 5,
    /// Light, with sectors
    #[serde(rename = "Light with sectors")]
    LightWithSectors = 6,
    /// Leading Light Front
    #[serde(rename = "Leading Light Front")]
    LeadingLightFront = 7,
    /// Leading Light Rear
    #[serde(rename = "Leading Light Rear")]
    LeadingLightRear = 8,
    /// Beacon, Cardinal N
    #[serde(rename = "Beacon, Cardinal N")]
    BeaconCardinalN = 9,
    /// Beacon, Cardinal E
    #[serde(rename = "Beacon, Cardinal E")]
    BeaconCardinalE = 10,
    /// Beacon, Cardinal S
    #[serde(rename = "Beacon, Cardinal S")]
    BeaconCardinalS = 11,
    /// Beacon, Cardinal W
    #[serde(rename = "Beacon, Cardinal W")]
    BeaconCardinalW = 12,
    /// Beacon, Port hand
    #[serde(rename = "Beacon, Port hand")]
    BeaconPortHand = 13,
    /// Beacon, Starboard hand
    #[serde(rename = "Beacon, Starboard hand")]
    BeaconStarboardHand = 14,
    /// Beacon, Preferred Channel port hand
    #[serde(rename = "Beacon, Preferred Channel port hand")]
    BeaconPreferredChannelPortHand = 15,
    /// Beacon, Preferred Channel starboard hand
    #[serde(rename = "Beacon, Preferred Channel starboard hand")]
    BeaconPreferredChannelStarboardHand = 16,
    /// Beacon, Isolated danger
    #[serde(rename = "Beacon, Isolated danger")]
    BeaconIsolatedDanger = 17,
    /// Beacon, Safe water
    #[serde(rename = "Beacon, Safe water")]
    BeaconSafeWater = 18,
    /// Beacon, Special mark
    #[serde(rename = "Beacon, Special mark")]
    BeaconSpecialMark = 19,
    /// Cardinal Mark N
    #[serde(rename = "Cardinal Mark N")]
    CardinalMarkN = 20,
    /// Cardinal Mark E
    #[serde(rename = "Cardinal Mark E")]
    CardinalMarkE = 21,
    /// Cardinal Mark S
    #[serde(rename = "Cardinal Mark S")]
    CardinalMarkS = 22,
    /// Cardinal Mark W
    #[serde(rename = "Cardinal Mark W")]
    CardinalMarkW = 23,
    /// Port hand Mark
    #[serde(rename = "Port hand Mark")]
    PortHandMark = 24,
    /// Starboard hand Mark
    #[serde(rename = "Starboard hand Mark")]
    StarboardHandMark = 25,
    /// Preferred Channel Port hand
    #[serde(rename = "Preferred Channel Port hand")]
    PreferredChannelPortHand = 26,
    /// Preferred Channel Starboard hand
    #[serde(rename = "Preferred Channel Starboard hand")]
    PreferredChannelStarboardHand = 27,
    /// Isolated danger
    #[serde(rename = "Isolated danger")]
    IsolatedDanger = 28,
    /// Safe Water
    #[serde(rename = "Safe Water")]
    SafeWater = 29,
    /// Special Mark
    #[serde(rename = "Special Mark")]
    SpecialMark = 30,
    /// Light Vessel / LANBY / Rigs
    #[serde(rename = "Light Vessel")]
    LightVessel = 31,
}

impl NavAid {
    /// Create NavAid from raw bits
    pub fn from_bits(bits: u8) -> Self {
        match bits {
            0 => NavAid::NotSpecified,
            1 => NavAid::ReferencePoint,
            2 => NavAid::Racon,
            3 => NavAid::FixedStructure,
            4 => NavAid::Spare,
            5 => NavAid::Light,
            6 => NavAid::LightWithSectors,
            7 => NavAid::LeadingLightFront,
            8 => NavAid::LeadingLightRear,
            9 => NavAid::BeaconCardinalN,
            10 => NavAid::BeaconCardinalE,
            11 => NavAid::BeaconCardinalS,
            12 => NavAid::BeaconCardinalW,
            13 => NavAid::BeaconPortHand,
            14 => NavAid::BeaconStarboardHand,
            15 => NavAid::BeaconPreferredChannelPortHand,
            16 => NavAid::BeaconPreferredChannelStarboardHand,
            17 => NavAid::BeaconIsolatedDanger,
            18 => NavAid::BeaconSafeWater,
            19 => NavAid::BeaconSpecialMark,
            20 => NavAid::CardinalMarkN,
            21 => NavAid::CardinalMarkE,
            22 => NavAid::CardinalMarkS,
            23 => NavAid::CardinalMarkW,
            24 => NavAid::PortHandMark,
            25 => NavAid::StarboardHandMark,
            26 => NavAid::PreferredChannelPortHand,
            27 => NavAid::PreferredChannelStarboardHand,
            28 => NavAid::IsolatedDanger,
            29 => NavAid::SafeWater,
            30 => NavAid::SpecialMark,
            31 => NavAid::LightVessel,
            _ => NavAid::NotSpecified,
        }
    }
}

/// Station Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum StationType {
    /// All types of mobiles
    AllMobiles = 0,
    /// Reserved for future use
    Reserved1 = 1,
    /// All types of Class B mobile stations
    AllClassB = 2,
    /// SAR airborne mobile station
    SarAirborne = 3,
    /// Aid to navigation station
    AidToNavigation = 4,
    /// Class B shipborne mobile station (SO)
    ClassBShipborne = 5,
    /// Regional use and inland waterways
    Regional = 6,
    /// Regional use and inland waterways
    Regional2 = 7,
    /// Regional use and inland waterways
    Regional3 = 8,
    /// Regional use and inland waterways
    Regional4 = 9,
    /// Reserved for future use
    Reserved10 = 10,
    /// Reserved for future use
    Reserved11 = 11,
    /// Reserved for future use
    Reserved12 = 12,
    /// Reserved for future use
    Reserved13 = 13,
    /// Reserved for future use
    Reserved14 = 14,
    /// Reserved for future use
    Reserved15 = 15,
}

impl StationType {
    pub fn from_bits(bits: u8) -> Self {
        match bits {
            0 => StationType::AllMobiles,
            1 => StationType::Reserved1,
            2 => StationType::AllClassB,
            3 => StationType::SarAirborne,
            4 => StationType::AidToNavigation,
            5 => StationType::ClassBShipborne,
            6 => StationType::Regional,
            7 => StationType::Regional2,
            8 => StationType::Regional3,
            9 => StationType::Regional4,
            10 => StationType::Reserved10,
            11 => StationType::Reserved11,
            12 => StationType::Reserved12,
            13 => StationType::Reserved13,
            14 => StationType::Reserved14,
            15 => StationType::Reserved15,
            _ => StationType::AllMobiles,
        }
    }
}

/// Transmit/Receive Mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum TransmitMode {
    /// TxA/TxB, RxA/RxB
    TxATxBRxARxB = 0,
    /// TxA, RxA/RxB
    TxARxARxB = 1,
    /// TxB, RxA/RxB
    TxBRxARxB = 2,
    /// Reserved for future use
    Reserved = 3,
}

impl TransmitMode {
    pub fn from_bits(bits: u8) -> Self {
        match bits {
            0 => TransmitMode::TxATxBRxARxB,
            1 => TransmitMode::TxARxARxB,
            2 => TransmitMode::TxBRxARxB,
            3 => TransmitMode::Reserved,
            _ => TransmitMode::TxATxBRxARxB,
        }
    }
}

/// Station Intervals
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum StationIntervals {
    /// Autonomous mode
    Autonomous = 0,
    /// 10 minutes
    Minutes10 = 1,
    /// 6 minutes
    Minutes6 = 2,
    /// 3 minutes
    Minutes3 = 3,
    /// 1 minute
    Minute1 = 4,
    /// 30 seconds
    Seconds30 = 5,
    /// 15 seconds
    Seconds15 = 6,
    /// 10 seconds
    Seconds10 = 7,
    /// 5 seconds
    Seconds5 = 8,
    /// Next shorter reporting interval
    NextShorter = 9,
    /// Next longer reporting interval
    NextLonger = 10,
    /// Reserved for future use
    Reserved11 = 11,
    /// Reserved for future use
    Reserved12 = 12,
    /// Reserved for future use
    Reserved13 = 13,
    /// Reserved for future use
    Reserved14 = 14,
    /// Reserved for future use
    Reserved15 = 15,
}

impl StationIntervals {
    pub fn from_bits(bits: u8) -> Self {
        match bits {
            0 => StationIntervals::Autonomous,
            1 => StationIntervals::Minutes10,
            2 => StationIntervals::Minutes6,
            3 => StationIntervals::Minutes3,
            4 => StationIntervals::Minute1,
            5 => StationIntervals::Seconds30,
            6 => StationIntervals::Seconds15,
            7 => StationIntervals::Seconds10,
            8 => StationIntervals::Seconds5,
            9 => StationIntervals::NextShorter,
            10 => StationIntervals::NextLonger,
            11 => StationIntervals::Reserved11,
            12 => StationIntervals::Reserved12,
            13 => StationIntervals::Reserved13,
            14 => StationIntervals::Reserved14,
            15 => StationIntervals::Reserved15,
            _ => StationIntervals::Autonomous,
        }
    }
}
