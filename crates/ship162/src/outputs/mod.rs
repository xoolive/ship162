pub mod tcp;
pub mod udp;
pub mod websocket;

use std::sync::atomic::{AtomicU64, Ordering};

static SERIAL_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Format a NMEA sentence with Norwegian coast guard timestamp tag block:
/// `\s:SERIAL,c:TIMESTAMP*CHECKSUM\!AIVDM,...\r\n`
pub fn format_timestamped_nmea(timestamp: f64, nmea_sentence: &str) -> String {
    let serial = SERIAL_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tag_block_data = format!("s:{},c:{:.0}", serial, timestamp);
    let checksum = tag_block_data.bytes().fold(0u8, |acc, b| acc ^ b);
    format!(
        "\\{}*{:02X}\\{}\r\n",
        tag_block_data, checksum, nmea_sentence
    )
}
