//! Digital Signal Processing (DSP) module for AIS signal processing.
//!
//! This module provides a streaming architecture for composing and testing DSP blocks independently.
//! Each DSP block implements the [`Stream`] trait, allowing flexible chaining and integration.
//!
//! The main entry point for AIS signal processing is the [`ais`] submodule, which contains
//! the core logic for decoding AIS messages from IQ sample streams.
//!
//! Utility functions are provided for converting raw IQ samples from various formats
//! (`u8`, `i8`, `i16`, `f32`) into `Complex<f32>` for further processing.
//!
use num_complex::Complex;

pub mod afc;
pub mod ais;
pub mod cic5;
pub mod ema;
pub mod fir;
pub mod rotate;
pub mod sample_rate;
pub mod scatter;
pub mod upsampler;

/// Tag structure passed along with samples through the DSP chain
#[derive(Debug, Clone, Default)]
pub struct Tag {
    pub sample_idx: usize,
    pub sample_lvl: f32,
    pub ppm: f32,
    pub mode: u32,
    pub group: u64,
    pub level: f32,
}

/// Core streaming trait for DSP blocks
pub trait Stream<TIn, TOut> {
    /// Process input samples and produce output samples
    fn receive(&mut self, data: &[TIn], tag: &mut Tag) -> Vec<TOut>;

    /// Reset internal state (optional)
    fn reset(&mut self) {}
}

/// Helper trait for stateless transformations
pub trait StreamStateless<TIn, TOut> {
    fn process(&self, data: &[TIn], tag: &mut Tag) -> Vec<TOut>;
}

// Implement Stream for any StreamStateless
impl<T, TIn, TOut> Stream<TIn, TOut> for T
where
    T: StreamStateless<TIn, TOut>,
{
    fn receive(&mut self, data: &[TIn], tag: &mut Tag) -> Vec<TOut> {
        self.process(data, tag)
    }
}

/// Convert IQ samples from u8 format (RTL-SDR) to `Complex<f32>`
pub fn convert_samples_cu8(samples: &[u8]) -> Vec<Complex<f32>> {
    samples
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            let i = (chunk[0] as f32 - 127.5) / 128.0;
            let q = (chunk[1] as f32 - 127.5) / 128.0;
            Complex::new(i, q)
        })
        .collect()
}

/// Convert IQ samples from i8 format to `Complex<f32>`
pub fn convert_samples_cs8(samples: &[i8]) -> Vec<Complex<f32>> {
    samples
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            let i = chunk[0] as f32 / 128.0;
            let q = chunk[1] as f32 / 128.0;
            Complex::new(i, q)
        })
        .collect()
}

/// Convert IQ samples from i16 format to `Complex<f32>`
pub fn convert_samples_cs16(samples: &[i16]) -> Vec<Complex<f32>> {
    samples
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            let i = chunk[0] as f32 / 32768.0;
            let q = chunk[1] as f32 / 32768.0;
            Complex::new(i, q)
        })
        .collect()
}

/// Convert IQ samples from f32 (GQRX) format to `Complex<f32>`
pub fn convert_samples_cf32(samples: &[f32]) -> Vec<Complex<f32>> {
    samples
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| Complex::new(chunk[0], chunk[1]))
        .collect()
}
