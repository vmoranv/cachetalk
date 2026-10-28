use crate::CalibrationError;
use std::collections::BTreeSet;

pub trait EvictionOracle {
    fn is_eviction_set(&self, candidate: &[usize]) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvictionSetBuilder {
    min_size: usize,
    line_size: usize,
}

impl EvictionSetBuilder {
    pub fn new() -> Self {
        Self {
            min_size: 1,
            line_size: 64,
        }
    }

    pub fn with_min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
    }

    pub fn with_line_size(mut self, line_size: usize) -> Self {
        self.line_size = line_size;
        self
    }

    pub fn min_size(&self) -> usize {
        self.min_size
    }

    pub fn line_size(&self) -> usize {
        self.line_size
    }

    pub fn build<O: EvictionOracle>(
        &self,
        candidates: &[usize],
        oracle: &O,
    ) -> Result<Vec<usize>, CalibrationError> {
        if candidates.is_empty() {
            return Err(CalibrationError::EmptyCandidates);
        }
        if self.min_size == 0 || self.min_size > candidates.len() {
            return Err(CalibrationError::InvalidConfiguration(format!(
                "min_size {} must be between 1 and {}",
                self.min_size,
                candidates.len()
            )));
        }
        if !oracle.is_eviction_set(candidates) {
            return Err(CalibrationError::NotEvicting);
        }

        let mut current = candidates.to_vec();
        let mut granularity = 2usize;

        loop {
            if current.len() <= self.min_size {
                break;
            }

            let chunk_size = current.len().div_ceil(granularity);
            let mut reduced = false;
            let mut start = 0;

            while start < current.len() {
                let end = (start + chunk_size).min(current.len());
                let chunk = &current[start..end];
                let mut complement = Vec::with_capacity(current.len() - chunk.len());
                complement.extend_from_slice(&current[..start]);
                complement.extend_from_slice(&current[end..]);

                if complement.len() >= self.min_size && oracle.is_eviction_set(&complement) {
                    current = complement;
                    granularity = 2.max(granularity.saturating_sub(1));
                    reduced = true;
                    break;
                }

                if chunk.len() >= self.min_size
                    && chunk.len() < current.len()
                    && oracle.is_eviction_set(chunk)
                {
                    current = chunk.to_vec();
                    granularity = 2;
                    reduced = true;
                    break;
                }

                start = end;
            }

            if !reduced {
                if granularity >= current.len() {
                    break;
                }
                granularity = (granularity * 2).min(current.len());
            }
        }

        Ok(current)
    }
}

impl Default for EvictionSetBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntheticSetOracle {
    line_size: usize,
    set_count: usize,
    associativity: usize,
    target_set: usize,
}

impl SyntheticSetOracle {
    pub fn new(
        line_size: usize,
        set_count: usize,
        associativity: usize,
        target_set: usize,
    ) -> Result<Self, CalibrationError> {
        if line_size == 0 {
            return Err(CalibrationError::InvalidConfiguration(
                "line_size must be non-zero".to_string(),
            ));
        }
        if set_count == 0 {
            return Err(CalibrationError::InvalidConfiguration(
                "set_count must be non-zero".to_string(),
            ));
        }
        if associativity == 0 {
            return Err(CalibrationError::InvalidConfiguration(
                "associativity must be non-zero".to_string(),
            ));
        }
        if target_set >= set_count {
            return Err(CalibrationError::InvalidConfiguration(format!(
                "target_set {} must be below set_count {}",
                target_set, set_count
            )));
        }

        Ok(Self {
            line_size,
            set_count,
            associativity,
            target_set,
        })
    }

    fn set_index(&self, address: usize) -> usize {
        (address / self.line_size) % self.set_count
    }
}

impl EvictionOracle for SyntheticSetOracle {
    fn is_eviction_set(&self, candidate: &[usize]) -> bool {
        let mut lines = BTreeSet::new();
        for &address in candidate {
            if self.set_index(address) == self.target_set {
                lines.insert(address / self.line_size);
                if lines.len() >= self.associativity {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oracle() -> SyntheticSetOracle {
        SyntheticSetOracle::new(64, 16, 4, 3).unwrap()
    }

    fn candidates() -> Vec<usize> {
        vec![0, 192, 64, 1216, 128, 2240, 256, 3264, 320, 4288, 384, 5312]
    }

    #[test]
    fn synthetic_oracle_validates_configuration() {
        assert_eq!(
            SyntheticSetOracle::new(0, 16, 4, 3).unwrap_err(),
            CalibrationError::InvalidConfiguration("line_size must be non-zero".to_string())
        );
        assert_eq!(
            SyntheticSetOracle::new(64, 0, 4, 3).unwrap_err(),
            CalibrationError::InvalidConfiguration("set_count must be non-zero".to_string())
        );
        assert_eq!(
            SyntheticSetOracle::new(64, 16, 0, 3).unwrap_err(),
            CalibrationError::InvalidConfiguration("associativity must be non-zero".to_string())
        );
        assert_eq!(
            SyntheticSetOracle::new(64, 16, 4, 16).unwrap_err(),
            CalibrationError::InvalidConfiguration(
                "target_set 16 must be below set_count 16".to_string()
            )
        );
    }

    #[test]
    fn synthetic_oracle_counts_distinct_target_lines() {
        let oracle = oracle();
        assert!(!oracle.is_eviction_set(&[]));
        assert!(!oracle.is_eviction_set(&[0, 64, 128]));
        assert!(!oracle.is_eviction_set(&[192, 192, 192]));
        assert!(oracle.is_eviction_set(&[192, 1216, 2240, 3264]));
        assert!(oracle.is_eviction_set(&candidates()));
    }

    #[test]
    fn builder_defaults_and_overrides() {
        let builder = EvictionSetBuilder::new();
        assert_eq!(builder.min_size(), 1);
        assert_eq!(builder.line_size(), 64);

        let builder = builder.with_min_size(4).with_line_size(128);
        assert_eq!(builder.min_size(), 4);
        assert_eq!(builder.line_size(), 128);
        assert_eq!(EvictionSetBuilder::default(), EvictionSetBuilder::new());
    }

    #[test]
    fn builder_reduces_to_minimal_eviction_set() {
        let oracle = oracle();
        let result = EvictionSetBuilder::new()
            .with_min_size(4)
            .build(&candidates(), &oracle)
            .unwrap();

        assert_eq!(result.len(), 4);
        assert!(result
            .iter()
            .all(|&address| { oracle.set_index(address) == 3 && result.contains(&address) }));
        assert!(oracle.is_eviction_set(&result));
    }

    #[test]
    fn builder_respects_larger_minimum_size() {
        let oracle = oracle();
        let result = EvictionSetBuilder::new()
            .with_min_size(6)
            .build(&candidates(), &oracle)
            .unwrap();

        assert_eq!(result.len(), 6);
        assert!(oracle.is_eviction_set(&result));
        assert!(
            result
                .iter()
                .filter(|&&address| oracle.set_index(address) == 3)
                .count()
                >= 4
        );
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
                .build(&candidates(), &oracle)
                .unwrap_err(),
            CalibrationError::InvalidConfiguration(
                "min_size 0 must be between 1 and 12".to_string()
            )
        );
        assert_eq!(
            EvictionSetBuilder::new()
                .with_min_size(13)
                .build(&candidates(), &oracle)
                .unwrap_err(),
            CalibrationError::InvalidConfiguration(
                "min_size 13 must be between 1 and 12".to_string()
            )
        );
    }

    #[test]
    fn builder_rejects_non_evicting_candidates() {
        let oracle = oracle();
        assert_eq!(
            EvictionSetBuilder::new()
                .build(&[0, 64, 128], &oracle)
                .unwrap_err(),
            CalibrationError::NotEvicting
        );
    }
}
