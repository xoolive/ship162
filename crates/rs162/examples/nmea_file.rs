use rs162::{decode::mmsi::MmsiInfo, sources::nmea::NmeaFileSource};
use serde_json::json;
use std::env;

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

    for result in NmeaFileSource::new(filename)? {
        match result {
            Ok(message) => {
                let output = json!({
                    "message": message,
                    "mmsi_info": MmsiInfo::from_message(&message).ok()
                });
                let json = serde_json::to_string(&output)?;
                println!("{}", json);
            }
            Err(e) => eprintln!("Parse error: {}", e),
        }
    }

    Ok(())
}
