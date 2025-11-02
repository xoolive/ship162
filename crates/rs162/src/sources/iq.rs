//! Generic I/Q source for reading complex samples from any `Read` source.
//!
//! Formats with 16/32-bit samples are little-endian (Cs16, Cf32).

use futures::Stream;
use num_complex::Complex;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufReader, Read};
use std::marker::PhantomData;
use std::net::TcpStream;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::mpsc;

use crate::dsp::ais::{AisDemodulatedMessage, AisDemodulator};

#[derive(Debug, Copy, Clone)]
pub enum IqFormat {
    Cu8,  // Complex unsigned 8-bit
    Cs8,  // Complex signed 8-bit
    Cs16, // Complex signed 16-bit
    Cf32, // Complex 32-bit float
}

const DEFAULT_CHUNK_SAMPLES: usize = 8192;

impl IqFormat {
    fn bytes_per_sample(self) -> usize {
        match self {
            IqFormat::Cu8 | IqFormat::Cs8 => 2,
            IqFormat::Cs16 => 4,
            IqFormat::Cf32 => 8,
        }
    }
}

fn convert_bytes_to_complex(format: IqFormat, buffer: &[u8]) -> Vec<Complex<f32>> {
    match format {
        IqFormat::Cu8 => buffer
            .chunks_exact(2)
            .map(|c| Complex::new((c[0] as f32 - 127.5) / 128.0, (c[1] as f32 - 127.5) / 128.0))
            .collect(),
        IqFormat::Cs8 => buffer
            .chunks_exact(2)
            .map(|c| Complex::new((c[0] as i8) as f32 / 128.0, (c[1] as i8) as f32 / 128.0))
            .collect(),
        IqFormat::Cs16 => buffer
            .chunks_exact(4)
            .map(|c| {
                Complex::new(
                    i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0,
                    i16::from_le_bytes([c[2], c[3]]) as f32 / 32768.0,
                )
            })
            .collect(),
        IqFormat::Cf32 => buffer
            .chunks_exact(8)
            .map(|c| {
                Complex::new(
                    f32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                    f32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                )
            })
            .collect(),
    }
}

pub struct IqSource<R: Read> {
    reader: R,
    format: IqFormat,
    demodulator: AisDemodulator,
    chunk_size: usize,
    message_buffer: VecDeque<AisDemodulatedMessage>,
    terminated: bool,
}

impl<R: Read> IqSource<R> {
    pub fn new(reader: R, sample_rate: u32, format: IqFormat) -> Self {
        let demodulator = AisDemodulator::new(sample_rate);
        Self {
            reader,
            format,
            demodulator,
            chunk_size: DEFAULT_CHUNK_SAMPLES,
            message_buffer: VecDeque::new(),
            terminated: false,
        }
    }

    fn read_samples(&mut self) -> Result<Vec<Complex<f32>>, std::io::Error> {
        let bytes_per_sample = self.format.bytes_per_sample();
        let mut buffer = vec![0u8; self.chunk_size * bytes_per_sample];
        // TODO: partial trailing data at EOF is dropped (same for async!)
        // use rollover buffer.
        self.reader.read_exact(&mut buffer)?;
        let samples = convert_bytes_to_complex(self.format, &buffer);
        Ok(samples)
    }

    fn next_message(&mut self) -> Option<Result<AisDemodulatedMessage, std::io::Error>> {
        if self.terminated {
            return None;
        }
        if let Some(msg) = self.message_buffer.pop_front() {
            return Some(Ok(msg));
        }

        loop {
            match self.read_samples() {
                Ok(samples) => {
                    if samples.is_empty() {
                        self.terminated = true;
                        return None; // EOF
                    }
                    let messages = self.demodulator.demodulate(&samples);
                    self.message_buffer.extend(messages);
                    if let Some(msg) = self.message_buffer.pop_front() {
                        return Some(Ok(msg));
                    }
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        self.terminated = true;
                        return None;
                    }
                    self.terminated = true;
                    return Some(Err(e));
                }
            }
        }
    }
}

impl IqSource<BufReader<File>> {
    pub fn from_file<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        format: IqFormat,
    ) -> Result<Self, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        Ok(Self::new(reader, sample_rate, format))
    }
}

impl IqSource<BufReader<TcpStream>> {
    pub fn from_tcp(
        addr: &str,
        sample_rate: u32,
        format: IqFormat,
    ) -> Result<Self, std::io::Error> {
        let stream = TcpStream::connect(addr)?;
        let reader = BufReader::new(stream);
        Ok(Self::new(reader, sample_rate, format))
    }
}

impl<R: Read> Iterator for IqSource<R> {
    type Item = Result<AisDemodulatedMessage, std::io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_message()
    }
}

pub struct AsyncIqSource<R: AsyncRead + Unpin + Send + 'static> {
    rx: mpsc::Receiver<Result<AisDemodulatedMessage, std::io::Error>>,
    _task: tokio::task::JoinHandle<()>,
    _phantom: PhantomData<R>,
}

impl<R: AsyncRead + Unpin + Send + 'static> AsyncIqSource<R> {
    pub fn new(reader: R, sample_rate: u32, format: IqFormat) -> Self {
        let (tx, rx) = mpsc::channel::<Result<AisDemodulatedMessage, std::io::Error>>(32);
        let mut reader = reader;
        let handle = tokio::spawn(async move {
            let mut demodulator = AisDemodulator::new(sample_rate);
            let chunk_size = DEFAULT_CHUNK_SAMPLES;
            let bytes_per_sample = format.bytes_per_sample();
            let mut buffer = vec![0u8; chunk_size * bytes_per_sample];
            loop {
                match reader.read_exact(&mut buffer).await {
                    Ok(_) => {
                        let samples: Vec<Complex<f32>> = convert_bytes_to_complex(format, &buffer);
                        let messages = demodulator.demodulate(&samples);
                        for msg in messages {
                            if tx.send(Ok(msg)).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        if e.kind() != std::io::ErrorKind::UnexpectedEof {
                            let _ = tx.send(Err(e)).await;
                        }
                        return;
                    }
                }
            }
        });
        Self {
            rx,
            _task: handle,
            _phantom: PhantomData,
        }
    }
}

impl<R: AsyncRead + Unpin + Send + 'static> Stream for AsyncIqSource<R> {
    type Item = Result<AisDemodulatedMessage, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}
