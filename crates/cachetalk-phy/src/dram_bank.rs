use crate::PhyError;
use cachetalk_hal::MicroArchHal;
use std::fmt;
use std::sync::Arc;

pub struct DramBankProbe {
    hal: Arc<dyn MicroArchHal>,
    addresses: Vec<usize>,
}

impl fmt::Debug for DramBankProbe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DramBankProbe")
            .field("hal", &self.hal.name())
            .field("addresses", &self.addresses)
            .finish()
    }
}

impl DramBankProbe {
    pub fn new(hal: Arc<dyn MicroArchHal>, addresses: Vec<usize>) -> Result<Self, PhyError> {
        if addresses.is_empty() {
            return Err(PhyError::EmptyEvictionSet);
        }
        Ok(Self { hal, addresses })
    }

    pub fn probe_bank(&self, index: usize) -> Result<u64, PhyError> {
        let address = *self
            .addresses
            .get(index)
            .ok_or(PhyError::IndexOutOfBounds(index))?;
        Ok(self.hal.probe_address(address))
    }

    pub fn scan(&self) -> Vec<u64> {
        self.addresses
            .iter()
            .map(|&address| self.hal.probe_address(address))
            .collect()
    }

    pub fn addresses(&self) -> &[usize] {
        &self.addresses
    }

    pub fn hal(&self) -> &Arc<dyn MicroArchHal> {
        &self.hal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cachetalk_hal::{CacheTopology, MockHal};

    fn probe() -> DramBankProbe {
        let hal: Arc<dyn MicroArchHal> =
            Arc::new(MockHal::with_probe_latency(CacheTopology::unknown(), 11));
        DramBankProbe::new(hal, vec![0x1000, 0x2000, 0x3000]).unwrap()
    }

    #[test]
    fn rejects_empty_addresses() {
        let hal: Arc<dyn MicroArchHal> = Arc::new(MockHal::new(CacheTopology::unknown()));
        assert_eq!(
            DramBankProbe::new(hal, Vec::new()).unwrap_err(),
            PhyError::EmptyEvictionSet
        );
    }

    #[test]
    fn probes_individual_banks() {
        let probe = probe();
        assert_eq!(probe.probe_bank(1), Ok(11));
        assert_eq!(
            probe.probe_bank(3).unwrap_err(),
            PhyError::IndexOutOfBounds(3)
        );
        assert_eq!(probe.addresses(), &[0x1000, 0x2000, 0x3000]);
    }

    #[test]
    fn scans_all_addresses() {
        let probe = probe();
        assert_eq!(probe.scan(), vec![11, 11, 11]);
        assert_eq!(probe.hal().name(), "mock");
    }
}
