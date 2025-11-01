//! RTL-SDR source for AIS reception using rtl-sdr-rs crate
//!
//! This module provides real-time AIS signal demodulation from RTL-SDR dongles.

use crate::dsp::ais::{AisDemodulatedMessage, AisDemodulator, AIS_SAMPLE_RATE_288K};
use crate::dsp::convert_samples_cu8;
use futures::Stream;
use num_complex::Complex;
use rtl_sdr_rs::{RtlSdr, TunerGain, DEFAULT_BUF_LENGTH};
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::mpsc as mpsc_sync;
use std::task::{Context, Poll};
use std::thread;
use tokio::sync::mpsc as mpsc_tokio;
use tracing::error;

/// Configuration for RTL-SDR AIS reception
#[derive(Debug, Clone)]
pub struct RtlSdrConfig {
    /// Device index (0 for first device)
    pub device_index: usize,
    /// Center frequency in Hz (e.g., 162_000_000 for AIS)
    pub frequency: u32,
    /// Sample rate in Hz (must be 96000 for current demodulator)
    pub sample_rate: u32,
    /// Tuner gain (None for AGC, Some(gain) for manual)
    pub gain: Option<i32>,
}

impl Default for RtlSdrConfig {
    fn default() -> Self {
        Self {
            device_index: 0,
            frequency: 162_000_000,            // AIS frequency
            sample_rate: AIS_SAMPLE_RATE_288K, // Required by demodulator
            gain: Some(496),                   // Use max gain
        }
    }
}
impl RtlSdrConfig {
    pub fn with_index(device_index: usize) -> Self {
        Self {
            device_index,
            ..Self::default()
        }
    }
}

/// RTL-SDR AIS receiver
/// Example usage
///
/// ```no_run
/// use rs162::sources::rtlsdr::{RtlSdrReceiver, RtlSdrConfig};
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut receiver = RtlSdrReceiver::new()?;
///
///     receiver.receive(|message| {
///         println!("AIS Message on channel {}: {} bytes",
///                  message.channel, message.bits.len());
///     })?;
///     for msg in receiver {
///         if let Some(ais_msg) = msg.decode() {
///             let output = json!({
///                 "signal_level": msg.signal_level,
///                 "timestamp": msg.timestamp,
///                 "channel": msg.channel,
///                 "message": ais_msg,
///                 "mmsi_info": MmsiInfo::from_message(&ais_msg).ok()
///             });
///             println!("{}", serde_json::to_string(&output).unwrap());
///         }
///     }
///     Ok(())
/// }
/// ```
pub struct RtlSdrReceiver {
    demodulator: AisDemodulator,
    sample_receiver: mpsc_sync::Receiver<Vec<Complex<f32>>>,
    message_buffer: VecDeque<AisDemodulatedMessage>,
    _handle: thread::JoinHandle<()>, // Keep handle to prevent thread from being dropped
}

impl RtlSdrReceiver {
    /// Create a new RTL-SDR receiver with default configuration
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_config(RtlSdrConfig::default())
    }

    /// Create a new RTL-SDR receiver with custom configuration
    pub fn with_config(config: RtlSdrConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let demodulator = AisDemodulator::new(config.sample_rate);
        let (tx, rx) = mpsc_sync::channel();

        // Spawn RTL-SDR reading thread
        let handle = thread::spawn(move || {
            let mut rtl = RtlSdr::open(config.device_index).expect("No such device");

            rtl.set_sample_rate(config.sample_rate)
                .expect("Failed to set sample rate");
            rtl.set_center_freq(config.frequency)
                .expect("Failed to set frequency");
            match config.gain {
                Some(gain) => rtl
                    .set_tuner_gain(TunerGain::Manual(gain))
                    .expect("Failed to set gain"),
                None => rtl
                    .set_tuner_gain(TunerGain::Auto)
                    .expect("Failed to set automatic gain"),
            };

            // Reset bias tee (if supported)
            let _ = rtl.set_bias_tee(false);

            // Create sample buffer
            let mut buffer = Box::new([0u8; DEFAULT_BUF_LENGTH]);

            rtl.reset_buffer().expect("Unable to reset buffer");

            loop {
                if let Ok(bytes_read) = rtl.read_sync(&mut *buffer) {
                    // Convert samples and update stats
                    let samples = convert_samples_cu8(&buffer[..bytes_read]);

                    if tx.send(samples).is_err() {
                        return; // Stop reading if receiver is dropped
                    }
                }
            }
        });

        Ok(Self {
            demodulator,
            sample_receiver: rx,
            message_buffer: VecDeque::new(),
            _handle: handle,
        })
    }

    /// Get the next AIS message
    fn next_message(&mut self) -> Option<AisDemodulatedMessage> {
        // Return buffered message if available
        if let Some(msg) = self.message_buffer.pop_front() {
            return Some(msg);
        }

        // Process incoming samples
        loop {
            match self.sample_receiver.try_recv() {
                Ok(samples) => {
                    let messages = self.demodulator.demodulate(&samples);

                    // Add all messages to buffer
                    for msg in messages {
                        self.message_buffer.push_back(msg);
                    }

                    // Return first message if any
                    if let Some(msg) = self.message_buffer.pop_front() {
                        return Some(msg);
                    }
                    // Continue processing if no messages were found
                }
                Err(mpsc_sync::TryRecvError::Empty) => {
                    // No new samples available yet, try again
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    continue;
                }
                Err(mpsc_sync::TryRecvError::Disconnected) => {
                    // RTL-SDR thread has stopped
                    return None;
                }
            }
        }
    }
}

impl Iterator for RtlSdrReceiver {
    type Item = AisDemodulatedMessage;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_message()
    }
}

pub struct AsyncRtlSdrReceiver {
    demodulator: AisDemodulator,
    sample_receiver: mpsc_tokio::Receiver<Vec<Complex<f32>>>,
    message_buffer: VecDeque<AisDemodulatedMessage>,
    _handle: std::thread::JoinHandle<()>, // Keep handle to prevent thread from being dropped
}

impl AsyncRtlSdrReceiver {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Self::with_config(RtlSdrConfig::default()).await
    }

    pub async fn with_config(config: RtlSdrConfig) -> Result<Self, Box<dyn std::error::Error>> {
        let demodulator = AisDemodulator::new(config.sample_rate);
        let (tx, rx) = mpsc_tokio::channel::<Vec<Complex<f32>>>(32);

        // Spawn blocking SDR reading thread
        let handle = std::thread::spawn(move || {
            let mut rtl = RtlSdr::open(config.device_index).expect("No such device");
            rtl.set_sample_rate(config.sample_rate)
                .expect("Failed to set sample rate");
            rtl.set_center_freq(config.frequency)
                .expect("Failed to set frequency");
            match config.gain {
                Some(gain) => rtl
                    .set_tuner_gain(TunerGain::Manual(gain))
                    .expect("Failed to set gain"),
                None => rtl
                    .set_tuner_gain(TunerGain::Auto)
                    .expect("Failed to set automatic gain"),
            };

            let mut buffer = Box::new([0u8; DEFAULT_BUF_LENGTH]);
            rtl.reset_buffer().expect("Unable to reset buffer");

            loop {
                match rtl.read_sync(&mut *buffer) {
                    Ok(bytes_read) => {
                        let samples = convert_samples_cu8(&buffer[..bytes_read]);
                        if tx.blocking_send(samples).is_err() {
                            println!("Async RTL-SDR receiver channel closed");
                            return; // Stop reading if receiver is dropped
                        }
                    }
                    Err(err) => {
                        error!("Error reading from RTL-SDR device: {:?}", err);
                    }
                }
            }
        });

        Ok(Self {
            demodulator,
            sample_receiver: rx,
            message_buffer: VecDeque::new(),
            _handle: handle,
        })
    }
}

impl Stream for AsyncRtlSdrReceiver {
    type Item = AisDemodulatedMessage;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(msg) = self.message_buffer.pop_front() {
            return Poll::Ready(Some(msg));
        }

        match self.sample_receiver.poll_recv(cx) {
            Poll::Ready(Some(samples)) => {
                let messages = self.demodulator.demodulate(&samples);
                self.message_buffer.extend(messages);
                match self.message_buffer.pop_front() {
                    Some(msg) => Poll::Ready(Some(msg)),
                    None => Poll::Pending,
                }
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtlsdr_config_default() {
        let config = RtlSdrConfig::default();
        assert_eq!(config.frequency, 162_000_000);
        assert_eq!(config.sample_rate, 288_000);
        assert_eq!(config.device_index, 0);
    }
}
