use cachetalk_hal::MicroArchHal;
use std::sync::Arc;

pub struct PortContention {
    hal: Arc<dyn MicroArchHal>,
    address: usize,
}

impl PortContention {
    pub fn new(hal: Arc<dyn MicroArchHal>, address: usize) -> Self {
        Self { hal, address }
    }

    pub fn contend(&self, iterations: usize) -> u64 {
        (0..iterations)
            .map(|_| self.hal.probe_address(self.address))
            .sum()
    }

    pub fn measure(&self, rounds: usize, iterations: usize) -> Vec<u64> {
        (0..rounds)
            .map(|_| {
                let start = self.hal.read_timer();
                self.contend(iterations);
                self.hal.read_timer().wrapping_sub(start)
            })
            .collect()
    }

    pub fn hal(&self) -> &Arc<dyn MicroArchHal> {
        &self.hal
    }

    pub fn address(&self) -> usize {
        self.address
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cachetalk_hal::{CacheTopology, MockHal};

    #[test]
    fn contention_accumulates_latency() {
        let hal = Arc::new(MockHal::with_probe_latency(CacheTopology::unknown(), 7));
        let contention = PortContention::new(hal.clone(), 0x4000);
        assert_eq!(contention.contend(5), 35);
        assert_eq!(hal.timer(), 35);
        assert_eq!(contention.address(), 0x4000);
        assert_eq!(contention.hal().name(), "mock");
    }

    #[test]
    fn measure_reports_timer_delta() {
        let hal = Arc::new(MockHal::with_probe_latency(CacheTopology::unknown(), 3));
        let contention = PortContention::new(hal, 0x4000);
        assert_eq!(contention.measure(4, 2), vec![6, 6, 6, 6]);
        assert_eq!(contention.measure(0, 2), Vec::<u64>::new());
    }
}
