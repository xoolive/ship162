//! Print AIS messages from aisstream.io over the English Channel
//!
//! ```sh
//! AISSTREAM_API_KEY=... cargo run --example aisstream --features aisstream
//! ```

use rs162::sources::aisstream::{AisStreamReceiver, BoundingBox, Subscription};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("AISSTREAM_API_KEY")?;
    let channel = BoundingBox::new([48.5, -6.0], [52.0, 3.0]);
    let subscription = Subscription::new(api_key, vec![channel]);
    let mut receiver = AisStreamReceiver::connect(&subscription).await?;
    while let Some(message) = receiver.next().await {
        let message = message?;
        println!(
            "{:.3} {}",
            message.timestamp,
            serde_json::to_string(&message.message)?
        );
    }
    Ok(())
}
