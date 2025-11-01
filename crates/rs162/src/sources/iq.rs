//! Generic I/Q source for reading complex samples from any `Read` source.

use num_complex::Complex;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufReader, Read};
use std::net::TcpStream;
use std::path::Path;

use crate::dsp::ais::{AisDemodulatedMessage, AisDemodulator};

#[derive(Debug, Copy, Clone)]
pub enum IqFormat {
    Cu8,  // Complex unsigned 8-bit
    Cs8,  // Complex signed 8-bit
    Cs16, // Complex signed 16-bit
    Cf32, // Complex 32-bit float
}

pub struct IqSource<R: Read> {
    reader: BufReader<R>,
    format: IqFormat,
    demodulator: AisDemodulator,
    chunk_size: usize,
    message_buffer: VecDeque<AisDemodulatedMessage>,
}

impl<R: Read> IqSource<R> {
    pub fn new(reader: R, sample_rate: u32, format: IqFormat) -> Self {
        let reader = BufReader::new(reader);
        let demodulator = AisDemodulator::new(sample_rate);
        Self {
            reader,
            format,
            demodulator,
            chunk_size: 8192,
            message_buffer: VecDeque::new(),
        }
    }

    fn read_samples(&mut self) -> Result<Vec<Complex<f32>>, std::io::Error> {
        let bytes_per_sample = match self.format {
            IqFormat::Cu8 | IqFormat::Cs8 => 2,
            IqFormat::Cs16 => 4,
            IqFormat::Cf32 => 8,
        };
        let mut buffer = vec![0u8; self.chunk_size * bytes_per_sample];
        self.reader.read_exact(&mut buffer)?;

        let samples = match self.format {
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
        };
        Ok(samples)
    }

    fn next_message(&mut self) -> Option<Result<AisDemodulatedMessage, std::io::Error>> {
        if let Some(msg) = self.message_buffer.pop_front() {
            return Some(Ok(msg));
        }

        loop {
            match self.read_samples() {
                Ok(samples) => {
                    if samples.is_empty() {
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
                        return None;
                    }
                    return Some(Err(e));
                }
            }
        }
    }
}

impl IqSource<File> {
    pub fn from_file<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        format: IqFormat,
    ) -> Result<Self, std::io::Error> {
        let file = File::open(path)?;
        Ok(Self::new(file, sample_rate, format))
    }
}

impl IqSource<TcpStream> {
    pub fn from_tcp(
        addr: &str,
        sample_rate: u32,
        format: IqFormat,
    ) -> Result<Self, std::io::Error> {
        let stream = TcpStream::connect(addr)?;
        Ok(Self::new(stream, sample_rate, format))
    }
}

impl<R: Read> Iterator for IqSource<R> {
    type Item = Result<AisDemodulatedMessage, std::io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_message()
    }
}
