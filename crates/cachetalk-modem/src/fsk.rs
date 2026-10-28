use std::f64::consts::{PI, TAU};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FskConfig {
    pub sample_rate: u32,
    pub mark_freq: u32,
    pub space_freq: u32,
    pub samples_per_bit: u32,
}

impl Default for FskConfig {
    fn default() -> Self {
        Self {
            sample_rate: 1000,
            mark_freq: 100,
            space_freq: 50,
            samples_per_bit: 100,
        }
    }
}

impl FskConfig {
    fn validate(&self) -> Result<(), FskError> {
        if self.sample_rate == 0 {
            return Err(FskError::InvalidConfiguration(
                "sample rate must be non-zero".to_string(),
            ));
        }
        if self.samples_per_bit == 0 {
            return Err(FskError::InvalidConfiguration(
                "samples per bit must be non-zero".to_string(),
            ));
        }
        if self.mark_freq == 0 || self.space_freq == 0 {
            return Err(FskError::InvalidConfiguration(
                "mark and space frequencies must be non-zero".to_string(),
            ));
        }
        if self.mark_freq == self.space_freq {
            return Err(FskError::InvalidConfiguration(
                "mark and space frequencies must differ".to_string(),
            ));
        }
        let nyquist = self.sample_rate as u64;
        if (self.mark_freq as u64) * 2 >= nyquist {
            return Err(FskError::InvalidConfiguration(format!(
                "mark frequency {} is not below Nyquist limit",
                self.mark_freq
            )));
        }
        if (self.space_freq as u64) * 2 >= nyquist {
            return Err(FskError::InvalidConfiguration(format!(
                "space frequency {} is not below Nyquist limit",
                self.space_freq
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FskError {
    InvalidConfiguration(String),
    Incomplete,
    InvalidBit(u8),
}

impl fmt::Display for FskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(message) => {
                write!(f, "invalid FSK configuration: {message}")
            }
            Self::Incomplete => write!(f, "incomplete FSK sample stream"),
            Self::InvalidBit(bit) => write!(f, "invalid bit value: {bit}"),
        }
    }
}

impl std::error::Error for FskError {}

pub fn modulate(bits: &[u8], config: FskConfig) -> Result<Vec<f32>, FskError> {
    config.validate()?;
    for &bit in bits {
        if bit > 1 {
            return Err(FskError::InvalidBit(bit));
        }
    }
    let sample_count = bits
        .len()
        .checked_mul(config.samples_per_bit as usize)
        .ok_or_else(|| FskError::InvalidConfiguration("sample count overflow".to_string()))?;
    let mut samples = Vec::with_capacity(sample_count);
    for &bit in bits {
        let frequency = if bit == 1 {
            config.mark_freq
        } else {
            config.space_freq
        };
        let step = TAU * frequency as f64 / config.sample_rate as f64;
        let mut phase = 0.0;
        for _ in 0..config.samples_per_bit {
            samples.push(if phase < PI { 1.0 } else { -1.0 });
            phase += step;
            if phase >= TAU {
                phase -= TAU;
            }
        }
    }
    Ok(samples)
}

pub fn demodulate(
    samples: &[f32],
    bit_count: usize,
    config: FskConfig,
) -> Result<Vec<u8>, FskError> {
    config.validate()?;
    let needed = bit_count
        .checked_mul(config.samples_per_bit as usize)
        .ok_or_else(|| FskError::InvalidConfiguration("sample count overflow".to_string()))?;
    if samples.len() < needed {
        return Err(FskError::Incomplete);
    }
    let mut bits = Vec::with_capacity(bit_count);
    let width = config.samples_per_bit as usize;
    for index in 0..bit_count {
        let window = &samples[index * width..(index + 1) * width];
        let mark = goertzel_power(window, config.sample_rate, config.mark_freq);
        let space = goertzel_power(window, config.sample_rate, config.space_freq);
        bits.push(if mark >= space { 1 } else { 0 });
    }
    Ok(bits)
}

pub fn goertzel_power(samples: &[f32], sample_rate: u32, frequency: u32) -> f64 {
    if samples.is_empty() || sample_rate == 0 || frequency == 0 {
        return 0.0;
    }
    if (frequency as u64) * 2 >= sample_rate as u64 {
        return 0.0;
    }
    let count = samples.len() as f64;
    let bin = (count * frequency as f64 / sample_rate as f64).round();
    if bin <= 0.0 || bin > count / 2.0 {
        return 0.0;
    }
    let omega = TAU * bin / count;
    let coefficient = 2.0 * omega.cos();
    let mut previous = 0.0;
    let mut previous_previous = 0.0;
    for &sample in samples {
        let current = coefficient * previous - previous_previous + sample as f64;
        previous_previous = previous;
        previous = current;
    }
    let power = previous * previous + previous_previous * previous_previous
        - coefficient * previous * previous_previous;
    power.max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_bits() {
        let config = FskConfig::default();
        let bits = [1, 0, 1, 1, 0, 0, 1, 0];
        let samples = modulate(&bits, config).unwrap();
        assert_eq!(samples.len(), bits.len() * config.samples_per_bit as usize);
        assert_eq!(demodulate(&samples, bits.len(), config).unwrap(), bits);
    }

    #[test]
    fn round_trips_every_byte() {
        let config = FskConfig::default();
        let data: Vec<u8> = (0..=255).collect();
        let bits = crate::bits::bytes_to_bits(&data);
        let samples = modulate(&bits, config).unwrap();
        let decoded = demodulate(&samples, bits.len(), config).unwrap();
        assert_eq!(crate::bits::bits_to_bytes(&decoded).unwrap(), data);
    }

    #[test]
    fn detects_mark_and_space_energy() {
        let config = FskConfig::default();
        let mark = modulate(&[1], config).unwrap();
        let space = modulate(&[0], config).unwrap();
        let mark_power = goertzel_power(&mark, config.sample_rate, config.mark_freq);
        let space_power = goertzel_power(&space, config.sample_rate, config.space_freq);
        assert!(mark_power > 0.0);
        assert!(space_power > 0.0);
        assert!(
            goertzel_power(&mark, config.sample_rate, config.mark_freq)
                > goertzel_power(&mark, config.sample_rate, config.space_freq)
        );
        assert!(
            goertzel_power(&space, config.sample_rate, config.space_freq)
                > goertzel_power(&space, config.sample_rate, config.mark_freq)
        );
    }

    #[test]
    fn rejects_invalid_bits_and_short_streams() {
        let config = FskConfig::default();
        assert_eq!(modulate(&[2], config), Err(FskError::InvalidBit(2)));
        let samples = modulate(&[1, 0], config).unwrap();
        assert_eq!(
            demodulate(&samples[..samples.len() - 1], 2, config),
            Err(FskError::Incomplete)
        );
    }

    #[test]
    fn ignores_extra_samples() {
        let config = FskConfig::default();
        let mut samples = modulate(&[1, 0, 1], config).unwrap();
        samples.extend_from_slice(&[0.0; 17]);
        assert_eq!(demodulate(&samples, 3, config).unwrap(), [1, 0, 1]);
    }

    #[test]
    fn rejects_invalid_configuration() {
        let base = FskConfig::default();
        let invalid = [
            FskConfig {
                sample_rate: 0,
                ..base
            },
            FskConfig {
                samples_per_bit: 0,
                ..base
            },
            FskConfig {
                mark_freq: 0,
                ..base
            },
            FskConfig {
                space_freq: 0,
                ..base
            },
            FskConfig {
                mark_freq: base.space_freq,
                ..base
            },
            FskConfig {
                mark_freq: base.sample_rate / 2,
                ..base
            },
            FskConfig {
                space_freq: base.sample_rate / 2,
                ..base
            },
        ];
        for config in invalid {
            assert!(matches!(
                modulate(&[0], config),
                Err(FskError::InvalidConfiguration(_))
            ));
        }
    }

    #[test]
    fn goertzel_rejects_invalid_inputs() {
        assert_eq!(goertzel_power(&[], 1000, 100), 0.0);
        assert_eq!(goertzel_power(&[1.0], 0, 100), 0.0);
        assert_eq!(goertzel_power(&[1.0], 1000, 0), 0.0);
        assert_eq!(goertzel_power(&[1.0], 1000, 500), 0.0);
    }
}
