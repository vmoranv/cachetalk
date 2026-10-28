use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EccError {
    TooShort,
    Uncorrectable,
}

impl fmt::Display for EccError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort => write!(f, "encoded data is too short"),
            Self::Uncorrectable => write!(f, "uncorrectable Hamming error"),
        }
    }
}

impl std::error::Error for EccError {}

pub fn encoded_len(original_len: usize) -> usize {
    (original_len * 14 + 7) / 8
}

pub fn encode_hamming(data: &[u8]) -> Vec<u8> {
    let mut bits = Vec::with_capacity(data.len() * 14);
    for &byte in data {
        for nibble in [byte & 0x0F, byte >> 4] {
            let d0 = (nibble >> 0) & 1;
            let d1 = (nibble >> 1) & 1;
            let d2 = (nibble >> 2) & 1;
            let d3 = (nibble >> 3) & 1;
            let p1 = d0 ^ d1 ^ d3;
            let p2 = d0 ^ d2 ^ d3;
            let p4 = d1 ^ d2 ^ d3;
            bits.extend_from_slice(&[p1, p2, d0, p4, d1, d2, d3]);
        }
    }
    pack_bits(&bits)
}

pub fn decode_hamming(encoded: &[u8], original_len: usize) -> Result<Vec<u8>, EccError> {
    let needed_bits = original_len * 14;
    if encoded.len() * 8 < needed_bits {
        return Err(EccError::TooShort);
    }
    let bits = unpack_bits(encoded);
    let mut output = Vec::with_capacity(original_len);
    for byte_index in 0..original_len {
        let mut byte = 0u8;
        for nibble_index in 0..2 {
            let offset = byte_index * 14 + nibble_index * 7;
            let codeword = &bits[offset..offset + 7];
            let mut corrected = [0u8; 7];
            corrected.copy_from_slice(codeword);
            let s1 = corrected[0] ^ corrected[2] ^ corrected[4] ^ corrected[6];
            let s2 = corrected[1] ^ corrected[2] ^ corrected[5] ^ corrected[6];
            let s4 = corrected[3] ^ corrected[4] ^ corrected[5] ^ corrected[6];
            let syndrome = (s1 as usize) | ((s2 as usize) << 1) | ((s4 as usize) << 2);
            if syndrome > 7 {
                return Err(EccError::Uncorrectable);
            }
            if syndrome != 0 {
                corrected[syndrome - 1] ^= 1;
            }
            let nibble =
                corrected[2] | (corrected[4] << 1) | (corrected[5] << 2) | (corrected[6] << 3);
            byte |= nibble << (nibble_index * 4);
        }
        output.push(byte);
    }
    Ok(output)
}

fn pack_bits(bits: &[u8]) -> Vec<u8> {
    let mut packed = vec![0u8; (bits.len() + 7) / 8];
    for (index, &bit) in bits.iter().enumerate() {
        if bit != 0 {
            packed[index / 8] |= 1 << (7 - (index % 8));
        }
    }
    packed
}

fn unpack_bits(bytes: &[u8]) -> Vec<u8> {
    let mut bits = Vec::with_capacity(bytes.len() * 8);
    for &byte in bytes {
        for shift in (0..8).rev() {
            bits.push((byte >> shift) & 1);
        }
    }
    bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_all_byte_values() {
        let data: Vec<u8> = (0..=255).collect();
        let encoded = encode_hamming(&data);
        assert_eq!(encoded.len(), encoded_len(data.len()));
        assert_eq!(decode_hamming(&encoded, data.len()).unwrap(), data);
    }

    #[test]
    fn corrects_single_bit_errors() {
        let data = [0xA5, 0x3C];
        let mut encoded = encode_hamming(&data);
        for bit in 0..(encoded.len() * 8) {
            let mut damaged = encoded.clone();
            damaged[bit / 8] ^= 1 << (7 - (bit % 8));
            assert_eq!(decode_hamming(&damaged, data.len()).unwrap(), data);
        }
        encoded[0] ^= 0x80;
        assert_eq!(decode_hamming(&encoded, data.len()).unwrap(), data);
    }

    #[test]
    fn rejects_short_input() {
        assert_eq!(decode_hamming(&[0], 2), Err(EccError::TooShort));
    }
}
