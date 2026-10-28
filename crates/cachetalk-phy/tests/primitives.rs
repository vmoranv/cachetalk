use cachetalk_hal::{CacheTopology, EvictionSet, MicroArchHal, MockHal};
use cachetalk_phy::{CacheOccupancy, DramBankProbe, PhyError, PortContention, PrimeProbe};
use std::sync::Arc;

fn hal_with_latency(latency: u64) -> Arc<MockHal> {
    Arc::new(MockHal::with_probe_latency(
        CacheTopology::unknown(),
        latency,
    ))
}

fn eviction_set(length: usize) -> EvictionSet {
    EvictionSet::new((0..length).map(|index| 0x1000 + index * 64).collect(), 64)
}

#[test]
fn prime_probe_rejects_invalid_configuration() {
    let hal: Arc<dyn MicroArchHal> = hal_with_latency(100);

    assert_eq!(
        PrimeProbe::new(hal.clone(), EvictionSet::new(Vec::new(), 64)).unwrap_err(),
        PhyError::EmptyEvictionSet
    );
    match PrimeProbe::new(hal, EvictionSet::new(vec![0x1000], 0)) {
        Err(PhyError::InvalidConfiguration(message)) => {
            assert_eq!(message, "cache line size must be non-zero");
        }
        other => panic!("expected invalid configuration, got {other:?}"),
    }
}

#[test]
fn prime_probe_primes_and_probes_every_address() {
    let hal = hal_with_latency(100);
    let probe = PrimeProbe::new(hal.clone(), eviction_set(2)).unwrap();

    assert_eq!(probe.prime(), 200);
    assert_eq!(hal.timer(), 200);

    assert_eq!(probe.probe(), 100);
    assert_eq!(hal.timer(), 400);

    assert_eq!(probe.probe_slot(), 100);
    assert_eq!(hal.timer(), 800);
    assert_eq!(probe.measure(3), vec![100, 100, 100]);
    assert_eq!(hal.timer(), 2000);
}

#[test]
fn prime_probe_exposes_configuration() {
    let hal: Arc<dyn MicroArchHal> = hal_with_latency(100);
    let set = eviction_set(2);
    let probe = PrimeProbe::new(hal, set.clone()).unwrap();

    assert_eq!(probe.eviction_set(), &set);
    assert_eq!(probe.hal().name(), "mock");
}

#[test]
fn occupancy_rejects_invalid_geometry() {
    let hal: Arc<dyn MicroArchHal> = hal_with_latency(100);

    match CacheOccupancy::new(hal.clone(), 0, 64, 100) {
        Err(PhyError::InvalidConfiguration(message)) => {
            assert_eq!(message, "line count must be non-zero");
        }
        Ok(_) => panic!("expected invalid configuration, got a valid occupancy"),
        Err(other) => panic!("expected invalid configuration, got {other:?}"),
    }
    match CacheOccupancy::new(hal.clone(), 8, 0, 100) {
        Err(PhyError::InvalidConfiguration(message)) => {
            assert_eq!(message, "cache line size must be non-zero");
        }
        Ok(_) => panic!("expected invalid configuration, got a valid occupancy"),
        Err(other) => panic!("expected invalid configuration, got {other:?}"),
    }
    match CacheOccupancy::new(hal, usize::MAX, 2, 100) {
        Err(PhyError::InvalidConfiguration(message)) => {
            assert_eq!(message, "buffer size overflows usize");
        }
        Ok(_) => panic!("expected invalid configuration, got a valid occupancy"),
        Err(other) => panic!("expected invalid configuration, got {other:?}"),
    }
}

#[test]
fn occupancy_samples_one_latency_per_line() {
    let hal: Arc<dyn MicroArchHal> = hal_with_latency(100);
    let mut occupancy = CacheOccupancy::new(hal, 8, 64, 100).unwrap();

    assert_eq!(occupancy.line_count(), 8);
    assert_eq!(occupancy.cache_line_size(), 64);
    assert_eq!(occupancy.stride(), 64);
    assert_eq!(occupancy.buffer_len(), 512);

    occupancy.prime();
    let latencies = occupancy.probe_slot().to_vec();
    assert_eq!(latencies, vec![100; 8]);
    assert_eq!(occupancy.sample_latencies(), &latencies[..]);
    assert_eq!(occupancy.slow_line_count(), 8);
    assert_eq!(occupancy.occupancy_ratio(), 1.0);
    assert!(occupancy.is_occupied());

    occupancy.set_threshold(101);
    assert_eq!(occupancy.threshold(), 101);
    assert_eq!(occupancy.slow_line_count(), 0);
    assert_eq!(occupancy.occupancy_ratio(), 0.0);
    assert!(!occupancy.is_occupied());
}

#[test]
fn occupancy_transitions_are_idempotent() {
    let hal = hal_with_latency(100);
    let mut occupancy = CacheOccupancy::new(hal.clone(), 8, 64, 100).unwrap();

    occupancy.set_occupied(true);
    assert_eq!(hal.evict_count(), 0);

    occupancy.set_occupied(true);
    assert_eq!(hal.evict_count(), 0);

    occupancy.set_occupied(false);
    assert_eq!(hal.evict_count(), 8);

    occupancy.set_occupied(false);
    assert_eq!(hal.evict_count(), 8);

    occupancy.set_occupied(true);
    assert_eq!(hal.evict_count(), 8);
}

#[test]
fn contention_accumulates_latency() {
    let hal = hal_with_latency(7);
    let contention = PortContention::new(hal.clone(), 0x4000);

    assert_eq!(contention.contend(5), 35);
    assert_eq!(hal.timer(), 35);
    assert_eq!(contention.address(), 0x4000);
    assert_eq!(contention.hal().name(), "mock");
}

#[test]
fn contention_measures_timer_deltas() {
    let hal = hal_with_latency(3);
    let contention = PortContention::new(hal, 0x4000);

    assert_eq!(contention.measure(4, 2), vec![6, 6, 6, 6]);
    assert!(contention.measure(0, 2).is_empty());
}

#[test]
fn dram_bank_probe_requires_addresses() {
    let hal: Arc<dyn MicroArchHal> = hal_with_latency(11);

    assert_eq!(
        DramBankProbe::new(hal, Vec::new()).unwrap_err(),
        PhyError::EmptyEvictionSet
    );
}

#[test]
fn dram_bank_probe_indexes_and_scans() {
    let hal = hal_with_latency(11);
    let probe = DramBankProbe::new(hal, vec![0x1000, 0x2000, 0x3000]).unwrap();

    assert_eq!(probe.probe_bank(1), Ok(11));
    assert_eq!(
        probe.probe_bank(3).unwrap_err(),
        PhyError::IndexOutOfBounds(3)
    );
    assert_eq!(probe.addresses(), &[0x1000, 0x2000, 0x3000]);
    assert_eq!(probe.scan(), vec![11, 11, 11]);
    assert_eq!(probe.hal().name(), "mock");
}

#[test]
fn phy_error_display_messages() {
    assert_eq!(
        PhyError::EmptyEvictionSet.to_string(),
        "eviction set is empty"
    );
    assert_eq!(
        PhyError::InvalidConfiguration("bad geometry".to_string()).to_string(),
        "invalid configuration: bad geometry"
    );
    assert_eq!(
        PhyError::IndexOutOfBounds(7).to_string(),
        "index out of bounds: 7"
    );
}
