use crate::PhyError;
use cachetalk_hal::{EvictionSet, MicroArchHal};
use std::fmt;
use std::sync::Arc;

pub struct PrimeProbe {
    hal: Arc<dyn MicroArchHal>,
    eviction_set: EvictionSet,
}

impl fmt::Debug for PrimeProbe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrimeProbe")
            .field("hal", &self.hal.name())
            .field("eviction_set", &self.eviction_set)
            .finish()
    }
}

impl PrimeProbe {
    pub fn new(hal: Arc<dyn MicroArchHal>, eviction_set: EvictionSet) -> Result<Self, PhyError> {
        if eviction_set.is_empty() {
            return Err(PhyError::EmptyEvictionSet);
        }
        if eviction_set.cache_line_size == 0 {
            return Err(PhyError::InvalidConfiguration(
                "cache line size must be non-zero".to_string(),
            ));
        }
        Ok(Self { hal, eviction_set })
    }

    pub fn prime(&self) -> u64 {
        self.eviction_set
            .addresses
            .iter()
            .map(|&address| self.hal.probe_address(address))
            .sum()
    }

    pub fn probe(&self) -> u64 {
        self.eviction_set
            .addresses
            .iter()
            .map(|&address| self.hal.probe_address(address))
            .min()
            .unwrap_or(u64::MAX)
    }

    pub fn probe_slot(&self) -> u64 {
        self.prime();
        self.probe()
    }

    pub fn measure(&self, rounds: usize) -> Vec<u64> {
        (0..rounds).map(|_| self.probe_slot()).collect()
    }

    pub fn hal(&self) -> &Arc<dyn MicroArchHal> {
        &self.hal
    }

    pub fn eviction_set(&self) -> &EvictionSet {
        &self.eviction_set
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cachetalk_hal::{CacheTopology, MockHal};

    fn eviction_set(length: usize) -> EvictionSet {
        EvictionSet::new((0..length).map(|index| 0x1000 + index * 64).collect(), 64)
    }

    #[test]
    fn rejects_empty_eviction_set() {
        let hal: Arc<dyn MicroArchHal> = Arc::new(MockHal::new(CacheTopology::unknown()));
        assert_eq!(
            PrimeProbe::new(hal, EvictionSet::new(Vec::new(), 64)).unwrap_err(),
            PhyError::EmptyEvictionSet
        );
    }

    #[test]
    fn rejects_zero_cache_line_size() {
        let hal: Arc<dyn MicroArchHal> = Arc::new(MockHal::new(CacheTopology::unknown()));
        assert!(matches!(
            PrimeProbe::new(hal, EvictionSet::new(vec![0x1000], 0)),
            Err(PhyError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn prime_and_probe_use_eviction_set() {
        let hal = Arc::new(MockHal::with_probe_latency(CacheTopology::unknown(), 30));
        let probe = PrimeProbe::new(hal.clone(), eviction_set(4)).unwrap();
        assert_eq!(probe.prime(), 120);
        assert_eq!(hal.timer(), 120);
        assert_eq!(probe.probe(), 30);
        assert_eq!(hal.timer(), 240);
        assert_eq!(probe.probe_slot(), 30);
        assert_eq!(probe.measure(3), vec![30, 30, 30]);
    }

    #[test]
    fn exposes_configuration() {
        let hal: Arc<dyn MicroArchHal> = Arc::new(MockHal::new(CacheTopology::unknown()));
        let set = eviction_set(2);
        let probe = PrimeProbe::new(hal, set.clone()).unwrap();
        assert_eq!(probe.eviction_set(), &set);
        assert_eq!(probe.hal().name(), "mock");
    }
}
