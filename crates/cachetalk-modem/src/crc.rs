pub const CRC8_POLYNOMIAL: u8 = 0x07;
pub const CRC8_INITIAL: u8 = 0xFF;

pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = CRC8_INITIAL;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ CRC8_POLYNOMIAL
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_has_initial_value() {
        assert_eq!(crc8(&[]), CRC8_INITIAL);
    }

    #[test]
    fn known_vectors() {
        // CRC-8 with polynomial 0x07 and initial value 0xFF.
        assert_eq!(crc8(b"123456789"), 0xFB);
        assert_eq!(crc8(&[0x00]), 0xF3);
    }

    #[test]
    fn detects_single_bit_changes() {
        let original = crc8(b"cachetalk");
        let mut changed = *b"cachetalk";
        changed[0] ^= 0x01;
        assert_ne!(original, crc8(&changed));
    }
}
