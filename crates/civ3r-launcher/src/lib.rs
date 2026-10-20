//! Launch a Windows game and native viewer without modifying the asset source.
//!
//! This Unix launcher shares its installation, authentication and child-process
//! lifecycle between macOS and Linux. Display-specific code lives in the viewer.
#![forbid(unsafe_code)]

#[cfg(unix)]
mod startup;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use startup::launch;
