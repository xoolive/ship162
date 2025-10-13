//! 5-stage Cascaded Integrator-Comb (CIC5) Filters
//!
//! This module provides efficient implementations of 5-stage Cascaded Integrator-Comb (CIC) filters
//! for complex sample streams, commonly used in Software Defined Radio (SDR) pipelines.
//!
//! # Overview
//!
//! - [`Downsample2CIC5`]: Performs CIC5 filtering with downsampling by a factor of 2. Useful for sample rate reduction in protocols such as AIS.
//! - [`FilterCIC5`]: Applies CIC5 filtering without changing the sample rate, providing smoothing and noise reduction.
//!
//! Both filters operate on `Complex<f32>` samples and implement the [`Stream`] trait for integration into DSP pipelines.
//!
//! # Usage
//!
//! Instantiate the desired filter and process input samples via the `receive` method. Internal state is managed automatically, and can be reset with the `reset` method.
//!
//! # References
//!
//! - [CIC Filters - Hogenauer, E. B.](https://ieeexplore.ieee.org/document/1163445)
//! - [SDR DSP Techniques](https://www.dsprelated.com/showarticle/1337.php)
//!
//! # Examples
//!
//! ```rust
//! use crate::dsp::cic5::{Downsample2CIC5, FilterCIC5};
//! use num_complex::Complex;
//!
//! let mut filter = Downsample2CIC5::new();
//! let input: Vec<Complex<f32>> = vec![/* ... */];
//! let mut tag = Tag::default();
//! let output = filter.receive(&input, &mut tag);
//! ```
use super::{Stream, Tag};
use num_complex::Complex;

/// CIC5 downsample by 2 filter.
///
/// Implements a 5-stage Cascaded Integrator-Comb (CIC) filter for complex
/// samples, performing downsampling by a factor of 2. Used for efficient sample
/// rate reduction in SDR pipelines, especially for AIS and similar protocols.
pub struct Downsample2CIC5 {
    h0: Complex<f32>,
    h1: Complex<f32>,
    h2: Complex<f32>,
    h3: Complex<f32>,
    h4: Complex<f32>,
}

impl Downsample2CIC5 {
    pub fn new() -> Self {
        Self {
            h0: Complex::new(0.0, 0.0),
            h1: Complex::new(0.0, 0.0),
            h2: Complex::new(0.0, 0.0),
            h3: Complex::new(0.0, 0.0),
            h4: Complex::new(0.0, 0.0),
        }
    }
}

impl Default for Downsample2CIC5 {
    fn default() -> Self {
        Self::new()
    }
}

impl Stream<Complex<f32>, Complex<f32>> for Downsample2CIC5 {
    fn receive(&mut self, data: &[Complex<f32>], _tag: &mut Tag) -> Vec<Complex<f32>> {
        if data.len() < 2 {
            return data.to_vec();
        }

        let len = (data.len() / 2) * 2;
        let mut output = Vec::with_capacity(len / 2);

        let mut i = 0;
        while i < len {
            // First sample: MA1 operations
            let mut z = data[i];

            let r0 = z;
            z += self.h0;

            let r1 = z;
            z += self.h1;

            let r2 = z;
            z += self.h2;

            let r3 = z;
            z += self.h3;

            let r4 = z; // Not used
            z += self.h4;

            output.push(z * 0.03125);

            // Second sample: MA2 operations
            z = data[i + 1];

            self.h0 = z;
            z += r0;

            self.h1 = z;
            z += r1;

            self.h2 = z;
            z += r2;

            self.h3 = z;
            z += r3;

            self.h4 = z;
            z += r4; // Not used

            i += 2;
        }

        output
    }

    fn reset(&mut self) {
        self.h0 = Complex::new(0.0, 0.0);
        self.h1 = Complex::new(0.0, 0.0);
        self.h2 = Complex::new(0.0, 0.0);
        self.h3 = Complex::new(0.0, 0.0);
        self.h4 = Complex::new(0.0, 0.0);
    }
}

/// CIC5 filter (no downsampling).
///
/// Implements a 5-stage Cascaded Integrator-Comb (CIC) filter for complex
/// samples, without changing the sample rate. Used for smoothing and noise
/// reduction in SDR pipelines.
pub struct FilterCIC5 {
    h0: Complex<f32>,
    h1: Complex<f32>,
    h2: Complex<f32>,
    h3: Complex<f32>,
    h4: Complex<f32>,
}

impl FilterCIC5 {
    pub fn new() -> Self {
        Self {
            h0: Complex::new(0.0, 0.0),
            h1: Complex::new(0.0, 0.0),
            h2: Complex::new(0.0, 0.0),
            h3: Complex::new(0.0, 0.0),
            h4: Complex::new(0.0, 0.0),
        }
    }
}

impl Default for FilterCIC5 {
    fn default() -> Self {
        Self::new()
    }
}

impl Stream<Complex<f32>, Complex<f32>> for FilterCIC5 {
    fn receive(&mut self, data: &[Complex<f32>], _tag: &mut Tag) -> Vec<Complex<f32>> {
        if data.len() < 2 {
            return data.to_vec();
        }

        let len = (data.len() / 2) * 2;
        let mut output = Vec::with_capacity(len);

        let mut i = 0;
        while i < len {
            // First sample: MA1
            let mut z = data[i];

            let r0 = z;
            z += self.h0;
            let r1 = z;
            z += self.h1;
            let r2 = z;
            z += self.h2;
            let r3 = z;
            z += self.h3;
            let r4 = z; // KEEP r4!
            z += self.h4;

            output.push(z * 0.03125);

            // Second sample: MA2
            z = data[i + 1];

            self.h0 = z;
            z += r0;
            self.h1 = z;
            z += r1;
            self.h2 = z;
            z += r2;
            self.h3 = z;
            z += r3;
            self.h4 = z;
            z += r4; // USE r4!

            output.push(z * 0.03125);

            i += 2;
        }

        output
    }

    fn reset(&mut self) {
        self.h0 = Complex::new(0.0, 0.0);
        self.h1 = Complex::new(0.0, 0.0);
        self.h2 = Complex::new(0.0, 0.0);
        self.h3 = Complex::new(0.0, 0.0);
        self.h4 = Complex::new(0.0, 0.0);
    }
}
