//! Simple Linear Interpolation Upsampler
//!
//! This module provides a simple upsampler using linear interpolation between samples.
//! It is ported from AIS-catcher's `Upsample` class and is designed for small upsampling
//! ratios (e.g., 3.0 MS/s → 3.072 MS/s, ratio = 1.024).
//!
//! The linear interpolation approach is efficient and proven effective for AIS reception
//! in real-world deployments, avoiding the complexity and overhead of sinc interpolation
//! libraries like rubato.

use super::{Stream, Tag};
use num_complex::Complex;

/// Simple linear interpolation upsampler
///
/// Upsamples IQ samples using linear interpolation between consecutive samples.
/// The upsampling ratio is specified as `rate_out / rate_in`.
///
/// # Example
///
/// ```
/// use rs162::dsp::{upsampler::SimpleUpsampler, Stream, Tag};
/// use num_complex::Complex;
///
/// let mut upsampler = SimpleUpsampler::new(3_000_000, 3_072_000);
/// let input = vec![Complex::new(1.0, 0.0), Complex::new(0.0, 1.0)];
/// let mut tag = Tag::default();
/// let output = upsampler.receive(&input, &mut tag);
/// // Output will have approximately 1.024x more samples than input
/// ```
#[derive(Debug, Clone)]
pub struct SimpleUpsampler {
    /// Input sample rate in Hz
    rate_in: u32,
    /// Output sample rate in Hz
    rate_out: u32,
    /// Interpolation phase accumulator (0.0 to 1.0)
    alpha: f32,
    /// Phase increment per input sample (rate_in / rate_out)
    increment: f32,
    /// Previous input sample for interpolation
    prev_sample: Complex<f32>,
    /// Internal output buffer
    output: Vec<Complex<f32>>,
    /// Current write position in output buffer
    idx_out: usize,
}

impl SimpleUpsampler {
    /// Create a new upsampler with the specified input and output sample rates
    ///
    /// # Arguments
    ///
    /// * `rate_in` - Input sample rate in Hz (e.g., 3_000_000 for 3 MS/s)
    /// * `rate_out` - Output sample rate in Hz (e.g., 3_072_000 for 3.072 MS/s)
    ///
    /// # Panics
    ///
    /// Panics if `rate_in > rate_out` (downsampling not supported) or if either rate is zero.
    pub fn new(rate_in: u32, rate_out: u32) -> Self {
        assert!(rate_in > 0, "Input sample rate must be greater than 0");
        assert!(rate_out > 0, "Output sample rate must be greater than 0");
        assert!(
            rate_in <= rate_out,
            "SimpleUpsampler only supports upsampling (rate_in <= rate_out)"
        );

        let increment = rate_in as f32 / rate_out as f32;

        Self {
            rate_in,
            rate_out,
            alpha: 0.0,
            increment,
            prev_sample: Complex::new(0.0, 0.0),
            output: Vec::new(),
            idx_out: 0,
        }
    }

    /// Get the input sample rate in Hz
    pub fn input_rate(&self) -> u32 {
        self.rate_in
    }

    /// Get the output sample rate in Hz
    pub fn output_rate(&self) -> u32 {
        self.rate_out
    }

    /// Get the upsampling ratio (rate_out / rate_in)
    pub fn ratio(&self) -> f32 {
        1.0 / self.increment
    }

    /// Reset the upsampler state
    pub fn reset(&mut self) {
        self.alpha = 0.0;
        self.prev_sample = Complex::new(0.0, 0.0);
        self.idx_out = 0;
        self.output.clear();
    }
}

impl Stream<Complex<f32>, Complex<f32>> for SimpleUpsampler {
    /// Process input samples and produce upsampled output
    ///
    /// Uses linear interpolation: `output = (1 - alpha) * prev + alpha * current`
    ///
    /// The algorithm follows AIS-catcher's implementation:
    /// 1. For each input sample, generate multiple output samples by interpolating
    /// 2. Increment alpha by `increment` for each output sample
    /// 3. When alpha >= 1.0, move to the next input sample and wrap alpha
    fn receive(&mut self, data: &[Complex<f32>], _tag: &mut Tag) -> Vec<Complex<f32>> {
        if data.is_empty() {
            return Vec::new();
        }

        // Estimate output size: input_len * ratio, rounded up
        let estimated_output_len = ((data.len() as f32 * self.ratio()).ceil() as usize).max(1);

        // Pre-allocate output buffer
        let mut output = Vec::with_capacity(estimated_output_len + 1);

        let mut a = self.prev_sample;
        let mut alpha = self.alpha;

        for &b in data {
            // Generate output samples by interpolating between a and b
            loop {
                // Linear interpolation: (1 - alpha) * a + alpha * b
                let interpolated = a * (1.0 - alpha) + b * alpha;
                output.push(interpolated);

                alpha += self.increment;

                if alpha >= 1.0 {
                    break;
                }
            }

            // Move to next input sample
            alpha -= 1.0;
            a = b;
        }

        // Save state for next call
        self.prev_sample = a;
        self.alpha = alpha;

        output
    }

    fn reset(&mut self) {
        Self::reset(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upsampler_creation() {
        let upsampler = SimpleUpsampler::new(3_000_000, 3_072_000);
        assert!((upsampler.ratio() - 1.024).abs() < 0.001);
    }

    #[test]
    fn test_upsampler_interpolation() {
        let mut upsampler = SimpleUpsampler::new(1, 3); // 3x upsampling for clearer interpolation
        let input = vec![Complex::new(0.0, 0.0), Complex::new(1.0, 1.0)];

        let mut tag = Tag::default();
        let output = upsampler.receive(&input, &mut tag);

        // With 3x upsampling, we should get interpolated values between samples
        assert!(output.len() >= 3);

        // First output should be close to first input
        assert!((output[0].re - 0.0).abs() < 0.1);

        // Some outputs should be interpolated between 0.0 and 1.0
        let has_interpolated = output.iter().any(|s| s.re > 0.1 && s.re < 0.9);
        assert!(
            has_interpolated,
            "Should have interpolated samples between 0 and 1"
        );
    }

    #[test]
    #[should_panic(expected = "SimpleUpsampler only supports upsampling")]
    fn test_upsampler_downsampling_panics() {
        SimpleUpsampler::new(3_072_000, 3_000_000);
    }

    #[test]
    fn test_upsampler_output_count() {
        let mut upsampler = SimpleUpsampler::new(3_000_000, 3_072_000);
        let input_len = 1000;
        let input: Vec<Complex<f32>> = (0..input_len)
            .map(|i| Complex::new(i as f32, (i as f32) * 0.5))
            .collect();

        let mut tag = Tag::default();
        let output = upsampler.receive(&input, &mut tag);

        // Expected output length: input_len * (3.072 / 3.0) ≈ input_len * 1.024
        let expected_len = (input_len as f32 * 1.024).round() as usize;

        // Allow ±1 sample tolerance due to phase accumulation
        assert!(
            output.len() >= expected_len - 1 && output.len() <= expected_len + 1,
            "Output length {} not within ±1 of expected {}",
            output.len(),
            expected_len
        );
    }

    #[test]
    fn test_upsampler_no_phase_shift_i_q() {
        let mut upsampler = SimpleUpsampler::new(3_000_000, 3_072_000);
        let input: Vec<Complex<f32>> = (0..100)
            .map(|i| Complex::new((i as f32).sin(), (i as f32).cos()))
            .collect();

        let mut tag = Tag::default();
        let output = upsampler.receive(&input, &mut tag);

        // Verify I and Q are both interpolated (no channel should be zero unless input is zero)
        let non_zero_i = output.iter().filter(|s| s.re.abs() > 0.01).count();
        let non_zero_q = output.iter().filter(|s| s.im.abs() > 0.01).count();

        assert!(non_zero_i > output.len() / 2, "I channel has samples");
        assert!(non_zero_q > output.len() / 2, "Q channel has samples");
    }

    #[test]
    fn test_upsampler_reset() {
        let mut upsampler = SimpleUpsampler::new(3_000_000, 3_072_000);
        let input = vec![Complex::new(1.0, 1.0)];

        let mut tag = Tag::default();
        let _ = upsampler.receive(&input, &mut tag);

        upsampler.reset();

        assert_eq!(upsampler.alpha, 0.0);
        assert_eq!(upsampler.prev_sample, Complex::new(0.0, 0.0));
    }

    #[test]
    fn test_upsampler_streaming() {
        let mut upsampler = SimpleUpsampler::new(3_000_000, 3_072_000);

        // Process in chunks to test streaming behavior
        let chunk1: Vec<Complex<f32>> = (0..100).map(|i| Complex::new(i as f32, 0.0)).collect();
        let chunk2: Vec<Complex<f32>> = (100..200).map(|i| Complex::new(i as f32, 0.0)).collect();

        let mut tag = Tag::default();
        let output1 = upsampler.receive(&chunk1, &mut tag);
        let output2 = upsampler.receive(&chunk2, &mut tag);

        // Both chunks should produce output
        assert!(!output1.is_empty());
        assert!(!output2.is_empty());

        // Total output should be approximately (chunk1 + chunk2) * 1.024
        let total_input = chunk1.len() + chunk2.len();
        let total_output = output1.len() + output2.len();
        let expected = (total_input as f32 * 1.024).round() as usize;

        assert!(
            total_output >= expected - 2 && total_output <= expected + 2,
            "Total output {} not within ±2 of expected {}",
            total_output,
            expected
        );
    }
}
