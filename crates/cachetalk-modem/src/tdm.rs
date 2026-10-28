use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TdmConfig {
    slot_count: u32,
    slot_width_ticks: u64,
    guard_ticks: u64,
    slot_period: u64,
    cycle_ticks: u64,
}

impl TdmConfig {
    pub fn new(slot_count: u32, slot_width_ticks: u64, guard_ticks: u64) -> Result<Self, TdmError> {
        if slot_count == 0 {
            return Err(TdmError::InvalidConfiguration(
                "slot_count must be greater than zero".to_string(),
            ));
        }
        if slot_width_ticks == 0 {
            return Err(TdmError::InvalidConfiguration(
                "slot_width_ticks must be greater than zero".to_string(),
            ));
        }
        let slot_period = slot_width_ticks.checked_add(guard_ticks).ok_or_else(|| {
            TdmError::InvalidConfiguration("slot period overflows u64".to_string())
        })?;
        let cycle_ticks = slot_period
            .checked_mul(u64::from(slot_count))
            .ok_or_else(|| {
                TdmError::InvalidConfiguration("cycle duration overflows u64".to_string())
            })?;
        Ok(Self {
            slot_count,
            slot_width_ticks,
            guard_ticks,
            slot_period,
            cycle_ticks,
        })
    }

    pub fn slot_count(&self) -> u32 {
        self.slot_count
    }

    pub fn slot_width_ticks(&self) -> u64 {
        self.slot_width_ticks
    }

    pub fn guard_ticks(&self) -> u64 {
        self.guard_ticks
    }

    pub fn slot_period(&self) -> u64 {
        self.slot_period
    }

    pub fn cycle_ticks(&self) -> u64 {
        self.cycle_ticks
    }

    pub fn slot_at(&self, now: u64) -> u32 {
        ((now % self.cycle_ticks) / self.slot_period) as u32
    }

    pub fn is_active(&self, now: u64) -> bool {
        (now % self.slot_period) < self.slot_width_ticks
    }

    pub fn current_slot_start(&self, now: u64) -> u64 {
        let cycle_start = now - (now % self.cycle_ticks);
        cycle_start + u64::from(self.slot_at(now)) * self.slot_period
    }

    pub fn next_slot_start(&self, now: u64) -> u64 {
        self.current_slot_start(now)
            .saturating_add(self.slot_period)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TdmError {
    InvalidConfiguration(String),
}

impl fmt::Display for TdmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid tdm configuration: {message}")
            }
        }
    }
}

impl std::error::Error for TdmError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_configuration() {
        assert!(TdmConfig::new(0, 10, 1).is_err());
        assert!(TdmConfig::new(1, 0, 1).is_err());
        assert!(TdmConfig::new(2, u64::MAX, 1).is_err());
        assert!(TdmConfig::new(3, u64::MAX / 2, 0).is_err());
        assert!(TdmConfig::new(1, 1, 0).is_ok());
    }

    #[test]
    fn computes_slot_geometry() {
        let config = TdmConfig::new(4, 90, 10).unwrap();
        assert_eq!(config.slot_count(), 4);
        assert_eq!(config.slot_width_ticks(), 90);
        assert_eq!(config.guard_ticks(), 10);
        assert_eq!(config.slot_period(), 100);
        assert_eq!(config.cycle_ticks(), 400);
    }

    #[test]
    fn identifies_slots_and_activity() {
        let config = TdmConfig::new(4, 90, 10).unwrap();
        assert_eq!(config.slot_at(0), 0);
        assert_eq!(config.slot_at(89), 0);
        assert_eq!(config.slot_at(90), 0);
        assert_eq!(config.slot_at(100), 1);
        assert_eq!(config.slot_at(399), 3);
        assert_eq!(config.slot_at(400), 0);

        assert!(config.is_active(0));
        assert!(config.is_active(89));
        assert!(!config.is_active(90));
        assert!(!config.is_active(99));
        assert!(config.is_active(100));
    }

    #[test]
    fn computes_slot_boundaries() {
        let config = TdmConfig::new(4, 90, 10).unwrap();
        assert_eq!(config.current_slot_start(0), 0);
        assert_eq!(config.current_slot_start(150), 100);
        assert_eq!(config.next_slot_start(150), 200);
        assert_eq!(config.current_slot_start(399), 300);
        assert_eq!(config.next_slot_start(399), 400);
        assert_eq!(config.current_slot_start(450), 400);
        assert_eq!(config.next_slot_start(450), 500);
    }

    #[test]
    fn wraps_at_cycle_boundary() {
        let config = TdmConfig::new(2, 10, 0).unwrap();
        assert_eq!(config.cycle_ticks(), 20);
        assert_eq!(config.slot_at(19), 1);
        assert_eq!(config.slot_at(20), 0);
        assert_eq!(config.current_slot_start(20), 20);
        assert_eq!(config.next_slot_start(20), 30);
    }

    #[test]
    fn next_slot_saturates() {
        let config = TdmConfig::new(1, 10, 0).unwrap();
        assert_eq!(config.next_slot_start(u64::MAX - 5), u64::MAX);
    }
}
