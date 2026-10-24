//! Generic I/Q source for reading complex samples from any `Read` source.
//!
//! Formats with 16/32-bit samples are little-endian (Cs16, Cf32).

use desperado::{IqAsyncSource, IqFormat, IqSource, Result};
use futures::stream::StreamExt;
use futures::Stream as FuturesStream;
use std::collections::VecDeque;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

#[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "hackrf"))]
use desperado::DeviceConfig;
#[cfg(any(
    feature = "rtlsdr",
    feature = "soapy",
    feature = "airspy",
    feature = "hackrf"
))]
use desperado::Gain;

#[cfg(feature = "airspy")]
use desperado::airspy::{
    AirspyConfig, AirspyGainMode, AsyncAirspySdrReader, DeviceSelector as AirspyDeviceSelector,
};
#[cfg(feature = "hackrf")]
use desperado::hackrf::HackRfConfig;
#[cfg(feature = "rtlsdr")]
use desperado::rtlsdr::{DeviceSelector, RtlSdrConfig};
#[cfg(feature = "soapy")]
use desperado::soapy::SoapyConfig;

use crate::dsp::ais::{AisDemodulatedMessage, AisDemodulator};
use crate::dsp::sample_rate::SampleRateAdapter;

const DEFAULT_CHUNK_SAMPLES: usize = 8192;
const AIS_FREQ: u32 = 162_000_000;

// Recommended gain settings for AIS reception (max gain is best for 162 MHz)
#[cfg(feature = "rtlsdr")]
const RTLSDR_GAIN: f64 = 49.6; // Maximum gain for RTL-SDR
#[cfg(feature = "soapy")]
const SOAPY_GAIN: f64 = 49.6; // Maximum gain, same as RTL-SDR

pub struct AisIqSource {
    source: IqSource,
    demodulator: AisDemodulator,
    adapter: SampleRateAdapter,
    message_buffer: VecDeque<AisDemodulatedMessage>,
}

impl AisIqSource {
    pub fn new(source: IqSource, sample_rate: u32) -> Self {
        Self {
            source,
            demodulator: AisDemodulator::new(),
            adapter: SampleRateAdapter::new(sample_rate),
            message_buffer: VecDeque::new(),
        }
    }

    pub fn from_file<P: AsRef<Path>>(path: P, sample_rate: u32, format: IqFormat) -> Result<Self> {
        let source =
            IqSource::from_file(path, AIS_FREQ, sample_rate, DEFAULT_CHUNK_SAMPLES, format)?;
        Ok(Self::new(source, sample_rate))
    }

    pub fn from_tcp(addr: &str, port: u16, sample_rate: u32, format: IqFormat) -> Result<Self> {
        let source = IqSource::from_tcp(
            addr,
            port,
            AIS_FREQ,
            sample_rate,
            DEFAULT_CHUNK_SAMPLES,
            format,
        )?;
        Ok(Self::new(source, sample_rate))
    }

    /// Create an AIS IQ source from a DeviceConfig for SDR devices
    #[cfg(any(feature = "rtlsdr", feature = "soapy"))]
    pub fn from_device_config(config: DeviceConfig, sample_rate: u32) -> Result<Self> {
        let source = IqSource::from_device_config(config)?;
        Ok(Self::new(source, sample_rate))
    }

    #[cfg(feature = "rtlsdr")]
    pub fn from_rtlsdr(
        device: DeviceSelector,
        sample_rate: u32,
        gain: Option<f64>,
        bias_tee: bool,
    ) -> Result<Self> {
        let rtlsdr_config = RtlSdrConfig {
            device,
            center_freq: AIS_FREQ,
            sample_rate,
            gain: gain.map(Gain::Manual).unwrap_or(Gain::Manual(RTLSDR_GAIN)),
            bias_tee,
            freq_correction_ppm: 0,
        };
        let config = DeviceConfig::RtlSdr(rtlsdr_config);
        Self::from_device_config(config, sample_rate)
    }

    #[cfg(feature = "soapy")]
    pub fn from_soapy(
        args: &str,
        channel: usize,
        sample_rate: u32,
        gain: Option<f64>,
        bias_tee: bool,
    ) -> Result<Self> {
        let soapy_config = SoapyConfig {
            args: args.to_string(),
            center_freq: AIS_FREQ as f64,
            sample_rate: sample_rate as f64,
            channel,
            gain: gain.map(Gain::Manual).unwrap_or(Gain::Manual(SOAPY_GAIN)),
            bias_tee,
        };
        let config = DeviceConfig::Soapy(soapy_config);
        Self::from_device_config(config, sample_rate)
    }

    pub fn next_message(&mut self) -> Option<Result<AisDemodulatedMessage>> {
        if let Some(msg) = self.message_buffer.pop_front() {
            return Some(Ok(msg));
        }
        loop {
            match self.source.next() {
                Some(Ok(samples)) => {
                    // Convert to 96 kHz using adapter
                    let samples_96k = self.adapter.process(&samples);

                    let messages = self.demodulator.demodulate(&samples_96k);
                    self.message_buffer.extend(messages);
                    if let Some(msg) = self.message_buffer.pop_front() {
                        return Some(Ok(msg));
                    }
                }
                Some(Err(e)) => {
                    return Some(Err(e));
                }
                None => {
                    return None;
                }
            }
        }
    }
}

impl Iterator for AisIqSource {
    type Item = Result<AisDemodulatedMessage>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_message()
    }
}

pub struct AisAsyncIqSource {
    pub handle: tokio::task::JoinHandle<()>,
    rx: mpsc::Receiver<Result<AisDemodulatedMessage>>,
}

impl AisAsyncIqSource {
    pub fn new(
        handle: tokio::task::JoinHandle<()>,
        rx: mpsc::Receiver<Result<AisDemodulatedMessage>>,
    ) -> Self {
        Self { handle, rx }
    }

    /// Close the consumer and wait for the demodulator to stop its IQ source.
    pub async fn stop(&mut self) -> std::result::Result<(), tokio::task::JoinError> {
        self.rx.close();
        (&mut self.handle).await
    }

    pub fn from_file<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        format: IqFormat,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> {
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage>>(32);
        async move {
            let source = IqAsyncSource::from_file(
                path,
                AIS_FREQ,
                sample_rate,
                DEFAULT_CHUNK_SAMPLES,
                format,
            )
            .await?;
            let handle = spawn_demodulator_task(source, tx, sample_rate);
            Ok(Self { handle, rx })
        }
    }

    pub fn from_stdin(sample_rate: u32, format: IqFormat) -> AisAsyncIqSource {
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage>>(32);
        let source =
            IqAsyncSource::from_stdin(AIS_FREQ, sample_rate, DEFAULT_CHUNK_SAMPLES, format);
        let handle = spawn_demodulator_task(source, tx, sample_rate);
        AisAsyncIqSource { handle, rx }
    }

    pub fn from_tcp(
        addr: &str,
        port: u16,
        sample_rate: u32,
        format: IqFormat,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> + '_ {
        let addr = addr.to_string();
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage>>(32);
        async move {
            let source = IqAsyncSource::from_tcp(
                &addr,
                port,
                AIS_FREQ,
                sample_rate,
                DEFAULT_CHUNK_SAMPLES,
                format,
            )
            .await?;
            let handle = spawn_demodulator_task(source, tx, sample_rate);
            Ok(AisAsyncIqSource { handle, rx })
        }
    }

    /// Create an async AIS IQ source from a DeviceConfig for SDR devices
    #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "hackrf"))]
    pub fn from_device_config(
        config: DeviceConfig,
        sample_rate: u32,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> {
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage>>(32);
        async move {
            let source = IqAsyncSource::from_device_config(&config).await?;
            let handle = spawn_demodulator_task(source, tx, sample_rate);
            Ok(AisAsyncIqSource { handle, rx })
        }
    }

    #[cfg(feature = "rtlsdr")]
    pub fn from_rtlsdr(
        device: DeviceSelector,
        sample_rate: u32,
        gain: Gain,
        bias_tee: bool,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> {
        let rtlsdr_config = RtlSdrConfig {
            device,
            center_freq: AIS_FREQ,
            sample_rate,
            gain,
            bias_tee,
            freq_correction_ppm: 0,
        };
        let config = DeviceConfig::RtlSdr(rtlsdr_config);
        Self::from_device_config(config, sample_rate)
    }

    #[cfg(feature = "soapy")]
    pub fn from_soapy(
        args: &str,
        sample_rate: u32,
        gain: Gain,
        bias_tee: bool,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> {
        let soapy_config = SoapyConfig {
            args: args.to_string(),
            center_freq: AIS_FREQ as f64,
            sample_rate: sample_rate as f64,
            channel: 0,
            gain,
            bias_tee,
        };
        let config = DeviceConfig::Soapy(soapy_config);
        Self::from_device_config(config, sample_rate)
    }

    #[cfg(feature = "airspy")]
    pub fn from_airspy(
        device: AirspyDeviceSelector,
        sample_rate: u32,
        gain: Gain,
        bias_tee: bool,
        lna_gain: Option<u8>,
        mixer_gain: Option<u8>,
        vga_gain: Option<u8>,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> {
        let airspy_config = AirspyConfig {
            device,
            center_freq: AIS_FREQ,
            sample_rate,
            gain,
            bias_tee,
            packing: false,
            lna_gain,
            mixer_gain,
            vga_gain,
            gain_mode: AirspyGainMode::default(),
        };
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage>>(32);
        async move {
            let source = IqAsyncSource::Airspy(AsyncAirspySdrReader::new(&airspy_config)?);
            let handle = spawn_demodulator_task(source, tx, sample_rate);
            Ok(AisAsyncIqSource { handle, rx })
        }
    }

    #[cfg(feature = "hackrf")]
    pub fn from_hackrf(
        device_index: usize,
        sample_rate: u32,
        gain: Gain,
        amp_enable: bool,
        bias_tee: bool,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> {
        let hackrf_config = HackRfConfig {
            device_index,
            center_freq: AIS_FREQ as u64,
            sample_rate,
            gain,
            amp_enable,
            bias_tee,
        };
        let config = DeviceConfig::HackRf(hackrf_config);
        Self::from_device_config(config, sample_rate)
    }
}

impl FuturesStream for AisAsyncIqSource {
    type Item = Result<AisDemodulatedMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

pub fn spawn_demodulator_task(
    mut source: IqAsyncSource,
    tx: mpsc::Sender<Result<AisDemodulatedMessage>>,
    sample_rate: u32,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Create sample rate adapter and demodulator
        let mut adapter = SampleRateAdapter::new(sample_rate);
        let mut demodulator = AisDemodulator::new();

        'read: loop {
            // Closing the AIS consumer must also wake an idle/stalled IQ reader.
            let chunk = tokio::select! {
                biased;
                _ = tx.closed() => break,
                chunk = source.next() => chunk,
            };
            match chunk {
                Some(Ok(samples)) => {
                    // Convert to 96 kHz using adapter
                    let samples_96k = adapter.process(&samples);
                    let messages = demodulator.demodulate(&samples_96k);
                    for msg in messages {
                        if tx.send(Ok(msg)).await.is_err() {
                            break 'read;
                        }
                    }
                }
                Some(Err(e)) => {
                    let _ = tx.send(Err(e)).await;
                    break;
                }
                None => break,
            }
        }
        // File/stdin/TCP readers have no background hardware workers to join.
        if !matches!(
            source,
            IqAsyncSource::IqAsyncFile(_)
                | IqAsyncSource::IqAsyncZstdFile(_)
                | IqAsyncSource::IqAsyncStdin(_)
                | IqAsyncSource::IqAsyncTcp(_)
        ) {
            if let Err(error) = source.stop().await {
                tracing::warn!(%error, "Unable to stop IQ source cleanly");
            }
        }
    })
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use std::time::Duration;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    #[tokio::test]
    async fn explicit_stop_joins_an_idle_iq_reader() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut source = AisAsyncIqSource::from_tcp("127.0.0.1", port, 288_000, IqFormat::Cu8)
            .await
            .unwrap();
        let (_connection, _) = listener.accept().await.unwrap();
        timeout(Duration::from_secs(2), source.stop())
            .await
            .expect("stop must wake an idle reader")
            .unwrap();
        assert!(source.handle.is_finished());
    }

    #[tokio::test]
    async fn dropping_consumer_wakes_demodulator_without_iq_samples() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let source =
            IqAsyncSource::from_tcp("127.0.0.1", port, AIS_FREQ, 288_000, 1024, IqFormat::Cu8)
                .await
                .unwrap();
        let (_connection, _) = listener.accept().await.unwrap();
        let (tx, rx) = mpsc::channel(1);
        let handle = spawn_demodulator_task(source, tx, 288_000);
        drop(rx);
        timeout(Duration::from_secs(2), handle)
            .await
            .expect("a dropped consumer must wake an idle reader")
            .unwrap();
    }
}
