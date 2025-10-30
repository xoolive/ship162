use rs162::{
    decode::mmsi::MmsiInfo,
    sources::iqfile::{IqFileFormat, IqFileSource},
};
use serde_json::json;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    let filename = if args.len() == 1 {
        // Default to the data file in the same crate
        "crates/rs162/data/ais_96k.bin"
    } else if args.len() == 2 {
        &args[1]
    } else {
        eprintln!("Usage: {} [nmea_file]", args[0]);
        eprintln!("If no file is specified, uses data/ais_96k.bin");
        std::process::exit(1);
    };

    for result in IqFileSource::new(filename, 96000, IqFileFormat::Cu8)? {
        match result {
            Ok(msg) => {
                if let Some(ais_msg) = msg.decode() {
                    let output = json!({
                        "signal_level": msg.signal_level,
                        "timestamp": msg.timestamp,
                        "channel": msg.channel,
                        "message": ais_msg,
                        "mmsi_info": MmsiInfo::from_message(&ais_msg).ok()
                    });
                    println!("{}", serde_json::to_string(&output)?);
                }
            }
            Err(e) => eprintln!("Read error: {}", e),
        }
    }

    Ok(())
}
