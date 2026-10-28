#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
mod aarch64;
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod apple;
#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
mod fallback;
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
))]
mod x86_64;

use crate::{HalError, MicroArchHal};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn detect() -> Result<Box<dyn MicroArchHal>, HalError> {
    Ok(Box::new(apple::AppleSiliconHal::new()?))
}

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
pub fn detect() -> Result<Box<dyn MicroArchHal>, HalError> {
    Ok(Box::new(aarch64::Aarch64Hal::new()?))
}

#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
))]
pub fn detect() -> Result<Box<dyn MicroArchHal>, HalError> {
    Ok(Box::new(x86_64::X86_64Hal::new()?))
}

#[cfg(not(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "windows", target_arch = "x86_64")
)))]
pub fn detect() -> Result<Box<dyn MicroArchHal>, HalError> {
    Ok(Box::new(fallback::FallbackHal::new()?))
}
