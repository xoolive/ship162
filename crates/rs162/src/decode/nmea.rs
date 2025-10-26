//! NMEA AIS Message Parser and Decoder
//!
//! This library provides functionality to parse NMEA AIVDM/AIVDO messages
//! and convert them to binary u8 data.

use std::collections::HashMap;

/// Represents a parsed NMEA AIS message
#[derive(Debug, Clone, PartialEq)]
pub struct NmeaAisMessage {
    /// Message type (AIVDM or AIVDO)
    pub message_type: String,
    /// Fragment count (total number of fragments)
    pub fragment_count: u8,
    /// Fragment number (1-based)
    pub fragment_number: u8,
    /// Sequential message ID for multi-sentence messages
    pub message_id: Option<String>,
    /// Radio channel code (A, B, 1, 2)
    pub channel: char,
    /// Data payload (6-bit encoded)
    pub payload: String,
    /// Number of fill bits (0-5)
    pub fill_bits: u8,
    /// NMEA checksum
    pub checksum: u8,
}

/// Error types for NMEA parsing
#[derive(Debug, thiserror::Error)]
pub enum NmeaError {
    #[error("Invalid NMEA sentence format")]
    InvalidFormat,
    #[error("Invalid checksum")]
    InvalidChecksum,
    #[error("Invalid fragment count: {0}")]
    InvalidFragmentCount(u8),
    #[error("Invalid fragment number: {0}")]
    InvalidFragmentNumber(u8),
    #[error("Invalid fill bits: {0}")]
    InvalidFillBits(u8),
    #[error("Invalid 6-bit character: {0}")]
    Invalid6BitChar(char),
    #[error("Incomplete multi-fragment message")]
    IncompleteMessage,
}

/// Convert AIS 6-bit character to binary value
fn ais_6bit_to_binary(c: char) -> Result<u8, NmeaError> {
    let byte = c as u8;
    match byte {
        48..=87 => Ok(byte - 48),  // '0' to 'W' -> 0 to 39
        96..=119 => Ok(byte - 56), // '`' to 'w' -> 40 to 63
        _ => Err(NmeaError::Invalid6BitChar(c)),
    }
}

impl NmeaAisMessage {
    /// Parse a NMEA AIVDM/AIVDO sentence
    pub fn parse(sentence: &str) -> Result<Self, NmeaError> {
        let sentence = sentence.trim();

        // Remove leading '!' if present
        let sentence = sentence.strip_prefix('!').unwrap_or(sentence);

        // Split by '*' to separate data and checksum
        let parts: Vec<&str> = sentence.split('*').collect();
        if parts.len() != 2 {
            return Err(NmeaError::InvalidFormat);
        }

        let data_part = parts[0];
        let checksum_str = parts[1];

        // Parse checksum
        let checksum =
            u8::from_str_radix(checksum_str, 16).map_err(|_| NmeaError::InvalidFormat)?;

        // Verify checksum
        let calculated_checksum = data_part.bytes().fold(0u8, |acc, b| acc ^ b);
        if calculated_checksum != checksum {
            return Err(NmeaError::InvalidChecksum);
        }

        // Split data fields by comma
        let fields: Vec<&str> = data_part.split(',').collect();
        if fields.len() != 7 {
            return Err(NmeaError::InvalidFormat);
        }

        // Parse fields
        let message_type = fields[0].to_string();
        if message_type != "AIVDM"
            && message_type != "AIVDO"
            && message_type != "BSVDM"
            && message_type != "B1VDM"
            && message_type != "B2VDM"
        {
            return Err(NmeaError::InvalidFormat);
        }

        let fragment_count = fields[1]
            .parse::<u8>()
            .map_err(|_| NmeaError::InvalidFormat)?;
        if fragment_count == 0 {
            return Err(NmeaError::InvalidFragmentCount(fragment_count));
        }

        let fragment_number = fields[2]
            .parse::<u8>()
            .map_err(|_| NmeaError::InvalidFormat)?;
        if fragment_number == 0 || fragment_number > fragment_count {
            return Err(NmeaError::InvalidFragmentNumber(fragment_number));
        }

        let message_id = if fields[3].is_empty() {
            None
        } else {
            Some(fields[3].to_string())
        };

        let channel = fields[4].chars().next().ok_or(NmeaError::InvalidFormat)?;

        let payload = fields[5].to_string();

        let fill_bits = fields[6]
            .parse::<u8>()
            .map_err(|_| NmeaError::InvalidFormat)?;
        if fill_bits > 5 {
            return Err(NmeaError::InvalidFillBits(fill_bits));
        }

        Ok(NmeaAisMessage {
            message_type,
            fragment_count,
            fragment_number,
            message_id,
            channel,
            payload,
            fill_bits,
            checksum,
        })
    }

    /// Convert the 6-bit encoded payload to binary data
    pub fn payload_to_binary(&self) -> Result<Vec<u8>, NmeaError> {
        let mut binary_data = Vec::new();
        let mut bit_buffer = 0u32;
        let mut bits_in_buffer = 0;

        for c in self.payload.chars() {
            let six_bits = ais_6bit_to_binary(c)?;

            // Add 6 bits to buffer
            bit_buffer = (bit_buffer << 6) | (six_bits as u32);
            bits_in_buffer += 6;

            // Extract complete bytes
            while bits_in_buffer >= 8 {
                let byte = (bit_buffer >> (bits_in_buffer - 8)) & 0xFF;
                binary_data.push(byte as u8);
                bits_in_buffer -= 8;
            }
        }

        // Handle remaining bits (if any) considering fill bits
        if bits_in_buffer > 0 {
            let remaining_bits = bits_in_buffer - self.fill_bits as usize;
            if remaining_bits > 0 {
                let byte = (bit_buffer << (8 - remaining_bits)) & 0xFF;
                binary_data.push(byte as u8);
            }
        }

        Ok(binary_data)
    }

    /// Check if this is a complete single-fragment message
    pub fn is_complete(&self) -> bool {
        self.fragment_count == 1 && self.fragment_number == 1
    }

    /// Check if this is a multi-fragment message
    pub fn is_multi_fragment(&self) -> bool {
        self.fragment_count > 1
    }

    /// Extract the talker ID from the message type (e.g., "AI" from "AIVDM")
    pub fn talker(&self) -> &str {
        if self.message_type.len() >= 2 {
            &self.message_type[..2]
        } else {
            ""
        }
    }

    /// Extract the message type suffix (e.g., "VDM" from "AIVDM")
    pub fn message_type_suffix(&self) -> &str {
        if self.message_type.len() > 2 {
            &self.message_type[2..]
        } else {
            &self.message_type
        }
    }

    /// Get the AIS message ID from the binary payload (first 6 bits)
    pub fn ais_message_id(&self) -> Result<u8, NmeaError> {
        let binary = self.payload_to_binary()?;
        if binary.is_empty() {
            return Err(NmeaError::InvalidFormat);
        }
        Ok(binary[0] >> 2) // First 6 bits
    }

    /// Create from string (alternative constructor)
    pub fn from_string(sentence: &str) -> Result<Self, NmeaError> {
        Self::parse(sentence)
    }

    /// Create from bytes
    pub fn from_bytes(sentence: &[u8]) -> Result<Self, NmeaError> {
        let sentence_str = std::str::from_utf8(sentence).map_err(|_| NmeaError::InvalidFormat)?;
        Self::parse(sentence_str)
    }

    /// Check if the message is valid (checksum verification)
    pub fn is_valid(&self) -> bool {
        // The message was already validated during parsing, so if we have
        // a parsed message, it's valid
        true
    }
}

/// Multi-fragment message assembler
#[derive(Debug, Default)]
pub struct MessageAssembler {
    fragments: HashMap<String, Vec<Option<NmeaAisMessage>>>,
}

impl MessageAssembler {
    /// Create a new message assembler
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a fragment and return complete message if ready
    pub fn add_fragment(&mut self, message: NmeaAisMessage) -> Result<Option<Vec<u8>>, NmeaError> {
        if message.is_complete() {
            // Single fragment message - return immediately
            return Ok(Some(message.payload_to_binary()?));
        }

        // Multi-fragment message
        let message_id = message
            .message_id
            .as_ref()
            .ok_or(NmeaError::IncompleteMessage)?
            .clone();

        // Get or create fragment array
        let fragments = self
            .fragments
            .entry(message_id.clone())
            .or_insert_with(|| vec![None; message.fragment_count as usize]);

        // Ensure fragment array is correct size
        if fragments.len() != message.fragment_count as usize {
            return Err(NmeaError::InvalidFragmentCount(message.fragment_count));
        }

        // Store fragment
        let index = (message.fragment_number - 1) as usize;
        if index >= fragments.len() {
            return Err(NmeaError::InvalidFragmentNumber(message.fragment_number));
        }

        fragments[index] = Some(message);

        // Check if all fragments are received
        if fragments.iter().all(|f| f.is_some()) {
            // Assemble complete message
            let mut complete_payload = String::new();
            let mut total_fill_bits = 0;

            for frag in fragments.iter().flatten() {
                complete_payload.push_str(&frag.payload);
                total_fill_bits = frag.fill_bits; // Use fill bits from last fragment
            }

            // Create temporary message for conversion
            let temp_message = NmeaAisMessage {
                message_type: "AIVDM".to_string(),
                fragment_count: 1,
                fragment_number: 1,
                message_id: None,
                channel: 'A', // Doesn't matter for binary conversion
                payload: complete_payload,
                fill_bits: total_fill_bits,
                checksum: 0, // Doesn't matter for binary conversion
            };

            // Remove completed message from assembler
            self.fragments.remove(&message_id);

            return Ok(Some(temp_message.payload_to_binary()?));
        }

        Ok(None)
    }

    /// Clear old incomplete messages (call periodically)
    pub fn clear_old_messages(&mut self) {
        // In a real implementation, you might want to track timestamps
        // and remove messages that are too old
        self.fragments.clear();
    }

    /// Assemble from an iterable of messages
    pub fn assemble_from_iterable(messages: Vec<NmeaAisMessage>) -> Result<Vec<u8>, NmeaError> {
        let mut assembler = Self::new();

        for message in messages {
            if let Some(binary) = assembler.add_fragment(message)? {
                return Ok(binary);
            }
        }

        Err(NmeaError::IncompleteMessage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_fragment() {
        let sentence = "!AIVDM,1,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0*5C";
        let message = NmeaAisMessage::parse(sentence).unwrap();

        assert_eq!(message.message_type, "AIVDM");
        assert_eq!(message.fragment_count, 1);
        assert_eq!(message.fragment_number, 1);
        assert_eq!(message.message_id, None);
        assert_eq!(message.channel, 'B');
        assert_eq!(message.payload, "177KQJ5000G?tO`K>RA1wUbN0TKH");
        assert_eq!(message.fill_bits, 0);
        assert_eq!(message.checksum, 0x5C);
        assert!(message.is_complete());
    }

    #[test]
    fn test_parse_multi_fragment() {
        let sentence1 =
            "!AIVDM,2,1,3,B,55P5TL01VIaAL@7WKO@mBplU@<PDhh000000001S;AJ::4A80?4i@E53,0*3E";
        let message1 = NmeaAisMessage::parse(sentence1).unwrap();

        assert_eq!(message1.fragment_count, 2);
        assert_eq!(message1.fragment_number, 1);
        assert_eq!(message1.message_id, Some("3".to_string()));
        assert!(message1.is_multi_fragment());
        assert!(!message1.is_complete());

        let sentence2 = "!AIVDM,2,2,3,B,1@0000000000000,2*55";
        let message2 = NmeaAisMessage::parse(sentence2).unwrap();

        assert_eq!(message2.fragment_count, 2);
        assert_eq!(message2.fragment_number, 2);
        assert_eq!(message2.message_id, Some("3".to_string()));
        assert_eq!(message2.fill_bits, 2);
    }

    #[test]
    fn test_payload_to_binary() {
        let sentence = "!AIVDM,1,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0*5C";
        let message = NmeaAisMessage::parse(sentence).unwrap();
        let binary = message.payload_to_binary().unwrap();

        // Should decode to binary data
        assert!(!binary.is_empty());

        // First few bits should represent message type 1 (position report)
        // Message type is in bits 0-5, so first 6 bits should be 000001 (1)
        assert_eq!(binary[0] >> 2, 1); // First 6 bits shifted right by 2
    }

    #[test]
    fn test_message_assembler() {
        let mut assembler = MessageAssembler::new();

        // Add first fragment
        let sentence1 =
            "!AIVDM,2,1,3,B,55P5TL01VIaAL@7WKO@mBplU@<PDhh000000001S;AJ::4A80?4i@E53,0*3E";
        let message1 = NmeaAisMessage::parse(sentence1).unwrap();
        let result1 = assembler.add_fragment(message1).unwrap();
        assert!(result1.is_none()); // Not complete yet

        // Add second fragment
        let sentence2 = "!AIVDM,2,2,3,B,1@0000000000000,2*55";
        let message2 = NmeaAisMessage::parse(sentence2).unwrap();
        let result2 = assembler.add_fragment(message2).unwrap();
        assert!(result2.is_some()); // Should be complete now

        let binary = result2.unwrap();
        assert!(!binary.is_empty());
    }

    #[test]
    fn test_invalid_checksum() {
        let sentence = "!AIVDM,1,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0*FF";
        let result = NmeaAisMessage::parse(sentence);
        assert!(matches!(result, Err(NmeaError::InvalidChecksum)));
    }

    #[test]
    fn test_6bit_conversion() {
        // Test various 6-bit characters according to AIS standard
        assert_eq!(ais_6bit_to_binary('0').unwrap(), 0); // ASCII 48
        assert_eq!(ais_6bit_to_binary('9').unwrap(), 9); // ASCII 57
        assert_eq!(ais_6bit_to_binary(':').unwrap(), 10); // ASCII 58
        assert_eq!(ais_6bit_to_binary(';').unwrap(), 11); // ASCII 59
        assert_eq!(ais_6bit_to_binary('<').unwrap(), 12); // ASCII 60
        assert_eq!(ais_6bit_to_binary('=').unwrap(), 13); // ASCII 61
        assert_eq!(ais_6bit_to_binary('>').unwrap(), 14); // ASCII 62
        assert_eq!(ais_6bit_to_binary('?').unwrap(), 15); // ASCII 63
        assert_eq!(ais_6bit_to_binary('@').unwrap(), 16); // ASCII 64
        assert_eq!(ais_6bit_to_binary('A').unwrap(), 17); // ASCII 65
        assert_eq!(ais_6bit_to_binary('W').unwrap(), 39); // ASCII 87
        assert_eq!(ais_6bit_to_binary('`').unwrap(), 40); // ASCII 96
        assert_eq!(ais_6bit_to_binary('a').unwrap(), 41); // ASCII 97
        assert_eq!(ais_6bit_to_binary('w').unwrap(), 63); // ASCII 119
    }

    #[test]
    fn test_edge_cases_for_6bit_conversion() {
        // Test boundary characters
        assert_eq!(ais_6bit_to_binary('0').unwrap(), 0); // ASCII 48
        assert_eq!(ais_6bit_to_binary('?').unwrap(), 15); // ASCII 63
        assert_eq!(ais_6bit_to_binary('@').unwrap(), 16); // ASCII 64
        assert_eq!(ais_6bit_to_binary('W').unwrap(), 39); // ASCII 87
        assert_eq!(ais_6bit_to_binary('`').unwrap(), 40); // ASCII 96
        assert_eq!(ais_6bit_to_binary('w').unwrap(), 63); // ASCII 119

        // Test invalid characters
        assert!(ais_6bit_to_binary('/').is_err()); // ASCII 47 (too low)
        assert!(ais_6bit_to_binary('X').is_err()); // ASCII 88 (gap)
        assert!(ais_6bit_to_binary('_').is_err()); // ASCII 95 (gap)
        assert!(ais_6bit_to_binary('x').is_err()); // ASCII 120 (too high)
        assert!(ais_6bit_to_binary('€').is_err()); // Non-ASCII
    }

    #[test]
    fn test_invalid_field_count() {
        // Test message with missing fields (should have 7 fields)
        let sentence = "!AIVDM,,A,91b77=h3h00nHt0Q3r@@07000<0b,0*69";
        let result = NmeaAisMessage::parse(sentence);
        assert!(result.is_err());
        assert!(matches!(result, Err(NmeaError::InvalidFormat)));
    }

    #[test]
    fn test_valid_message_attributes() {
        let sentence = "!AIVDM,1,1,,B,91b55wi;hbOS@OdQAC062Ch2089h,0*30";
        let message = NmeaAisMessage::parse(sentence).unwrap();

        assert!(message.is_valid());
        assert!(message.is_complete());
        assert!(!message.is_multi_fragment());
    }

    #[test]
    fn test_single_vs_multi_fragment() {
        let single = "!AIVDM,1,1,,B,91b55wi;hbOS@OdQAC062Ch2089h,0*30";
        let message = NmeaAisMessage::parse(single).unwrap();

        assert!(message.is_complete());
        assert!(!message.is_multi_fragment());
    }

    #[test]
    fn test_from_string_vs_parse() {
        let sentence = "!AIVDM,1,1,,B,15M67FC000G?ufbE`FepT@3n00Sa,0*5C";

        let old = NmeaAisMessage::parse(sentence).unwrap();
        let new = NmeaAisMessage::from_string(sentence).unwrap();

        assert_eq!(old, new);
    }

    #[test]
    fn test_assembler_from_iterable() {
        let messages = vec![
            NmeaAisMessage::parse(
                "!AIVDM,2,1,4,A,55O0W7`00001L@gCWGA2uItLth@DqtL5@F22220j1h742t0Ht0000000,0*08",
            )
            .unwrap(),
            NmeaAisMessage::parse("!AIVDM,2,2,4,A,000000000000000,2*20").unwrap(),
        ];

        let binary = MessageAssembler::assemble_from_iterable(messages).unwrap();
        assert!(!binary.is_empty());
    }

    #[test]
    fn test_talker_extraction() {
        let msg1 = "!AIVDM,1,1,,B,91b55wi;hbOS@OdQAC062Ch2089h,0*30";
        let message1 = NmeaAisMessage::parse(msg1).unwrap();
        assert_eq!(message1.talker(), "AI");

        let msg2 = "!AIVDM,1,1,,A,8@30oni?1j020@00,0*23";
        let message2 = NmeaAisMessage::parse(msg2).unwrap();
        assert_eq!(message2.talker(), "AI");
    }

    #[test]
    fn test_message_type_suffix() {
        let msg1 = "!AIVDM,1,1,,B,91b55wi;hbOS@OdQAC062Ch2089h,0*30";
        let message1 = NmeaAisMessage::parse(msg1).unwrap();
        assert_eq!(message1.message_type_suffix(), "VDM");

        let msg2 = "!AIVDM,1,1,,A,8@30oni?1j020@00,0*23";
        let message2 = NmeaAisMessage::parse(msg2).unwrap();
        assert_eq!(message2.message_type_suffix(), "VDM");
    }

    #[test]
    fn test_message_attributes() {
        let sentence = "!AIVDM,1,1,,A,85Mwp`1Kf3aCnsNvBWLi=wQuNhA5t43N`5nCuI=p<IBfVqnMgPGs,0*47";
        let message = NmeaAisMessage::parse(sentence).unwrap();

        assert_eq!(message.ais_message_id().unwrap(), 8);
        assert_eq!(message.fragment_count, 1);
        assert_eq!(message.fragment_number, 1);
        assert_eq!(message.message_id, None);
        assert_eq!(message.channel, 'A');
        assert_eq!(
            message.payload,
            "85Mwp`1Kf3aCnsNvBWLi=wQuNhA5t43N`5nCuI=p<IBfVqnMgPGs"
        );
        assert_eq!(message.checksum, 0x47);
    }

    #[test]
    fn test_message_validity() {
        let valid_msg = "!AIVDM,1,1,,A,85Mwp`1Kf3aCnsNvBWLi=wQuNhA5t43N`5nCuI=p<IBfVqnMgPGs,0*47";
        let message = NmeaAisMessage::parse(valid_msg).unwrap();
        assert!(message.is_valid());

        // Test with invalid checksum
        let invalid_msg = "!AIVDM,1,1,,A,85Mwp`1Kf3aCnsNvBWLi=wQuNhA5t43N`5nCuI=p<IBfVqnMgPGt,0*47";
        let result = NmeaAisMessage::parse(invalid_msg);
        assert!(result.is_err());
        assert!(matches!(result, Err(NmeaError::InvalidChecksum)));
    }

    #[test]
    fn test_from_bytes() {
        let sentence = b"!AIVDM,1,1,,A,85Mwp`1Kf3aCnsNvBWLi=wQuNhA5t43N`5nCuI=p<IBfVqnMgPGs,0*47";

        let from_parse = NmeaAisMessage::parse(std::str::from_utf8(sentence).unwrap()).unwrap();
        let from_bytes = NmeaAisMessage::from_bytes(sentence).unwrap();

        assert_eq!(from_parse, from_bytes);
    }

    #[test]
    fn test_message_equality() {
        let sentence = "!AIVDM,1,1,,B,F030p:j2N2P5aJR0r;6f3rj10000,0*11";

        let first_obj = NmeaAisMessage::parse(sentence).unwrap();
        let second_obj = NmeaAisMessage::parse(sentence).unwrap();

        // They should be equal
        assert_eq!(first_obj, second_obj);
    }

    #[test]
    fn test_missing_checksum() {
        // Test with missing checksum (should still work if we modify the parser)
        let sentence = "!AIVDM,1,1,,A,100u3FP04r28t0<WcshcQI<H0H79,0";
        let result = NmeaAisMessage::parse(sentence);
        // This should fail with our current implementation since checksum is required
        assert!(result.is_err());
    }

    #[test]
    fn test_valid_checksum_identification() {
        let sentence = "!AIVDM,1,1,,B,15NG6V0P01G?cFhE`R2IU?wn28R>,0*05";
        let message = NmeaAisMessage::parse(sentence).unwrap();
        assert!(message.is_valid());
    }

    #[test]
    fn test_invalid_checksum_identification() {
        let sentence = "!AIVDM,1,1,,B,15NG6V0P01G?cFhE`R2IU?wn28R>,0*04";
        let result = NmeaAisMessage::parse(sentence);
        assert!(result.is_err());
        assert!(matches!(result, Err(NmeaError::InvalidChecksum)));
    }

    #[test]
    fn test_valid_multi_part_checksum() {
        let messages = vec![
            NmeaAisMessage::parse(
                "!AIVDM,2,1,4,A,55O0W7`00001L@gCWGA2uItLth@DqtL5@F22220j1h742t0Ht0000000,0*08",
            )
            .unwrap(),
            NmeaAisMessage::parse("!AIVDM,2,2,4,A,000000000000000,2*20").unwrap(),
        ];

        let binary = MessageAssembler::assemble_from_iterable(messages).unwrap();
        assert!(!binary.is_empty());
    }

    #[test]
    fn test_invalid_multi_part_checksum_first() {
        // The first sentence has an invalid checksum
        let message1 = NmeaAisMessage::parse(
            "!AIVDM,2,1,4,A,55O0W7`00001L@gCWGA2uItLth@DqtL5@F22220j1h742t0Ht0000000,0*09",
        );
        assert!(message1.is_err());
        assert!(matches!(message1, Err(NmeaError::InvalidChecksum)));
    }

    #[test]
    fn test_invalid_multi_part_checksum_second() {
        // The second sentence has an invalid checksum
        let message2 = NmeaAisMessage::parse("!AIVDM,2,2,4,A,000000000000000,2*21");
        assert!(message2.is_err());
        assert!(matches!(message2, Err(NmeaError::InvalidChecksum)));
    }

    #[test]
    fn test_fill_bits_validation() {
        // Test invalid fill bits (> 5)
        // Calculate correct checksum for this message
        let data_part = "AIVDM,1,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,6";
        let calculated_checksum = data_part.bytes().fold(0u8, |acc, b| acc ^ b);
        let sentence = format!(
            "!AIVDM,1,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,6*{:02X}",
            calculated_checksum
        );

        let result = NmeaAisMessage::parse(&sentence);
        assert!(result.is_err());
        assert!(matches!(result, Err(NmeaError::InvalidFillBits(6))));
    }

    #[test]
    fn test_fragment_validation() {
        // Test invalid fragment number (0)
        let data_part = "AIVDM,1,0,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0";
        let calculated_checksum = data_part.bytes().fold(0u8, |acc, b| acc ^ b);
        let sentence = format!(
            "!AIVDM,1,0,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0*{:02X}",
            calculated_checksum
        );

        let result = NmeaAisMessage::parse(&sentence);
        assert!(result.is_err());
        assert!(matches!(result, Err(NmeaError::InvalidFragmentNumber(0))));

        // Test fragment number > fragment count
        let data_part = "AIVDM,1,2,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0";
        let calculated_checksum = data_part.bytes().fold(0u8, |acc, b| acc ^ b);
        let sentence = format!(
            "!AIVDM,1,2,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0*{:02X}",
            calculated_checksum
        );

        let result = NmeaAisMessage::parse(&sentence);
        assert!(result.is_err());
        assert!(matches!(result, Err(NmeaError::InvalidFragmentNumber(2))));

        // Test invalid fragment count (0)
        let data_part = "AIVDM,0,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0";
        let calculated_checksum = data_part.bytes().fold(0u8, |acc, b| acc ^ b);
        let sentence = format!(
            "!AIVDM,0,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0*{:02X}",
            calculated_checksum
        );

        let result = NmeaAisMessage::parse(&sentence);
        assert!(result.is_err());
        assert!(matches!(result, Err(NmeaError::InvalidFragmentCount(0))));
    }

    #[test]
    fn test_non_ais_message_types() {
        // Test non-AIS message type
        // Calculate correct checksum for this message
        let data_part = "GPGGA,1,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0";
        let calculated_checksum = data_part.bytes().fold(0u8, |acc, b| acc ^ b);
        let sentence = format!(
            "!GPGGA,1,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0*{:02X}",
            calculated_checksum
        );

        let result = NmeaAisMessage::parse(&sentence);
        assert!(result.is_err());
        assert!(matches!(result, Err(NmeaError::InvalidFormat)));
    }

    #[test]
    fn test_empty_and_malformed_sentences() {
        // Test empty sentence
        let result = NmeaAisMessage::parse("");
        assert!(result.is_err());

        // Test sentence without checksum
        let result = NmeaAisMessage::parse("!AIVDM,1,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0");
        assert!(result.is_err());

        // Test sentence with malformed checksum
        let result = NmeaAisMessage::parse("!AIVDM,1,1,,B,177KQJ5000G?tO`K>RA1wUbN0TKH,0*ZZ");
        assert!(result.is_err());
    }

    #[test]
    fn test_binary_conversion_with_fill_bits() {
        // Test message with fill bits
        let sentence = "!AIVDM,2,2,3,B,1@0000000000000,2*55";
        let message = NmeaAisMessage::parse(sentence).unwrap();
        let binary = message.payload_to_binary().unwrap();

        // Should handle fill bits correctly
        assert!(!binary.is_empty());
        assert_eq!(message.fill_bits, 2);
    }

    #[test]
    fn test_assembler_error_cases() {
        let mut assembler = MessageAssembler::new();

        // Test adding fragment without message ID for multi-fragment
        let mut message = NmeaAisMessage::parse(
            "!AIVDM,2,1,3,B,55P5TL01VIaAL@7WKO@mBplU@<PDhh000000001S;AJ::4A80?4i@E53,0*3E",
        )
        .unwrap();
        message.message_id = None; // Remove message ID

        let result = assembler.add_fragment(message);
        assert!(result.is_err());
        assert!(matches!(result, Err(NmeaError::IncompleteMessage)));
    }
}
