//! Generic I/Q source for reading complex samples from any `Read` source.
//!
//! Formats with 16/32-bit samples are little-endian (Cs16, Cf32).

use desperado::{IqAsyncSource, IqFormat, IqSource, Result};
use futures::stream::StreamExt;
use futures::Stream;
use std::collections::VecDeque;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

use crate::dsp::ais::{AisDemodulatedMessage, AisDemodulator};

const DEFAULT_CHUNK_SAMPLES: usize = 8192;

pub struct AisIqSource {
    source: IqSource,
    demodulator: AisDemodulator,
    message_buffer: VecDeque<AisDemodulatedMessage>,
}

impl AisIqSource {
    pub fn new(source: IqSource, sample_rate: u32) -> Self {
        Self {
            source,
            demodulator: AisDemodulator::new(sample_rate),
            message_buffer: VecDeque::new(),
        }
    }

    pub fn from_file<P: AsRef<Path>>(path: P, sample_rate: u32, format: IqFormat) -> Result<Self> {
        let center_freq = 162_000_000;
        let source = IqSource::from_file(
            path,
            center_freq,
            sample_rate,
            DEFAULT_CHUNK_SAMPLES,
            format,
        )?;
        Ok(Self {
            source,
            demodulator: AisDemodulator::new(sample_rate),
            message_buffer: VecDeque::new(),
        })
    }

    pub fn from_tcp(addr: &str, port: u16, sample_rate: u32, format: IqFormat) -> Result<Self> {
        let center_freq = 162_000_000;
        let source = IqSource::from_tcp(
            addr,
            port,
            center_freq,
            sample_rate,
            DEFAULT_CHUNK_SAMPLES,
            format,
        )?;
        Ok(Self {
            source,
            demodulator: AisDemodulator::new(sample_rate),
            message_buffer: VecDeque::new(),
        })
    }

    #[cfg(feature = "pluto")]
    pub fn from_pluto(uri: &str, sample_rate: u32, gain: f64) -> Result<Self> {
        let center_freq = 162_000_000;
        let source = IqSource::from_pluto(uri, center_freq, sample_rate as i64, gain)?;
        Ok(Self {
            source,
            demodulator: AisDemodulator::new(sample_rate),
            message_buffer: VecDeque::new(),
        })
    }

    #[cfg(feature = "rtlsdr")]
    pub fn from_rtlsdr(device_index: usize, sample_rate: u32) -> Result<Self> {
        let center_freq = 162_000_000;
        let source = IqSource::from_rtlsdr(device_index, center_freq, sample_rate, None)?;
        Ok(Self {
            source,
            demodulator: AisDemodulator::new(sample_rate),
            message_buffer: VecDeque::new(),
        })
    }

    #[cfg(feature = "soapy")]
    pub fn from_soapy(
        args: &str,
        channel: usize,
        sample_rate: u32,
        gain: Option<f64>,
        gain_element: &str,
    ) -> Result<Self> {
        let center_freq = 162_000_000;
        let source =
            IqSource::from_soapy(args, channel, center_freq, sample_rate, gain, gain_element)?;
        Ok(Self {
            source,
            demodulator: AisDemodulator::new(sample_rate),
            message_buffer: VecDeque::new(),
        })
    }

    pub fn next_message(&mut self) -> Option<Result<AisDemodulatedMessage>> {
        if let Some(msg) = self.message_buffer.pop_front() {
            return Some(Ok(msg));
        }
        loop {
            match self.source.next() {
                Some(Ok(samples)) => {
                    let messages = self.demodulator.demodulate(&samples);
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
        let center_freq = 162_000_000;
        async move {
            let source = IqAsyncSource::from_file(
                path,
                center_freq,
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
        let center_freq = 162_000_000;
        let source =
            IqAsyncSource::from_stdin(center_freq, sample_rate, DEFAULT_CHUNK_SAMPLES, format);
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
        let center_freq = 162_000_000;
        async move {
            let source = IqAsyncSource::from_tcp(
                &addr,
                port,
                center_freq,
                sample_rate,
                DEFAULT_CHUNK_SAMPLES,
                format,
            )
            .await?;
            let handle = spawn_demodulator_task(source, tx, sample_rate);
            Ok(AisAsyncIqSource { handle, rx })
        }
    }

    #[cfg(feature = "pluto")]
    pub fn from_pluto(
        uri: &str,
        sample_rate: u32,
        gain: f64,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> + '_ {
        let uri = uri.to_string();
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage>>(32);
        let center_freq = 162_000_000;
        async move {
            let source =
                IqAsyncSource::from_pluto(&uri, center_freq, sample_rate as i64, gain).await?;
            let handle = spawn_demodulator_task(source, tx, sample_rate);
            Ok(AisAsyncIqSource { handle, rx })
        }
    }

    #[cfg(feature = "rtlsdr")]
    pub fn from_rtlsdr(
        device_index: usize,
        sample_rate: u32,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> {
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage>>(32);
        let center_freq = 162_000_000;
        async move {
            let source =
                IqAsyncSource::from_rtlsdr(device_index, center_freq, sample_rate, None).await?;
            let handle = spawn_demodulator_task(source, tx, sample_rate);
            Ok(AisAsyncIqSource { handle, rx })
        }
    }

    #[cfg(feature = "soapy")]
    pub fn from_soapy(
        args: &str,
        sample_rate: u32,
        gain: Option<f64>,
        gain_element: &str,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource>> {
        let args = args.to_string();
        let gain_element = gain_element.to_string();
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage>>(32);
        let center_freq = 162_000_000;
        async move {
            let source =
                IqAsyncSource::from_soapy(&args, 0, center_freq, sample_rate, gain, &gain_element)
                    .await?;
            let handle = spawn_demodulator_task(source, tx, sample_rate);
            Ok(AisAsyncIqSource { handle, rx })
        }
    }
}

impl Stream for AisAsyncIqSource {
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
        let mut demodulator = AisDemodulator::new(sample_rate);
        loop {
            match source.next().await {
                Some(Ok(samples)) => {
                    let messages = demodulator.demodulate(&samples);
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
