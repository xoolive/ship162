use desperado::IqFormat;
use rs162::{decode::mmsi::MmsiInfo, dsp::ais::AIS_SAMPLE_RATE_288K, sources::AisIqSource};
use serde_json::json;
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:1234".to_string());

    let host = addr.split(':').next().unwrap_or(&addr);
    let port = if addr.contains(':') {
        0
    } else {
        addr.split(':').nth(1).unwrap_or("1234").parse()?
    };

    let receiver = AisIqSource::from_tcp(host, port, AIS_SAMPLE_RATE_288K, IqFormat::Cu8)?;

    for result in receiver {
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
                    println!("{}", serde_json::to_string(&output).unwrap());
                }
            }
            Err(e) => {
                eprintln!("stream error: {e}.\nhelp: try `rtl_tcp -a 127.0.0.1 -p 1234 -f 162000000 -s 288000 -g 49.6`");
                break;
            }
        }
    }

    Ok(())
}
