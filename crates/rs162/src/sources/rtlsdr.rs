//! RTL-SDR source for AIS reception using rtl-sdr-rs crate
//!
//! This module provides real-time AIS signal demodulation from RTL-SDR dongles.

use crate::dsp::ais::{AisDemodulatedMessage, AIS_SAMPLE_RATE_288K};
use crate::sources::iq::{AisAsyncIqSource, AisIqSource};

use desperado::rtlsdr::RtlSdrConfig;
use desperado::{IqAsyncSource, IqSource};
use futures::Stream;
use rtl_sdr_rs::error::RtlsdrError;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

/// RTL-SDR AIS receiver
/// Example usage
///
/// ```no_run
/// use rs162::sources::rtlsdr::RtlSdrReceiver;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut receiver = RtlSdrReceiver::new()?;
///
///     for item in receiver {
///         match item {
///             Ok(msg) => {
///                 println!("AIS Message on channel {}: {} bits", msg.channel, msg.bits.len());
///             }
///             Err(e) => {
///                 eprintln!("Receiver error: {e}");
///             }
///         }
///     }
///
///     Ok(())
/// }
/// ```
pub struct RtlSdrReceiver {
    source: AisIqSource,
}

impl RtlSdrReceiver {
    /// Create a new RTL-SDR receiver with default configuration
    pub fn new() -> Result<Self, RtlsdrError> {
        Self::with_config(RtlSdrConfig {
            center_freq: 162_000_000,
            sample_rate: AIS_SAMPLE_RATE_288K,
            device_index: 0,
            gain: None,
        })
    }

    /// Create a new RTL-SDR receiver with custom configuration
    pub fn with_config(config: RtlSdrConfig) -> Result<Self, RtlsdrError> {
        let source = IqSource::from_rtlsdr(
            config.device_index,
            config.sample_rate,
            AIS_SAMPLE_RATE_288K,
            config.gain,
        )?;
        Ok(Self {
            source: AisIqSource::new(source, config.sample_rate),
        })
    }
}

impl Iterator for RtlSdrReceiver {
    type Item = Result<AisDemodulatedMessage, io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.source.next_message()
    }
}

pub struct AsyncRtlSdrReceiver {
    source: AisAsyncIqSource,
}

impl AsyncRtlSdrReceiver {
    pub async fn new() -> Result<Self, RtlsdrError> {
        Self::with_config(RtlSdrConfig {
            center_freq: 162_000_000,
            sample_rate: AIS_SAMPLE_RATE_288K,
            device_index: 0,
            gain: None,
        })
        .await
    }

    /// Create a new RTL-SDR receiver with custom configuration
    pub async fn with_config(config: RtlSdrConfig) -> Result<Self, RtlsdrError> {
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage, std::io::Error>>(32);
        let source = IqAsyncSource::from_rtlsdr(
            config.device_index,
            162_000_000,
            AIS_SAMPLE_RATE_288K,
            config.gain,
        )
        .await?;
        let handle = super::iq::spawn_demodulator_task(source, tx, AIS_SAMPLE_RATE_288K);

        Ok(Self {
            source: AisAsyncIqSource::new(handle, rx),
        })
    }
}

impl Stream for AsyncRtlSdrReceiver {
    type Item = Result<AisDemodulatedMessage, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.source).poll_next(cx)
    }
}
