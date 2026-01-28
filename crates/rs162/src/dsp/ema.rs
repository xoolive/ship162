//! Phase Search Demodulation using Exponential Moving Average (EMA).
//!
//! This module provides the [`PhaseSearchEMA`] struct, which implements a robust phase search
//! algorithm for symbol decoding in noisy Software Defined Radio (SDR) environments.
//!
//! # Overview
//!
//! The phase search demodulation technique rotates incoming complex samples to align the
//! constellation, applies a set of phase classifiers, and tracks the optimal phase using an
//! exponential moving average (EMA). This approach is designed to extract symbols reliably
//! even in the presence of significant noise.
//!
//! # Features
//!
//! - Maintains buffers and state for optimal phase search.
//! - Rotates complex samples for constellation alignment.
//! - Uses multiple phase classifiers for robust symbol extraction.
//! - Tracks the best phase using EMA for noise resilience.
//! - Supports configurable delay and search window parameters.
//!
//! # Usage
//!
//! Instantiate [`PhaseSearchEMA`] with desired parameters and use it as a stream processor
//! for complex samples. The output is a stream of demodulated symbols.
//!
//! # Example
//!
//! ```rust
//! use num_complex::Complex;
//! use rs162::dsp::ema::PhaseSearchEMA;
//! use rs162::dsp::{Stream, Tag};
//!
//! let mut demod = PhaseSearchEMA::with_params(2);
//! let input_samples = vec![Complex::new(1.0, 0.0); 8];
//! let mut tag = Tag::default();
//! let symbols = demod.receive(&input_samples, &mut tag);
//! assert_eq!(symbols.len(), input_samples.len());
//! ```
//!
//! # References
//!
//! - SDR phase search techniques
//! - Exponential moving average filtering
use super::{Stream, Tag};
use num_complex::Complex;
use std::f32::consts::PI;

/// Performs phase search demodulation using an exponential moving average.
///
/// This struct maintains buffers and state for searching the optimal phase for
/// symbol decoding.  It rotates incoming complex samples, applies a set of
/// phase classifiers, and tracks the best phase using an exponential moving
/// average. Used for robust symbol extraction in noisy SDR environments.
pub struct PhaseSearchEMA {
    /// Current rotation index for constellation alignment.
    rot: usize,
    /// Shift registers for phase bits.
    bits: Vec<u32>,
    /// Exponential moving average values for each phase.
    ma: Vec<f32>,
    /// Index of the current best phase.
    max_idx: usize,
    /// Precomputed phase classifier vectors.
    phase: Vec<Complex<f32>>,
    /// Number of phase classifiers.
    n_phases: usize,
    /// Search window size for local maximum.
    n_search: usize,
    /// Delay for bit extraction.
    n_delay: usize,
    /// EMA weighting factor.
    weight: f32,
}

impl PhaseSearchEMA {
    pub fn with_params(n_delay: usize) -> Self {
        const N_PHASES: usize = 16;
        const N_SEARCH: usize = 1;
        const WEIGHT: f32 = 0.85;

        let mut phase = Vec::with_capacity(N_PHASES / 2);
        for j in 0..(N_PHASES / 2) {
            let angle = PI * (j as f32 + 0.5) / (N_PHASES as f32 / 2.0);
            phase.push(Complex::new(angle.cos(), angle.sin()));
        }

        Self {
            rot: 0,
            bits: vec![0; N_PHASES],
            ma: vec![0.0; N_PHASES],
            max_idx: 0,
            phase,
            n_phases: N_PHASES,
            n_search: N_SEARCH,
            n_delay,
            weight: WEIGHT,
        }
    }
}

impl Stream<Complex<f32>, f32> for PhaseSearchEMA {
    fn receive(&mut self, data: &[Complex<f32>], _tag: &mut Tag) -> Vec<f32> {
        let mut output = Vec::with_capacity(data.len());

        for &sample in data {
            // Rotate by (1j)^rot to align constellation
            let (re, im) = match self.rot {
                0 => (sample.re, sample.im),
                1 => (-sample.im, sample.re),
                2 => (-sample.re, -sample.im),
                3 => (sample.im, -sample.re),
                _ => unreachable!(),
            };

            self.rot = (self.rot + 1) & 3;

            // Phase search - linear classification
            for j in 0..(self.n_phases / 2) {
                let a = re * self.phase[j].re;
                let b = im * self.phase[j].im;

                let t1 = a + b;
                self.bits[j] = (self.bits[j] << 1) | if t1 > 0.0 { 1 } else { 0 };
                self.ma[j] = self.weight * self.ma[j] + (1.0 - self.weight) * t1.abs();

                let t2 = a - b;
                self.bits[self.n_phases - 1 - j] =
                    (self.bits[self.n_phases - 1 - j] << 1) | if t2 > 0.0 { 1 } else { 0 };
                self.ma[self.n_phases - 1 - j] =
                    self.weight * self.ma[self.n_phases - 1 - j] + (1.0 - self.weight) * t2.abs();

                // Prevent error propagation
                if !self.ma[j].is_finite() {
                    self.ma[j] = 0.0;
                }
                if !self.ma[self.n_phases - 1 - j].is_finite() {
                    self.ma[self.n_phases - 1 - j] = 0.0;
                }
            }

            // Local search for maximum
            let start_idx = (self.max_idx + self.n_phases - self.n_search) & (self.n_phases - 1);
            let mut max_val = self.ma[start_idx];
            self.max_idx = start_idx;

            for p in 0..(self.n_search * 2 + 1) {
                let idx = (start_idx + p) & (self.n_phases - 1);
                if self.ma[idx] > max_val {
                    max_val = self.ma[idx];
                    self.max_idx = idx;
                }
            }

            // Extract bit from best phase
            let b2 = (self.bits[self.max_idx] >> (self.n_delay + 1)) & 1;
            let b1 = (self.bits[self.max_idx] >> self.n_delay) & 1;

            let b = if (b1 ^ b2) != 0 { 1.0 } else { -1.0 };
            output.push(b);
        }

        output
    }

    fn reset(&mut self) {
        self.rot = 0;
        self.bits.fill(0);
        self.ma.fill(0.0);
        self.max_idx = 0;
    }
}
