use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use cachetalk_hal::MicroArchHal;
use cachetalk_modem::{
    decode_manchester, demodulate, encode_frame, encode_manchester, encoded_len, modulate,
    FrameDecoder, FrameError, FskConfig, FskError, ManchesterError, HEADER_LEN,
};
use cachetalk_phy::{CacheOccupancy, PhyError};

use crate::semantic::{MessageAssembler, MessageEncoder, SemanticError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CovertMode {
    PrimeProbe,
    Fsk,
    PortContention,
}

impl CovertMode {
    pub const ALL: [Self; 3] = [Self::PrimeProbe, Self::Fsk, Self::PortContention];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PrimeProbe => "prime_probe",
            Self::Fsk => "fsk",
            Self::PortContention => "port_contention",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|mode| mode.as_str() == value)
    }
}

impl Default for CovertMode {
    fn default() -> Self {
        Self::Fsk
    }
}

impl fmt::Display for CovertMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelConfig {
    pub default_mode: CovertMode,
    pub fsk: FskConfig,
    pub max_payload: usize,
    pub use_ecc: bool,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            default_mode: CovertMode::Fsk,
            fsk: FskConfig::default(),
            max_payload: 1024,
            use_ecc: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportError {
    Unavailable(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) => write!(f, "transport unavailable: {message}"),
        }
    }
}

impl Error for TransportError {}

pub trait CovertTransport {
    fn send(&mut self, samples: Vec<f32>) -> Result<(), TransportError>;

    fn receive(&mut self, timeout: Duration) -> Option<Vec<f32>>;

    fn pending(&self) -> usize {
        0
    }

    fn name(&self) -> &'static str {
        "custom"
    }
}

#[derive(Clone, Debug, Default)]
pub struct LoopbackTransport {
    queue: Arc<(Mutex<VecDeque<Vec<f32>>>, Condvar)>,
}

impl LoopbackTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&self, samples: Vec<f32>) {
        let (queue, condvar) = &*self.queue;
        let mut queue = queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        queue.push_back(samples);
        condvar.notify_one();
    }

    pub fn len(&self) -> usize {
        let (queue, _) = &*self.queue;
        queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        let (queue, _) = &*self.queue;
        queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }
}

impl CovertTransport for LoopbackTransport {
    fn send(&mut self, samples: Vec<f32>) -> Result<(), TransportError> {
        self.push(samples);
        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Option<Vec<f32>> {
        let (queue, condvar) = &*self.queue;
        let timeout = timeout.min(Duration::from_secs(86_400));
        let queue = queue
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (mut queue, _) = condvar
            .wait_timeout_while(queue, timeout, |queue| queue.is_empty())
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        queue.pop_front()
    }

    fn pending(&self) -> usize {
        self.len()
    }

    fn name(&self) -> &'static str {
        "loopback"
    }
}

#[derive(Clone)]
pub struct CacheOccupancyTransport {
    occupancy: Arc<Mutex<CacheOccupancy>>,
    medium: LoopbackTransport,
}

impl CacheOccupancyTransport {
    pub fn new(
        hal: Arc<dyn MicroArchHal>,
        line_count: usize,
        cache_line_size: usize,
        threshold: u64,
    ) -> Result<Self, PhyError> {
        let occupancy = CacheOccupancy::new(hal, line_count, cache_line_size, threshold)?;
        Ok(Self::from_occupancy(Arc::new(Mutex::new(occupancy))))
    }

    pub fn from_occupancy(occupancy: Arc<Mutex<CacheOccupancy>>) -> Self {
        Self {
            occupancy,
            medium: LoopbackTransport::new(),
        }
    }

    pub fn occupancy(&self) -> Arc<Mutex<CacheOccupancy>> {
        Arc::clone(&self.occupancy)
    }

    pub fn medium(&self) -> &LoopbackTransport {
        &self.medium
    }
}

impl CovertTransport for CacheOccupancyTransport {
    fn send(&mut self, samples: Vec<f32>) -> Result<(), TransportError> {
        let mut occupancy = self
            .occupancy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for &sample in &samples {
            occupancy.set_occupied(sample > 0.0);
        }
        drop(occupancy);
        self.medium.push(samples);
        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Option<Vec<f32>> {
        self.medium.receive(timeout)
    }

    fn pending(&self) -> usize {
        self.medium.pending()
    }

    fn name(&self) -> &'static str {
        "cache_occupancy"
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SendReport {
    pub mode: CovertMode,
    pub message_bytes: usize,
    pub frames: usize,
    pub frame_bytes: usize,
    pub bits: usize,
    pub samples: usize,
    pub priority: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReceiveReport {
    pub mode: CovertMode,
    pub message: String,
    pub ber: f64,
    pub frames: usize,
    pub frame_bytes: usize,
}

#[derive(Debug)]
pub enum ChannelError {
    UnsupportedMode(CovertMode),
    Timeout,
    Semantic(SemanticError),
    Frame(FrameError),
    Fsk(FskError),
    Manchester(ManchesterError),
    Transport(TransportError),
}

impl fmt::Display for ChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedMode(mode) => write!(
                f,
                "mode '{mode}' requires a calibrated hardware transport; only 'fsk' is available in the in-process transport"
            ),
            Self::Timeout => write!(f, "timed out waiting for a covert message"),
            Self::Semantic(error) => write!(f, "semantic error: {error}"),
            Self::Frame(error) => write!(f, "frame error: {error}"),
            Self::Fsk(error) => write!(f, "fsk error: {error}"),
            Self::Manchester(error) => write!(f, "manchester error: {error}"),
            Self::Transport(error) => write!(f, "transport error: {error}"),
        }
    }
}

impl Error for ChannelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Semantic(error) => Some(error),
            Self::Frame(error) => Some(error),
            Self::Fsk(error) => Some(error),
            Self::Manchester(error) => Some(error),
            Self::Transport(error) => Some(error),
            Self::UnsupportedMode(_) | Self::Timeout => None,
        }
    }
}

impl From<SemanticError> for ChannelError {
    fn from(error: SemanticError) -> Self {
        Self::Semantic(error)
    }
}

impl From<FrameError> for ChannelError {
    fn from(error: FrameError) -> Self {
        Self::Frame(error)
    }
}

impl From<FskError> for ChannelError {
    fn from(error: FskError) -> Self {
        Self::Fsk(error)
    }
}

impl From<ManchesterError> for ChannelError {
    fn from(error: ManchesterError) -> Self {
        Self::Manchester(error)
    }
}

impl From<TransportError> for ChannelError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}

#[derive(Clone, Debug)]
pub struct Channel<T: CovertTransport> {
    transport: T,
    config: ChannelConfig,
    encoder: MessageEncoder,
    assembler: MessageAssembler,
    frame_decoder: FrameDecoder,
    next_sequence: u8,
    reference_bits: Vec<u8>,
}

impl<T: CovertTransport> Channel<T> {
    pub fn new(transport: T) -> Self {
        Self::with_config(transport, ChannelConfig::default())
    }

    pub fn with_config(transport: T, config: ChannelConfig) -> Self {
        let encoder = MessageEncoder::new(config.max_payload);
        Self {
            transport,
            config,
            encoder,
            assembler: MessageAssembler::new(),
            frame_decoder: FrameDecoder::new(),
            next_sequence: 0,
            reference_bits: Vec::new(),
        }
    }

    pub fn config(&self) -> &ChannelConfig {
        &self.config
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn default_mode(&self) -> CovertMode {
        self.config.default_mode
    }

    pub fn pending_transmissions(&self) -> usize {
        self.transport.pending()
    }

    pub fn send_message(
        &mut self,
        message: &str,
        mode: CovertMode,
        priority: u8,
    ) -> Result<SendReport, ChannelError> {
        if mode != CovertMode::Fsk {
            return Err(ChannelError::UnsupportedMode(mode));
        }
        let chunks = self.encoder.encode(message.as_bytes())?;
        let mut report = SendReport {
            mode,
            message_bytes: message.len(),
            frames: 0,
            frame_bytes: 0,
            bits: 0,
            samples: 0,
            priority,
        };
        for chunk in chunks {
            let frame = encode_frame(self.next_sequence, &chunk, self.config.use_ecc)?;
            let symbols = encode_manchester(&frame);
            let samples = modulate(&symbols, self.config.fsk)?;
            report.frames += 1;
            report.frame_bytes += frame.len();
            report.bits += symbols.len();
            report.samples += samples.len();
            self.transport.send(samples)?;
            self.remember_reference(&symbols);
            self.next_sequence = self.next_sequence.wrapping_add(1);
        }
        Ok(report)
    }

    pub fn receive_message(
        &mut self,
        mode: CovertMode,
        timeout: Duration,
    ) -> Result<ReceiveReport, ChannelError> {
        if mode != CovertMode::Fsk {
            return Err(ChannelError::UnsupportedMode(mode));
        }
        if self.config.fsk.samples_per_bit == 0 {
            return Err(ChannelError::Fsk(FskError::InvalidConfiguration(
                "samples per bit must be non-zero".to_string(),
            )));
        }
        let deadline = Instant::now().checked_add(timeout);
        let mut report = ReceiveReport {
            mode,
            message: String::new(),
            ber: 0.0,
            frames: 0,
            frame_bytes: 0,
        };
        let mut compared_bits = 0usize;
        let mut bit_errors = 0usize;
        loop {
            let wait = deadline
                .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|| Duration::from_secs(86_400));
            let samples = self.transport.receive(wait).ok_or(ChannelError::Timeout)?;
            let width = self.config.fsk.samples_per_bit as usize;
            let bit_count = samples.len() / width;
            if bit_count == 0 {
                if deadline.map_or(false, |deadline| Instant::now() >= deadline) {
                    return Err(ChannelError::Timeout);
                }
                continue;
            }
            let bits = demodulate(&samples, bit_count, self.config.fsk)?;
            let reference_len = bit_count.min(self.reference_bits.len());
            if reference_len > 0 {
                let reference: Vec<u8> = self.reference_bits.drain(..reference_len).collect();
                bit_errors += reference
                    .iter()
                    .zip(&bits[..reference_len])
                    .filter(|(expected, observed)| expected != observed)
                    .count();
                compared_bits += reference_len;
            }
            let bytes = decode_manchester(&bits)?;
            let frames = self.frame_decoder.feed(&bytes)?;
            for frame in &frames {
                report.frames += 1;
                report.frame_bytes += HEADER_LEN
                    + if frame.ecc {
                        encoded_len(frame.payload.len() + 1)
                    } else {
                        frame.payload.len() + 1
                    };
                if let Some(message) = self.assembler.push(frame)? {
                    report.message = message;
                    report.ber = if compared_bits == 0 {
                        0.0
                    } else {
                        bit_errors as f64 / compared_bits as f64
                    };
                    return Ok(report);
                }
            }
            if deadline.map_or(false, |deadline| Instant::now() >= deadline) {
                return Err(ChannelError::Timeout);
            }
        }
    }

    fn remember_reference(&mut self, bits: &[u8]) {
        self.reference_bits.extend_from_slice(bits);
        let capacity = self.config.max_payload.saturating_mul(16).max(4096);
        if self.reference_bits.len() > capacity {
            let excess = self.reference_bits.len() - capacity;
            self.reference_bits.drain(..excess);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use cachetalk_hal::{CacheTopology, MockHal};

    #[derive(Clone, Debug, Default)]
    struct FailingTransport;

    impl CovertTransport for FailingTransport {
        fn send(&mut self, _samples: Vec<f32>) -> Result<(), TransportError> {
            Err(TransportError::Unavailable("test transport".to_string()))
        }

        fn receive(&mut self, _timeout: Duration) -> Option<Vec<f32>> {
            None
        }
    }

    #[test]
    fn loopback_transport_round_trips_samples() {
        let mut transport = LoopbackTransport::new();
        assert!(transport.is_empty());
        transport.send(vec![0.0, 1.0, 0.5]).unwrap();
        assert_eq!(transport.len(), 1);
        assert_eq!(transport.pending(), 1);
        assert_eq!(
            transport.receive(Duration::from_millis(10)),
            Some(vec![0.0, 1.0, 0.5])
        );
        assert!(transport.receive(Duration::from_millis(1)).is_none());
    }

    #[test]
    fn cache_occupancy_transport_drives_occupancy_and_queues_samples() {
        let hal = Arc::new(MockHal::new(CacheTopology::unknown()));
        let mut transport = CacheOccupancyTransport::new(hal.clone(), 8, 64, 100).unwrap();
        assert_eq!(transport.occupancy().lock().unwrap().line_count(), 8);
        transport.send(vec![0.0, 1.0, 0.0]).unwrap();
        assert_eq!(hal.evict_count(), 16);
        assert_eq!(transport.pending(), 1);
        assert_eq!(transport.name(), "cache_occupancy");
        assert_eq!(transport.receive(Duration::ZERO), Some(vec![0.0, 1.0, 0.0]));
        assert_eq!(transport.pending(), 0);
    }

    #[test]
    fn channel_round_trips_message() {
        let transport = LoopbackTransport::new();
        let mut sender = Channel::new(transport.clone());
        let mut receiver = Channel::new(transport);
        let report = sender.send_message("hello", CovertMode::Fsk, 5).unwrap();
        assert_eq!(report.frames, 1);
        assert_eq!(report.message_bytes, 5);
        assert_eq!(report.priority, 5);
        assert!(report.bits > 0);
        assert_eq!(
            report.samples,
            report.bits * FskConfig::default().samples_per_bit as usize
        );

        let received = receiver
            .receive_message(CovertMode::Fsk, Duration::from_secs(1))
            .unwrap();
        assert_eq!(received.mode, CovertMode::Fsk);
        assert_eq!(received.message, "hello");
        assert_eq!(received.ber, 0.0);
        assert_eq!(received.frames, 1);
        assert_eq!(received.frame_bytes, report.frame_bytes);
    }

    #[test]
    fn channel_round_trips_multi_chunk_message() {
        let transport = LoopbackTransport::new();
        let config = ChannelConfig {
            max_payload: 16,
            ..ChannelConfig::default()
        };
        let mut sender = Channel::with_config(transport.clone(), config.clone());
        let mut receiver = Channel::with_config(transport, config);
        let message = "hello cachetalk";
        let report = sender.send_message(message, CovertMode::Fsk, 3).unwrap();
        assert_eq!(report.frames, 2);
        assert_eq!(sender.pending_transmissions(), 2);

        let received = receiver
            .receive_message(CovertMode::Fsk, Duration::from_secs(1))
            .unwrap();
        assert_eq!(received.message, message);
        assert_eq!(received.frames, 2);
        assert_eq!(received.frame_bytes, report.frame_bytes);
    }

    #[test]
    fn loopback_send_queues_samples() {
        let mut transport = LoopbackTransport::new();
        let mut sender = Channel::new(transport.clone());
        let report = sender.send_message("x", CovertMode::Fsk, 1).unwrap();
        assert_eq!(transport.len(), report.frames);
        assert_eq!(sender.pending_transmissions(), report.frames);
        let samples = transport.receive(Duration::ZERO).unwrap();
        assert_eq!(samples.len(), report.samples);
        assert!(transport.is_empty());
    }

    #[test]
    fn unsupported_modes_are_rejected() {
        let mut channel = Channel::new(LoopbackTransport::new());
        for mode in [CovertMode::PrimeProbe, CovertMode::PortContention] {
            assert!(matches!(
                channel.send_message("x", mode, 5),
                Err(ChannelError::UnsupportedMode(rejected)) if rejected == mode
            ));
            assert!(matches!(
                channel.receive_message(mode, Duration::from_millis(10)),
                Err(ChannelError::UnsupportedMode(rejected)) if rejected == mode
            ));
        }
    }

    #[test]
    fn receive_times_out_on_empty_transport() {
        let mut channel = Channel::new(LoopbackTransport::new());
        let start = Instant::now();
        assert!(matches!(
            channel.receive_message(CovertMode::Fsk, Duration::from_millis(20)),
            Err(ChannelError::Timeout)
        ));
        assert!(start.elapsed() >= Duration::from_millis(10));
    }

    #[test]
    fn mode_parse_and_display() {
        assert_eq!(CovertMode::default(), CovertMode::Fsk);
        for mode in CovertMode::ALL {
            assert_eq!(CovertMode::parse(mode.as_str()), Some(mode));
            assert_eq!(mode.to_string(), mode.as_str());
        }
        assert_eq!(CovertMode::parse("unknown"), None);
    }

    #[test]
    fn transport_errors_propagate() {
        let mut channel = Channel::new(FailingTransport);
        match channel.send_message("x", CovertMode::Fsk, 5) {
            Err(ChannelError::Transport(TransportError::Unavailable(message))) => {
                assert_eq!(message, "test transport");
            }
            other => panic!("expected transport error, got {other:?}"),
        }
    }
}
