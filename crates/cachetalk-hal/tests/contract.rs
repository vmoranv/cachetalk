use cachetalk_hal::{detect, CacheTopology, EvictionSet, HalError, MicroArchHal, MockHal};

#[test]
fn cache_topology_defaults_are_unknown() {
    let topology = CacheTopology::unknown();
    assert_eq!(topology.l1d_size, 32 * 1024);
    assert_eq!(topology.cache_line_size, 64);
    assert_eq!(topology.l1_associativity, 8);
    assert_eq!(topology.l2_size, 256 * 1024);
    assert_eq!(topology.l2_associativity, 8);
    assert_eq!(topology.l3_size, 8 * 1024 * 1024);
    assert_eq!(topology.l3_associativity, 16);
    assert_eq!(topology.llc_slices, 1);
    assert_eq!(topology.physical_cores, 1);
    assert_eq!(CacheTopology::default(), CacheTopology::unknown());
}

#[test]
fn eviction_set_reports_length() {
    let set = EvictionSet::new(vec![64, 128, 192], 64);
    assert_eq!(set.len(), 3);
    assert!(!set.is_empty());
    assert_eq!(set.addresses, vec![64, 128, 192]);
    assert_eq!(set.cache_line_size, 64);

    let empty = EvictionSet::new(Vec::new(), 64);
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());
}

#[test]
fn mock_hal_tracks_timer_and_probes() {
    let hal = MockHal::new(CacheTopology::unknown());
    assert_eq!(hal.probe_latency(), 100);
    assert_eq!(hal.timer(), 0);

    assert_eq!(hal.probe_address(0x1000), 100);
    assert_eq!(hal.timer(), 100);
    assert_eq!(hal.probe_address(0x2000), 100);
    assert_eq!(hal.timer(), 200);

    hal.advance(25);
    assert_eq!(hal.timer(), 225);
    assert_eq!(hal.read_timer(), 225);

    hal.set_probe_latency(7);
    assert_eq!(hal.probe_address(0x3000), 7);
    assert_eq!(hal.timer(), 232);

    assert_eq!(hal.name(), "mock");
    assert_eq!(hal.get_cache_topology(), CacheTopology::unknown());
}

#[test]
fn mock_hal_tracks_flush_and_evict_calls() {
    let hal = MockHal::new(CacheTopology::unknown());
    hal.flush_address(0x1000);
    hal.flush_address(0x2000);
    hal.evict_address(0x3000);

    assert_eq!(hal.flush_count(), 2);
    assert_eq!(hal.evict_count(), 1);
}

#[test]
fn mock_hal_binds_cores_within_topology() {
    let hal = MockHal::new(CacheTopology::unknown());
    assert_eq!(hal.bound_core(), None);

    assert_eq!(hal.bind_to_core(0), Ok(()));
    assert_eq!(hal.bound_core(), Some(0));

    let err = hal.bind_to_core(4).unwrap_err();
    assert_eq!(
        err,
        HalError::CoreBindingFailed("core 4 is outside the reported topology".into())
    );
    assert_eq!(hal.bound_core(), Some(0));
}

#[test]
fn hal_error_display_messages() {
    assert_eq!(
        HalError::UnsupportedArch("arm".into()).to_string(),
        "unsupported architecture: arm"
    );
    assert_eq!(
        HalError::CoreBindingFailed("boom".into()).to_string(),
        "core binding failed: boom"
    );
    assert_eq!(
        HalError::TopologyDetectionFailed("oops".into()).to_string(),
        "topology detection failed: oops"
    );
    assert_eq!(
        HalError::InvalidConfiguration("bad".into()).to_string(),
        "invalid configuration: bad"
    );
    assert_eq!(
        HalError::Platform("boom".into()).to_string(),
        "platform error: boom"
    );
}

#[test]
fn detect_returns_a_usable_hal() {
    match detect() {
        Ok(hal) => {
            assert!(!hal.name().is_empty());
            let topology = hal.get_cache_topology();
            assert!(topology.cache_line_size > 0);
            assert!(topology.physical_cores >= 1);
        }
        Err(err) => assert!(!err.to_string().is_empty()),
    }
}
