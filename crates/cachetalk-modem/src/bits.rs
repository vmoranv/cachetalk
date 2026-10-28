use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BitError {
    InvalidBit(u8),
    UnalignedLength(usize),
}

impl fmt::Display for BitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidBit(bit) => write!(f, "invalid bit value: {bit}"),
            Self::UnalignedLength(length) => {
                write!(f, "bit length {length} is not a multiple of 8")
            }
        }
    }
}

impl std::error::Error for BitError {}

pub fn bytes_to_bits(bytes: &[u8]) -> Vec<u8> {
    let mut bits = Vec::with_capacity(bytes.len() * 8);
    for &byte in bytes {
        for shift in (0..8).rev() {
            bits.push((byte >> shift) & 1);
        }
    }
    bits
}

pub fn bits_to_bytes(bits: &[u8]) -> Result<Vec<u8>, BitError> {
    if bits.len() % 8 != 0 {
        return Err(BitError::UnalignedLength(bits.len()));
    }
    let mut bytes = Vec::with_capacity(bits.len() / 8);
    for chunk in bits.chunks_exact(8) {
        let mut byte = 0u8;
        for &bit in chunk {
            if bit > 1 {
                return Err(BitError::InvalidBit(bit));
            }
            byte = (byte << 1) | bit;
        }
        bytes.push(byte);
    }
    Ok(bytes)
}

pub fn bits_to_bytes_padded(bits: &[u8]) -> Result<Vec<u8>, BitError> {
    let mut padded = bits.to_vec();
    while padded.len() % 8 != 0 {
        padded.push(0);
    }
    bits_to_bytes(&padded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_bytes_msb_first() {
        let bytes = [0x00, 0xA5, 0xFF];
        let bits = bytes_to_bits(&bytes);
        assert_eq!(&bits[..8], &[0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&bits[8..16], &[1, 0, 1, 0, 0, 1, 0, 1]);
        assert_eq!(bits_to_bytes(&bits).unwrap(), bytes);
    }

    #[test]
    fn rejects_unaligned_and_invalid_bits() {
        assert_eq!(bits_to_bytes(&[0, 1, 0]), Err(BitError::UnalignedLength(3)));
        let mut bits = [0u8; 8];
        bits[3] = 2;
        assert_eq!(bits_to_bytes(&bits), Err(BitError::InvalidBit(2)));
    }

    #[test]
    fn padded_packing_zero_fills() {
        assert_eq!(bits_to_bytes_padded(&[1, 0, 1]).unwrap(), vec![0xA0]);
    }
}
