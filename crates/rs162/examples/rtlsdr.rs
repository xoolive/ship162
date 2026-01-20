use desperado::Result;
use rs162::{decode::mmsi::MmsiInfo, dsp::ais::AIS_SAMPLE_RATE_288K, sources::iq::AisIqSource};
use serde_json::json;

fn main() -> Result<()> {
    let receiver = AisIqSource::from_rtlsdr(0, AIS_SAMPLE_RATE_288K, None, false)?;

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
