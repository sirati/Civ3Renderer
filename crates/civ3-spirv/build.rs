//! Compile the application's shader composition outside the generic viewer.
#![forbid(unsafe_code)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    shader_build::build(shader_build::Config {
        shader_crate: std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("../civ3-shaders"),
        source_packages: &["civ3-shaders", "viewer-shaders", "tile-render-shaders", "tile-render-shared", "glass-shaders", "glass-shared", "marker-shaders"],
        precompiled_envs: &["CIV3_PRECOMPILED_SPV"],
        spirv_env: "CIV3_SPV",
        fallback_env: "CIV3_SPV_FALLBACK",
        output_name: "civ3_shaders.spv",
        dual_source_entries: &["text_dual_fs", "subpixel_dual_fs"],
    })
}
