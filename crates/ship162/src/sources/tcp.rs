use anyhow::Result;
use rs162::sources::nmea_ts::AsyncTimestampedNmeaTcpSource;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::state::AppState;

pub struct TcpSource {
    host: String,
    port: u16,
    state: Arc<Mutex<AppState>>,
}

impl TcpSource {
    pub fn new(host: String, port: u16, state: Arc<Mutex<AppState>>) -> Self {
        Self { host, port, state }
    }

    pub async fn run(&self) -> Result<()> {
        loop {
            match self.connect_and_process().await {
                Ok(_) => {
                    eprintln!("Connection closed, reconnecting...");
                }
                Err(e) => {
                    eprintln!("Error: {}, reconnecting in 5 seconds...", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn connect_and_process(&self) -> Result<()> {
        let mut source =
            AsyncTimestampedNmeaTcpSource::new(&format!("{}:{}", self.host, self.port)).await?;

        info!("Connected to {}:{}", self.host, self.port);

        while let Some(line) = source.next().await {
            if let Ok(sentence) = line {
                let state = self.state.lock().await;
                super::process_sentence(state, sentence).await;
            }
        }

        Ok(())
    }
}
