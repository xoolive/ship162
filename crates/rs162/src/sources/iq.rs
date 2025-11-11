//! Generic I/Q source for reading complex samples from any `Read` source.
//!
//! Formats with 16/32-bit samples are little-endian (Cs16, Cf32).

use desperado::{IqAsyncSource, IqFormat, IqSource};
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

    pub fn from_file<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        format: IqFormat,
    ) -> Result<Self, std::io::Error> {
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

    pub fn from_tcp(
        addr: &str,
        port: u16,
        sample_rate: u32,
        format: IqFormat,
    ) -> Result<Self, std::io::Error> {
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

    pub fn next_message(&mut self) -> Option<Result<AisDemodulatedMessage, std::io::Error>> {
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
    type Item = Result<AisDemodulatedMessage, std::io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_message()
    }
}

pub struct AisAsyncIqSource {
    pub handle: tokio::task::JoinHandle<()>,
    rx: mpsc::Receiver<Result<AisDemodulatedMessage, std::io::Error>>,
}

impl AisAsyncIqSource {
    pub fn new(
        handle: tokio::task::JoinHandle<()>,
        rx: mpsc::Receiver<Result<AisDemodulatedMessage, std::io::Error>>,
    ) -> Self {
        Self { handle, rx }
    }

    pub fn from_file<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        format: IqFormat,
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource, std::io::Error>> {
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage, std::io::Error>>(32);
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
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage, std::io::Error>>(32);
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
    ) -> impl std::future::Future<Output = Result<AisAsyncIqSource, std::io::Error>> + '_ {
        let addr = addr.to_string();
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage, std::io::Error>>(32);
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
}

impl Stream for AisAsyncIqSource {
    type Item = Result<AisDemodulatedMessage, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

pub fn spawn_demodulator_task(
    mut source: IqAsyncSource,
    tx: mpsc::Sender<Result<AisDemodulatedMessage, std::io::Error>>,
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
