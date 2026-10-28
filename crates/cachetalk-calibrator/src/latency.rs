use crate::CalibrationError;
use cachetalk_hal::MicroArchHal;

#[derive(Clone, Debug, PartialEq)]
pub struct LatencyStats {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub median: f64,
    pub p95: f64,
    pub p99: f64,
    pub stddev: f64,
    pub count: usize,
}

impl LatencyStats {
    pub fn from_samples(samples: &[u64]) -> Result<Self, CalibrationError> {
        if samples.is_empty() {
            return Err(CalibrationError::EmptySamples);
        }

        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        let count = sorted.len();
        let min = sorted[0] as f64;
        let max = sorted[count - 1] as f64;
        let sum: f64 = sorted.iter().map(|&sample| sample as f64).sum();
        let mean = sum / count as f64;
        let median = if count % 2 == 0 {
            let middle = count / 2;
            (sorted[middle - 1] as f64 + sorted[middle] as f64) / 2.0
        } else {
            sorted[count / 2] as f64
        };
        let percentile = |fraction: f64| {
            let index = ((fraction * count as f64).ceil() as usize)
                .saturating_sub(1)
                .min(count - 1);
            sorted[index] as f64
        };
        let variance = sorted
            .iter()
            .map(|&sample| {
                let difference = sample as f64 - mean;
                difference * difference
            })
            .sum::<f64>()
            / count as f64;

        Ok(Self {
            min,
            max,
            mean,
            median,
            p95: percentile(0.95),
            p99: percentile(0.99),
            stddev: variance.sqrt(),
            count,
        })
    }
}

pub fn measure_probe_samples(
    hal: &dyn MicroArchHal,
    addresses: &[usize],
    rounds: usize,
) -> Result<Vec<u64>, CalibrationError> {
    if addresses.is_empty() {
        return Err(CalibrationError::InvalidConfiguration(
            "addresses must not be empty".to_string(),
        ));
    }
    if rounds == 0 {
        return Err(CalibrationError::InvalidConfiguration(
            "rounds must be greater than zero".to_string(),
        ));
    }

    let capacity = addresses.len().saturating_mul(rounds);
    let mut samples = Vec::with_capacity(capacity);
    for _ in 0..rounds {
        for &address in addresses {
            samples.push(hal.probe_address(address));
        }
    }
    Ok(samples)
}

pub fn measure_probe_latency(
    hal: &dyn MicroArchHal,
    addresses: &[usize],
    rounds: usize,
) -> Result<LatencyStats, CalibrationError> {
    let samples = measure_probe_samples(hal, addresses, rounds)?;
    LatencyStats::from_samples(&samples)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cachetalk_hal::{CacheTopology, MockHal};

    #[test]
    fn rejects_empty_samples() {
        assert_eq!(
            LatencyStats::from_samples(&[]).unwrap_err(),
            CalibrationError::EmptySamples
        );
    }

    #[test]
    fn computes_even_sample_statistics() {
        let stats = LatencyStats::from_samples(&[10, 20, 30, 40]).unwrap();
        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.max, 40.0);
        assert_eq!(stats.mean, 25.0);
        assert_eq!(stats.median, 25.0);
        assert_eq!(stats.p95, 40.0);
        assert_eq!(stats.p99, 40.0);
        assert!((stats.stddev - 125.0_f64.sqrt()).abs() < 1e-9);
        assert_eq!(stats.count, 4);
    }

    #[test]
    fn computes_odd_median_and_percentiles() {
        let samples: Vec<u64> = (1..=100).collect();
        let stats = LatencyStats::from_samples(&samples).unwrap();
        assert_eq!(stats.median, 50.5);
        assert_eq!(stats.p95, 95.0);
        assert_eq!(stats.p99, 99.0);

        let odd = LatencyStats::from_samples(&[10, 20, 30]).unwrap();
        assert_eq!(odd.median, 20.0);
    }

    #[test]
    fn measures_samples_in_rounds_over_addresses() {
        let hal = MockHal::new(CacheTopology::unknown());
        let samples = measure_probe_samples(&hal, &[0x1000, 0x2000], 2).unwrap();
        assert_eq!(samples, vec![100, 100, 100, 100]);
        assert_eq!(hal.timer(), 400);

        let stats = measure_probe_latency(&hal, &[0x1000, 0x2000], 1).unwrap();
        assert_eq!(stats.count, 2);
        assert_eq!(stats.mean, 100.0);
    }

    #[test]
    fn rejects_invalid_measurement_requests() {
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
}
