mod mock;
mod platform;

pub use mock::MockHal;

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheTopology {
    pub l1d_size: usize,
    pub cache_line_size: usize,
    pub l1_associativity: usize,
    pub l2_size: usize,
    pub l2_associativity: usize,
    pub l3_size: usize,
    pub l3_associativity: usize,
    pub llc_slices: usize,
    pub physical_cores: usize,
}

impl CacheTopology {
    pub fn unknown() -> Self {
        Self {
            l1d_size: 32 * 1024,
            cache_line_size: 64,
            l1_associativity: 8,
            l2_size: 256 * 1024,
            l2_associativity: 8,
            l3_size: 8 * 1024 * 1024,
            l3_associativity: 16,
            llc_slices: 1,
            physical_cores: 1,
        }
    }
}

impl Default for CacheTopology {
    fn default() -> Self {
        Self::unknown()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvictionSet {
    pub addresses: Vec<usize>,
    pub cache_line_size: usize,
}

impl EvictionSet {
    pub fn new(addresses: Vec<usize>, cache_line_size: usize) -> Self {
        Self {
            addresses,
            cache_line_size,
        }
    }

    pub fn len(&self) -> usize {
        self.addresses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HalError {
    UnsupportedArch(String),
    CoreBindingFailed(String),
    TopologyDetectionFailed(String),
    InvalidConfiguration(String),
    Platform(String),
}

impl fmt::Display for HalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedArch(message) => write!(f, "unsupported architecture: {message}"),
            Self::CoreBindingFailed(message) => write!(f, "core binding failed: {message}"),
            Self::TopologyDetectionFailed(message) => {
                write!(f, "topology detection failed: {message}")
            }
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid configuration: {message}")
            }
            Self::Platform(message) => write!(f, "platform error: {message}"),
        }
    }
}

impl std::error::Error for HalError {}

pub trait MicroArchHal: Send + Sync {
    fn read_timer(&self) -> u64;
    fn probe_address(&self, address: usize) -> u64;
    fn flush_address(&self, address: usize);
    fn evict_address(&self, address: usize);
    fn get_cache_topology(&self) -> CacheTopology;
    fn bind_to_core(&self, core_id: usize) -> Result<(), HalError>;
    fn name(&self) -> &'static str {
        "unknown"
    }
}

pub fn detect() -> Result<Box<dyn MicroArchHal>, HalError> {
    platform::detect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_hal_implements_object_safe_trait() {
        let hal: Box<dyn MicroArchHal> = Box::new(MockHal::new(CacheTopology::unknown()));
        assert_eq!(hal.name(), "mock");
        assert_eq!(hal.read_timer(), 0);
    }

    #[test]
    fn mock_probe_advances_timer_and_reports_latency() {
        let hal = MockHal::with_probe_latency(CacheTopology::unknown(), 42);
        assert_eq!(hal.probe_address(0x1000), 42);
        assert_eq!(hal.read_timer(), 42);
        assert_eq!(hal.probe_address(0x2000), 42);
        assert_eq!(hal.read_timer(), 84);
    }

    #[test]
    fn mock_bind_and_counters() {
        let mut topology = CacheTopology::unknown();
        topology.physical_cores = 4;
        let hal = MockHal::new(topology);
        assert!(hal.bind_to_core(2).is_ok());
        assert_eq!(hal.bound_core(), Some(2));
        assert!(hal.bind_to_core(4).is_err());
        hal.flush_address(0x1000);
        hal.evict_address(0x2000);
        assert_eq!(hal.flush_count(), 1);
        assert_eq!(hal.evict_count(), 1);
    }
}
