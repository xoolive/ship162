//! Sample Rate Adapter for AIS Demodulation
//!
//! This module provides a unified interface for converting various sample rates
//! to the standard 96 kHz rate required by the AIS demodulator.
//!
//! Follows AIS-catcher's architecture with explicit decimation chains using
//! cascaded ÷2 (CIC5) and ÷3 (DSK filter) stages.
//!
//! # Supported Sample Rates
//!
//! ## Pure ÷2 Chains (CIC5 cascades):
//! - 96 kHz: Direct passthrough
//! - 192 kHz: ÷2
//! - 384 kHz: ÷2÷2
//! - 768 kHz: ÷2÷2÷2
//! - 1.536 MHz: ÷2÷2÷2÷2
//! - 3.072 MHz: ÷2÷2÷2÷2÷2
//! - 6.144 MHz: ÷2÷2÷2÷2÷2÷2
//! - 12.288 MHz: ÷2÷2÷2÷2÷2÷2÷2
//!
//! ## Mixed ÷2 and ÷3 Chains:
//! - 288 kHz: ÷3
//! - 576 kHz: ÷2 then ÷3
//! - 1.152 MHz: ÷2÷2 then ÷3
//! - 2.304 MHz: ÷2÷2÷2 then ÷3
//!
//! # Upsampling
//!
//! Non-standard sample rates are automatically upsampled to the nearest
//! supported rate. For example:
//! - 3 MS/s → upsampled to 3.072 MS/s → ÷2÷2÷2÷2÷2 → 96 kHz
//! - 6 MS/s → upsampled to 6.144 MS/s → ÷2÷2÷2÷2÷2÷2 → 96 kHz

use super::{cic5::Downsample2CIC5, fir, upsampler::SimpleUpsampler, Stream, Tag};
use num_complex::Complex;

/// Sample rate adapter that converts input samples to 96 kHz for AIS demodulation.
///
/// Automatically handles upsampling and decimation based on input sample rate.
///
/// # Example
///
/// ```
/// use rs162::dsp::sample_rate::SampleRateAdapter;
/// use num_complex::Complex;
///
/// // Create adapter for Airspy Mini at 3 MS/s
/// let mut adapter = SampleRateAdapter::new(3_000_000);
///
/// // Process samples - output will be at 96 kHz
/// let input: Vec<Complex<f32>> = vec![Complex::new(1.0, 0.0); 30000]; // 10ms at 3MS/s
/// let output = adapter.process(&input);
/// assert!(output.len() > 900 && output.len() < 1000); // ~960 samples (10ms at 96kHz)
/// ```
pub struct SampleRateAdapter {
    /// Optional upsampler for non-standard rates
    upsampler: Option<SimpleUpsampler>,
    /// Decimation chain variant
    chain: DecimationChain,
}

/// Decimation chain variants following AIS-catcher's explicit cascade structure
enum DecimationChain {
    /// 96 kHz: Direct passthrough (no decimation)
    Direct96k,

    /// 192 kHz: ÷2 (1 stage)
    From192k { ds1: Downsample2CIC5 },

    /// 384 kHz: ÷2÷2 (2 stages)
    From384k {
        ds2: Downsample2CIC5,
        ds1: Downsample2CIC5,
    },

    /// 768 kHz: ÷2÷2÷2 (3 stages)
    From768k {
        ds3: Downsample2CIC5,
        ds2: Downsample2CIC5,
        ds1: Downsample2CIC5,
    },

    /// 1.536 MHz: ÷2÷2÷2÷2 (4 stages)
    From1536k {
        ds4: Downsample2CIC5,
        ds3: Downsample2CIC5,
        ds2: Downsample2CIC5,
        ds1: Downsample2CIC5,
    },

    /// 3.072 MHz: ÷2÷2÷2÷2÷2 (5 stages)
    From3072k {
        ds5: Downsample2CIC5,
        ds4: Downsample2CIC5,
        ds3: Downsample2CIC5,
        ds2: Downsample2CIC5,
        ds1: Downsample2CIC5,
    },

    /// 6.144 MHz: ÷2÷2÷2÷2÷2÷2 (6 stages)
    From6144k {
        ds6: Downsample2CIC5,
        ds5: Downsample2CIC5,
        ds4: Downsample2CIC5,
        ds3: Downsample2CIC5,
        ds2: Downsample2CIC5,
        ds1: Downsample2CIC5,
    },

    /// 12.288 MHz: ÷2÷2÷2÷2÷2÷2÷2 (7 stages)
    From12288k {
        ds7: Downsample2CIC5,
        ds6: Downsample2CIC5,
        ds5: Downsample2CIC5,
        ds4: Downsample2CIC5,
        ds3: Downsample2CIC5,
        ds2: Downsample2CIC5,
        ds1: Downsample2CIC5,
    },

    /// 288 kHz: ÷3 (DSK filter)
    From288k { dsk: fir::DownsampleKFilter },

    /// 576 kHz: ÷2 then ÷3
    From576k {
        ds1: Downsample2CIC5,
        dsk: fir::DownsampleKFilter,
    },

    /// 1.152 MHz: ÷2÷2 then ÷3
    From1152k {
        ds2: Downsample2CIC5,
        ds1: Downsample2CIC5,
        dsk: fir::DownsampleKFilter,
    },

    /// 2.304 MHz: ÷2÷2÷2 then ÷3
    From2304k {
        ds3: Downsample2CIC5,
        ds2: Downsample2CIC5,
        ds1: Downsample2CIC5,
        dsk: fir::DownsampleKFilter,
    },
}

impl SampleRateAdapter {
    /// Create a new sample rate adapter for the specified input sample rate.
    ///
    /// Automatically selects the appropriate decimation chain and upsampler
    /// configuration based on the input rate.
    ///
    /// # Arguments
    ///
    /// * `sample_rate` - The input sample rate in Hz
    ///
    /// # Panics
    ///
    /// Panics if the sample rate cannot be handled (too low or too high).
    ///
    /// # Example
    ///
    /// ```
    /// use rs162::dsp::sample_rate::SampleRateAdapter;
    ///
    /// let adapter_96k = SampleRateAdapter::new(96_000);
    /// let adapter_3m = SampleRateAdapter::new(3_000_000);  // Airspy Mini
    /// let adapter_6m = SampleRateAdapter::new(6_000_000);  // Airspy Mini
    /// ```
    pub fn new(sample_rate: u32) -> Self {
        // Supported standard rates (exact match)
        const STANDARD_RATES: &[u32] = &[
            96_000, 192_000, 288_000, 384_000, 576_000, 768_000, 1_152_000, 1_536_000, 2_304_000,
            3_072_000, 6_144_000, 12_288_000,
        ];

        // Check if we have an exact match
        if STANDARD_RATES.contains(&sample_rate) {
            return Self {
                upsampler: None,
                chain: DecimationChain::new(sample_rate),
            };
        }

        // Find the nearest higher standard rate for upsampling
        let target_rate = STANDARD_RATES
            .iter()
            .find(|&&r| r > sample_rate)
            .copied()
            .unwrap_or_else(|| {
                panic!(
                    "Sample rate {} Hz is too high (max supported: {} Hz)",
                    sample_rate, 12_288_000
                )
            });

        // Create upsampler and decimation chain
        Self {
            upsampler: Some(SimpleUpsampler::new(sample_rate, target_rate)),
            chain: DecimationChain::new(target_rate),
        }
    }

    /// Process input samples and convert to 96 kHz output.
    ///
    /// # Arguments
    ///
    /// * `samples` - Input IQ samples at the configured sample rate
    ///
    /// # Returns
    ///
    /// A vector of IQ samples at 96 kHz, ready for AIS demodulation.
    pub fn process(&mut self, samples: &[Complex<f32>]) -> Vec<Complex<f32>> {
        let mut tag = Tag::default();

        // Step 1: Upsample if needed
        let upsampled = if let Some(ref mut upsampler) = self.upsampler {
            upsampler.receive(samples, &mut tag)
        } else {
            samples.to_vec()
        };

        // Step 2: Decimate to 96 kHz
        self.chain.receive(&upsampled, &mut tag)
    }

    /// Reset the internal state of the adapter.
    ///
    /// Clears any buffered samples or filter state.
    pub fn reset(&mut self) {
        if let Some(ref mut upsampler) = self.upsampler {
            upsampler.reset();
        }
        self.chain.reset();
    }

    /// Get the input sample rate this adapter expects.
    pub fn input_sample_rate(&self) -> u32 {
        if let Some(ref upsampler) = self.upsampler {
            upsampler.input_rate()
        } else {
            self.chain.input_rate()
        }
    }

    /// Get the output sample rate (always 96 kHz).
    pub const fn output_sample_rate(&self) -> u32 {
        96_000
    }
}

impl DecimationChain {
    fn new(sample_rate: u32) -> Self {
        match sample_rate {
            96_000 => Self::Direct96k,

            192_000 => Self::From192k {
                ds1: Downsample2CIC5::new(),
            },

            384_000 => Self::From384k {
                ds2: Downsample2CIC5::new(),
                ds1: Downsample2CIC5::new(),
            },

            768_000 => Self::From768k {
                ds3: Downsample2CIC5::new(),
                ds2: Downsample2CIC5::new(),
                ds1: Downsample2CIC5::new(),
            },

            1_536_000 => Self::From1536k {
                ds4: Downsample2CIC5::new(),
                ds3: Downsample2CIC5::new(),
                ds2: Downsample2CIC5::new(),
                ds1: Downsample2CIC5::new(),
            },

            3_072_000 => Self::From3072k {
                ds5: Downsample2CIC5::new(),
                ds4: Downsample2CIC5::new(),
                ds3: Downsample2CIC5::new(),
                ds2: Downsample2CIC5::new(),
                ds1: Downsample2CIC5::new(),
            },

            6_144_000 => Self::From6144k {
                ds6: Downsample2CIC5::new(),
                ds5: Downsample2CIC5::new(),
                ds4: Downsample2CIC5::new(),
                ds3: Downsample2CIC5::new(),
                ds2: Downsample2CIC5::new(),
                ds1: Downsample2CIC5::new(),
            },

            12_288_000 => Self::From12288k {
                ds7: Downsample2CIC5::new(),
                ds6: Downsample2CIC5::new(),
                ds5: Downsample2CIC5::new(),
                ds4: Downsample2CIC5::new(),
                ds3: Downsample2CIC5::new(),
                ds2: Downsample2CIC5::new(),
                ds1: Downsample2CIC5::new(),
            },

            288_000 => Self::From288k {
                dsk: fir::DownsampleKFilter::with_params(3, fir::BLACKMAN_HARRIS_28_3),
            },

            576_000 => Self::From576k {
                ds1: Downsample2CIC5::new(),
                dsk: fir::DownsampleKFilter::with_params(3, fir::BLACKMAN_HARRIS_28_3),
            },

            1_152_000 => Self::From1152k {
                ds2: Downsample2CIC5::new(),
                ds1: Downsample2CIC5::new(),
                dsk: fir::DownsampleKFilter::with_params(3, fir::BLACKMAN_HARRIS_28_3),
            },

            2_304_000 => Self::From2304k {
                ds3: Downsample2CIC5::new(),
                ds2: Downsample2CIC5::new(),
                ds1: Downsample2CIC5::new(),
                dsk: fir::DownsampleKFilter::with_params(3, fir::BLACKMAN_HARRIS_28_3),
            },

            _ => panic!(
                "Unsupported standard sample rate: {}. This is an internal error.",
                sample_rate
            ),
        }
    }

    fn receive(&mut self, samples: &[Complex<f32>], tag: &mut Tag) -> Vec<Complex<f32>> {
        match self {
            Self::Direct96k => samples.to_vec(),

            Self::From192k { ds1 } => ds1.receive(samples, tag),

            Self::From384k { ds2, ds1 } => {
                let temp = ds2.receive(samples, tag);
                ds1.receive(&temp, tag)
            }

            Self::From768k { ds3, ds2, ds1 } => {
                let temp1 = ds3.receive(samples, tag);
                let temp2 = ds2.receive(&temp1, tag);
                ds1.receive(&temp2, tag)
            }

            Self::From1536k { ds4, ds3, ds2, ds1 } => {
                let temp1 = ds4.receive(samples, tag);
                let temp2 = ds3.receive(&temp1, tag);
                let temp3 = ds2.receive(&temp2, tag);
                ds1.receive(&temp3, tag)
            }

            Self::From3072k {
                ds5,
                ds4,
                ds3,
                ds2,
                ds1,
            } => {
                let temp1 = ds5.receive(samples, tag);
                let temp2 = ds4.receive(&temp1, tag);
                let temp3 = ds3.receive(&temp2, tag);
                let temp4 = ds2.receive(&temp3, tag);
                ds1.receive(&temp4, tag)
            }

            Self::From6144k {
                ds6,
                ds5,
                ds4,
                ds3,
                ds2,
                ds1,
            } => {
                let temp1 = ds6.receive(samples, tag);
                let temp2 = ds5.receive(&temp1, tag);
                let temp3 = ds4.receive(&temp2, tag);
                let temp4 = ds3.receive(&temp3, tag);
                let temp5 = ds2.receive(&temp4, tag);
                ds1.receive(&temp5, tag)
            }

            Self::From12288k {
                ds7,
                ds6,
                ds5,
                ds4,
                ds3,
                ds2,
                ds1,
            } => {
                let temp1 = ds7.receive(samples, tag);
                let temp2 = ds6.receive(&temp1, tag);
                let temp3 = ds5.receive(&temp2, tag);
                let temp4 = ds4.receive(&temp3, tag);
                let temp5 = ds3.receive(&temp4, tag);
                let temp6 = ds2.receive(&temp5, tag);
                ds1.receive(&temp6, tag)
            }

            Self::From288k { dsk } => dsk.receive(samples, tag),

            Self::From576k { ds1, dsk } => {
                let temp = ds1.receive(samples, tag);
                dsk.receive(&temp, tag)
            }

            Self::From1152k { ds2, ds1, dsk } => {
                let temp1 = ds2.receive(samples, tag);
                let temp2 = ds1.receive(&temp1, tag);
                dsk.receive(&temp2, tag)
            }

            Self::From2304k { ds3, ds2, ds1, dsk } => {
                let temp1 = ds3.receive(samples, tag);
                let temp2 = ds2.receive(&temp1, tag);
                let temp3 = ds1.receive(&temp2, tag);
                dsk.receive(&temp3, tag)
            }
        }
    }

    fn reset(&mut self) {
        match self {
            Self::Direct96k => {}

            Self::From192k { ds1 } => {
                ds1.reset();
            }

            Self::From384k { ds2, ds1 } => {
                ds2.reset();
                ds1.reset();
            }

            Self::From768k { ds3, ds2, ds1 } => {
                ds3.reset();
                ds2.reset();
                ds1.reset();
            }

            Self::From1536k { ds4, ds3, ds2, ds1 } => {
                ds4.reset();
                ds3.reset();
                ds2.reset();
                ds1.reset();
            }

            Self::From3072k {
                ds5,
                ds4,
                ds3,
                ds2,
                ds1,
            } => {
                ds5.reset();
                ds4.reset();
                ds3.reset();
                ds2.reset();
                ds1.reset();
            }

            Self::From6144k {
                ds6,
                ds5,
                ds4,
                ds3,
                ds2,
                ds1,
            } => {
                ds6.reset();
                ds5.reset();
                ds4.reset();
                ds3.reset();
                ds2.reset();
                ds1.reset();
            }

            Self::From12288k {
                ds7,
                ds6,
                ds5,
                ds4,
                ds3,
                ds2,
                ds1,
            } => {
                ds7.reset();
                ds6.reset();
                ds5.reset();
                ds4.reset();
                ds3.reset();
                ds2.reset();
                ds1.reset();
            }

            Self::From288k { dsk } => {
                dsk.reset();
            }

            Self::From576k { ds1, dsk } => {
                ds1.reset();
                dsk.reset();
            }

            Self::From1152k { ds2, ds1, dsk } => {
                ds2.reset();
                ds1.reset();
                dsk.reset();
            }

            Self::From2304k { ds3, ds2, ds1, dsk } => {
                ds3.reset();
                ds2.reset();
                ds1.reset();
                dsk.reset();
            }
        }
    }

    fn input_rate(&self) -> u32 {
        match self {
            Self::Direct96k => 96_000,
            Self::From192k { .. } => 192_000,
            Self::From384k { .. } => 384_000,
            Self::From768k { .. } => 768_000,
            Self::From1536k { .. } => 1_536_000,
            Self::From3072k { .. } => 3_072_000,
            Self::From6144k { .. } => 6_144_000,
            Self::From12288k { .. } => 12_288_000,
            Self::From288k { .. } => 288_000,
            Self::From576k { .. } => 576_000,
            Self::From1152k { .. } => 1_152_000,
            Self::From2304k { .. } => 2_304_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_creation_96k() {
        let adapter = SampleRateAdapter::new(96_000);
        assert_eq!(adapter.input_sample_rate(), 96_000);
        assert_eq!(adapter.output_sample_rate(), 96_000);
    }

    #[test]
    fn test_adapter_creation_288k() {
        let adapter = SampleRateAdapter::new(288_000);
        assert_eq!(adapter.input_sample_rate(), 288_000);
        assert_eq!(adapter.output_sample_rate(), 96_000);
    }

    #[test]
    fn test_adapter_creation_3m_upsampled() {
        let adapter = SampleRateAdapter::new(3_000_000);
        assert_eq!(adapter.input_sample_rate(), 3_000_000);
        assert_eq!(adapter.output_sample_rate(), 96_000);
    }

    #[test]
    fn test_adapter_creation_3072k_exact() {
        let adapter = SampleRateAdapter::new(3_072_000);
        assert_eq!(adapter.input_sample_rate(), 3_072_000);
        assert_eq!(adapter.output_sample_rate(), 96_000);
    }

    #[test]
    fn test_direct_96k_passthrough() {
        let mut adapter = SampleRateAdapter::new(96_000);
        let input: Vec<Complex<f32>> = vec![Complex::new(1.0, 0.5); 960];
        let output = adapter.process(&input);
        assert_eq!(output.len(), 960);
    }

    #[test]
    fn test_from_192k() {
        let mut adapter = SampleRateAdapter::new(192_000);
        let input: Vec<Complex<f32>> = vec![Complex::new(1.0, 0.0); 1920]; // 10ms
        let output = adapter.process(&input);
        assert_eq!(output.len(), 960); // 10ms at 96k
    }

    #[test]
    fn test_from_3m_upsampled() {
        let mut adapter = SampleRateAdapter::new(3_000_000);
        let input: Vec<Complex<f32>> = vec![Complex::new(1.0, 0.0); 30000]; // 10ms
        let output = adapter.process(&input);
        // Allow margin for filter group delay
        assert!(output.len() > 900 && output.len() < 1000);
    }

    #[test]
    fn test_from_6m_upsampled() {
        let mut adapter = SampleRateAdapter::new(6_000_000);
        let input: Vec<Complex<f32>> = vec![Complex::new(1.0, 0.0); 60000]; // 10ms
        let output = adapter.process(&input);
        // Allow margin for filter group delay
        assert!(output.len() > 900 && output.len() < 1000);
    }

    #[test]
    fn test_reset() {
        let mut adapter = SampleRateAdapter::new(288_000);
        let input: Vec<Complex<f32>> = vec![Complex::new(1.0, 0.0); 2880];
        let _ = adapter.process(&input);
        adapter.reset();
        let output = adapter.process(&input);
        assert!(!output.is_empty());
    }
}
