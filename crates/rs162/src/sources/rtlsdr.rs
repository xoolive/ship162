//! RTL-SDR source for AIS reception using rtl-sdr-rs crate
//!
//! This module provides real-time AIS signal demodulation from RTL-SDR dongles.

use crate::dsp::ais::{AisDemodulatedMessage, AIS_SAMPLE_RATE_288K};
use crate::sources::iq::{AsyncIqSource, IqFormat, IqSource};
use futures::Stream;
use rtl_sdr_rs::error::RtlsdrError;
use rtl_sdr_rs::{RtlSdr, TunerGain, DEFAULT_BUF_LENGTH};
use std::io::{self, Read};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};

/// Configuration for RTL-SDR AIS reception
#[derive(Debug, Clone)]
pub struct RtlSdrConfig {
    /// Device index (0 for first device)
    pub device_index: usize,
    /// Center frequency in Hz (e.g., 162_000_000 for AIS)
    pub frequency: u32,
    /// Sample rate in Hz (must be 288000 = AIS_SAMPLE_RATE_288K for current demodulator)
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

struct RtlSdrReader {
    rtl: RtlSdr,
    buf: Vec<u8>,
    pos: usize,
    end: usize,
}

impl RtlSdrReader {
    fn new(config: &RtlSdrConfig) -> Result<Self, RtlsdrError> {
        let mut rtl = RtlSdr::open_with_index(config.device_index)?;
        rtl.set_sample_rate(config.sample_rate)?;
        rtl.set_center_freq(config.frequency)?;
        match config.gain {
            Some(gain) => rtl.set_tuner_gain(TunerGain::Manual(gain))?,
            None => rtl.set_tuner_gain(TunerGain::Auto)?,
        };
        let _ = rtl.set_bias_tee(false);
        rtl.reset_buffer()?;
        Ok(Self {
            rtl,
            buf: vec![0u8; DEFAULT_BUF_LENGTH],
            pos: 0,
            end: 0,
        })
    }
}

impl Read for RtlSdrReader {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        // NOTE: we do not use BufReader here to ensure device reads happen in large chunks.
        if self.pos == self.end {
            match self.rtl.read_sync(&mut self.buf[..]) {
                Ok(n) => {
                    self.pos = 0;
                    self.end = n;
                    if n == 0 {
                        return Ok(0);
                    }
                }
                Err(err) => {
                    // NOTE: `RtlsdrError: Into<Box<(dyn StdError + std::marker::Send + Sync + 'static)>>` is not satisfied
                    return Err(io::Error::new(io::ErrorKind::Other, err.to_string()));
                }
            }
        }

        let available = self.end - self.pos;
        let to_copy = available.min(dst.len());
        dst[..to_copy].copy_from_slice(&self.buf[self.pos..self.pos + to_copy]);
        self.pos += to_copy;
        Ok(to_copy)
    }
}

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
    inner: IqSource<RtlSdrReader>,
}

impl RtlSdrReceiver {
    /// Create a new RTL-SDR receiver with default configuration
    pub fn new() -> Result<Self, RtlsdrError> {
        Self::with_config(RtlSdrConfig::default())
    }

    /// Create a new RTL-SDR receiver with custom configuration
    pub fn with_config(config: RtlSdrConfig) -> Result<Self, RtlsdrError> {
        let reader = RtlSdrReader::new(&config)?;
        let inner = IqSource::new(reader, config.sample_rate, IqFormat::Cu8);
        Ok(Self { inner })
    }
}

impl Iterator for RtlSdrReceiver {
    type Item = Result<AisDemodulatedMessage, io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// Async reader over an internal blocking thread and mpsc channel.
/// Maintains an eof flag to avoid repeated polls after end-of-stream.
struct AsyncRtlSdrReader {
    rx: tokio::sync::mpsc::Receiver<Result<Vec<u8>, io::Error>>,
    buf: Vec<u8>,
    pos: usize,
    eof: bool,
    _handle: std::thread::JoinHandle<()>,
}

impl AsyncRtlSdrReader {
    fn new(config: &RtlSdrConfig) -> Result<Self, RtlsdrError> {
        let (tx_data, rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, io::Error>>(32);
        let (tx_init, rx_init) = std::sync::mpsc::channel::<Result<(), RtlsdrError>>();
        let cfg = config.clone();

        let handle = std::thread::spawn(move || {
            let init_res = (|| -> Result<RtlSdr, RtlsdrError> {
                let mut rtl = RtlSdr::open_with_index(cfg.device_index)?;
                rtl.set_sample_rate(cfg.sample_rate)?;
                rtl.set_center_freq(cfg.frequency)?;
                match cfg.gain {
                    Some(gain) => rtl.set_tuner_gain(TunerGain::Manual(gain))?,
                    None => rtl.set_tuner_gain(TunerGain::Auto)?,
                };
                let _ = rtl.set_bias_tee(false);
                rtl.reset_buffer()?;
                Ok(rtl)
            })();

            match init_res {
                Ok(rtl) => {
                    let _ = tx_init.send(Ok(()));
                    let mut buffer = vec![0u8; DEFAULT_BUF_LENGTH];
                    loop {
                        match rtl.read_sync(&mut buffer) {
                            Ok(bytes_read) => {
                                if bytes_read == 0 {
                                    let _ = tx_data.blocking_send(Ok(Vec::new()));
                                    return;
                                }
                                let chunk = buffer[..bytes_read].to_vec();
                                if tx_data.blocking_send(Ok(chunk)).is_err() {
                                    return;
                                }
                            }
                            Err(e) => {
                                let _ = tx_data.blocking_send(Err(io::Error::new(
                                    io::ErrorKind::Other,
                                    e.to_string(),
                                )));
                                return;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx_init.send(Err(e));
                }
            }
        });

        match rx_init.recv() {
            Ok(Ok(())) => Ok(Self {
                rx,
                buf: Vec::new(),
                pos: 0,
                eof: false,
                _handle: handle,
            }),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(RtlsdrError::RtlsdrErr(
                "Async RTL-SDR init failed".to_string(),
            )),
        }
    }
}

impl AsyncRead for AsyncRtlSdrReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        dst: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.eof {
            return Poll::Ready(Ok(()));
        }

        if self.pos < self.buf.len() {
            let remaining = self.buf.len() - self.pos;
            let to_copy = remaining.min(dst.remaining());
            dst.put_slice(&self.buf[self.pos..self.pos + to_copy]);
            self.pos += to_copy;
            return Poll::Ready(Ok(()));
        }

        match self.rx.poll_recv(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                if chunk.is_empty() {
                    self.eof = true; // by device
                    return Poll::Ready(Ok(()));
                }
                self.buf = chunk;
                self.pos = 0;
                let to_copy = self.buf.len().min(dst.remaining());
                dst.put_slice(&self.buf[..to_copy]);
                self.pos = to_copy;
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Err(e)),
            Poll::Ready(None) => {
                self.eof = true; // channel closed
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct AsyncRtlSdrReceiver {
    inner: AsyncIqSource<AsyncRtlSdrReader>,
}

impl AsyncRtlSdrReceiver {
    pub async fn new() -> Result<Self, RtlsdrError> {
        Self::with_config(RtlSdrConfig::default()).await
    }

    pub async fn with_config(config: RtlSdrConfig) -> Result<Self, RtlsdrError> {
        let reader = AsyncRtlSdrReader::new(&config)?;
        let inner = AsyncIqSource::new(reader, config.sample_rate, IqFormat::Cu8);
        Ok(Self { inner })
    }
}

impl Stream for AsyncRtlSdrReceiver {
    type Item = Result<AisDemodulatedMessage, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
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
