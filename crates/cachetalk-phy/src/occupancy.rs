use crate::PhyError;
use cachetalk_hal::MicroArchHal;
use std::sync::Arc;

pub struct CacheOccupancy {
    hal: Arc<dyn MicroArchHal>,
    buffer: Vec<u8>,
    line_count: usize,
    cache_line_size: usize,
    stride: usize,
    threshold: u64,
    last_latencies: Vec<u64>,
    last_set: Option<bool>,
}

impl CacheOccupancy {
    pub fn new(
        hal: Arc<dyn MicroArchHal>,
        line_count: usize,
        cache_line_size: usize,
        threshold: u64,
    ) -> Result<Self, PhyError> {
        if line_count == 0 {
            return Err(PhyError::InvalidConfiguration(
                "line count must be non-zero".to_string(),
            ));
        }
        if cache_line_size == 0 {
            return Err(PhyError::InvalidConfiguration(
                "cache line size must be non-zero".to_string(),
            ));
        }
        let size = line_count.checked_mul(cache_line_size).ok_or_else(|| {
            PhyError::InvalidConfiguration("buffer size overflows usize".to_string())
        })?;
        Ok(Self {
            hal,
            buffer: vec![0u8; size],
            line_count,
            cache_line_size,
            stride: cache_line_size,
            threshold,
            last_latencies: vec![0; line_count],
            last_set: None,
        })
    }

    pub fn prime(&mut self) {
        let base = self.buffer.as_mut_ptr();
        for index in 0..self.line_count {
            let offset = index * self.stride;
            unsafe {
                std::ptr::write_volatile(base.add(offset), 1);
            }
        }
        self.last_set = Some(true);
    }

    pub fn evict(&mut self) {
        let base = self.buffer.as_ptr() as usize;
        for index in 0..self.line_count {
            self.hal.evict_address(base + index * self.stride);
        }
        self.last_set = Some(false);
    }

    pub fn set_occupied(&mut self, occupied: bool) {
        if self.last_set == Some(occupied) {
            return;
        }
        if occupied {
            self.prime();
        } else {
            self.evict();
        }
    }

    pub fn probe(&mut self) -> &[u64] {
        let base = self.buffer.as_ptr() as usize;
        for (index, latency) in self.last_latencies.iter_mut().enumerate() {
            *latency = self.hal.probe_address(base + index * self.stride);
        }
        &self.last_latencies
    }

    pub fn probe_slot(&mut self) -> &[u64] {
        self.prime();
        self.probe()
    }

    pub fn slow_line_count(&self) -> usize {
        self.last_latencies
            .iter()
            .filter(|&&latency| latency >= self.threshold)
            .count()
    }

    pub fn occupancy_ratio(&self) -> f64 {
        if self.last_latencies.is_empty() {
            return 0.0;
        }
        self.slow_line_count() as f64 / self.last_latencies.len() as f64
    }

    pub fn is_occupied(&self) -> bool {
        self.slow_line_count() > self.line_count / 2
    }

    pub fn sample_latencies(&self) -> &[u64] {
        &self.last_latencies
    }

    pub fn last_latencies(&self) -> &[u64] {
        &self.last_latencies
    }

    pub fn threshold(&self) -> u64 {
        self.threshold
    }

    pub fn set_threshold(&mut self, threshold: u64) {
        self.threshold = threshold;
    }

    pub fn line_count(&self) -> usize {
        self.line_count
    }

    pub fn cache_line_size(&self) -> usize {
        self.cache_line_size
    }

    pub fn stride(&self) -> usize {
        self.stride
    }

    pub fn buffer_len(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cachetalk_hal::{CacheTopology, MockHal};

    fn occupancy(threshold: u64) -> CacheOccupancy {
        let hal: Arc<dyn MicroArchHal> = Arc::new(MockHal::new(CacheTopology::unknown()));
        CacheOccupancy::new(hal, 8, 64, threshold).unwrap()
    }

    fn occupancy_with_hal(threshold: u64) -> (CacheOccupancy, Arc<MockHal>) {
        let hal = Arc::new(MockHal::new(CacheTopology::unknown()));
        let occupancy = CacheOccupancy::new(hal.clone(), 8, 64, threshold).unwrap();
        (occupancy, hal)
    }

    #[test]
    fn rejects_invalid_geometry() {
        let hal: Arc<dyn MicroArchHal> = Arc::new(MockHal::new(CacheTopology::unknown()));
        assert!(matches!(
            CacheOccupancy::new(hal.clone(), 0, 64, 100),
            Err(PhyError::InvalidConfiguration(_))
        ));
        assert!(matches!(
            CacheOccupancy::new(hal, 8, 0, 100),
            Err(PhyError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn allocates_one_line_per_sample() {
        let occupancy = occupancy(100);
        assert_eq!(occupancy.line_count(), 8);
        assert_eq!(occupancy.cache_line_size(), 64);
        assert_eq!(occupancy.stride(), 64);
        assert_eq!(occupancy.buffer_len(), 512);
    }

    #[test]
    fn probe_collects_latencies_and_classifies() {
        let mut occupancy = occupancy(100);
        occupancy.prime();
        let latencies = occupancy.probe_slot().to_vec();
        assert_eq!(latencies, vec![100; 8]);
        assert_eq!(occupancy.slow_line_count(), 8);
        assert_eq!(occupancy.occupancy_ratio(), 1.0);
        assert!(occupancy.is_occupied());

        occupancy.set_threshold(101);
        assert_eq!(occupancy.slow_line_count(), 0);
        assert_eq!(occupancy.occupancy_ratio(), 0.0);
        assert!(!occupancy.is_occupied());
        assert_eq!(occupancy.threshold(), 101);
    }

    #[test]
    fn detects_partial_occupancy() {
        let mut occupancy = occupancy(100);
        occupancy.prime();
        occupancy.probe();
        occupancy.last_latencies[0] = 50;
        occupancy.last_latencies[1] = 50;
        occupancy.last_latencies[2] = 50;
        assert_eq!(occupancy.slow_line_count(), 5);
        assert!(occupancy.is_occupied());
        assert_eq!(occupancy.sample_latencies(), occupancy.last_latencies());
    }

    #[test]
    fn set_occupied_skips_redundant_transitions() {
        let (mut occupancy, hal) = occupancy_with_hal(100);
        let mut expected = Vec::new();
        for occupied in [false, false, true, true, false] {
            occupancy.set_occupied(occupied);
            expected.push(hal.evict_count());
        }
        assert_eq!(expected, vec![8, 8, 8, 8, 16]);
    }

    #[test]
    fn prime_marks_occupied_without_evicting() {
        let (mut occupancy, hal) = occupancy_with_hal(100);
        occupancy.set_occupied(true);
        assert_eq!(hal.evict_count(), 0);
        occupancy.set_occupied(false);
        assert_eq!(hal.evict_count(), 8);
    }
}
