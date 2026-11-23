//! Native lock-free audio engine and shared wire protocol for Civilization III.
#![forbid(unsafe_code)]

pub mod wire;

#[cfg(not(target_arch = "x86"))]
pub mod amb;
#[cfg(not(target_arch = "x86"))]
pub mod decoder;
#[cfg(not(target_arch = "x86"))]
pub mod dsp;
#[cfg(not(target_arch = "x86"))]
pub mod mixer;
#[cfg(not(target_arch = "x86"))]
pub mod output;
#[cfg(not(target_arch = "x86"))]
pub mod path;
