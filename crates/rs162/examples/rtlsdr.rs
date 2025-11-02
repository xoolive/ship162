use rs162::{decode::mmsi::MmsiInfo, sources::rtlsdr::RtlSdrReceiver};
use rtl_sdr_rs::error::Result;
use serde_json::json;

fn main() -> Result<()> {
    let receiver = RtlSdrReceiver::new()?;

    for msg_res in receiver {
        match msg_res {
            Ok(msg) => {
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
            Err(e) => {
                eprintln!("I/O error reading samples: {e}");
                continue;
            }
        }
    }

    Ok(())
}
