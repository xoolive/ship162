use futures::StreamExt; // for .next().await
use rs162::sources::rtlsdr::AsyncRtlSdrReceiver;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut receiver = AsyncRtlSdrReceiver::new().await?;

    while let Some(msg) = receiver.next().await {
        if let Some(ais_msg) = msg.decode() {
            let output = json!({
                "signal_level": msg.signal_level,
                "timestamp": msg.timestamp,
                "channel": msg.channel,
                "message": ais_msg
            });
            println!("{}", serde_json::to_string(&output)?);
        }
    }
    println!("Stream returned None, exiting loop"); // Debug log

    Ok(())
}
