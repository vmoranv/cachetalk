use crate::bits::{bits_to_bytes, bytes_to_bits};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManchesterError {
    InvalidPair(u8, u8),
    Incomplete,
}

impl fmt::Display for ManchesterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPair(first, second) => {
                write!(f, "invalid Manchester pair: {first}{second}")
            }
            Self::Incomplete => write!(f, "incomplete Manchester symbol"),
        }
    }
}

impl std::error::Error for ManchesterError {}

pub fn encode_manchester(data: &[u8]) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(data.len() * 16);
    for bit in bytes_to_bits(data) {
        if bit == 0 {
            encoded.extend_from_slice(&[0, 1]);
        } else {
            encoded.extend_from_slice(&[1, 0]);
        }
    }
    encoded
}

pub fn decode_manchester(bits: &[u8]) -> Result<Vec<u8>, ManchesterError> {
    if bits.len() % 2 != 0 {
        return Err(ManchesterError::Incomplete);
    }
    let mut decoded = Vec::with_capacity(bits.len() / 2);
    for pair in bits.chunks_exact(2) {
        decoded.push(match (pair[0], pair[1]) {
            (0, 1) => 0,
            (1, 0) => 1,
            (first, second) => return Err(ManchesterError::InvalidPair(first, second)),
        });
    }
    bits_to_bytes(&decoded).map_err(|_| ManchesterError::Incomplete)
}

#[derive(Clone, Debug, Default)]
pub struct ManchesterDecoder {
    pending: Option<u8>,
    bits: Vec<u8>,
}

impl ManchesterDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, bits: &[u8]) -> Result<Vec<u8>, ManchesterError> {
        let mut output = Vec::new();
        for &bit in bits {
            match self.pending.take() {
                None => self.pending = Some(bit),
                Some(first) => {
                    let decoded = match (first, bit) {
                        (0, 1) => 0,
                        (1, 0) => 1,
                        _ => return Err(ManchesterError::InvalidPair(first, bit)),
                    };
                    self.bits.push(decoded);
                    if self.bits.len() == 8 {
                        let byte = self.bits.iter().fold(0u8, |acc, &value| (acc << 1) | value);
                        output.push(byte);
                        self.bits.clear();
                    }
                }
            }
        }
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, ManchesterError> {
        if self.pending.is_some() || !self.bits.is_empty() {
            Err(ManchesterError::Incomplete)
        } else {
            Ok(Vec::new())
        }
    }

    pub fn reset(&mut self) {
        self.pending = None;
        self.bits.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_payload() {
        let data = [0x00, 0x5A, 0xFF];
        let encoded = encode_manchester(&data);
        assert_eq!(encoded.len(), data.len() * 16);
        assert_eq!(decode_manchester(&encoded).unwrap(), data);
    }

    #[test]
    fn rejects_invalid_pairs_and_odd_input() {
        assert_eq!(
            decode_manchester(&[0, 0]),
            Err(ManchesterError::InvalidPair(0, 0))
        );
        assert_eq!(decode_manchester(&[1]), Err(ManchesterError::Incomplete));
    }

    #[test]
    fn streaming_decoder_handles_split_symbols() {
        let data = [0xDE, 0xAD];
        let encoded = encode_manchester(&data);
        let mut decoder = ManchesterDecoder::new();
        assert!(decoder.feed(&encoded[..3]).unwrap().is_empty());
        let mut decoded = decoder.feed(&encoded[3..17]).unwrap();
        decoded.extend(decoder.feed(&encoded[17..]).unwrap());
        assert_eq!(decoded, data);
        assert!(decoder.finish().unwrap().is_empty());
    }

    #[test]
    fn streaming_decoder_reports_partial_symbol() {
        let mut decoder = ManchesterDecoder::new();
        decoder.feed(&[1]).unwrap();
        assert_eq!(decoder.finish(), Err(ManchesterError::Incomplete));
    }
}
