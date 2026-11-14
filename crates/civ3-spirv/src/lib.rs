//! Application-owned shader composition. The generic viewer has no glass dependency.
#![forbid(unsafe_code)]
pub const SPIRV: &[u8] = include_bytes!(env!("CIV3_SPV"));
pub const SPIRV_FALLBACK: &[u8] = include_bytes!(env!("CIV3_SPV_FALLBACK"));
pub const PAINTER: &str = "painted_fs";
