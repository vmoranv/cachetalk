use cachetalk_calibrator::{
    measure_probe_latency, measure_probe_samples, otsu_threshold, CalibrationError, EvictionOracle,
    EvictionSetBuilder, LatencyStats, SyntheticSetOracle,
};
use cachetalk_hal::{CacheTopology, MockHal};

fn oracle() -> SyntheticSetOracle {
    SyntheticSetOracle::new(64, 16, 4, 3).expect("oracle configuration")
}

fn candidates() -> Vec<usize> {
    vec![192, 1216, 2240, 3264, 0, 64, 128, 4096]
}

#[test]
fn synthetic_oracle_identifies_eviction_sets() {
    let oracle = oracle();
    assert!(!oracle.is_eviction_set(&[]));
    assert!(!oracle.is_eviction_set(&[0, 64, 128]));
    assert!(!oracle.is_eviction_set(&[192, 192, 192]));
    assert!(!oracle.is_eviction_set(&[192, 1216, 2240]));
    assert!(oracle.is_eviction_set(&[192, 1216, 2240, 3264]));
    assert!(oracle.is_eviction_set(&candidates()));
}

#[test]
fn builder_reduces_candidates_to_minimal_set() {
    let oracle = oracle();
    let reduced = EvictionSetBuilder::new()
        .with_min_size(4)
        .build(&candidates(), &oracle)
        .expect("reduction");

    assert_eq!(reduced, vec![192, 1216, 2240, 3264]);
    assert!(oracle.is_eviction_set(&reduced));
}

#[test]
fn builder_exposes_defaults_and_overrides() {
    let builder = EvictionSetBuilder::new();
    assert_eq!(builder.min_size(), 1);
    assert_eq!(builder.line_size(), 64);

    let builder = builder.with_min_size(4).with_line_size(128);
    assert_eq!(builder.min_size(), 4);
    assert_eq!(builder.line_size(), 128);
    assert_eq!(EvictionSetBuilder::default(), EvictionSetBuilder::new());
}

#[test]
fn builder_rejects_invalid_requests() {
    let oracle = oracle();
    assert_eq!(
        EvictionSetBuilder::new().build(&[], &oracle).unwrap_err(),
        CalibrationError::EmptyCandidates
    );
    assert_eq!(
        EvictionSetBuilder::new()
            .with_min_size(0)
            .build(&[192, 1216, 2240, 3264, 0, 64, 128], &oracle)
            .unwrap_err(),
        CalibrationError::InvalidConfiguration("min_size 0 must be between 1 and 7".to_string())
    );
    assert_eq!(
        EvictionSetBuilder::new()
            .with_min_size(9)
            .build(&candidates(), &oracle)
            .unwrap_err(),
        CalibrationError::InvalidConfiguration("min_size 9 must be between 1 and 8".to_string())
    );
    assert_eq!(
        EvictionSetBuilder::new()
            .build(&[0, 64, 128, 192], &oracle)
            .unwrap_err(),
        CalibrationError::NotEvicting
    );
}

#[test]
fn latency_stats_describe_samples() {
    let stats = LatencyStats::from_samples(&[10, 20, 30, 40]).expect("statistics");
    assert_eq!(stats.min, 10.0);
    assert_eq!(stats.max, 40.0);
    assert_eq!(stats.mean, 25.0);
    assert_eq!(stats.median, 25.0);
    assert_eq!(stats.p95, 40.0);
    assert_eq!(stats.p99, 40.0);
    assert_eq!(stats.count, 4);
    assert!((stats.stddev - 125.0_f64.sqrt()).abs() < 1e-9);

    let odd = LatencyStats::from_samples(&[10, 20, 30]).expect("statistics");
    assert_eq!(odd.median, 20.0);

    assert_eq!(
        LatencyStats::from_samples(&[]).unwrap_err(),
        CalibrationError::EmptySamples
    );
}

#[test]
fn probe_measurements_use_hal_latency() {
    let hal = MockHal::new(CacheTopology::unknown());
    let samples = measure_probe_samples(&hal, &[0x1000, 0x2000], 2).expect("samples");
    assert_eq!(samples, vec![100, 100, 100, 100]);
    assert_eq!(hal.timer(), 400);

    let stats = measure_probe_latency(&hal, &[0x1000, 0x2000], 1).expect("statistics");
    assert_eq!(stats.count, 2);
    assert_eq!(stats.mean, 100.0);
}

#[test]
fn probe_measurements_reject_invalid_requests() {
    let hal = MockHal::new(CacheTopology::unknown());
    assert_eq!(
        measure_probe_samples(&hal, &[], 1).unwrap_err(),
        CalibrationError::InvalidConfiguration("addresses must not be empty".to_string())
    );
    assert_eq!(
        measure_probe_samples(&hal, &[0x1000], 0).unwrap_err(),
        CalibrationError::InvalidConfiguration("rounds must be greater than zero".to_string())
    );
}

#[test]
fn otsu_threshold_splits_latency_clusters() {
    assert_eq!(otsu_threshold(&[]), None);
    assert_eq!(otsu_threshold(&[5, 5, 5]), None);
    assert_eq!(
        otsu_threshold(&[10, 10, 10, 10, 100, 100, 100, 100]),
        Some(10)
    );
    assert_eq!(otsu_threshold(&[5, 5, 5, 100]), Some(5));
    assert_eq!(otsu_threshold(&[1, 1, 2, 2, 10, 10]), Some(2));
}

#[test]
fn calibration_errors_render_actionable_messages() {
    assert_eq!(
        CalibrationError::EmptySamples.to_string(),
        "latency sample set is empty"
    );
    assert_eq!(
        CalibrationError::NotEvicting.to_string(),
        "candidate set does not evict the target"
    );
    assert_eq!(
        CalibrationError::InvalidConfiguration("bad".to_string()).to_string(),
        "invalid calibration configuration: bad"
    );
    assert_eq!(
        CalibrationError::EmptyCandidates.to_string(),
        "candidate address set is empty"
    );
}
