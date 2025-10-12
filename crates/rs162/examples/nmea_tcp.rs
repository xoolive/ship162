use rs162::decode::ais::Message;
use rs162::decode::nmea::{MessageAssembler, NmeaAisMessage};
use serde_json::json;
use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader};
use std::net::TcpStream;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    /*
     * The Norwegian Coastal Administration offers real-time AIS data.
     *
     * This live feed can be accessed via TCP/IP without prior registration. The
     * AIS data is freely available under the Norwegian license for public data:
     *
     * https://www.kystverket.no/en/sea-transport-and-ports/ais/access-to-ais-data/
     *
     * Data can be read from a TCP/IP socket and is encoded according to IEC
     * 62320-1: IP: 153.44.253.27 Port: 5631
     */

    let server_address = if args.len() == 1 {
        // Default server from the comment
        "153.44.253.27:5631"
    } else if args.len() == 2 {
        &args[1]
    } else {
        eprintln!("Usage: {} [server:port]", args[0]);
        eprintln!(
            "If no server is
    specified, uses 153.44.253.27:5631"
        );
        std::process::exit(1);
    };

    eprintln!("Connecting to {}...", server_address);

    let stream = TcpStream::connect(server_address)?;
    let reader = BufReader::new(stream);

    let mut assembler = MessageAssembler::new();
    // Buffer to store the original NMEA sentences for multi-part messages
    let mut sentence_buffer: HashMap<String, Vec<(String, u64, u64)>> = HashMap::new();

    eprintln!("Connected! Listening for AIS messages...");

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        // Parse the timestamped format: \s:serial,c:timestamp*checksum\!NMEA_MESSAGE
        let (serial, timestamp, nmea_sentence) = match parse_timestamped_line(line) {
            Ok(parsed) => parsed,
            Err(e) => {
                eprintln!("Error parsing timestamped line '{}': {}", line, e);
                continue;
            }
        };

        // Parse NMEA sentence
        match NmeaAisMessage::parse(&nmea_sentence) {
            Ok(nmea_msg) => {
                if nmea_msg.is_complete() {
                    // Single fragment message, decode directly
                    match Message::from_nmea(&[&nmea_sentence]) {
                        Ok(message) => {
                            let output = json!({
                                "serial": serial,
                                "timestamp": timestamp,
                                "message": message
                            });
                            println!("{}", serde_json::to_string(&output)?);
                        }
                        Err(e) => {
                            eprintln!(
                                "Error decoding single-part message '{}': {}",
                                nmea_sentence, e
                            );
                        }
                    }
                } else {
                    // Multi-fragment message
                    let message_id = match &nmea_msg.message_id {
                        Some(id) => id.clone(),
                        None => {
                            eprintln!("Multi-part message missing message ID: {}", nmea_sentence);
                            continue;
                        }
                    };

                    // Store the original sentence with metadata
                    let sentences = sentence_buffer.entry(message_id.clone()).or_default();

                    // Ensure we have enough space for all fragments
                    if sentences.len() < nmea_msg.fragment_count as usize {
                        sentences.resize(nmea_msg.fragment_count as usize, (String::new(), 0, 0));
                    }

                    // Store the sentence at the correct index (fragment_number is 1-based)
                    let index = (nmea_msg.fragment_number - 1) as usize;
                    if index < sentences.len() {
                        sentences[index] = (nmea_sentence.clone(), serial, timestamp);
                    }

                    // Use assembler to check if message is complete
                    match assembler.add_fragment(nmea_msg) {
                        Ok(Some(_binary)) => {
                            // Message is complete, decode it using the buffered sentences
                            let complete_sentences = sentence_buffer.remove(&message_id).unwrap();

                            // Filter out empty strings and convert to &str
                            let sentence_refs: Vec<&str> = complete_sentences
                                .iter()
                                .filter(|(s, _, _)| !s.is_empty())
                                .map(|(s, _, _)| s.as_str())
                                .collect();

                            // Use metadata from the first fragment
                            let (first_serial, first_timestamp) = complete_sentences
                                .iter()
                                .find(|(s, _, _)| !s.is_empty())
                                .map(|(_, s, t)| (*s, *t))
                                .unwrap_or((0, 0));

                            match Message::from_nmea(&sentence_refs) {
                                Ok(message) => {
                                    let output = json!({
                                        "serial": first_serial,
                                        "timestamp": first_timestamp,
                                        "message": message
                                    });
                                    println!("{}", serde_json::to_string(&output)?);
                                }
                                Err(e) => {
                                    eprintln!("Error decoding multi-part message: {}", e);
                                }
                            }
                        }
                        Ok(None) => {
                            // Still waiting for more fragments
                        }
                        Err(e) => {
                            eprintln!("Error assembling multi-part message: {}", e);
                            // Clean up the sentence buffer for this message ID
                            sentence_buffer.remove(&message_id);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Error parsing NMEA line '{}': {}", nmea_sentence, e);
            }
        }
    }

    Ok(())
}

/// Parse a timestamped line in the format: \s:serial,c:timestamp*checksum\!NMEA_MESSAGE
fn parse_timestamped_line(line: &str) -> Result<(u64, u64, String), Box<dyn std::error::Error>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamped_line() {
        let line = "\\s:2573300,c:1760280176*06\\!BSVDM,1,1,,A,E>jVETw9RPh30a6h:a20W2aRb@J@?v:9Am49P10888t@2=L=N0,4*64";

        let (serial, timestamp, nmea) = parse_timestamped_line(line).unwrap();

        assert_eq!(serial, 2573300);
        assert_eq!(timestamp, 1760280176);
        assert_eq!(
            nmea,
            "!BSVDM,1,1,,A,E>jVETw9RPh30a6h:a20W2aRb@J@?v:9Am49P10888t@2=L=N0,4*64"
        );
    }

    #[test]
    fn test_parse_timestamped_line_different_format() {
        let line = "\\s:2573575,c:1760280176*02\\!BSVDM,1,1,,B,B3mc;V0008L=i<b9vO>RKwt5oP06,0*48";

        let (serial, timestamp, nmea) = parse_timestamped_line(line).unwrap();

        assert_eq!(serial, 2573575);
        assert_eq!(timestamp, 1760280176);
        assert_eq!(nmea, "!BSVDM,1,1,,B,B3mc;V0008L=i<b9vO>RKwt5oP06,0*48");
    }
}
