use crate::crc::crc8;
use crate::ecc::{decode_hamming, encode_hamming, encoded_len, EccError};
use std::fmt;

pub const PREAMBLE_LEN: usize = 8;
pub const PREAMBLE: [u8; PREAMBLE_LEN] = [0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAB];
pub const FRAME_VERSION: u8 = 1;
pub const HEADER_LEN: usize = PREAMBLE_LEN + 5;

pub const FLAG_ECC: u8 = 0x01;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub sequence: u8,
    pub payload: Vec<u8>,
    pub ecc: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    BadPreamble,
    Incomplete,
    InvalidVersion(u8),
    UnsupportedFlags(u8),
    LengthTooLarge,
    CrcMismatch { expected: u8, actual: u8 },
    EccUncorrectable,
    TrailingBytes(usize),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadPreamble => write!(f, "bad preamble"),
            Self::Incomplete => write!(f, "incomplete frame"),
            Self::InvalidVersion(version) => write!(f, "invalid frame version: {version}"),
            Self::UnsupportedFlags(flags) => write!(f, "unsupported frame flags: {flags:#04x}"),
            Self::LengthTooLarge => write!(f, "payload length exceeds u16"),
            Self::CrcMismatch { expected, actual } => {
                write!(
                    f,
                    "crc mismatch: expected {expected:#04x}, actual {actual:#04x}"
                )
            }
            Self::EccUncorrectable => write!(f, "uncorrectable ecc error"),
            Self::TrailingBytes(count) => write!(f, "{count} trailing bytes after frame"),
        }
    }
}

impl std::error::Error for FrameError {}

pub fn encode_frame(sequence: u8, payload: &[u8], use_ecc: bool) -> Result<Vec<u8>, FrameError> {
    let payload_len = u16::try_from(payload.len()).map_err(|_| FrameError::LengthTooLarge)?;
    let mut header = [0u8; 5];
    header[0] = FRAME_VERSION;
    header[1] = if use_ecc { FLAG_ECC } else { 0 };
    header[2] = sequence;
    header[3..5].copy_from_slice(&payload_len.to_le_bytes());

    let mut crc_input = Vec::with_capacity(header.len() + payload.len());
    crc_input.extend_from_slice(&header);
    crc_input.extend_from_slice(payload);
    let checksum = crc8(&crc_input);

    let mut protected = Vec::with_capacity(payload.len() + 1);
    protected.extend_from_slice(payload);
    protected.push(checksum);

    if use_ecc {
        protected = encode_hamming(&protected);
    }

    let mut frame = Vec::with_capacity(HEADER_LEN + protected.len());
    frame.extend_from_slice(&PREAMBLE);
    frame.extend_from_slice(&header);
    frame.extend_from_slice(&protected);
    Ok(frame)
}

pub fn decode_frame(bytes: &[u8]) -> Result<Frame, FrameError> {
    if bytes.len() < PREAMBLE_LEN {
        return if PREAMBLE[..bytes.len()] == *bytes {
            Err(FrameError::Incomplete)
        } else {
            Err(FrameError::BadPreamble)
        };
    }
    if bytes[..PREAMBLE_LEN] != PREAMBLE {
        return Err(FrameError::BadPreamble);
    }
    if bytes.len() < HEADER_LEN {
        return Err(FrameError::Incomplete);
    }

    let version = bytes[PREAMBLE_LEN];
    if version != FRAME_VERSION {
        return Err(FrameError::InvalidVersion(version));
    }

    let flags = bytes[PREAMBLE_LEN + 1];
    if flags & !FLAG_ECC != 0 {
        return Err(FrameError::UnsupportedFlags(flags));
    }
    let ecc = flags & FLAG_ECC != 0;
    let sequence = bytes[PREAMBLE_LEN + 2];
    let payload_len =
        u16::from_le_bytes([bytes[PREAMBLE_LEN + 3], bytes[PREAMBLE_LEN + 4]]) as usize;
    let protected_len = if ecc {
        encoded_len(payload_len + 1)
    } else {
        payload_len + 1
    };
    let total_len = HEADER_LEN + protected_len;
    if bytes.len() < total_len {
        return Err(FrameError::Incomplete);
    }

    let raw_protected = &bytes[HEADER_LEN..total_len];
    let decoded;
    let protected = if ecc {
        decoded = decode_hamming(raw_protected, payload_len + 1).map_err(|error| match error {
            EccError::TooShort => FrameError::Incomplete,
            EccError::Uncorrectable => FrameError::EccUncorrectable,
        })?;
        decoded.as_slice()
    } else {
        raw_protected
    };

    let payload = protected[..payload_len].to_vec();
    let actual = protected[payload_len];
    let mut crc_input = Vec::with_capacity(5 + payload.len());
    crc_input.extend_from_slice(&bytes[PREAMBLE_LEN..HEADER_LEN]);
    crc_input.extend_from_slice(&payload);
    let expected = crc8(&crc_input);
    if actual != expected {
        return Err(FrameError::CrcMismatch { expected, actual });
    }
    if bytes.len() > total_len {
        return Err(FrameError::TrailingBytes(bytes.len() - total_len));
    }

    Ok(Frame {
        sequence,
        payload,
        ecc,
    })
}

#[derive(Clone, Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Result<Vec<Frame>, FrameError> {
        self.buffer.extend_from_slice(bytes);
        self.process()
    }

    pub fn finish(&mut self) -> Result<Vec<Frame>, FrameError> {
        let frames = self.process()?;
        if self.buffer.is_empty() {
            Ok(frames)
        } else {
            self.buffer.clear();
            Err(FrameError::Incomplete)
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    fn process(&mut self) -> Result<Vec<Frame>, FrameError> {
        let mut frames = Vec::new();
        loop {
            let Some(position) = find_preamble(&self.buffer) else {
                self.retain_preamble_suffix();
                break;
            };
            if position > 0 {
                self.buffer.drain(..position);
            }
            if self.buffer.len() < HEADER_LEN {
                break;
            }

            let version = self.buffer[PREAMBLE_LEN];
            if version != FRAME_VERSION {
                self.buffer.drain(..1);
                return Err(FrameError::InvalidVersion(version));
            }
            let flags = self.buffer[PREAMBLE_LEN + 1];
            if flags & !FLAG_ECC != 0 {
                self.buffer.drain(..1);
                return Err(FrameError::UnsupportedFlags(flags));
            }
            let ecc = flags & FLAG_ECC != 0;
            let payload_len =
                u16::from_le_bytes([self.buffer[PREAMBLE_LEN + 3], self.buffer[PREAMBLE_LEN + 4]])
                    as usize;
            let protected_len = if ecc {
                encoded_len(payload_len + 1)
            } else {
                payload_len + 1
            };
            let total_len = HEADER_LEN + protected_len;
            if self.buffer.len() < total_len {
                break;
            }

            match decode_frame(&self.buffer[..total_len]) {
                Ok(frame) => {
                    self.buffer.drain(..total_len);
                    frames.push(frame);
                }
                Err(FrameError::Incomplete) => break,
                Err(error) => {
                    self.buffer.drain(..total_len);
                    return Err(error);
                }
            }
        }
        Ok(frames)
    }

    fn retain_preamble_suffix(&mut self) {
        let max = PREAMBLE_LEN.saturating_sub(1).min(self.buffer.len());
        let mut keep = 0;
        for length in (1..=max).rev() {
            if self.buffer.ends_with(&PREAMBLE[..length]) {
                keep = length;
                break;
            }
        }
        let excess = self.buffer.len().saturating_sub(keep);
        if excess > 0 {
            self.buffer.drain(..excess);
        }
    }
}

fn find_preamble(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < PREAMBLE_LEN {
        return None;
    }
    bytes
        .windows(PREAMBLE_LEN)
        .position(|window| window == PREAMBLE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_without_ecc() {
        let payload = b"cachetalk";
        let encoded = encode_frame(7, payload, false).unwrap();
        assert_eq!(encoded.len(), HEADER_LEN + payload.len() + 1);
        assert_eq!(&encoded[..PREAMBLE_LEN], &PREAMBLE);
        let frame = decode_frame(&encoded).unwrap();
        assert_eq!(frame.sequence, 7);
        assert_eq!(frame.payload, payload);
        assert!(!frame.ecc);
    }

    #[test]
    fn round_trip_with_ecc() {
        let payload = b"noisy channel";
        let encoded = encode_frame(9, payload, true).unwrap();
        assert_eq!(encoded.len(), HEADER_LEN + encoded_len(payload.len() + 1));
        let frame = decode_frame(&encoded).unwrap();
        assert_eq!(frame.sequence, 9);
        assert_eq!(frame.payload, payload);
        assert!(frame.ecc);
    }

    #[test]
    fn rejects_bad_preamble() {
        let mut encoded = encode_frame(1, b"x", false).unwrap();
        encoded[0] = 0x00;
        assert_eq!(decode_frame(&encoded), Err(FrameError::BadPreamble));
    }

    #[test]
    fn reports_incomplete_frame() {
        let encoded = encode_frame(1, b"hello", false).unwrap();
        assert_eq!(
            decode_frame(&encoded[..encoded.len() - 1]),
            Err(FrameError::Incomplete)
        );
        assert_eq!(decode_frame(&[]), Err(FrameError::Incomplete));
    }

    #[test]
    fn rejects_invalid_version_and_flags() {
        let mut encoded = encode_frame(1, b"x", false).unwrap();
        encoded[PREAMBLE_LEN] = 2;
        assert_eq!(decode_frame(&encoded), Err(FrameError::InvalidVersion(2)));

        let mut encoded = encode_frame(1, b"x", false).unwrap();
        encoded[PREAMBLE_LEN + 1] = 0x80;
        assert_eq!(
            decode_frame(&encoded),
            Err(FrameError::UnsupportedFlags(0x80))
        );
    }

    #[test]
    fn detects_crc_mismatch() {
        let mut encoded = encode_frame(3, b"payload", false).unwrap();
        encoded[HEADER_LEN] ^= 0xFF;
        match decode_frame(&encoded) {
            Err(FrameError::CrcMismatch { .. }) => {}
            other => panic!("expected crc mismatch, got {other:?}"),
        }
    }

    #[test]
    fn rejects_trailing_bytes() {
        let mut encoded = encode_frame(1, b"x", false).unwrap();
        encoded.push(0);
        assert_eq!(decode_frame(&encoded), Err(FrameError::TrailingBytes(1)));
    }

    #[test]
    fn corrects_single_bit_with_ecc() {
        let payload = b"ecc";
        let mut encoded = encode_frame(4, payload, true).unwrap();
        encoded[HEADER_LEN + 2] ^= 0x01;
        let frame = decode_frame(&encoded).unwrap();
        assert_eq!(frame.payload, payload);
    }

    #[test]
    fn streaming_decoder_handles_split_frames() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&encode_frame(1, b"first", false).unwrap());
        stream.extend_from_slice(&encode_frame(2, b"second", true).unwrap());

        let mut decoder = FrameDecoder::new();
        let mut frames = Vec::new();
        for chunk in stream.chunks(3) {
            frames.extend(decoder.feed(chunk).unwrap());
        }
        frames.extend(decoder.finish().unwrap());
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].payload, b"first");
        assert_eq!(frames[1].payload, b"second");
        assert_eq!(frames[1].sequence, 2);
    }

    #[test]
    fn streaming_decoder_ignores_prefix_noise() {
        let mut stream = vec![0x00, 0x01, 0x02, 0x03];
        stream.extend_from_slice(&encode_frame(5, b"data", false).unwrap());
        let mut decoder = FrameDecoder::new();
        let frames = decoder.feed(&stream).unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].payload, b"data");
    }

    #[test]
    fn finish_reports_incomplete_tail() {
        let encoded = encode_frame(1, b"tail", false).unwrap();
        let mut decoder = FrameDecoder::new();
        assert!(decoder.feed(&encoded[..10]).unwrap().is_empty());
        assert_eq!(decoder.finish(), Err(FrameError::Incomplete));
    }
}
