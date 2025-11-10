/// Convert MMSI from raw bits (no conversion needed)
pub fn from_mmsi(mmsi: u32) -> u32 {
    mmsi
}

/// Convert MMSI from raw bits, returning None if zero (for optional fields)
pub fn from_mmsi_optional(mmsi: u32) -> Option<u32> {
    if mmsi == 0 {
        None
    } else {
        Some(mmsi)
    }
}

/// Convert turn rate from raw 8-bit signed value
pub fn from_turn(raw: u8) -> Option<f32> {
    let signed_val = raw as i8;

    match signed_val {
        -128 => None,         // Not available
        -127 => Some(-720.0), // Turning left at > 720 deg/min
        127 => Some(720.0),   // Turning right at > 720 deg/min
        val => {
            let rot_ais = val as f32;
            Some((rot_ais / 4.733).powi(2).copysign(rot_ais))
        }
    }
}

/// Convert speed from raw 10-bit value to knots
pub fn from_speed(raw: u16) -> Option<f32> {
    match raw {
        1023 => None,        // Not available
        1022 => Some(102.2), // 102.2 knots or higher
        val => Some(val as f32 / 10.0),
    }
}

/// Convert longitude from raw 28-bit signed value to degrees
pub fn from_longitude(raw: u32) -> Option<f64> {
    let signed_val = if raw & 0x08000000 != 0 {
        // Negative value - sign extend from 28 bits to 32 bits
        (raw as i32) | (0xF0000000u32 as i32)
    } else {
        raw as i32
    };

    if signed_val == 0x6791AC0 {
        // 181 degrees in raw format (not available marker)
        None
    } else {
        Some(signed_val as f64 / 600000.0)
    }
}

/// Convert latitude from raw 27-bit signed value to degrees
pub fn from_latitude(raw: u32) -> Option<f64> {
    let signed_val = if raw & 0x04000000 != 0 {
        // Negative value - sign extend from 27 bits to 32 bits
        (raw as i32) | (0xF8000000u32 as i32)
    } else {
        raw as i32
    };

    if signed_val == 0x3412140 {
        // 91 degrees in raw format (not available marker)
        None
    } else {
        Some(signed_val as f64 / 600000.0)
    }
}

/// Convert course from raw 12-bit value to degrees
pub fn from_course(raw: u16) -> Option<f32> {
    match raw {
        3600..=4095 => None, // Not available or invalid
        val => Some(val as f32 / 10.0),
    }
}

/// Convert heading from raw 9-bit value to degrees
pub fn from_heading(raw: u16) -> Option<u16> {
    if raw >= 360 {
        None
    } else {
        Some(raw)
    }
}

/// Convert IMO number from raw 30-bit value
pub fn from_imo(raw: u32) -> Option<u32> {
    if raw == 0 {
        None
    } else {
        Some(raw)
    }
}

/// Convert draught from raw 8-bit value (1/10 meters)
pub fn from_draught(raw: u8) -> Option<f32> {
    match raw {
        0 => None,         // Not available
        255 => Some(25.5), // 25.5m or greater
        val => Some(val as f32 / 10.0),
    }
}

/// Convert altitude from raw 12-bit value
pub fn from_altitude(raw: u16) -> Option<u16> {
    if raw == 4095 {
        None
    } else {
        Some(raw)
    }
}

pub fn from_year(raw: u16) -> Option<u16> {
    if raw == 0 {
        None
    } else {
        Some(raw)
    }
}

pub fn from_month(raw: u8) -> Option<u8> {
    if raw == 0 {
        None
    } else {
        Some(raw)
    }
}

pub fn from_day(raw: u8) -> Option<u8> {
    if raw == 0 {
        None
    } else {
        Some(raw)
    }
}

pub fn from_hour(raw: u8) -> Option<u8> {
    if raw == 24 {
        None
    } else {
        Some(raw)
    }
}

pub fn from_minute(raw: u8) -> Option<u8> {
    if raw == 60 {
        None
    } else {
        Some(raw)
    }
}

pub fn from_second(raw: u8) -> Option<u8> {
    if raw == 60 {
        None
    } else {
        Some(raw)
    }
}

/// Convert 6-bit ASCII string from raw bits (for vendor ID, 18 bits)
pub fn from_sixbit_ascii_18(raw: u32, length: usize) -> String {
    let mut result = String::new();

    for i in 0..length {
        let shift = 6 * (length - 1 - i);
        let char_bits = (raw >> shift) & 0x3F;

        let ch = match char_bits as u8 {
            0 => '@',                                 // null/padding
            1..=31 => (char_bits as u8 + 64) as char, // A-Z[\]^_ (add 64: gives 65-95)
            32..=63 => char_bits as u8 as char,       // space through ? (use as-is: gives 32-63)
            _ => '@',
        };

        if ch != '@' {
            result.push(ch);
        }
    }

    result.trim().to_string()
}

/// Convert 6-bit ASCII string from raw bits (for call sign, 42 bits)
pub fn from_sixbit_ascii_42(raw: u64, length: usize) -> String {
    let mut result = String::new();

    for i in 0..length {
        let shift = 6 * (length - 1 - i);
        let char_bits = (raw >> shift) & 0x3F;

        let ch = match char_bits as u8 {
            0 => '@',                                 // null/padding
            1..=31 => (char_bits as u8 + 64) as char, // A-Z[\]^_ (add 64: gives 65-95)
            32..=63 => char_bits as u8 as char,       // space through ? (use as-is: gives 32-63)
            _ => '@',
        };

        if ch != '@' {
            result.push(ch);
        }
    }

    result.trim().to_string()
}

/// Convert 6-bit ASCII string from raw bits (for callsign, 42 bits)
pub fn from_sixbit_ascii(raw: u64, length: usize) -> String {
    let mut result = String::new();

    for i in 0..length {
        let shift = 6 * (length - 1 - i);
        let char_bits = (raw >> shift) & 0x3F;

        let ch = match char_bits as u8 {
            0 => '@',                                 // null/padding
            1..=31 => (char_bits as u8 + 64) as char, // A-Z[\]^_ (add 64: gives 65-95)
            32..=63 => char_bits as u8 as char,       // space through ? (use as-is: gives 32-63)
            _ => '@',
        };

        if ch != '@' {
            result.push(ch);
        }
    }

    result.trim().to_string()
}

/// Convert longitude from 1/10 minutes (Type 17 specific)
pub fn from_10th_minutes_longitude(raw: u32) -> f64 {
    if raw == 0x3FFFF {
        // Not available
        0.0
    } else {
        // Sign extend from 18 bits
        let signed = if raw & 0x20000 != 0 {
            (raw | 0xFFFC0000) as i32
        } else {
            raw as i32
        };
        signed as f64 / 10.0
    }
}

/// Convert latitude from 1/10 minutes (Type 17 specific)
pub fn from_10th_minutes_latitude(raw: u32) -> f64 {
    if raw == 0x1FFFF {
        // Not available
        0.0
    } else {
        // Sign extend from 17 bits
        let signed = if raw & 0x10000 != 0 {
            (raw | 0xFFFE0000) as i32
        } else {
            raw as i32
        };
        signed as f64 / 10.0
    }
}

/// Convert 6-bit ASCII string from raw bits (for ship names, 120 bits)
pub fn from_sixbit_ascii_120(raw: u128, length: usize) -> String {
    let mut result = String::new();

    for i in 0..length {
        let shift = 6 * (length - 1 - i);
        let char_bits = (raw >> shift) & 0x3F;

        let ch = match char_bits as u8 {
            0 => '@',                                 // null/padding
            1..=31 => (char_bits as u8 + 64) as char, // A-Z[\]^_ (add 64: gives 65-95)
            32..=63 => char_bits as u8 as char,       // space through ? (use as-is: gives 32-63)
            _ => '@',
        };

        if ch != '@' {
            result.push(ch);
        }
    }

    result.trim().to_string()
}

/// Convert 6-bit ASCII string from raw bits (optional, returns None if empty)
pub fn from_sixbit_ascii_optional(raw: u128, length: usize) -> Option<String> {
    let result = from_sixbit_ascii_120(raw, length);
    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Convert 6-bit ASCII string from raw bits (for VIN, 48 bits)
pub fn from_sixbit_ascii_48(raw: u64, length: usize) -> String {
    let mut result = String::new();

    for i in 0..length {
        let shift = 6 * (length - 1 - i);
        let char_bits = (raw >> shift) & 0x3F;

        let ch = match char_bits as u8 {
            0 => '@',                                 // null/padding
            1..=31 => (char_bits as u8 + 64) as char, // A-Z[\]^_ (add 64: gives 65-95)
            32..=63 => char_bits as u8 as char,       // space through ? (use as-is: gives 32-63)
            _ => '@',
        };

        if ch != '@' {
            result.push(ch);
        }
    }

    result.trim().to_string()
}

/// Convert from 1/10th units (for 16-bit values)
pub fn from_10th_u16(raw: u16) -> f32 {
    raw as f32 / 10.0
}

/// Convert from 1/100th units (for 16-bit values)
pub fn from_100th_u16(raw: u16) -> f32 {
    raw as f32 / 100.0
}

/// Convert variable-length binary data from bit vector
pub fn from_variable_binary_data(bits: deku::bitvec::BitVec) -> Vec<u8> {
    let mut result = Vec::new();
    let mut byte_buffer = 0u8;
    let mut bit_count = 0;

    for bit in bits.iter() {
        byte_buffer = (byte_buffer << 1) | (*bit as u8);
        bit_count += 1;

        if bit_count == 8 {
            result.push(byte_buffer);
            byte_buffer = 0;
            bit_count = 0;
        }
    }

    // Handle remaining bits if any (pad with zeros)
    if bit_count > 0 {
        byte_buffer <<= 8 - bit_count;
        result.push(byte_buffer);
    }

    result
}

/// Convert coordinates from 1/10th minutes with sign extension
pub fn from_10th_minutes(raw: u32, bits: usize) -> f64 {
    let max_val = (1u32 << bits) - 1;
    if raw == max_val {
        // Not available
        0.0
    } else {
        // Sign extend based on the number of bits
        let sign_bit = 1u32 << (bits - 1);
        let signed = if raw & sign_bit != 0 {
            // Negative number - extend sign
            let mask = (!0u32) << bits;
            (raw | mask) as i32
        } else {
            raw as i32
        };
        signed as f64 / 10.0
    }
}

/// Convert longitude from 1/600 minutes (Type 27 specific)
pub fn from_longitude_600(raw: u32) -> Option<f64> {
    if raw == 0x3FFFF {
        // Not available
        None
    } else {
        // Sign extend from 18 bits
        let signed = if raw & 0x20000 != 0 {
            (raw | 0xFFFC0000) as i32
        } else {
            raw as i32
        };
        Some(signed as f64 / 600.0)
    }
}

/// Convert latitude from 1/600 minutes (Type 27 specific)
pub fn from_latitude_600(raw: u32) -> Option<f64> {
    if raw == 0x1FFFF {
        // Not available
        None
    } else {
        // Sign extend from 17 bits
        let signed = if raw & 0x10000 != 0 {
            (raw | 0xFFFE0000) as i32
        } else {
            raw as i32
        };
        Some(signed as f64 / 600.0)
    }
}

/// Convert speed (Type 27 specific)
pub fn from_speed_longrange(raw: u8) -> Option<u8> {
    if raw == 63 {
        None
    } else {
        Some(raw)
    }
}

/// Convert course (Type 27 specific)
pub fn from_course_longrange(raw: u16) -> Option<u16> {
    if raw >= 360 {
        None
    } else {
        Some(raw)
    }
}

/// Convert speed (SAR aircraft)
pub fn from_speed_sar(raw: u16) -> Option<u16> {
    if raw == 1023 {
        None
    } else {
        Some(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_turn() {
        assert_eq!(from_turn(0x80), None); // -128 -> not available
        assert_eq!(from_turn(0), Some(0.0));

        // rot_indicated = (rot_ais / 4.733)^2
        let val = 20.0_f32;
        let expected_rot = (val / 4.733).powi(2);
        assert!((from_turn(20).unwrap() - expected_rot).abs() < 1e-6);

        let val = -20.0_f32;
        let expected_rot = -(val / 4.733).powi(2);
        assert!((from_turn(0xec).unwrap() - expected_rot).abs() < 1e-6);

        assert_eq!(from_turn(127), Some(720.0));
        assert_eq!(from_turn(0x81), Some(-720.0)); // -127 as i8
    }

    #[test]
    fn test_from_speed() {
        assert_eq!(from_speed(0), Some(0.0));
        assert_eq!(from_speed(100), Some(10.0));
        assert_eq!(from_speed(1023), None);
    }

    #[test]
    fn test_from_longitude() {
        assert_eq!(from_longitude(0), Some(0.0));
        assert_eq!(from_longitude(0x6791AC0), None);

        // Test positive longitude (1 degree = 600000 raw units)
        assert!((from_longitude(600000).unwrap() - 1.0).abs() < 0.000001);

        // Test negative longitude - need to create proper 28-bit negative value
        // For -1 degree: raw value should be 0x08000000 | (0x10000000 - 600000)
        let neg_one_raw = 0x10000000 - 600000; // Two's complement for 28-bit
        assert!((from_longitude(neg_one_raw).unwrap() - (-1.0)).abs() < 0.000001);
    }

    #[test]
    fn test_from_latitude() {
        assert_eq!(from_latitude(0), Some(0.0));
        assert_eq!(from_latitude(0x3412140), None);

        // Test positive latitude (1 degree = 600000 raw units)
        assert!((from_latitude(600000).unwrap() - 1.0).abs() < 0.000001);

        // Test negative latitude - need to create proper 27-bit negative value
        // For -1 degree: raw value should be 0x08000000 - 600000
        let neg_one_raw = 0x08000000 - 600000; // Two's complement for 27-bit
        assert!((from_latitude(neg_one_raw).unwrap() - (-1.0)).abs() < 0.000001);
    }

    #[test]
    fn test_from_course() {
        assert_eq!(from_course(0), Some(0.0));
        assert_eq!(from_course(900), Some(90.0));
        assert_eq!(from_course(3600), None); // Not available
    }

    #[test]
    fn test_from_draught() {
        assert_eq!(from_draught(0), None);
        assert_eq!(from_draught(122), Some(12.2));
        assert_eq!(from_draught(255), Some(25.5));
    }

    #[test]
    fn test_sixbit_ascii_ais_standard() {
        // AIS 6-bit ASCII uses this mapping:
        // 0 = @ (null/padding)
        // 1-31 = add 64 to get ASCII 65-95 (A-Z[\]^_)
        // 32-63 = use as-is to get ASCII 32-63 (space through ?, includes 0-9)

        // Test individual character mappings based on actual AIS standard:
        assert_eq!(from_sixbit_ascii(6 << 30, 6).chars().next().unwrap(), 'F'); // 6 + 64 = 70 = 'F'
        assert_eq!(from_sixbit_ascii(15 << 30, 6).chars().next().unwrap(), 'O'); // 15 + 64 = 79 = 'O'
        assert_eq!(from_sixbit_ascii(51 << 30, 6).chars().next().unwrap(), '3'); // 51 as-is = 51 = '3'
        assert_eq!(from_sixbit_ascii(56 << 30, 6).chars().next().unwrap(), '8');
        // 56 as-is = 56 = '8'
    }

    #[test]
    fn test_variable_binary_data() {
        use deku::bitvec::BitVec;

        // Test with 8 bits (1 byte)
        let mut bits = BitVec::new();
        // Add bits for 0xEB (11101011)
        for &bit in &[true, true, true, false, true, false, true, true] {
            bits.push(bit);
        }

        let result = from_variable_binary_data(bits);
        assert_eq!(result, vec![0xEB]);

        // Test with partial byte (5 bits)
        let mut bits = BitVec::new();
        // Add bits for 0xF0 (11110, padded to 11110000)
        for &bit in &[true, true, true, true, false] {
            bits.push(bit);
        }

        let result = from_variable_binary_data(bits);
        assert_eq!(result, vec![0xF0]); // Padded with zeros
    }

    #[test]
    fn test_from_mmsi_optional() {
        assert_eq!(from_mmsi_optional(0), None);
        assert_eq!(from_mmsi_optional(123456789), Some(123456789));
    }
}
