use rs162::decode::ais::Message;
use rs162::decode::nmea::{MessageAssembler, NmeaAisMessage};
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    let filename = if args.len() == 1 {
        // Default to the data file in the same crate
        "crates/rs162/data/ais_nmea.txt"
    } else if args.len() == 2 {
        &args[1]
    } else {
        eprintln!("Usage: {} [nmea_file]", args[0]);
        eprintln!("If no file is specified, uses data/ais_nmea.txt");
        std::process::exit(1);
    };

    let file = File::open(filename)?;
    let reader = BufReader::new(file);

    let mut assembler = MessageAssembler::new();
    // Buffer to store the original NMEA sentences for multi-part messages
    let mut sentence_buffer: HashMap<String, Vec<String>> = HashMap::new();

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();

        if line.is_empty() || !line.starts_with('!') {
            continue;
        }

        // Parse NMEA sentence
        match NmeaAisMessage::parse(line) {
            Ok(nmea_msg) => {
                if nmea_msg.is_complete() {
                    // Single fragment message, decode directly
                    match Message::from_nmea(&[line]) {
                        Ok(message) => {
                            let json = serde_json::to_string(&message)?;
                            println!("{}", json);
                        }
                        Err(e) => {
                            eprintln!("Error decoding single-part message '{}': {}", line, e);
                        }
                    }
                } else {
                    // Multi-fragment message
                    let message_id = match &nmea_msg.message_id {
                        Some(id) => id.clone(),
                        None => {
                            eprintln!("Multi-part message missing message ID: {}", line);
                            continue;
                        }
                    };

                    // Store the original sentence
                    let sentences = sentence_buffer.entry(message_id.clone()).or_default();

                    // Ensure we have enough space for all fragments
                    if sentences.len() < nmea_msg.fragment_count as usize {
                        sentences.resize(nmea_msg.fragment_count as usize, String::new());
                    }

                    // Store the sentence at the correct index (fragment_number is 1-based)
                    let index = (nmea_msg.fragment_number - 1) as usize;
                    if index < sentences.len() {
                        sentences[index] = line.to_string();
                    }

                    // Use assembler to check if message is complete
                    match assembler.add_fragment(nmea_msg) {
                        Ok(Some(_binary)) => {
                            // Message is complete, decode it using the buffered sentences
                            let complete_sentences = sentence_buffer.remove(&message_id).unwrap();

                            // Filter out empty strings and convert to &str
                            let sentence_refs: Vec<&str> = complete_sentences
                                .iter()
                                .filter(|s| !s.is_empty())
                                .map(|s| s.as_str())
                                .collect();

                            match Message::from_nmea(&sentence_refs) {
                                Ok(message) => {
                                    let json = serde_json::to_string(&message)?;
                                    println!("{}", json);
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
                eprintln!("Error parsing NMEA line '{}': {}", line, e);
            }
        }
    }

    Ok(())
}
