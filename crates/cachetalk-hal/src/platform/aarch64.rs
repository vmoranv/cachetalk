use crate::{CacheTopology, HalError, MicroArchHal};

pub struct Aarch64Hal {
    topology: CacheTopology,
}

impl Aarch64Hal {
    pub fn new() -> Result<Self, HalError> {
        let physical_cores = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1);
        Ok(Self {
            topology: CacheTopology {
                l1d_size: 32 * 1024,
                cache_line_size: 64,
                l1_associativity: 8,
                l2_size: 512 * 1024,
                l2_associativity: 8,
                l3_size: 32 * 1024 * 1024,
                l3_associativity: 16,
                llc_slices: 1,
                physical_cores,
            },
        })
    }
}

impl MicroArchHal for Aarch64Hal {
    fn read_timer(&self) -> u64 {
        let value: u64;
        unsafe {
            core::arch::asm!(
                "isb",
                "mrs {}, cntvct_el0",
                out(reg) value,
                options(nostack, preserves_flags)
            );
        }
        value
    }

    fn probe_address(&self, address: usize) -> u64 {
        let start = self.read_timer();
        unsafe {
            std::ptr::read_volatile(address as *const u8);
        }
        let end = self.read_timer();
        end.wrapping_sub(start)
    }

    fn flush_address(&self, address: usize) {
        unsafe {
            core::arch::asm!("dc civac, {}", in(reg) address, options(nostack, preserves_flags));
            core::arch::asm!("dsb sy", options(nostack, preserves_flags));
        }
    }

    fn evict_address(&self, address: usize) {
        self.flush_address(address);
    }

    fn get_cache_topology(&self) -> CacheTopology {
        self.topology
    }

    fn bind_to_core(&self, _core_id: usize) -> Result<(), HalError> {
        Err(HalError::CoreBindingFailed(
            "affinity binding is not implemented for this target".to_string(),
        ))
    }

    fn name(&self) -> &'static str {
        "aarch64"
    }
}
