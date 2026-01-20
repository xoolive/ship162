//! Provide facilities to parse various data sources
//!
//! Available sources include:
//! - IQ file source for processing IQ data files.
//! - MQTT source for receiving AIS messages from MQTT brokers (e.g. Finnish Digitraffic).
//! - NMEA file source for handling NMEA formatted files.
//! - Timestamped NMEA source, e.g. for live AIS data streams over TCP (e.g. Norwegian Coastal Administration).
//! - SSH tunneling for secure remote connections.
//!

pub mod iq;
#[cfg(feature = "mqtt")]
pub mod mqtt;
pub mod nmea;
pub mod nmea_ts;
#[cfg(feature = "ssh")]
pub mod ssh;

pub use iq::{AisAsyncIqSource, AisIqSource};
pub use nmea::NmeaFileSource;
pub use nmea_ts::TimestampedNmeaTcpSource;
