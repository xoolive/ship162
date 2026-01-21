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

#[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "pluto"))]
use desperado::{DeviceConfig, Gain};

#[cfg(feature = "pluto")]
use desperado::pluto::PlutoConfig;
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
#[cfg(feature = "pluto")]
const PLUTO_GAIN: f64 = 73.0; // Maximum gain for PlutoSDR

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
    #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "pluto"))]
    pub fn from_device_config(config: DeviceConfig, sample_rate: u32) -> Result<Self> {
        let source = IqSource::from_device_config(config)?;
        Ok(Self::new(source, sample_rate))
    }

    #[cfg(feature = "pluto")]
    pub fn from_pluto(uri: &str, sample_rate: u32, gain: Option<f64>) -> Result<Self> {
        let pluto_config = PlutoConfig {
            uri: uri.to_string(),
            center_freq: AIS_FREQ as i64,
            sample_rate: sample_rate as i64,
            gain: gain.map(Gain::Manual).unwrap_or(Gain::Manual(PLUTO_GAIN)),
        };
        let config = DeviceConfig::Pluto(pluto_config);
        Self::from_device_config(config, sample_rate)
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
        gain_element: &str,
        bias_tee: bool,
    ) -> Result<Self> {
        let soapy_config = SoapyConfig {
            args: args.to_string(),
            center_freq: AIS_FREQ as f64,
            sample_rate: sample_rate as f64,
            channel,
            gain: gain.map(Gain::Manual).unwrap_or(Gain::Manual(SOAPY_GAIN)),
            gain_element: gain_element.to_string(),
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
    #[cfg(any(feature = "rtlsdr", feature = "soapy", feature = "pluto"))]
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

    #[cfg(feature = "pluto")]
    pub fn from_pluto(
        uri: &str,
        sample_rate: u32,
        gain: Option<f64>,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> + '_ {
        let pluto_config = PlutoConfig {
            uri: uri.to_string(),
            center_freq: AIS_FREQ as i64,
            sample_rate: sample_rate as i64,
            gain: gain.map(Gain::Manual).unwrap_or(Gain::Manual(PLUTO_GAIN)),
        };
        let config = DeviceConfig::Pluto(pluto_config);
        Self::from_device_config(config, sample_rate)
    }

    #[cfg(feature = "rtlsdr")]
    pub fn from_rtlsdr(
        device: DeviceSelector,
        sample_rate: u32,
        gain: Option<f64>,
        bias_tee: bool,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> {
        let rtlsdr_config = RtlSdrConfig {
            device,
            center_freq: AIS_FREQ,
            sample_rate,
            gain: gain.map(Gain::Manual).unwrap_or(Gain::Manual(RTLSDR_GAIN)),
            bias_tee,
        };
        let config = DeviceConfig::RtlSdr(rtlsdr_config);
        Self::from_device_config(config, sample_rate)
    }

    #[cfg(feature = "soapy")]
    pub fn from_soapy(
        args: &str,
        sample_rate: u32,
        gain: Option<f64>,
        gain_element: &str,
        bias_tee: bool,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> {
        let soapy_config = SoapyConfig {
            args: args.to_string(),
            center_freq: AIS_FREQ as f64,
            sample_rate: sample_rate as f64,
            channel: 0,
            gain: gain.map(Gain::Manual).unwrap_or(Gain::Manual(SOAPY_GAIN)),
            gain_element: gain_element.to_string(),
            bias_tee,
        };
        let config = DeviceConfig::Soapy(soapy_config);
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

        loop {
            match source.next().await {
                Some(Ok(samples)) => {
                    // Convert to 96 kHz using adapter
                    let samples_96k = adapter.process(&samples);
                    let messages = demodulator.demodulate(&samples_96k);
                    for msg in messages {
                        if tx.send(Ok(msg)).await.is_err() {
                            return;
                        }
                    }
                }
                Some(Err(e)) => {
                    let _ = tx.send(Err(e)).await;
                    return;
                }
                None => return,
            }
        }
    })
}
