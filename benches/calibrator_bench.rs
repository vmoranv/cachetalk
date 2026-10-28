mod common;

use std::sync::Arc;

use common::bench;

use cachetalk::calibrator::{
    measure_probe_samples, otsu_threshold, EvictionSetBuilder, LatencyStats, SyntheticSetOracle,
};
use cachetalk::hal::{CacheTopology, MicroArchHal, MockHal};

fn hal() -> Arc<dyn MicroArchHal> {
    Arc::new(MockHal::new(CacheTopology::unknown()))
}

fn bench_eviction_set_builder() {
    let candidates: Vec<usize> = (0..1024).map(|i| 0x1000 + i * 64).collect();
    let oracle = SyntheticSetOracle::new(64, 16, 4, 3).expect("oracle");

    bench("eviction_set_builder/build_1024_candidates", || {
        EvictionSetBuilder::new()
            .build(&candidates, &oracle)
            .expect("build")
    });
}

fn bench_latency_stats() {
    let samples: Vec<u64> = (0..10_000)
        .map(|i| if i % 2 == 0 { 50 } else { 200 })
        .collect();

    bench("latency_stats/from_10000_samples", || {
        LatencyStats::from_samples(&samples).expect("stats")
    });
    bench("latency_stats/otsu_10000_samples", || {
        otsu_threshold(&samples)
    });
}

fn bench_measure_probe_samples() {
    let mock = hal();
    let addresses: Vec<usize> = (0..64).map(|i| 0x1000 + i * 64).collect();

    bench("probe_samples/measure_64_addresses_64_rounds", || {
        measure_probe_samples(mock.as_ref(), &addresses, 64)
    });
}

fn main() {
    bench_eviction_set_builder();
    bench_latency_stats();
    bench_measure_probe_samples();
}
