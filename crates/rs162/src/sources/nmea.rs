//! NMEA file and stream source for AIS messages

use deku::reader::Reader;
use deku::DekuReader;

use crate::decode::nmea::{MessageAssembler, NmeaAisMessage, NmeaError};
use crate::prelude::Message;
use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Lines};
use std::path::Path;

/// Generic NMEA iterator that handles message assembly from any BufRead source
pub struct NmeaIterator<R: BufRead> {
    lines: Lines<R>,
    assembler: MessageAssembler,
}

impl<R: BufRead> NmeaIterator<R> {
    /// Create a new NMEA iterator from any BufRead source
    pub fn new(reader: R) -> Self {
        Self {
            lines: reader.lines(),
            assembler: MessageAssembler::new(),
        }
    }

    /// Get the next complete AIS message (handles multi-fragment assembly)
    pub fn next_complete_message(&mut self) -> Option<Result<Message, NmeaError>> {
        loop {
            // Read next line
            let line = match self.lines.next() {
                Some(Ok(line)) => line,
                Some(Err(_)) => return None, // IO error
                None => return None,         // EOF
            };

            let line = line.trim();

            // Parse NMEA message
            let nmea_msg = match NmeaAisMessage::parse(line) {
                Ok(msg) => msg,
                Err(e) => return Some(Err(e)),
            };

            // Add to assembler and check if we have a complete message
            match self.assembler.add_fragment(nmea_msg) {
                Ok(Some(complete_message)) => {
                    let cursor = Cursor::new(complete_message);
                    let mut reader = Reader::new(cursor);
                    return Some(Ok(Message::from_reader_with_ctx(&mut reader, ()).ok()?));
                }
                Ok(None) => continue, // Need more fragments
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

impl<R: BufRead> Iterator for NmeaIterator<R> {
    type Item = Result<Message, NmeaError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_complete_message()
    }
}

/// Convenience wrapper for file-based NMEA sources
pub struct NmeaFileSource {
    iterator: NmeaIterator<BufReader<File>>,
}

impl NmeaFileSource {
    /// Open a NMEA file
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Ok(Self {
            iterator: NmeaIterator::new(reader),
        })
    }
}

impl Iterator for NmeaFileSource {
    type Item = Result<Message, NmeaError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iterator.next()
    }
}

/// Create a NMEA iterator from any BufRead source
pub fn from_reader<R: BufRead>(reader: R) -> NmeaIterator<R> {
    NmeaIterator::new(reader)
}
