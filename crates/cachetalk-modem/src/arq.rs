use std::collections::{BTreeMap, VecDeque};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArqConfig {
    pub window_size: usize,
    pub max_retries: usize,
}

impl Default for ArqConfig {
    fn default() -> Self {
        Self {
            window_size: 4,
            max_retries: 3,
        }
    }
}

impl ArqConfig {
    pub fn new(window_size: usize, max_retries: usize) -> Result<Self, ArqError> {
        if !(1..=128).contains(&window_size) {
            return Err(ArqError::InvalidConfiguration(
                "window_size must be between 1 and 128".to_string(),
            ));
        }
        Ok(Self {
            window_size,
            max_retries,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArqError {
    InvalidConfiguration(String),
    WindowFull,
    SequenceNotInFlight(u8),
    RetryLimitExceeded(u8),
    OutOfWindow { expected: u8, received: u8 },
    NoInFlight,
}

impl fmt::Display for ArqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid arq configuration: {message}")
            }
            Self::WindowFull => write!(f, "arq send window is full"),
            Self::SequenceNotInFlight(sequence) => {
                write!(f, "sequence {sequence} is not in flight")
            }
            Self::RetryLimitExceeded(sequence) => {
                write!(f, "retry limit exceeded for sequence {sequence}")
            }
            Self::OutOfWindow { expected, received } => {
                write!(
                    f,
                    "sequence {received} outside window at expected {expected}"
                )
            }
            Self::NoInFlight => write!(f, "no frame in flight"),
        }
    }
}

impl std::error::Error for ArqError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InFlight {
    pub sequence: u8,
    pub payload: Vec<u8>,
    pub attempts: usize,
}

#[derive(Clone, Debug)]
pub struct ArqSender {
    config: ArqConfig,
    in_flight: BTreeMap<u8, InFlight>,
    order: VecDeque<u8>,
    next_sequence: u8,
}

impl ArqSender {
    pub fn new(config: ArqConfig) -> Result<Self, ArqError> {
        let config = ArqConfig::new(config.window_size, config.max_retries)?;
        Ok(Self {
            config,
            in_flight: BTreeMap::new(),
            order: VecDeque::new(),
            next_sequence: 0,
        })
    }

    pub fn send(&mut self, payload: Vec<u8>) -> Result<u8, ArqError> {
        if self.in_flight.len() >= self.config.window_size {
            return Err(ArqError::WindowFull);
        }
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.in_flight.insert(
            sequence,
            InFlight {
                sequence,
                payload,
                attempts: 0,
            },
        );
        self.order.push_back(sequence);
        Ok(sequence)
    }

    pub fn acknowledge(&mut self, sequence: u8) -> Result<(), ArqError> {
        if self.in_flight.remove(&sequence).is_none() {
            return Err(ArqError::SequenceNotInFlight(sequence));
        }
        self.order.retain(|item| *item != sequence);
        Ok(())
    }

    pub fn retransmit_oldest(&mut self) -> Result<InFlight, ArqError> {
        let sequence = *self.order.front().ok_or(ArqError::NoInFlight)?;
        let frame = self
            .in_flight
            .get_mut(&sequence)
            .ok_or(ArqError::NoInFlight)?;
        if frame.attempts >= self.config.max_retries {
            return Err(ArqError::RetryLimitExceeded(sequence));
        }
        frame.attempts += 1;
        Ok(frame.clone())
    }

    pub fn config(&self) -> ArqConfig {
        self.config
    }

    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }

    pub fn next_sequence(&self) -> u8 {
        self.next_sequence
    }

    pub fn in_flight(&self) -> Vec<InFlight> {
        self.order
            .iter()
            .filter_map(|sequence| self.in_flight.get(sequence).cloned())
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct ArqReceiver {
    config: ArqConfig,
    expected: u8,
    buffer: BTreeMap<u8, Vec<u8>>,
}

impl ArqReceiver {
    pub fn new(config: ArqConfig) -> Result<Self, ArqError> {
        let config = ArqConfig::new(config.window_size, config.max_retries)?;
        Ok(Self {
            config,
            expected: 0,
            buffer: BTreeMap::new(),
        })
    }

    pub fn receive(&mut self, sequence: u8, payload: Vec<u8>) -> Result<Vec<Vec<u8>>, ArqError> {
        let distance = sequence.wrapping_sub(self.expected);
        if distance as usize >= self.config.window_size {
            return Err(ArqError::OutOfWindow {
                expected: self.expected,
                received: sequence,
            });
        }

        if distance == 0 {
            let mut delivered = vec![payload];
            self.expected = self.expected.wrapping_add(1);
            while let Some(buffered) = self.buffer.remove(&self.expected) {
                delivered.push(buffered);
                self.expected = self.expected.wrapping_add(1);
            }
            Ok(delivered)
        } else {
            if self.buffer.contains_key(&sequence) {
                return Ok(Vec::new());
            }
            self.buffer.insert(sequence, payload);
            Ok(Vec::new())
        }
    }

    pub fn expected_sequence(&self) -> u8 {
        self.expected
    }

    pub fn buffered_count(&self) -> usize {
        self.buffer.len()
    }

    pub fn config(&self) -> ArqConfig {
        self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_configuration() {
        assert!(ArqConfig::new(0, 3).is_err());
        assert!(ArqConfig::new(129, 3).is_err());
        assert!(ArqConfig::new(1, 0).is_ok());
    }

    #[test]
    fn sender_tracks_window_and_sequences() {
        let mut sender = ArqSender::new(ArqConfig::new(2, 1).unwrap()).unwrap();
        assert_eq!(sender.send(b"a".to_vec()).unwrap(), 0);
        assert_eq!(sender.send(b"b".to_vec()).unwrap(), 1);
        assert_eq!(sender.send(b"c".to_vec()), Err(ArqError::WindowFull));
        sender.acknowledge(0).unwrap();
        assert_eq!(sender.send(b"c".to_vec()).unwrap(), 2);
        assert_eq!(sender.next_sequence(), 3);
        assert_eq!(sender.in_flight_count(), 2);
    }

    #[test]
    fn sender_retransmits_oldest_with_limit() {
        let mut sender = ArqSender::new(ArqConfig::new(4, 1).unwrap()).unwrap();
        sender.send(b"a".to_vec()).unwrap();
        sender.send(b"b".to_vec()).unwrap();
        let first = sender.retransmit_oldest().unwrap();
        assert_eq!(first.sequence, 0);
        assert_eq!(first.attempts, 1);
        assert_eq!(
            sender.retransmit_oldest(),
            Err(ArqError::RetryLimitExceeded(0))
        );
        sender.acknowledge(0).unwrap();
        let second = sender.retransmit_oldest().unwrap();
        assert_eq!(second.sequence, 1);
    }

    #[test]
    fn sender_reports_missing_ack() {
        let mut sender = ArqSender::new(ArqConfig::default()).unwrap();
        assert_eq!(sender.acknowledge(4), Err(ArqError::SequenceNotInFlight(4)));
        assert_eq!(sender.retransmit_oldest(), Err(ArqError::NoInFlight));
    }

    #[test]
    fn receiver_delivers_in_order() {
        let mut receiver = ArqReceiver::new(ArqConfig::default()).unwrap();
        assert_eq!(
            receiver.receive(0, b"a".to_vec()).unwrap(),
            vec![b"a".to_vec()]
        );
        assert_eq!(receiver.expected_sequence(), 1);
        assert_eq!(receiver.buffered_count(), 0);
    }

    #[test]
    fn receiver_buffers_and_releases_out_of_order() {
        let mut receiver = ArqReceiver::new(ArqConfig::default()).unwrap();
        assert!(receiver.receive(1, b"b".to_vec()).unwrap().is_empty());
        assert!(receiver.receive(3, b"d".to_vec()).unwrap().is_empty());
        assert_eq!(receiver.buffered_count(), 2);
        assert_eq!(
            receiver.receive(0, b"a".to_vec()).unwrap(),
            vec![b"a".to_vec(), b"b".to_vec()]
        );
        assert_eq!(receiver.expected_sequence(), 2);
        assert_eq!(
            receiver.receive(2, b"c".to_vec()).unwrap(),
            vec![b"c".to_vec(), b"d".to_vec()]
        );
        assert_eq!(receiver.expected_sequence(), 4);
    }

    #[test]
    fn receiver_handles_duplicates_and_old_sequences() {
        let mut receiver = ArqReceiver::new(ArqConfig::default()).unwrap();
        assert!(receiver.receive(2, b"c".to_vec()).unwrap().is_empty());
        assert!(receiver.receive(2, b"c".to_vec()).unwrap().is_empty());
        assert_eq!(receiver.buffered_count(), 1);
        receiver.receive(0, b"a".to_vec()).unwrap();
        assert_eq!(
            receiver.receive(0, b"a".to_vec()),
            Err(ArqError::OutOfWindow {
                expected: 1,
                received: 0
            })
        );
    }

    #[test]
    fn receiver_rejects_out_of_window() {
        let mut receiver = ArqReceiver::new(ArqConfig::new(4, 3).unwrap()).unwrap();
        assert_eq!(
            receiver.receive(4, b"e".to_vec()),
            Err(ArqError::OutOfWindow {
                expected: 0,
                received: 4
            })
        );
    }

    #[test]
    fn sequences_wrap() {
        let mut sender = ArqSender::new(ArqConfig::new(1, 0).unwrap()).unwrap();
        for _ in 0..=255u8 {
            let sequence = sender.send(Vec::new()).unwrap();
            sender.acknowledge(sequence).unwrap();
        }
        assert_eq!(sender.next_sequence(), 0);

        let mut receiver = ArqReceiver::new(ArqConfig::new(4, 0).unwrap()).unwrap();
        for _ in 0..=255u8 {
            let delivered = receiver
                .receive(receiver.expected_sequence(), Vec::new())
                .unwrap();
            assert_eq!(delivered.len(), 1);
        }
        assert_eq!(receiver.expected_sequence(), 0);
        assert_eq!(receiver.receive(0, b"wrapped".to_vec()).unwrap().len(), 1);
    }
}
