use std::sync::Arc;

use cachetalk_hal::{CacheTopology, EvictionSet, MicroArchHal, MockHal};
use cachetalk_phy::PrimeProbe;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let topology = CacheTopology::unknown();
    let hal: Arc<dyn MicroArchHal> = Arc::new(MockHal::new(topology));
    let addresses = (0..topology.l1_associativity)
        .map(|slot| 0x1000_0000 + slot * topology.cache_line_size)
        .collect();
    let eviction_set = EvictionSet::new(addresses, topology.cache_line_size);
    let probe = PrimeProbe::new(Arc::clone(&hal), eviction_set)?;

    println!("HAL: {}", probe.hal().name());
    println!("Eviction set entries: {}", probe.eviction_set().len());
    println!(
        "Cache line size: {} bytes",
        probe.eviction_set().cache_line_size
    );
    println!("Cold probe latency: {} cycles", probe.probe());
    println!("Prime latency: {} cycles", probe.prime());

    let samples = probe.measure(16);
    let total: u64 = samples.iter().sum();
    let average = total as f64 / samples.len() as f64;
    println!("Prime+probe samples: {samples:?}");
    println!("Average slot latency: {average:.2} cycles");

    Ok(())
}
