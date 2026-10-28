use crate::{CacheTopology, HalError, MicroArchHal};

pub struct X86_64Hal {
    topology: CacheTopology,
}

impl X86_64Hal {
    pub fn new() -> Result<Self, HalError> {
        let physical_cores = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1);
        Ok(Self {
            topology: CacheTopology {
                l1d_size: 32 * 1024,
                cache_line_size: 64,
                l1_associativity: 8,
                l2_size: 256 * 1024,
                l2_associativity: 8,
                l3_size: 8 * 1024 * 1024,
                l3_associativity: 16,
                llc_slices: 1,
                physical_cores,
            },
        })
    }
}

impl MicroArchHal for X86_64Hal {
    fn read_timer(&self) -> u64 {
        let low: u32;
        let high: u32;
        let mut auxiliary: u32 = 0;
        unsafe {
            core::arch::asm!(
                "rdtscp",
                out("eax") low,
                out("edx") high,
                inout("ecx") auxiliary,
                options(nostack, preserves_flags)
            );
        }
        ((high as u64) << 32) | low as u64
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
            core::arch::asm!("clflush [{}]", in(reg) address, options(nostack, preserves_flags));
            core::arch::asm!("mfence", options(nostack, preserves_flags));
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
        "x86_64"
    }
}
