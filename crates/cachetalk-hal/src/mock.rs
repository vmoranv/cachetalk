use crate::{CacheTopology, HalError, MicroArchHal};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub struct MockHal {
    timer: AtomicU64,
    probe_latency: AtomicU64,
    topology: CacheTopology,
    bound_core: AtomicUsize,
    flush_count: AtomicU64,
    evict_count: AtomicU64,
}

impl MockHal {
    pub fn new(topology: CacheTopology) -> Self {
        Self::with_probe_latency(topology, 100)
    }

    pub fn with_probe_latency(topology: CacheTopology, probe_latency: u64) -> Self {
        Self {
            timer: AtomicU64::new(0),
            probe_latency: AtomicU64::new(probe_latency),
            topology,
            bound_core: AtomicUsize::new(usize::MAX),
            flush_count: AtomicU64::new(0),
            evict_count: AtomicU64::new(0),
        }
    }

    pub fn timer(&self) -> u64 {
        self.timer.load(Ordering::SeqCst)
    }

    pub fn advance(&self, ticks: u64) {
        self.timer.fetch_add(ticks, Ordering::SeqCst);
    }

    pub fn probe_latency(&self) -> u64 {
        self.probe_latency.load(Ordering::SeqCst)
    }

    pub fn set_probe_latency(&self, ticks: u64) {
        self.probe_latency.store(ticks, Ordering::SeqCst);
    }

    pub fn topology(&self) -> CacheTopology {
        self.topology
    }

    pub fn bound_core(&self) -> Option<usize> {
        let value = self.bound_core.load(Ordering::SeqCst);
        if value == usize::MAX {
            None
        } else {
            Some(value)
        }
    }

    pub fn flush_count(&self) -> u64 {
        self.flush_count.load(Ordering::SeqCst)
    }

    pub fn evict_count(&self) -> u64 {
        self.evict_count.load(Ordering::SeqCst)
    }
}

impl MicroArchHal for MockHal {
    fn read_timer(&self) -> u64 {
        self.timer()
    }

    fn probe_address(&self, _address: usize) -> u64 {
        let latency = self.probe_latency.load(Ordering::SeqCst);
        self.timer.fetch_add(latency, Ordering::SeqCst);
        latency
    }

    fn flush_address(&self, _address: usize) {
        self.flush_count.fetch_add(1, Ordering::SeqCst);
    }

    fn evict_address(&self, _address: usize) {
        self.evict_count.fetch_add(1, Ordering::SeqCst);
    }

    fn get_cache_topology(&self) -> CacheTopology {
        self.topology
    }

    fn bind_to_core(&self, core_id: usize) -> Result<(), HalError> {
        if core_id >= self.topology.physical_cores.max(1) {
            return Err(HalError::CoreBindingFailed(format!(
                "core {core_id} is outside the reported topology"
            )));
        }
        self.bound_core.store(core_id, Ordering::SeqCst);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "mock"
    }
}
