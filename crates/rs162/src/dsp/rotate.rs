//! Frequency rotation and channel splitting.
//!
//! This module provides the `Rotate` struct, which implements a complex phasor-based frequency shifter.
//! It is typically used in SDR (Software Defined Radio) demodulation pipelines to rotate the frequency
//! of incoming complex samples and split them into two channels (up and down).
//!
//! # Features
//! - Maintains a complex phasor for sample-wise rotation.
//! - Allows setting the rotation angle dynamically.
//! - Splits input into two output channels: one with standard complex multiplication (up),
//!   and one with conjugate multiplication (down).
//! - Provides normalization to prevent phasor drift over time.
//! - Implements the `Stream` trait for single-channel processing.
//!
//! # Usage
//! Create a `Rotate` instance with a desired rotation angle per sample, then process incoming
//! complex sample buffers to obtain frequency-shifted outputs.
//!
//! # Example
//! ```rust
//! use num_complex::Complex;
//! use crate::dsp::rotate::Rotate;
//!
//! let mut rot = Rotate::new(std::f32::consts::PI * 25000.0 / 48000.0);
//! let input: Vec<Complex<f32>> = /* ... */;
//! let (up, down) = rot.receive_dual(&input, &mut tag);
//! ```
use super::{Stream, Tag};
use num_complex::Complex;

/// Rotate and split into two channels (A=up, B=down)
///
/// Maintains a complex phasor for rotation and a multiplier for the rotation step.
/// Used to frequency-shift and split input into two channels for SDR demodulation.
pub struct Rotate {
    /// Current rotation phasor.
    rot: Complex<f32>,
    /// Rotation step multiplier.
    mult: Complex<f32>,
}

impl Rotate {
    /// Create a new Rotate block
    /// angle: rotation angle per sample (e.g., PI * 25000.0 / 48000.0 for ±25kHz at 96kHz)
    pub fn new(angle: f32) -> Self {
        Self {
            rot: Complex::new(1.0, 0.0),
            mult: Complex::new(angle.cos(), angle.sin()),
        }
    }

    /// Set rotation angle
    pub fn set_rotation(&mut self, angle: f32) {
        self.mult = Complex::new(angle.cos(), angle.sin());
    }

    /// Process and return both channels (up, down)
    pub fn receive_dual(
        &mut self,
        data: &[Complex<f32>],
        _tag: &mut Tag,
    ) -> (Vec<Complex<f32>>, Vec<Complex<f32>>) {
        let mut output_up = Vec::with_capacity(data.len());
        let mut output_down = Vec::with_capacity(data.len());

        for &sample in data {
            let rr = sample.re * self.rot.re;
            let ii = sample.im * self.rot.im;
            let ri = sample.re * self.rot.im;
            let ir = sample.im * self.rot.re;

            // Channel A (output_up): standard complex multiply
            output_up.push(Complex::new(rr - ii, ir + ri));

            // Channel B (output_down): conjugate multiply
            output_down.push(Complex::new(rr + ii, ir - ri));

            // Update rotation phasor
            self.rot *= self.mult;
        }

        // Normalize to prevent drift
        let norm = self.rot.norm();
        if norm > 0.0 {
            self.rot /= norm;
        }

        (output_up, output_down)
    }
}

// Single channel stream (returns channel A only)
impl Stream<Complex<f32>, Complex<f32>> for Rotate {
    fn receive(&mut self, data: &[Complex<f32>], tag: &mut Tag) -> Vec<Complex<f32>> {
        let (up, _down) = self.receive_dual(data, tag);
        up
    }

    fn reset(&mut self) {
        self.rot = Complex::new(1.0, 0.0);
    }
}
