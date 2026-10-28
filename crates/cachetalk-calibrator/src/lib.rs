mod eviction_set;
mod latency;
mod threshold;

pub use eviction_set::{EvictionOracle, EvictionSetBuilder, SyntheticSetOracle};
pub use latency::{measure_probe_latency, measure_probe_samples, LatencyStats};
pub use threshold::otsu_threshold;

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalibrationError {
    EmptySamples,
    NotEvicting,
    InvalidConfiguration(String),
    EmptyCandidates,
}

impl fmt::Display for CalibrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptySamples => write!(f, "latency sample set is empty"),
            Self::NotEvicting => write!(f, "candidate set does not evict the target"),
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid calibration configuration: {message}")
            }
            Self::EmptyCandidates => write!(f, "candidate address set is empty"),
        }
    }
}

impl std::error::Error for CalibrationError {}

#[cfg(test)]
mod tests {
    use super::*;

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
}
