use rs162::{decode::mmsi::MmsiInfo, sources::rtlsdr::RtlSdrReceiver};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let receiver = RtlSdrReceiver::new()?;

    for msg in receiver {
        if let Some(ais_msg) = msg.decode() {
            let output = json!({
                "signal_level": msg.signal_level,
                "timestamp": msg.timestamp,
                "channel": msg.channel,
                "message": ais_msg,
                "mmsi_info": MmsiInfo::from_message(&ais_msg).ok()
            });
            println!("{}", serde_json::to_string(&output).unwrap());
        }
    }

    Ok(())
}
