//! Automatic Identification System (AIS) Demodulation Module
//!
//! This module provides a simplified AIS (Automatic Identification System) demodulator for marine VHF signals,
//! supporting sample rates of 96kHz and 288kHz. AIS operates at 161.975 MHz (Channel A) and 162.025 MHz (Channel B),
//! transmitting data at 9600 baud using GMSK modulation. The demodulator processes IQ samples through a modular DSP pipeline,
//! including downsampling, frequency rotation, filtering, frequency correction, symbol timing recovery, and message decoding.
//!
//! Key Features:
//! - Supports both AIS channels (A and B) with independent DSP pipelines.
//! - Modular DSP blocks for downsampling, filtering, frequency correction, and symbol recovery.
//! - Symbol timing recovery using Scatter PLL and phase search demodulation.
//! - Stateful message decoding with NRZI decoding, HDLC framing, bit stuffing, and CRC validation.
//! - Extracts and validates AIS messages, providing signal quality indicators and timestamps.
//!
//! Main Types:
//! - `AisDemodulator`: The main struct encapsulating the DSP pipeline and persistent decoder state.
//! - `AisDemodulatedMessage`: Represents a decoded AIS message, including payload, signal level, channel, and timestamp.
//!
//! Usage:
//! 1. Create an `AisDemodulator` instance with the desired sample rate.
//! 2. Feed IQ samples to the `demodulate` method to extract valid AIS messages.
//!
//! This module is designed for integration into SDR (Software Defined Radio) applications and AIS receivers.
//! It assumes the presence of supporting DSP blocks (FIR, CIC, AFC, PLL, etc.) in the crate.
//!

use crate::decode::nmea::NmeaAisMessage;
use crate::dsp::*;
use crate::prelude::Message;
use deku::reader::Reader;
use deku::DekuReader;
use num_complex::Complex;
use std::collections::HashSet;
use std::hash::Hash;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

/// AIS operates at 161.975 MHz (Channel A) and 162.025 MHz (Channel B)
/// Data rate is 9600 baud using GMSK modulation
pub const AIS_FREQ_A: f64 = 161.975e6;
pub const AIS_FREQ_B: f64 = 162.025e6;
pub const AIS_BAUD_RATE: f64 = 9600.0;

/// Standard sampling rate for simplified AIS demodulation
pub const AIS_SAMPLE_RATE_96K: u32 = 96000;
pub const AIS_SAMPLE_RATE_288K: u32 = 288000;

const MAX_AIS_LENGTH: usize = 128 * 8; // max bits in AIS message including FCS
const MIN_TRAINING_BITS: usize = 18;
const EXPECTED_CRC: u16 = 0xF0B8;

/// AIS demodulated message
#[derive(Debug, Clone)]
pub struct AisDemodulatedMessage {
    /// The decoded AIS message payload, as a vector of bytes.
    /// This excludes the FCS (Frame Check Sequence) and is the result of differential decoding.
    pub bits: Vec<u8>,
    /// The average signal strength for the message, used as a quality indicator.
    /// Calculated from the input samples during demodulation.
    pub signal_level: f32,
    /// The AIS channel on which the message was received.
    /// 'A' for 161.975 MHz, 'B' for 162.025 MHz.
    pub channel: char,
    /// The timestamp (seconds since UNIX epoch) when the message was decoded.
    /// Used for time correlation and logging.
    pub timestamp: u64,
    /// The NMEA sentences generated from this AIS message.
    pub nmea_sentences: Vec<String>,
}

impl PartialEq for AisDemodulatedMessage {
    fn eq(&self, other: &Self) -> bool {
        self.bits == other.bits && self.channel == other.channel
    }
}
impl Eq for AisDemodulatedMessage {}
impl Hash for AisDemodulatedMessage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.bits.hash(state);
        self.channel.hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Training,
    StartFlag,
    DataFcs,
}
/// Decoder state for a single channel/phase combination
#[derive(Debug, Clone)]
struct DecoderState {
    /// The current state of the decoder state machine.
    /// Can be Training (searching for preamble), StartFlag (searching for HDLC flag), or DataFcs (collecting message bits).
    state: State,
    /// The position or progress within the current state.
    /// Used to count bits or symbols for state transitions.
    position: usize,
    /// The count of consecutive '1' bits, used for bit stuffing detection in HDLC framing.
    one_seq_count: u8,
    /// The previous decoded symbol value (0 or 1), used for NRZI decoding.
    prev_d: u8,
    /// The last decoded bit value, used for detecting alternating patterns in the preamble.
    last_bit: u8,
    /// The vector of bits currently being accumulated for the message.
    /// This is cleared when a new message starts and filled until a valid message is detected.
    msg_bits: Vec<u8>,
    /// The accumulated signal level for all samples in the current message.
    /// Used to compute the average signal level for the message.
    level_accumulator: f32,
    /// The number of samples accumulated for signal level calculation.
    /// Used to compute the average signal level for the message.
    level_count: usize,
}

impl Default for DecoderState {
    fn default() -> Self {
        Self {
            state: State::Training,
            position: 0,
            one_seq_count: 0,
            prev_d: 0,
            last_bit: 0,
            msg_bits: Vec::with_capacity(MAX_AIS_LENGTH),
            level_accumulator: 0.0,
            level_count: 0,
        }
    }
}

/// Simplified AIS demodulator for 96kHz sample rate using modular DSP blocks.
///
/// This struct contains all DSP blocks and persistent state needed to demodulate AIS signals.
/// Each field represents a stage in the DSP pipeline or persistent state for message decoding.
///
/// **Important:** This demodulator only operates at 96 kHz. If your input samples are at a
/// different rate (288 kHz or 3 MS/s), use [`sample_rate::SampleRateAdapter`] to
/// convert them to 96 kHz first.
pub struct AisDemodulator {
    /// Frequency rotation and channel splitting.
    /// Rotates the input IQ samples to separate channels A and B (±25kHz offset).
    rotate: rotate::Rotate,
    /// CIC5 downsampler for channel A.
    /// Reduces sample rate by a factor of 2 for channel A.
    ds2a: cic5::Downsample2CIC5,
    /// CIC5 downsampler for channel B.
    /// Reduces sample rate by a factor of 2 for channel B.
    ds2b: cic5::Downsample2CIC5,
    /// CIC5 filter for channel A.
    /// Applies a 5-stage CIC filter to channel A for smoothing.
    fcic5a: cic5::FilterCIC5,
    /// CIC5 filter for channel B.
    /// Applies a 5-stage CIC filter to channel B for smoothing.
    fcic5b: cic5::FilterCIC5,
    /// Automatic frequency correction for channel A.
    /// Estimates and corrects frequency offset for channel A using squared FFT.
    cgf_a: afc::SquareFreqOffsetCorrection,
    /// Automatic frequency correction for channel B.
    /// Estimates and corrects frequency offset for channel B using squared FFT.
    cgf_b: afc::SquareFreqOffsetCorrection,
    /// Coherent FIR filter for channel A.
    /// Applies a matched FIR filter to channel A for symbol recovery.
    fc_a: fir::FilterComplex,
    /// Coherent FIR filter for channel B.
    /// Applies a matched FIR filter to channel B for symbol recovery.
    fc_b: fir::FilterComplex,
    /// Scatter PLL for channel A (timing phase distribution).
    /// Distributes samples for channel A across multiple timing phases for symbol alignment.
    s_a: scatter::ScatterPLL,
    /// Scatter PLL for channel B (timing phase distribution).
    /// Distributes samples for channel B across multiple timing phases for symbol alignment.
    s_b: scatter::ScatterPLL,
    /// Phase search demodulators for channel A (one per timing phase).
    /// Each element demodulates symbols for a specific timing phase in channel A.
    cd_ema_a: Vec<ema::PhaseSearchEMA>,
    /// Phase search demodulators for channel B (one per timing phase).
    /// Each element demodulates symbols for a specific timing phase in channel B.
    cd_ema_b: Vec<ema::PhaseSearchEMA>,

    /// Persistent decoder states for channel A (one per timing phase).
    /// Maintains state for decoding messages that span multiple symbol vectors in channel A.
    decoder_states_a: Vec<DecoderState>,
    /// Persistent decoder states for channel B (one per timing phase).
    /// Maintains state for decoding messages that span multiple symbol vectors in channel B.
    decoder_states_b: Vec<DecoderState>,
}

impl Default for AisDemodulator {
    fn default() -> Self {
        Self::new()
    }
}

impl AisDemodulator {
    /// Create a new AIS demodulator instance for 96 kHz sample rate.
    ///
    /// Initializes all DSP blocks and persistent state required for AIS demodulation.
    ///
    /// **Note:** This demodulator only accepts 96 kHz input. Use
    /// [`sample_rate::SampleRateAdapter`] to convert from other sample rates.
    ///
    /// # Example
    ///
    /// ```
    /// use rs162::dsp::ais::AisDemodulator;
    ///
    /// let mut demod = AisDemodulator::new();
    /// // Feed 96 kHz samples to demod.demodulate()
    /// ```
    pub fn new() -> Self {
        Self {
            rotate: rotate::Rotate::new(std::f32::consts::PI * 25000.0 / 48000.0),
            ds2a: cic5::Downsample2CIC5::new(),
            ds2b: cic5::Downsample2CIC5::new(),
            fcic5a: cic5::FilterCIC5::new(),
            fcic5b: cic5::FilterCIC5::new(),
            cgf_a: afc::SquareFreqOffsetCorrection::with_params(512, 187, false),
            cgf_b: afc::SquareFreqOffsetCorrection::with_params(512, 187, false),
            fc_a: fir::FilterComplex::with_taps(fir::COHERENT_TAPS),
            fc_b: fir::FilterComplex::with_taps(fir::COHERENT_TAPS),
            s_a: scatter::ScatterPLL::new(5), // this is 96000 / 9600 = 5
            s_b: scatter::ScatterPLL::new(5), // this is 96000 / 9600 = 5
            // Initialize 5 phase search states (48kHz / 9600 baud = 5 samples per symbol)
            cd_ema_a: (0..5)
                .map(|_| ema::PhaseSearchEMA::with_params(3))
                .collect(),
            cd_ema_b: (0..5)
                .map(|_| ema::PhaseSearchEMA::with_params(3))
                .collect(),
            // Initialize decoder states for 5 phases per channel
            decoder_states_a: vec![DecoderState::default(); 5],
            decoder_states_b: vec![DecoderState::default(); 5],
        }
    }

    /// Demodulate a slice of IQ samples and extract AIS messages.
    ///
    /// This is the main entry point for AIS demodulation. It processes the input samples
    /// through the DSP pipeline, including frequency rotation, filtering,
    /// frequency correction, symbol timing recovery, and message decoding.
    ///
    /// **Important:** Input samples must be at 96 kHz. Use
    /// [`sample_rate::SampleRateAdapter`] to convert from other rates.
    ///
    /// Returns a set of valid AIS messages detected in the input.
    pub fn demodulate(&mut self, iq_samples: &[Complex<f32>]) -> HashSet<AisDemodulatedMessage> {
        if iq_samples.is_empty() {
            return HashSet::new();
        }

        let mut tag = Tag::default();

        // Step 1: Rotate ±25kHz and split into channels A and B (at 96kHz)
        let (channel_a, channel_b) = self.rotate.receive_dual(iq_samples, &mut tag);

        // Step 2: Downsample to 48kHz (CIC5 by 2)
        let ds2a = self.ds2a.receive(&channel_a, &mut tag);
        let ds2b = self.ds2b.receive(&channel_b, &mut tag);

        // Step 3: Apply CIC5 filter (no downsampling, at 48kHz)
        let fcic5a = self.fcic5a.receive(&ds2a, &mut tag);
        let fcic5b = self.fcic5b.receive(&ds2b, &mut tag);

        // Step 4: Automatic Frequency Correction
        let cgf_a = self.cgf_a.receive(&fcic5a, &mut tag);
        let cgf_b = self.cgf_b.receive(&fcic5b, &mut tag);

        // Step 5: Coherent FIR filter
        let fc_a = self.fc_a.receive(&cgf_a, &mut tag);
        let fc_b = self.fc_b.receive(&cgf_b, &mut tag);

        // Step 6: Scatter PLL - distribute across timing phases
        let s_a = self.s_a.receive_scatter(&fc_a, &mut tag);
        let s_b = self.s_b.receive_scatter(&fc_b, &mut tag);

        let mut messages = HashSet::new();

        // Step 7: Phase Search demodulation for each timing phase
        // Step 8: Decode messages from symbols
        for (i, samples) in s_a.iter().enumerate() {
            let syms = self.cd_ema_a[i].receive(samples, &mut tag);
            let msgs = self.decode_ais_message_stateful(&syms, 'A', i, &mut tag);
            for msg in msgs {
                messages.insert(msg);
            }
        }

        for (i, samples) in s_b.iter().enumerate() {
            let syms = self.cd_ema_b[i].receive(samples, &mut tag);
            let msgs = self.decode_ais_message_stateful(&syms, 'B', i, &mut tag);
            for msg in msgs {
                messages.insert(msg);
            }
        }

        messages
    }

    /// Stateful AIS message decoder for a single channel and timing phase.
    ///
    /// Processes a slice of demodulated symbols, updating the persistent decoder state.
    /// Handles NRZI decoding, HDLC framing, bit stuffing, and CRC validation.
    /// Returns a vector of valid AIS messages detected in the symbol stream.
    fn decode_ais_message_stateful(
        &mut self,
        symbols: &[f32],
        channel: char,
        index: usize,
        tag: &mut Tag,
    ) -> Vec<AisDemodulatedMessage> {
        let mut accumulator = vec![];
        let decoder_state = match channel {
            'A' => &mut self.decoder_states_a[index],
            'B' => &mut self.decoder_states_b[index],
            _ => panic!("Invalid channel"),
        };

        if symbols.is_empty() {
            return accumulator;
        }

        // Initialize prev_d from first symbol if this is the first call
        if decoder_state.state == State::Training && decoder_state.position == 0 {
            if let Some(&first) = symbols.first() {
                decoder_state.prev_d = if first > 0.0 { 1 } else { 0 };
            }
        }

        for &s in symbols.iter() {
            let d: u8 = if s > 0.0 { 1 } else { 0 };

            // NRZI decode: transition = 0, no transition = 1
            let bit: u8 = if d == decoder_state.prev_d { 1 } else { 0 };
            decoder_state.prev_d = d;

            match decoder_state.state {
                State::Training => {
                    // Look for alternating pattern (preamble)
                    if bit != decoder_state.last_bit {
                        decoder_state.position += 1;
                    } else if decoder_state.position > MIN_TRAINING_BITS {
                        decoder_state.state = State::StartFlag;
                        decoder_state.position = if bit == 1 { 3 } else { 1 };
                    } else {
                        decoder_state.position = 0;
                    }
                }

                State::StartFlag => {
                    if decoder_state.position == 7 {
                        if bit == 0 {
                            decoder_state.state = State::DataFcs;
                            decoder_state.msg_bits.clear();
                            decoder_state.one_seq_count = 0;
                            decoder_state.position = 0;
                            decoder_state.level_accumulator = 0.0;
                            decoder_state.level_count = 0;
                        } else {
                            decoder_state.state = State::Training;
                            decoder_state.position = 0;
                        }
                    } else if bit == 1 {
                        decoder_state.position += 1;
                    } else {
                        decoder_state.state = State::Training;
                        decoder_state.position = 0;
                    }
                }

                State::DataFcs => {
                    decoder_state.msg_bits.push(bit);
                    decoder_state.position += 1;
                    decoder_state.level_accumulator += tag.sample_lvl;
                    decoder_state.level_count += 1;

                    if bit == 1 {
                        if decoder_state.one_seq_count == 5 {
                            // End of message detected
                            let level = if decoder_state.level_count > 0 {
                                decoder_state.level_accumulator / decoder_state.level_count as f32
                            } else {
                                0.0
                            };

                            // Remove flag bits
                            for _ in 1..=7 {
                                if !decoder_state.msg_bits.is_empty() {
                                    decoder_state.msg_bits.pop();
                                }
                            }

                            let bytes = Self::pack_bits_to_bytes_lsb(&decoder_state.msg_bits);

                            if bytes.len() >= 3 {
                                let calc_crc = Self::crc16(&bytes);
                                if calc_crc == EXPECTED_CRC {
                                    let signal_level = if level != 0.0 {
                                        10.0f32 + f32::log10(level)
                                    } else {
                                        0.0
                                    };

                                    let data = &bytes[..bytes.len() - 2];
                                    let rxtime = SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs();
                                    let msg = AisDemodulatedMessage {
                                        bits: data.to_vec(),
                                        signal_level,
                                        channel,
                                        timestamp: rxtime,
                                        nmea_sentences: vec![],
                                    };
                                    if msg.validate() {
                                        accumulator.push(msg);
                                    }
                                }
                            }

                            decoder_state.state = State::Training;
                            decoder_state.position = 0;
                        } else {
                            decoder_state.one_seq_count += 1;
                        }
                    } else {
                        if decoder_state.one_seq_count == 5 {
                            // Remove stuffed zero
                            if !decoder_state.msg_bits.is_empty() {
                                decoder_state.msg_bits.pop();
                                decoder_state.position -= 1;
                            }
                        }
                        decoder_state.one_seq_count = 0;
                    }

                    // Prevent runaway messages
                    if decoder_state.msg_bits.len() >= MAX_AIS_LENGTH {
                        decoder_state.state = State::Training;
                        decoder_state.position = 0;
                        decoder_state.one_seq_count = 0;
                        decoder_state.msg_bits.clear();
                    }
                }
            }

            decoder_state.last_bit = bit;
        }

        accumulator
    }

    /// Compute CRC-16 for AIS message validation.
    ///
    /// Calculates the CRC-16 checksum over the provided data using the AIS polynomial.
    /// Used to validate message integrity after decoding.
    fn crc16(data: &[u8]) -> u16 {
        let mut crc: u16 = 0xFFFF;
        let poly: u16 = 0x8408;
        for &byte in data {
            let mut b = byte;
            for _ in 0..8 {
                let mix = (crc ^ (b as u16)) & 0x01;
                crc >>= 1;
                if mix != 0 {
                    crc ^= poly;
                }
                b >>= 1;
            }
        }
        crc
    }

    /// Pack a slice of bits (LSB-first) into a vector of bytes.
    ///
    /// Converts a vector of bits (u8, 0 or 1) into bytes, with the least significant bit first.
    /// Used for assembling the decoded AIS message payload.
    fn pack_bits_to_bytes_lsb(bits: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(bits.len().div_ceil(8));

        for chunk in bits.chunks(8) {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                if bit != 0 {
                    byte |= 1 << i; // LSB-first
                }
            }
            bytes.push(byte);
        }

        bytes
    }
}

impl AisDemodulatedMessage {
    /// Validate the AIS message structure and content.
    ///
    /// Checks the message type, length, and structure according to AIS protocol rules.
    /// Returns true if the message is valid and can be processed further.
    pub fn validate(&self) -> bool {
        // Message length in bits (excluding FCS)
        let bit_len = self.bits.len() * 8;

        if bit_len == 0 {
            return true;
        }

        // Message type: first 6 bits (MSB first in first byte)
        let msg_type = (self.bits[0] >> 2) & 0x3F;

        if !(1..=27).contains(&msg_type) {
            return false;
        }

        // Minimum lengths for each type
        const ML: [usize; 27] = [
            149, 149, 149, 168, 418, 88, 72, 56, 168, 70, 168, 72, 40, 40, 88, 92, 80, 168, 312,
            70, 271, 145, 154, 160, 72, 60, 96,
        ];

        if bit_len < ML[msg_type as usize - 1] {
            return false;
        }

        true
    }

    /// Convert an AIS demodulated message to one or more NMEA sentences (AIVDM), fragmenting if needed.
    pub fn encode_nmea(&self) -> Self {
        // 1. Encode bits to AIS 6-bit ASCII
        let payload = encode_ais_6bit_ascii(&self.bits);

        // 2. Fragment payload (max 56 chars per fragment is typical for AIVDM)
        let max_payload_len = 56;
        let mut sentences = Vec::new();
        let total_fragments = payload.len().div_ceil(max_payload_len) as u8;

        for i in 0..total_fragments {
            let start = (i as usize) * max_payload_len;
            let end = ((i as usize + 1) * max_payload_len).min(payload.len());
            let frag_payload = &payload[start..end];

            // Fill bits: only for last fragment
            let fill_bits = if i == total_fragments - 1 {
                let total_bits = self.bits.len() * 8;
                (6 - (total_bits % 6)) % 6
            } else {
                0
            };

            // Build NmeaAisMessage
            let nmea_msg = NmeaAisMessage {
                message_type: "AIVDM".to_string(),
                fragment_count: total_fragments,
                fragment_number: i + 1,
                message_id: if total_fragments > 1 {
                    Some("1".to_string())
                } else {
                    None
                },
                channel: self.channel,
                payload: frag_payload.to_string(),
                fill_bits: fill_bits as u8,
                checksum: 0, // Will be computed below
            };

            // Serialize to NMEA sentence
            let fields = [
                nmea_msg.message_type.clone(),
                nmea_msg.fragment_count.to_string(),
                nmea_msg.fragment_number.to_string(),
                nmea_msg.message_id.clone().unwrap_or_default(),
                nmea_msg.channel.to_string(),
                nmea_msg.payload.clone(),
                nmea_msg.fill_bits.to_string(),
            ];
            let data_part = fields.join(",");
            let checksum = data_part.bytes().fold(0u8, |acc, b| acc ^ b);
            let sentence = format!("!{}*{:02X}", data_part, checksum);

            sentences.push(sentence);
        }
        Self {
            bits: self.bits.clone(),
            signal_level: self.signal_level,
            channel: self.channel,
            timestamp: self.timestamp,
            nmea_sentences: sentences,
        }
    }

    pub fn decode(&self) -> Option<Message> {
        if self.bits.len() < 14 {
            return None;
        }
        let cursor = Cursor::new(&self.bits);
        let mut reader = Reader::new(cursor);
        Message::from_reader_with_ctx(&mut reader, ()).ok()
    }
}

/// Encode binary payload as AIS 6-bit ASCII string (MSB-first, matching NMEA standard)
fn encode_ais_6bit_ascii(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut bit_buffer = 0u32;
    let mut bits_in_buffer = 0;

    for &byte in bytes {
        // Add byte to buffer (MSB-first)
        bit_buffer = (bit_buffer << 8) | (byte as u32);
        bits_in_buffer += 8;

        // Extract 6-bit groups while we have enough bits
        while bits_in_buffer >= 6 {
            let six_bit_val = ((bit_buffer >> (bits_in_buffer - 6)) & 0x3F) as u8;
            result.push(ais_6bit_to_char(six_bit_val));
            bits_in_buffer -= 6;
        }
    }

    // Handle remaining bits (pad with zeros if needed)
    if bits_in_buffer > 0 {
        let six_bit_val = ((bit_buffer << (6 - bits_in_buffer)) & 0x3F) as u8;
        result.push(ais_6bit_to_char(six_bit_val));
    }

    result
}

fn ais_6bit_to_char(val: u8) -> char {
    match val {
        0..=39 => (val + 48) as char,
        40..=63 => (val + 56) as char,
        _ => '?',
    }
}

#[cfg(test)]
mod tests {
    use crate::prelude::Message;

    use super::*;

    #[test]
    fn test_encode_nmea() {
        // Example AIS message bits (each u8 is a byte, as produced by pack_bits_to_bytes_lsb)
        let bits = vec![
            20, 58, 86, 192, 110, 0, 0, 0, 1, 1, 96, 231, 124, 181, 32, 20, 212, 6, 3, 21, 20, 115,
            192, 0, 0, 0, 0, 0, 0, 99, 3, 4, 70, 15, 192, 24, 240, 0, 193, 96, 32, 21, 146, 20, 0,
            0, 0, 0, 0, 0, 0, 0, 0,
        ];
        let msg = AisDemodulatedMessage {
            bits: bits.clone(),
            signal_level: 42.0,
            channel: 'B',
            timestamp: 1_700_000_000,
            nmea_sentences: vec![],
        };

        // Validate message structure
        assert!(msg.validate(), "AIS message should be valid");

        // Convert to NMEA sentences
        let nmea_encoded = msg.encode_nmea();
        assert!(
            !nmea_encoded.nmea_sentences.is_empty(),
            "Should produce at least one NMEA sentence"
        );

        // Check sentence contents
        assert_eq!(
            nmea_encoded.nmea_sentences[0],
            "!AIVDM,2,1,1,B,53aFh6p000010F3WO;DP5=@60iDDLt000000001S0hA63t0Ht031H20E,0*7F"
        );
        assert_eq!(
            nmea_encoded.nmea_sentences[1],
            "!AIVDM,2,2,1,B,TQ@000000000000,2*53"
        );

        let sentence_refs: Vec<&str> = nmea_encoded
            .nmea_sentences
            .iter()
            .map(|s| s.as_str())
            .collect();
        let message = Message::from_nmea(&sentence_refs).unwrap();

        if let Message::StaticAndVoyageData(msg) = message {
            assert_eq!(msg.mmsi, 244690971);
            assert_eq!(msg.destination, "LE HAVRE");
            assert_eq!(msg.shipname, "HASTA LUEGO");
            assert_eq!(msg.callsign, "PE 9725");
        } else {
            panic!("Decoded message is not StaticAndVoyageData");
        }
    }
}
