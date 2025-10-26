use rs162::sources::TimestampedNmeaTcpSource;
use serde_json::json;
use std::env;

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
        eprintln!("If no server is specified, uses 153.44.253.27:5631");
        std::process::exit(1);
    };

    eprintln!("Connecting to {}...", server_address);

    let source = TimestampedNmeaTcpSource::new(&args[1])?;

    for result in source {
        match result {
            Ok(timestamped_msg) => {
                if let Some(ais_msg) = timestamped_msg.decode() {
                    let output = json!({
                        "timestamp": timestamped_msg.timestamp,
                        "serial": timestamped_msg.serial,
                        "message": ais_msg
                    });
                    println!("{}", serde_json::to_string(&output)?);
                }
            }
            Err(e) => eprintln!("Parse error: {}", e),
        }
    }
    Ok(())
}
