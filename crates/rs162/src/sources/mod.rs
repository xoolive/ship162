//! Provide facilities to parse various data sources
//!
//! Available sources include:
//! - IQ file source for processing IQ data files.
//! - NMEA file source for handling NMEA formatted files.
//! - Timestamped NMEA source, e.g. for live AIS data streams over TCP.
//!
pub mod iq;
pub mod nmea;
pub mod nmea_ts;
pub mod rtlsdr;

pub use iq::{IqFormat, IqSource};
pub use nmea::NmeaFileSource;
pub use nmea_ts::TimestampedNmeaTcpSource;
pub use rtlsdr::{RtlSdrConfig, RtlSdrReceiver};
