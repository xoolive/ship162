//! Timestamped NMEA source for AIS messages with timestamp parsing

use deku::reader::Reader;
use deku::DekuReader;

use crate::decode::nmea::{MessageAssembler, NmeaAisMessage, NmeaError};
use crate::prelude::Message;
use std::io::{BufRead, BufReader, Cursor, Lines};
use std::net::TcpStream;

/// Represents a timestamped decoded AIS message
#[derive(Debug, Clone)]
pub struct TimestampedMessage {
    pub timestamp: u64,
    pub binary_data: Vec<u8>,
    pub serial: u64,
}

/// Generic timestamped NMEA iterator that handles message assembly from any BufRead source
pub struct TimestampedNmeaIterator<R: BufRead> {
    lines: Lines<R>,
    assembler: MessageAssembler,
}

impl<R: BufRead> TimestampedNmeaIterator<R> {
    /// Create a new timestamped NMEA iterator from any BufRead source
    pub fn new(reader: R) -> Self {
        Self {
            lines: reader.lines(),
            assembler: MessageAssembler::new(),
        }
    }

    /// Get the next complete timestamped AIS message (handles multi-fragment assembly)
    pub fn next_complete_message(&mut self) -> Option<Result<TimestampedMessage, NmeaError>> {
        loop {
            // Read next line
            let line = match self.lines.next() {
                Some(Ok(line)) => line,
                Some(Err(_)) => return None, // IO error
                None => return None,         // EOF
            };

            let line = line.trim();

            // Parse timestamp and NMEA sentence
            let (serial, timestamp, nmea_sentence) = match Self::parse_timestamped_line(line) {
                Ok(parsed) => parsed,
                Err(e) => {
                    eprintln!("Error parsing timestamped line '{}': {}", line, e);
                    continue;
                }
            };

            // Parse NMEA message
            let nmea_msg = match NmeaAisMessage::parse(&nmea_sentence) {
                Ok(msg) => msg,
                Err(e) => return Some(Err(e)),
            };

            // Add to assembler and check if we have a complete message
            match self.assembler.add_fragment(nmea_msg) {
                Ok(Some(complete_message)) => {
                    return Some(Ok(TimestampedMessage {
                        timestamp,
                        binary_data: complete_message,
                        serial,
                    }))
                }
                Ok(None) => continue, // Need more fragments
                Err(e) => return Some(Err(e)),
            }
        }
    }

    /// Parse a timestamped line in the format: \s:serial,c:timestamp*checksum\!NMEA_MESSAGE
    fn parse_timestamped_line(
        line: &str,
    ) -> Result<(u64, u64, String), Box<dyn std::error::Error>> {
        // Split by the backslash that precedes the NMEA message
        let parts: Vec<&str> = line.splitn(2, "\\!").collect();
        if parts.len() != 2 {
            return Err("Invalid timestamped format: missing \\!".into());
        }

        let timestamp_part = parts[0];
        let nmea_part = format!("!{}", parts[1]);

        // Parse timestamp part: \s:serial,c:timestamp*checksum
        if !timestamp_part.starts_with("\\s:") {
            return Err("Invalid timestamped format: missing \\s:".into());
        }

        let timestamp_content = &timestamp_part[3..]; // Remove "\s:"

        // Split by '*' to separate data and checksum
        let timestamp_parts: Vec<&str> = timestamp_content.split('*').collect();
        if timestamp_parts.len() != 2 {
            return Err("Invalid timestamped format: missing checksum".into());
        }

        let data_part = timestamp_parts[0];
        // We could verify the checksum here if needed

        // Parse s:serial,c:timestamp
        let fields: Vec<&str> = data_part.split(',').collect();
        if fields.len() != 2 {
            return Err("Invalid timestamped format: expected s:serial,c:timestamp".into());
        }

        let serial = fields[0]
            .parse::<u64>()
            .map_err(|_| "Invalid serial number")?;

        if !fields[1].starts_with("c:") {
            return Err("Invalid timestamped format: missing c: prefix".into());
        }

        let timestamp = fields[1][2..]
            .parse::<u64>()
            .map_err(|_| "Invalid timestamp")?;

        Ok((serial, timestamp, nmea_part))
    }
}

impl<R: BufRead> Iterator for TimestampedNmeaIterator<R> {
    type Item = Result<TimestampedMessage, NmeaError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_complete_message()
    }
}

/// Convenience wrapper for file-based timestamped NMEA sources
pub struct TimestampedNmeaTcpSource {
    iterator: TimestampedNmeaIterator<BufReader<TcpStream>>,
}

impl TimestampedNmeaTcpSource {
    /// Open a TCP connection to a timestamped NMEA source
    pub fn new(server_address: &str) -> Result<Self, std::io::Error> {
        let stream = TcpStream::connect(server_address)?;
        let reader = BufReader::new(stream);
        Ok(Self {
            iterator: TimestampedNmeaIterator::new(reader),
        })
    }
}

impl Iterator for TimestampedNmeaTcpSource {
    type Item = Result<TimestampedMessage, NmeaError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iterator.next()
    }
}

impl TimestampedMessage {
    /// Decode the binary data into an AIS Message
    pub fn decode(&self) -> Option<Message> {
        let cursor = Cursor::new(&self.binary_data);
        let mut reader = Reader::new(cursor);
        Message::from_reader_with_ctx(&mut reader, ()).ok()
    }
}
