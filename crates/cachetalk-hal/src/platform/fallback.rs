use crate::{CacheTopology, HalError, MicroArchHal};
use std::sync::OnceLock;
use std::time::Instant;

pub struct FallbackHal {
    topology: CacheTopology,
}

impl FallbackHal {
    pub fn new() -> Result<Self, HalError> {
        let physical_cores = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1);
        Ok(Self {
            topology: CacheTopology {
                physical_cores,
                ..CacheTopology::unknown()
            },
        })
    }
}

fn origin() -> &'static Instant {
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now)
}

impl MicroArchHal for FallbackHal {
    fn read_timer(&self) -> u64 {
        origin().elapsed().as_nanos() as u64
    }

    fn probe_address(&self, address: usize) -> u64 {
        let start = self.read_timer();
        unsafe {
            std::ptr::read_volatile(address as *const u8);
        }
        let end = self.read_timer();
        end.wrapping_sub(start)
    }

    fn flush_address(&self, _address: usize) {}

    fn evict_address(&self, _address: usize) {}

    fn get_cache_topology(&self) -> CacheTopology {
        self.topology
    }

    fn bind_to_core(&self, _core_id: usize) -> Result<(), HalError> {
        Err(HalError::CoreBindingFailed(
            "affinity binding is unavailable on this target".to_string(),
        ))
    }

    fn name(&self) -> &'static str {
        "fallback"
    }
}
