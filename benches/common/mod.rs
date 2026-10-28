#![allow(dead_code)]

use std::hint::black_box;
use std::time::{Duration, Instant};

const MIN_MEASURE: Duration = Duration::from_millis(100);
const WARMUP_ITERS: u64 = 10;
const MAX_ITERS: u64 = 1 << 28;

fn calibrate<F, R>(mut routine: F) -> u64
where
    F: FnMut() -> R,
{
    for _ in 0..WARMUP_ITERS {
        black_box(routine());
    }
    let mut iters = 1u64;
    loop {
        let start = Instant::now();
        for _ in 0..iters {
            black_box(routine());
        }
        if start.elapsed() >= MIN_MEASURE || iters >= MAX_ITERS {
            return iters;
        }
        iters = iters.saturating_mul(2);
    }
}

fn report(name: &str, iters: u64, elapsed: Duration) {
    let ns_per_iter = elapsed.as_nanos() as f64 / iters as f64;
    println!("{name}: {ns_per_iter:.1} ns/iter ({iters} iters)");
}

pub fn bench<F, R>(name: &str, mut routine: F)
where
    F: FnMut() -> R,
{
    let iters = calibrate(&mut routine);
    let start = Instant::now();
    for _ in 0..iters {
        black_box(routine());
    }
    report(name, iters, start.elapsed());
}

pub fn bench_batched<S, F, R, T>(name: &str, mut setup: F, mut routine: R)
where
    F: FnMut() -> S,
    R: FnMut(S) -> T,
{
    let iters = calibrate(|| {
        let state = setup();
        black_box(routine(state));
    });
    let start = Instant::now();
    for _ in 0..iters {
        let state = setup();
        black_box(routine(state));
    }
    report(name, iters, start.elapsed());
}
