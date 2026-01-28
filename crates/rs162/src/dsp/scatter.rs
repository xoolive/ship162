//! Distribute incoming complex samples across multiple timing phases for symbol recovery
//!
//! This module provides the `ScatterPLL` struct, which maintains state for round-robin distribution of samples
//! into multiple timing phases. This aids in symbol timing recovery by organizing samples according to their phase,
//! allowing downstream processing to analyze and recover symbols more effectively.
//!
//! # Features
//! - Configurable number of timing phases.
//! - Tracks sample indices and energy levels per phase.
//! - Updates tagging information for each phase to assist with downstream processing.
//!
//! # Usage
//! Instantiate `ScatterPLL` with the desired number of phases, then call `receive_scatter` with incoming samples
//! and a mutable tag to distribute samples and update phase information.
//!
//! # Example
//! ```rust
//! use num_complex::Complex;
//! use rs162::dsp::scatter::ScatterPLL;
//! use rs162::dsp::Tag;
//!
//! let mut pll = ScatterPLL::new(4);
//! let data = vec![Complex::new(1.0, 0.0), Complex::new(0.0, 1.0)];
//! let mut tag = Tag::default();
//! let outputs = pll.receive_scatter(&data, &mut tag);
//! assert_eq!(outputs.len(), 4);
//! ```
use super::Tag;
use num_complex::Complex;

/// ScatterPLL distributes samples across multiple timing phases for symbol recovery.
///
/// Maintains state for distributing incoming samples round-robin across timing phases,
/// aiding in symbol timing recovery for SDR demodulation.
pub struct ScatterPLL {
    /// Number of timing phases.
    n_phases: usize,
    /// Index of the last symbol phase used.
    last_symbol: usize,
    /// Global sample index counter.
    sample_idx: usize,
}

impl ScatterPLL {
    pub fn new(n_phases: usize) -> Self {
        Self {
            n_phases,
            last_symbol: 0,
            sample_idx: 0,
        }
    }

    /// Scatter samples across timing phases (round-robin)
    pub fn receive_scatter(
        &mut self,
        data: &[Complex<f32>],
        tag: &mut Tag,
    ) -> Vec<Vec<Complex<f32>>> {
        let mut outputs = vec![Vec::new(); self.n_phases];
        let mut levels = vec![0.0f32; self.n_phases];
        let mut idxs = vec![0usize; self.n_phases];

        for &sample in data.iter() {
            outputs[self.last_symbol].push(sample);
            levels[self.last_symbol] += sample.norm_sqr();
            idxs[self.last_symbol] = self.sample_idx;
            self.last_symbol = (self.last_symbol + 1) % self.n_phases;
            self.sample_idx += 1;
        }

        // Update tag for each phase (last sample in each output)
        for i in 0..self.n_phases {
            if !outputs[i].is_empty() {
                tag.sample_idx = idxs[i];
                tag.sample_lvl = levels[i] / outputs[i].len() as f32;
                // If you want to store per-phase tags, clone tag here
                // Or pass tag to downstream processing
            }
        }

        outputs
    }
}
