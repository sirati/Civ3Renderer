//! Compile this package's shader module using the shared host build helper.
#![forbid(unsafe_code)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    shader_build::build(shader_build::Config {
        shader_crate: std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR")?).join("../tile-render-test-shaders"),
        source_packages: &["tile-render-test-shaders", "tile-render-shaders", "tile-render-shared"],
        precompiled_envs: &["TILE_RENDER_TEST_PRECOMPILED_SPV"],
        spirv_env: "TILE_RENDER_TEST_SPV",
        fallback_env: "TILE_RENDER_TEST_SPV_FALLBACK",
        output_name: "tile_render_test_shaders.spv",
        dual_source_entries: &["text_dual_fs", "subpixel_dual_fs"],
    })
}
