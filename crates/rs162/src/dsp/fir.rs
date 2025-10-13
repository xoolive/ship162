//! Finite Impulse Response (FIR) Filters
//!
//! This module provides implementations of finite impulse response (FIR) filters for complex samples,
//! suitable for software-defined radio (SDR) pipelines and other DSP applications.
//!
//! # Features
//!
//! - **FilterComplex**: A complex FIR filter that applies user-provided taps to incoming samples.
//!   Maintains an internal buffer for convolution and supports streaming operation.
//!
//! - **DownsampleKFilter**: A FIR filter with downsampling by a user-specified factor `k`.
//!   Useful for sample rate reduction and anti-aliasing.
//!
//! - **Predefined Filter Taps**: Includes constants for commonly used filter coefficients, such as
//!   `COHERENT_TAPS` and `BLACKMAN_HARRIS_28_3`.
//!
//! # Usage
//!
//! Create a filter instance with desired taps, then process incoming complex samples using the
//! `Stream` trait interface. Reset filters as needed between processing runs.
//!
//! # Example
//!
//! ```rust
//! use num_complex::Complex;
//! use crate::dsp::filter::{FilterComplex, COHERENT_TAPS};
//!
//! let mut filter = FilterComplex::with_taps(COHERENT_TAPS);
//! let input: Vec<Complex<f32>> = /* ... */;
//! let mut tag = Tag::default();
//! let output = filter.receive(&input, &mut tag);
//! ```
//!
use super::{Stream, Tag};
use num_complex::Complex;

/// Complex FIR filter.
///
/// Implements a finite impulse response filter for complex samples using
/// user-provided taps.  Maintains an internal buffer for convolution, suitable
/// for SDR pipelines.
pub struct FilterComplex {
    /// Internal buffer for convolution.
    buffer: Vec<Complex<f32>>,
    /// Filter coefficients (taps).
    taps: Vec<f32>,
}

impl FilterComplex {
    pub fn with_taps(taps: &[f32]) -> Self {
        let n_taps = taps.len();
        Self {
            buffer: vec![Complex::new(0.0, 0.0); n_taps * 2],
            taps: taps.to_vec(),
        }
    }

    fn dot(&self, data: &[Complex<f32>]) -> Complex<f32> {
        let mut x = Complex::new(0.0, 0.0);
        for (i, &tap) in self.taps.iter().enumerate() {
            x += tap * data[i];
        }
        x
    }
}

impl Stream<Complex<f32>, Complex<f32>> for FilterComplex {
    fn receive(&mut self, data: &[Complex<f32>], _tag: &mut Tag) -> Vec<Complex<f32>> {
        let mut output = Vec::with_capacity(data.len());
        let n_taps = self.taps.len();

        if data.len() < n_taps {
            // Handle short inputs
            for &sample in data {
                for i in 1..n_taps {
                    self.buffer[i - 1] = self.buffer[i];
                }
                self.buffer[n_taps - 1] = sample;
                output.push(self.dot(&self.buffer[..n_taps]));
            }
            return output;
        }

        // Fill initial part of output
        let mut ptr = n_taps - 1;
        let tmp = 0..(n_taps - 1);
        for j in tmp {
            self.buffer[ptr] = data[j];
            output.push(self.dot(&self.buffer[j..]));
            ptr += 1;
        }

        // Main processing loop
        for i in 0..(data.len() - n_taps + 1) {
            output.push(self.dot(&data[i..]));
        }

        // Save tail for next call
        ptr = 0;
        let tmp = (data.len() - n_taps + 1)..data.len();
        for i in tmp {
            self.buffer[ptr] = data[i];
            ptr += 1;
        }

        output
    }

    fn reset(&mut self) {
        self.buffer.fill(Complex::new(0.0, 0.0));
    }
}

/// FIR filter with downsampling by factor k.
///
/// This struct implements a generic FIR filter for complex samples, followed by
/// downsampling.  The filter coefficients (taps) are user-provided, and the
/// filter maintains an internal buffer for convolution. Used for sample rate
/// reduction and anti-aliasing in SDR pipelines.
pub struct DownsampleKFilter {
    k: usize,
    buffer: Vec<Complex<f32>>,
    taps: Vec<f32>,
    count: usize,
}

impl DownsampleKFilter {
    pub fn with_params(k: usize, taps: &[f32]) -> Self {
        Self {
            k,
            buffer: vec![Complex::new(0.0, 0.0); taps.len() * 2],
            taps: taps.to_vec(),
            count: 0,
        }
    }
    pub fn dot(&self, data: &[Complex<f32>]) -> Complex<f32> {
        let mut x = Complex::new(0.0, 0.0);
        for (i, &tap) in self.taps.iter().enumerate() {
            x += tap * data[i];
        }
        x
    }
}

impl Stream<Complex<f32>, Complex<f32>> for DownsampleKFilter {
    fn receive(&mut self, data: &[Complex<f32>], _tag: &mut Tag) -> Vec<Complex<f32>> {
        let mut output = Vec::with_capacity(data.len());
        let n_taps = self.taps.len();
        for &sample in data {
            // Shift buffer: move all elements one position forward
            for i in (1..n_taps).rev() {
                self.buffer[i] = self.buffer[i - 1];
            }

            // Insert new sample at the beginning
            self.buffer[0] = sample;

            // Apply filter and downsample by factor k
            if self.count.is_multiple_of(self.k) {
                let filtered = self.dot(&self.buffer);
                output.push(filtered);
            }

            self.count += 1;
        }
        output
    }
    fn reset(&mut self) {
        self.buffer.fill(Complex::new(0.0, 0.0));
        self.count = 0
    }
}

pub const COHERENT_TAPS: &[f32; 17] = &[
    2.069_957_2e-6_f32,
    3.186_101_5e-5_f32,
    3.406_053e-4_f32,
    2.528_929_9e-3_f32,
    1.304_114_5e-2_f32,
    4.670_767_5e-2_f32,
    1.161_861_4e-1_f32,
    2.007_307_9e-1_f32,
    2.408_613_9e-1_f32,
    2.007_307_9e-1_f32,
    1.161_861_4e-1_f32,
    4.670_767_5e-2_f32,
    1.304_114_5e-2_f32,
    2.528_929_9e-3_f32,
    3.406_053e-4_f32,
    3.186_101_5e-5_f32,
    2.069_957_2e-6_f32,
];

/// BlackMan-Harris 28,3 window coefficients
pub const BLACKMAN_HARRIS_28_3: &[f32; 26] = &[
    6.325_423_6e-5_f32,
    -2.900_152_5e-4_f32,
    -1.542_062_5e-3_f32,
    -1.649_724_6e-3_f32,
    3.127_939e-3_f32,
    1.094_944_1e-2_f32,
    9.049_758e-3_f32,
    -1.436_858_4e-2_f32,
    -4.456_159_5e-2_f32,
    -3.448_836_5e-2_f32,
    5.534_742_8e-2_f32,
    2.018_279_1e-1_f32,
    3.165_346e-1_f32,
    3.165_346e-1_f32,
    2.018_279_1e-1_f32,
    5.534_742_8e-2_f32,
    -3.448_836_5e-2_f32,
    -4.456_159_5e-2_f32,
    -1.436_858_4e-2_f32,
    9.049_758e-3_f32,
    1.094_944_1e-2_f32,
    3.127_939e-3_f32,
    -1.649_724_6e-3_f32,
    -1.542_062_5e-3_f32,
    -2.900_152_5e-4_f32,
    6.325_423_6e-5_f32,
];
