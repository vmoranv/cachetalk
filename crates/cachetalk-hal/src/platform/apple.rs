use crate::{CacheTopology, HalError, MicroArchHal};
use std::ffi::{c_char, c_int, c_void, CString};

extern "C" {
    fn mach_absolute_time() -> u64;
    fn sysctlbyname(
        name: *const c_char,
        oldp: *mut c_void,
        oldlenp: *mut usize,
        newp: *mut c_void,
        newlen: usize,
    ) -> c_int;
}

fn read_sysctl(name: &str) -> Option<u64> {
    let name = CString::new(name).ok()?;
    let mut value: u64 = 0;
    let mut length = std::mem::size_of::<u64>();
    let result = unsafe {
        sysctlbyname(
            name.as_ptr(),
            &mut value as *mut u64 as *mut c_void,
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if result == 0 {
        Some(value)
    } else {
        None
    }
}

fn detect_topology() -> CacheTopology {
    let fallback_cores = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1);
    let physical_cores = read_sysctl("hw.physicalcpu")
        .map(|value| value as usize)
        .unwrap_or(fallback_cores)
        .max(1);
    let cache_line_size = read_sysctl("hw.cachelinesize")
        .map(|value| value as usize)
        .unwrap_or(128)
        .max(16);
    let l1d_size = read_sysctl("hw.l1dcachesize")
        .map(|value| value as usize)
        .unwrap_or(128 * 1024);
    let l2_size = read_sysctl("hw.l2cachesize")
        .map(|value| value as usize)
        .unwrap_or(4 * 1024 * 1024);
    let l3_size = read_sysctl("hw.l3cachesize")
        .map(|value| value as usize)
        .unwrap_or(0);
    CacheTopology {
        l1d_size,
        cache_line_size,
        l1_associativity: 8,
        l2_size,
        l2_associativity: 16,
        l3_size,
        l3_associativity: if l3_size == 0 { 0 } else { 16 },
        llc_slices: 1,
        physical_cores,
    }
}

pub struct AppleSiliconHal {
    topology: CacheTopology,
}

impl AppleSiliconHal {
    pub fn new() -> Result<Self, HalError> {
        let topology = detect_topology();
        if topology.cache_line_size == 0 {
            return Err(HalError::TopologyDetectionFailed(
                "cache line size is unavailable".to_string(),
            ));
        }
        Ok(Self { topology })
    }
}

impl MicroArchHal for AppleSiliconHal {
    fn read_timer(&self) -> u64 {
        unsafe { mach_absolute_time() }
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
            "macOS does not expose user-space thread affinity".to_string(),
        ))
    }

    fn name(&self) -> &'static str {
        "apple-silicon"
    }
}
