//! Automatic Frequency Correction (AFC) using Squared Signal FFT Analysis.
//!
//! This module provides an implementation of automatic frequency correction for complex baseband signals.
//! The main struct, [`SquareFreqOffsetCorrection`], estimates and corrects frequency offsets by squaring
//! the input signal, performing an FFT, and searching for the frequency offset peak. The estimated offset
//! is then used to rotate the output samples, compensating for the detected frequency error.
//!
//! # Features
//! - Squared signal FFT-based frequency offset estimation.
//! - Configurable FFT size and search window.
//! - Wide search mode for handling larger offsets.
//! - Efficient in-place FFT implementation.
//! - Integration with streaming interfaces via the [`Stream`] trait.
//!
//! # Example
//! ```rust
//! use num_complex::Complex;
//! use rs162::dsp::afc::SquareFreqOffsetCorrection;
//! use rs162::dsp::{Stream, Tag};
//!
//! let mut afc = SquareFreqOffsetCorrection::with_params(64, 4, false);
//! let mut tag = Tag::default();
//! let input = vec![Complex::new(0.1, 0.0); 64];
//! let corrected = afc.receive(&input, &mut tag);
//! assert_eq!(corrected.len(), input.len());
//! ```
//!
//! # Implementation Details
//! - The FFT is performed in-place using a radix-2 algorithm.
//! - Bit-reversal is used for sample ordering before FFT.
//! - Frequency correction is applied by rotating output samples with the estimated offset.
//! - The module is designed for real-time streaming applications.
//!
//! # References
//! - Digital Signal Processing techniques for AFC.
//! - FFT-based frequency estimation in communication systems.
//!
use super::{Stream, Tag};
use num_complex::Complex;
use std::f32::consts::PI;

/// Performs automatic frequency correction by analyzing the squared signal's
/// FFT.
///
/// This struct maintains buffers and state for frequency offset estimation and
/// correction.  It works by squaring the input complex samples, performing an
/// FFT, and searching for the frequency offset peak. The estimated offset is
/// then used to rotate the output samples to correct for the detected frequency
/// error.
pub struct SquareFreqOffsetCorrection {
    /// Buffer for squared input samples for FFT
    fft_data: Vec<Complex<f32>>,
    /// Buffer for output samples after correction
    output: Vec<Complex<f32>>,
    /// Cumulative sum buffer for wide search
    cumsum: Vec<f32>,
    /// Current complex rotation factor
    rot: Complex<f32>,
    /// FFT size (number of samples per window)
    n: usize,
    /// log2(n), used for bit-reversal and FFT
    log_n: usize,
    /// Current sample count in the window
    count: usize,
    /// Search window size for peak detection
    window: usize,
    /// Enables wide search mode for larger offsets
    wide: bool,
}

impl SquareFreqOffsetCorrection {
    pub fn with_params(n: usize, window: usize, wide: bool) -> Self {
        let log_n = (n as f32).log2() as usize;
        Self {
            fft_data: vec![Complex::new(0.0, 0.0); n],
            output: vec![Complex::new(0.0, 0.0); n],
            cumsum: vec![0.0; n],
            rot: Complex::new(1.0, 0.0),
            n,
            log_n,
            count: 0,
            window,
            wide,
        }
    }

    fn correct_frequency(&mut self) -> f32 {
        let n = self.n;

        // Perform FFT on squared signal
        fft_in_place(&mut self.fft_data);

        let delta = (9600.0 / 48000.0 * n as f32) as usize;
        let mut wi = 0;

        if self.wide {
            if self.cumsum.len() < n {
                self.cumsum.resize(n, 0.0);
            }

            let m = (12500.0 / 48000.0 * n as f32) as usize;
            let ofs = (m - delta) / 2;
            let mut wm = -1.0f32;

            self.cumsum[0] = 0.0;
            for i in 1..n {
                let p = self.fft_data[(i + n / 2) % n].norm();
                self.cumsum[i] = self.cumsum[i - 1] + p;
            }

            for i in 0..(n - m) {
                let v = self.cumsum[i + m] - self.cumsum[i]
                    + 0.6
                        * (self.fft_data[(i + ofs + n / 2) % n].norm()
                            + self.fft_data[(i + ofs + delta + n / 2) % n].norm());
                if v > wm {
                    wm = v;
                    wi = i;
                }
            }
            wi = wi + m / 2 - n / 2;
        }

        let mut max_val = 0.0f32;
        let mut fz = -1.0f32;

        for i in (wi + self.window)..(wi + n - self.window - delta) {
            let h = self.fft_data[(i + n / 2) % n].norm()
                + self.fft_data[(i + delta + n / 2) % n].norm();

            if h > max_val {
                max_val = h;
                fz = (n / 2) as f32 - (i as f32 + delta as f32 / 2.0);
            }
        }

        let f = fz / 2.0 / n as f32;
        let rot_step = Complex::new(0.0, f * 2.0 * PI).exp();

        for i in 0..n {
            self.rot *= rot_step;
            self.output[i] *= self.rot;
        }

        self.rot /= self.rot.norm();

        f * 48000.0 / 162.0
    }
}

impl Stream<Complex<f32>, Complex<f32>> for SquareFreqOffsetCorrection {
    fn receive(&mut self, data: &[Complex<f32>], tag: &mut Tag) -> Vec<Complex<f32>> {
        let mut result = Vec::new();

        if self.fft_data.len() < self.n {
            self.fft_data.resize(self.n, Complex::new(0.0, 0.0));
        }
        if self.output.len() < self.n {
            self.output.resize(self.n, Complex::new(0.0, 0.0));
        }

        for &sample in data {
            // Store squared sample at bit-reversed position
            let pos = reverse_bits(self.count, self.log_n);
            self.fft_data[pos] = sample * sample;
            self.output[self.count] = sample;

            self.count += 1;

            if self.count == self.n {
                // TODO yield together with tag here
                tag.ppm = self.correct_frequency();
                result.extend_from_slice(&self.output);
                self.count = 0;
            }
        }

        result
    }

    fn reset(&mut self) {
        self.rot = Complex::new(1.0, 0.0);
        self.count = 0;
    }
}

fn reverse_bits(mut n: usize, bits: usize) -> usize {
    let mut result = 0;
    for _ in 0..bits {
        result = (result << 1) | (n & 1);
        n >>= 1;
    }
    result
}

fn fft_in_place(data: &mut [Complex<f32>]) {
    let n = data.len();
    if n <= 1 {
        return;
    }

    let log_n = (n as f32).log2() as usize;

    // Pre-compute twiddle factors
    let mut omega: Vec<Complex<f32>> = Vec::with_capacity(n);
    for s in 0..n {
        let angle = -2.0 * PI * s as f32 / n as f32;
        omega.push(Complex::new(angle.cos(), angle.sin()));
    }

    // Radix-2 FFT
    let mut m = 2;
    let mut m2 = 1;
    let mut r = n;

    for _ in 0..log_n {
        let mut w = 0;
        r >>= 1;

        for j in 0..m2 {
            let o = omega[w];

            let mut k = 0;
            while k < n {
                let t = o * data[k + j + m2];
                data[k + j + m2] = data[k + j] - t;
                data[k + j] += t;
                k += m;
            }

            w += r;
        }

        m2 = m;
        m <<= 1;
    }
}
