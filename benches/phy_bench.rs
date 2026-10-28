mod common;

use std::sync::Arc;

use common::{bench, bench_batched};

use cachetalk::hal::{CacheTopology, EvictionSet, MicroArchHal, MockHal};
use cachetalk::phy::{CacheOccupancy, DramBankProbe, PortContention, PrimeProbe};

fn hal() -> Arc<dyn MicroArchHal> {
    Arc::new(MockHal::new(CacheTopology::unknown()))
}

fn bench_prime_probe() {
    let addresses: Vec<usize> = (0..64).map(|i| 0x1000 + i * 64).collect();
    let probe = PrimeProbe::new(hal(), EvictionSet::new(addresses, 64)).expect("prime probe");

    bench("prime_probe/prime", || probe.prime());
    bench("prime_probe/probe", || probe.probe());
    bench("prime_probe/probe_slot", || probe.probe_slot());
    bench("prime_probe/measure_64_rounds", || probe.measure(64));
}

fn bench_cache_occupancy() {
    bench_batched(
        "cache_occupancy/prime_64_lines",
        || CacheOccupancy::new(hal(), 64, 64, 90).expect("occupancy"),
        |mut occupancy| {
            occupancy.prime();
            occupancy.line_count()
        },
    );
    bench_batched(
        "cache_occupancy/evict_64_lines",
        || {
            let mut occupancy = CacheOccupancy::new(hal(), 64, 64, 90).expect("occupancy");
            occupancy.prime();
            occupancy
        },
        |mut occupancy| {
            occupancy.evict();
            occupancy.line_count()
        },
    );
    bench_batched(
        "cache_occupancy/probe_64_lines",
        || {
            let mut occupancy = CacheOccupancy::new(hal(), 64, 64, 90).expect("occupancy");
            occupancy.prime();
            occupancy
        },
        |mut occupancy| occupancy.probe().len(),
    );
    bench_batched(
        "cache_occupancy/probe_slot_64_lines",
        || {
            let mut occupancy = CacheOccupancy::new(hal(), 64, 64, 90).expect("occupancy");
            occupancy.prime();
            occupancy
        },
        |mut occupancy| occupancy.probe_slot().len(),
    );
}

fn bench_port_contention() {
    let contention = PortContention::new(hal(), 0x1000);

    bench("port_contention/contend_1024", || contention.contend(1024));
    bench("port_contention/measure_64x1024", || {
        contention.measure(64, 1024)
    });
}

fn bench_dram_bank() {
    let addresses: Vec<usize> = (0..32).map(|i| 0x1000 + i * 0x1000).collect();
    let probe = DramBankProbe::new(hal(), addresses).expect("bank probe");

    bench("dram_bank/scan_32_addresses", || probe.scan());
}

fn main() {
    bench_prime_probe();
    bench_cache_occupancy();
    bench_port_contention();
    bench_dram_bank();
}
