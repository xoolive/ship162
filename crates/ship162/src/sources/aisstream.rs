//! aisstream.io source, a websocket relay of decoded AIS messages

use anyhow::{anyhow, Context, Result};
use rs162::sources::aisstream::{AisStreamReceiver, Subscription};
use tokio::sync::mpsc::Sender;
use tracing::info;

use crate::{
    sources::{AisStreamSettings, TimedMessage, AISSTREAM_API_KEY},
    status::{ReconnectingSource, SourceRuntime, SourceStatus},
};

pub struct AisStreamSource {
    tx: Sender<TimedMessage>,
    runtime: SourceRuntime,
    settings: AisStreamSettings,
}

impl AisStreamSource {
    pub fn new(
        tx: Sender<TimedMessage>,
        runtime: SourceRuntime,
        settings: AisStreamSettings,
    ) -> Self {
        Self {
            tx,
            runtime,
            settings,
        }
    }
}

/// The API key: `key`, or else the contents of `key_file`, or else `AISSTREAM_API_KEY`
///
/// The key file and the environment are read again on every connection, so
/// that the key can be rotated without restarting.
pub fn api_key(settings: &AisStreamSettings) -> Result<String> {
    let nonempty = |key: &str| Some(key.trim().to_string()).filter(|key| !key.is_empty());
    match (&settings.key, &settings.key_file) {
        (Some(_), Some(_)) => Err(anyhow!(
            "aisstream.io: set either `key` or `key_file`, not both"
        )),
        (Some(key), None) => nonempty(key).ok_or_else(|| anyhow!("aisstream.io `key` is empty")),
        (None, Some(path)) => {
            let path = crate::expanduser(std::path::PathBuf::from(path));
            let key = std::fs::read_to_string(&path)
                .with_context(|| format!("cannot read aisstream.io key file {}", path.display()))?;
            nonempty(&key)
                .ok_or_else(|| anyhow!("aisstream.io key file {} is empty", path.display()))
        }
        (None, None) => std::env::var(AISSTREAM_API_KEY)
            .ok()
            .and_then(|key| nonempty(&key))
            .ok_or_else(|| {
                anyhow!("no aisstream.io API key: set `key`, `key_file` or {AISSTREAM_API_KEY}")
            }),
    }
}

impl ReconnectingSource for AisStreamSource {
    fn runtime(&self) -> &SourceRuntime {
        &self.runtime
    }

    async fn connect_once(&self) -> Result<()> {
        let mut subscription =
            Subscription::new(api_key(&self.settings)?, self.settings.bbox.to_vec());
        subscription.mmsi = self.settings.mmsi.clone();
        subscription.message_types = self.settings.message_types.clone();

        let mut receiver =
            AisStreamReceiver::connect_to(self.settings.url(), &subscription).await?;
        self.runtime.report(SourceStatus::Connected);
        let mut compression_reported = false;
        while let Some(message) = receiver.next().await {
            let message = message?;
            if !compression_reported {
                if let Some(compression) = receiver.compression_enabled() {
                    if !compression {
                        info!("aisstream.io did not enable compression; bandwidth may be limited");
                    }
                    compression_reported = true;
                }
            }
            self.tx
                // Decoded messages, without raw bits: soft deduplication only
                .send(TimedMessage::received(
                    &self.runtime,
                    message.timestamp,
                    message.message,
                    None,
                ))
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_api_key_from_config_or_file() {
        let settings = AisStreamSettings {
            key: Some(" abc123 ".into()),
            ..Default::default()
        };
        assert_eq!(api_key(&settings).unwrap(), "abc123");
        assert!(!format!("{settings:?}").contains("abc123"));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("key");
        std::fs::write(&path, "  def456\n").unwrap();
        let mut settings = AisStreamSettings {
            key_file: Some(path.to_string_lossy().into_owned()),
            ..Default::default()
        };
        assert_eq!(api_key(&settings).unwrap(), "def456");

        std::fs::write(&path, "\n").unwrap();
        assert!(api_key(&settings)
            .unwrap_err()
            .to_string()
            .contains("empty"));

        settings.key = Some("abc123".into());
        assert!(api_key(&settings)
            .unwrap_err()
            .to_string()
            .contains("not both"));

        if std::env::var(AISSTREAM_API_KEY).is_err() {
            let error = api_key(&AisStreamSettings::default()).unwrap_err();
            assert!(error.to_string().contains(AISSTREAM_API_KEY));
        }
    }
}
