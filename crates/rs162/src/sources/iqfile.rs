//! I/Q file source for reading complex samples from various formats

use num_complex::Complex;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::dsp::ais::{AisDemodulatedMessage, AisDemodulator};

pub enum IqFileFormat {
    Cu8,  // Complex unsigned 8-bit
    Cs8,  // Complex signed 8-bit
    Cs16, // Complex signed 16-bit
    Cf32, // Complex 32-bit float
}

pub struct IqFileSource {
    reader: BufReader<File>,
    format: IqFileFormat,
    demodulator: AisDemodulator,
    chunk_size: usize,
    message_buffer: VecDeque<AisDemodulatedMessage>, // Buffer for individual messages
}

impl IqFileSource {
    pub fn new<P: AsRef<Path>>(
        path: P,
        sample_rate: u32,
        format: IqFileFormat,
    ) -> Result<Self, std::io::Error> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let demodulator = AisDemodulator::new(sample_rate);
        Ok(Self {
            reader,
            format,
            demodulator,
            chunk_size: 8192, // Default chunk size
            message_buffer: VecDeque::new(),
        })
    }

    /// Read a chunk of IQ samples and convert to Complex<f32>
    fn read_samples(&mut self) -> Result<Vec<Complex<f32>>, std::io::Error> {
        match self.format {
            IqFileFormat::Cf32 => self.read_cf32(),
            IqFileFormat::Cu8 => self.read_cu8(),
            IqFileFormat::Cs8 => self.read_cs8(),
            IqFileFormat::Cs16 => self.read_cs16(),
        }
    }

    fn read_cf32(&mut self) -> Result<Vec<Complex<f32>>, std::io::Error> {
        let mut buffer = vec![0u8; self.chunk_size * 8]; // 2 f32s per sample
        let bytes_read = self.reader.read(&mut buffer)?;
        let samples_read = bytes_read / 8;

        let mut samples = Vec::with_capacity(samples_read);
        for i in 0..samples_read {
            let i_bytes = &buffer[i * 8..i * 8 + 4];
            let q_bytes = &buffer[i * 8 + 4..i * 8 + 8];
            let i_val = f32::from_le_bytes([i_bytes[0], i_bytes[1], i_bytes[2], i_bytes[3]]);
            let q_val = f32::from_le_bytes([q_bytes[0], q_bytes[1], q_bytes[2], q_bytes[3]]);
            samples.push(Complex::new(i_val, q_val));
        }
        Ok(samples)
    }

    fn read_cu8(&mut self) -> Result<Vec<Complex<f32>>, std::io::Error> {
        let mut buffer = vec![0u8; self.chunk_size * 2]; // 2 u8s per sample
        let bytes_read = self.reader.read(&mut buffer)?;
        let samples_read = bytes_read / 2;

        let mut samples = Vec::with_capacity(samples_read);
        for i in 0..samples_read {
            let i_val = (buffer[i * 2] as f32 - 127.5) / 128.0;
            let q_val = (buffer[i * 2 + 1] as f32 - 127.5) / 128.0;
            samples.push(Complex::new(i_val, q_val));
        }
        Ok(samples)
    }

    fn read_cs8(&mut self) -> Result<Vec<Complex<f32>>, std::io::Error> {
        let mut buffer = vec![0u8; self.chunk_size * 2]; // 2 i8s per sample
        let bytes_read = self.reader.read(&mut buffer)?;
        let samples_read = bytes_read / 2;

        let mut samples = Vec::with_capacity(samples_read);
        for i in 0..samples_read {
            let i_val = (buffer[i * 2] as i8) as f32 / 128.0;
            let q_val = (buffer[i * 2 + 1] as i8) as f32 / 128.0;
            samples.push(Complex::new(i_val, q_val));
        }
        Ok(samples)
    }

    fn read_cs16(&mut self) -> Result<Vec<Complex<f32>>, std::io::Error> {
        let mut buffer = vec![0u8; self.chunk_size * 4]; // 2 i16s per sample
        let bytes_read = self.reader.read(&mut buffer)?;
        let samples_read = bytes_read / 4;

        let mut samples = Vec::with_capacity(samples_read);
        for i in 0..samples_read {
            let i_bytes = &buffer[i * 4..i * 4 + 2];
            let q_bytes = &buffer[i * 4 + 2..i * 4 + 4];
            let i_val = i16::from_le_bytes([i_bytes[0], i_bytes[1]]) as f32 / 32768.0;
            let q_val = i16::from_le_bytes([q_bytes[0], q_bytes[1]]) as f32 / 32768.0;
            samples.push(Complex::new(i_val, q_val));
        }
        Ok(samples)
    }

    /// Get the next individual AIS message
    fn next_message(&mut self) -> Option<Result<AisDemodulatedMessage, std::io::Error>> {
        // Return buffered message if available
        if let Some(msg) = self.message_buffer.pop_front() {
            return Some(Ok(msg));
        }

        // Read more samples and demodulate
        loop {
            match self.read_samples() {
                Ok(samples) => {
                    if samples.is_empty() {
                        return None; // EOF
                    }

                    let messages = self.demodulator.demodulate(&samples);

                    // Add all messages to buffer
                    for msg in messages {
                        self.message_buffer.push_back(msg);
                    }

                    // Return first message if any
                    if let Some(msg) = self.message_buffer.pop_front() {
                        return Some(Ok(msg));
                    }
                    // Continue reading if no messages were found
                }
                Err(e) => return Some(Err(e)),
            }
        }
    }
}

impl Iterator for IqFileSource {
    type Item = Result<AisDemodulatedMessage, std::io::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_message()
    }
}
