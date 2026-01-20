use desperado::Result;
use futures::StreamExt; // for .next().await
use rs162::{dsp::ais::AIS_SAMPLE_RATE_288K, sources::iq::AisAsyncIqSource};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    let mut receiver = AisAsyncIqSource::from_rtlsdr(0, AIS_SAMPLE_RATE_288K, None, false).await?;

    while let Some(msg_res) = receiver.next().await {
        match msg_res {
            Ok(msg) => {
                if let Some(ais_msg) = msg.decode() {
                    let output = json!({
                        "signal_level": msg.signal_level,
                        "timestamp": msg.timestamp,
                        "channel": msg.channel,
                        "message": ais_msg
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
    println!("Stream returned None, exiting loop"); // Debug log

    Ok(())
}
