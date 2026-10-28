use cachetalk_modem::Frame;
use std::fmt;
use std::string::FromUtf8Error;

pub const CHUNK_HEADER_LEN: usize = 5;
pub const CHUNK_FLAG_FIRST: u8 = 0x01;

#[derive(Debug)]
pub enum SemanticError {
    EmptyMessage,
    MessageTooLarge(usize),
    InvalidChunk,
    UnsupportedChunkFlags(u8),
    UnexpectedChunk,
    SequenceMismatch { expected: u8, received: u8 },
    ChunkOverflow { total: usize, received: usize },
    InvalidUtf8(FromUtf8Error),
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMessage => write!(f, "message must not be empty"),
            Self::MessageTooLarge(length) => {
                write!(f, "message length {length} exceeds the supported limit")
            }
            Self::InvalidChunk => write!(f, "malformed chunk"),
            Self::UnsupportedChunkFlags(flags) => {
                write!(f, "unsupported chunk flags: {flags:#04x}")
            }
            Self::UnexpectedChunk => {
                write!(f, "received a continuation chunk without a message start")
            }
            Self::SequenceMismatch { expected, received } => {
                write!(
                    f,
                    "sequence mismatch: expected {expected}, received {received}"
                )
            }
            Self::ChunkOverflow { total, received } => write!(
                f,
                "chunk data exceeds declared message length: {received} > {total}"
            ),
            Self::InvalidUtf8(error) => write!(f, "message is not valid UTF-8: {error}"),
        }
    }
}

impl std::error::Error for SemanticError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidUtf8(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MessageEncoder {
    max_payload: usize,
}

impl MessageEncoder {
    pub fn new(max_payload: usize) -> Self {
        Self { max_payload }
    }

    pub fn max_payload(&self) -> usize {
        self.max_payload
    }

    pub fn chunk_capacity(&self) -> usize {
        self.max_payload.saturating_sub(CHUNK_HEADER_LEN)
    }

    pub fn encode(&self, message: &[u8]) -> Result<Vec<Vec<u8>>, SemanticError> {
        if message.is_empty() {
            return Err(SemanticError::EmptyMessage);
        }
        let capacity = self.chunk_capacity();
        if capacity == 0 {
            return Err(SemanticError::InvalidChunk);
        }
        let total = u32::try_from(message.len())
            .map_err(|_| SemanticError::MessageTooLarge(message.len()))?;
        let total_bytes = total.to_le_bytes();
        Ok(message
            .chunks(capacity)
            .enumerate()
            .map(|(index, chunk)| {
                let mut payload = Vec::with_capacity(CHUNK_HEADER_LEN + chunk.len());
                payload.push(if index == 0 { CHUNK_FLAG_FIRST } else { 0 });
                payload.extend_from_slice(&total_bytes);
                payload.extend_from_slice(chunk);
                payload
            })
            .collect())
    }
}

#[derive(Clone, Debug, Default)]
pub struct MessageAssembler {
    buffer: Vec<u8>,
    expected_sequence: Option<u8>,
    total: Option<usize>,
}

impl MessageAssembler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        self.expected_sequence = None;
        self.total = None;
    }

    pub fn push(&mut self, frame: &Frame) -> Result<Option<String>, SemanticError> {
        if let Some(expected) = self.expected_sequence {
            if frame.sequence != expected {
                let received = frame.sequence;
                self.reset();
                return Err(SemanticError::SequenceMismatch { expected, received });
            }
        }

        let payload = &frame.payload;
        if payload.len() < CHUNK_HEADER_LEN {
            self.reset();
            return Err(SemanticError::InvalidChunk);
        }
        let flags = payload[0];
        if flags & !CHUNK_FLAG_FIRST != 0 {
            self.reset();
            return Err(SemanticError::UnsupportedChunkFlags(flags));
        }
        let total = u32::from_le_bytes([payload[1], payload[2], payload[3], payload[4]]) as usize;
        if total == 0 {
            self.reset();
            return Err(SemanticError::InvalidChunk);
        }
        let data = &payload[CHUNK_HEADER_LEN..];
        if flags & CHUNK_FLAG_FIRST != 0 {
            self.buffer.clear();
            self.total = Some(total);
        } else if self.total.is_none() {
            self.reset();
            return Err(SemanticError::UnexpectedChunk);
        } else if self.total != Some(total) {
            self.reset();
            return Err(SemanticError::InvalidChunk);
        }

        self.buffer.extend_from_slice(data);
        if self.buffer.len() > total {
            let received = self.buffer.len();
            self.reset();
            return Err(SemanticError::ChunkOverflow { total, received });
        }
        self.expected_sequence = Some(frame.sequence.wrapping_add(1));

        if self.buffer.len() == total {
            let bytes = std::mem::take(&mut self.buffer);
            self.reset();
            return String::from_utf8(bytes)
                .map(Some)
                .map_err(SemanticError::InvalidUtf8);
        }
        Ok(None)
    }
}

pub fn bit_error_rate(reference: &[u8], observed: &[u8]) -> f64 {
    let total = reference.len().max(observed.len());
    if total == 0 {
        return 0.0;
    }
    let errors = (0..total)
        .filter(|&index| {
            reference.get(index).copied().unwrap_or(0) != observed.get(index).copied().unwrap_or(0)
        })
        .count();
    errors as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(sequence: u8, payload: Vec<u8>) -> Frame {
        Frame {
            sequence,
            payload,
            ecc: false,
        }
    }

    fn declared_total(chunk: &[u8]) -> usize {
        u32::from_le_bytes([chunk[1], chunk[2], chunk[3], chunk[4]]) as usize
    }

    #[test]
    fn encoder_chunks_and_assembler_round_trips() {
        let encoder = MessageEncoder::new(16);
        assert_eq!(encoder.chunk_capacity(), 11);
        let message = "hello cachetalk";
        let chunks = encoder.encode(message.as_bytes()).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0][0], CHUNK_FLAG_FIRST);
        assert_eq!(chunks[1][0], 0);
        assert_eq!(declared_total(&chunks[0]), message.len());
        assert_eq!(declared_total(&chunks[1]), message.len());

        let mut assembler = MessageAssembler::new();
        assert_eq!(assembler.push(&frame(0, chunks[0].clone())).unwrap(), None);
        assert_eq!(
            assembler.push(&frame(1, chunks[1].clone())).unwrap(),
            Some(message.to_string())
        );
    }

    #[test]
    fn encoder_rejects_empty_messages_and_tiny_payloads() {
        assert!(matches!(
            MessageEncoder::new(16).encode(b""),
            Err(SemanticError::EmptyMessage)
        ));
        assert!(matches!(
            MessageEncoder::new(CHUNK_HEADER_LEN).encode(b"x"),
            Err(SemanticError::InvalidChunk)
        ));
    }

    #[test]
    fn assembler_rejects_sequence_gaps() {
        let encoder = MessageEncoder::new(16);
        let chunks = encoder.encode(b"a message longer than one chunk").unwrap();
        let mut assembler = MessageAssembler::new();
        assert_eq!(assembler.push(&frame(0, chunks[0].clone())).unwrap(), None);
        assert!(matches!(
            assembler.push(&frame(2, chunks[1].clone())),
            Err(SemanticError::SequenceMismatch {
                expected: 1,
                received: 2
            })
        ));
    }

    #[test]
    fn assembler_rejects_malformed_and_unexpected_chunks() {
        let mut assembler = MessageAssembler::new();
        assert!(matches!(
            assembler.push(&frame(0, vec![0, 1])),
            Err(SemanticError::InvalidChunk)
        ));
        assert!(matches!(
            assembler.push(&frame(0, vec![0x02, 1, 0, 0, 0])),
            Err(SemanticError::UnsupportedChunkFlags(0x02))
        ));
        assert!(matches!(
            assembler.push(&frame(0, vec![0, 1, 0, 0, 0])),
            Err(SemanticError::UnexpectedChunk)
        ));
    }

    #[test]
    fn assembler_rejects_overflow_and_inconsistent_totals() {
        let mut payload = vec![CHUNK_FLAG_FIRST];
        payload.extend_from_slice(&3u32.to_le_bytes());
        payload.extend_from_slice(&[1, 2, 3, 4]);
        let mut assembler = MessageAssembler::new();
        assert!(matches!(
            assembler.push(&frame(0, payload)),
            Err(SemanticError::ChunkOverflow {
                total: 3,
                received: 4
            })
        ));

        let encoder = MessageEncoder::new(16);
        let chunks = encoder.encode(b"a message longer than one chunk").unwrap();
        let mut first = chunks[0].clone();
        first[1..5].copy_from_slice(&100u32.to_le_bytes());
        let mut assembler = MessageAssembler::new();
        assert_eq!(assembler.push(&frame(0, first)).unwrap(), None);
        assert!(matches!(
            assembler.push(&frame(1, chunks[1].clone())),
            Err(SemanticError::InvalidChunk)
        ));
    }

    #[test]
    fn assembler_rejects_invalid_utf8() {
        let encoder = MessageEncoder::new(16);
        let chunks = encoder.encode(&[0xFF]).unwrap();
        let mut assembler = MessageAssembler::new();
        assert!(matches!(
            assembler.push(&frame(0, chunks[0].clone())),
            Err(SemanticError::InvalidUtf8(_))
        ));
    }

    #[test]
    fn bit_error_rate_counts_mismatches_and_length_differences() {
        assert_eq!(bit_error_rate(&[], &[]), 0.0);
        assert_eq!(bit_error_rate(&[1, 0, 1, 1], &[1, 0, 1, 1]), 0.0);
        assert!((bit_error_rate(&[1, 0, 1, 1], &[1, 0, 1, 0]) - 0.25).abs() < f64::EPSILON);
        assert!((bit_error_rate(&[1, 0], &[1, 0, 1, 1]) - 0.5).abs() < f64::EPSILON);
    }
}
