pub mod dram_bank;
pub mod occupancy;
pub mod port_contention;
pub mod prime_probe;

pub use dram_bank::DramBankProbe;
pub use occupancy::CacheOccupancy;
pub use port_contention::PortContention;
pub use prime_probe::PrimeProbe;

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhyError {
    EmptyEvictionSet,
    InvalidConfiguration(String),
    IndexOutOfBounds(usize),
}

impl fmt::Display for PhyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyEvictionSet => write!(f, "eviction set is empty"),
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid configuration: {message}")
            }
            Self::IndexOutOfBounds(index) => write!(f, "index out of bounds: {index}"),
        }
    }
}

impl std::error::Error for PhyError {}
