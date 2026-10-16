//! Compiles `tile-render-shaders` to SPIR-V with rust-gpu.
//!
//! `cargo-gpu-install` fetches the exact nightly toolchain rust-gpu needs and
//! builds its codegen backend once (cached per user), so this crate itself
//! builds on stable and for any target: the SPIR-V is produced on the host
//! and embedded, which is what makes cross-compiling the renderer to Windows
//! work without a Windows shader toolchain.
use std::path::PathBuf;

use cargo_gpu_install::install::Install;
use cargo_gpu_install::spirv_builder::{ShaderPanicStrategy, SpirvMetadata};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?);
    let shader_crate = manifest_dir.join("../tile-render-shaders").canonicalize()?;
    let shared_crate = manifest_dir.join("../tile-render-shared").canonicalize()?;
    println!("cargo::rerun-if-changed={}", shader_crate.display());
    println!("cargo::rerun-if-changed={}", shared_crate.display());

    let backend = Install::from_shader_crate(shader_crate.clone()).within_build_script().run()?;
    let mut builder = backend.to_spirv_builder(shader_crate, "spirv-unknown-vulkan1.2");
    builder.build_script.defaults = true;
    builder.spirv_metadata = SpirvMetadata::NameVariables;
    builder.shader_panic_strategy = ShaderPanicStrategy::SilentExit;
    let result = builder.build()?;
    let spv = result.module.unwrap_single();
    println!("cargo::rustc-env=TILE_RENDER_SPV={}", spv.display());
    Ok(())
}
