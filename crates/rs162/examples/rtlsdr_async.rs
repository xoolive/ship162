use futures::StreamExt; // for .next().await
use rs162::sources::rtlsdr::AsyncRtlSdrReceiver;
use rtl_sdr_rs::error::Result;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    let mut receiver = AsyncRtlSdrReceiver::new().await?;

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
