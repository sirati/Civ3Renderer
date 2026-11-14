//! Shared host-side shader compilation for independent renderer and effect
//! libraries. Shader entry points remain in the caller's shader crate.
#![forbid(unsafe_code)]

mod build_dual_source;
mod build_support;

use std::path::{Path, PathBuf};
use std::process::Command;

use cargo_gpu_install::install::Install;
use cargo_gpu_install::spirv_builder::{ShaderPanicStrategy, SpirvMetadata};
use cargo_gpu_install::spirv_source::{CrateMetadata, SpirvSource};

/// Build-script inputs. The first set precompiled variable wins. Extra
/// variables can preserve an existing application's build override.
pub struct Config<'a> {
    pub shader_crate: PathBuf,
    pub source_packages: &'a [&'a str],
    pub precompiled_envs: &'a [&'a str],
    pub spirv_env: &'a str,
    pub fallback_env: &'a str,
    pub output_name: &'a str,
    pub dual_source_entries: &'a [&'a str],
}

/// Compile or import a shader module from a Cargo build script. The same
/// build-script executable handles the isolated compiler child invocation.
pub fn build(config: Config<'_>) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(mode) = std::env::args_os().nth(1) {
        if mode == "--compile-source-shader" {
            let shader_crate = std::env::args_os().nth(2).ok_or("missing shader crate")?;
            let shader_target = std::env::args_os().nth(3).ok_or("missing shader target")?;
            return compile_source_shader(shader_crate.into(), shader_target.into(), &config);
        }
        return Err("unknown shader build helper mode".into());
    }
    for name in config.precompiled_envs {
        println!("cargo::rerun-if-env-changed={name}");
    }
    for name in config.precompiled_envs {
        if let Some(path) = std::env::var_os(name) {
            let path = PathBuf::from(path).canonicalize()?;
            println!("cargo::rerun-if-changed={}", path.display());
            return emit_shader(&path, false, &config);
        }
    }
    let shader_crate = config.shader_crate.canonicalize()?;
    let metadata = CrateMetadata::query(shader_crate.clone())?;
    // Cargo resolves path and Git dependencies to their real manifests.
    // Their source trees need not be siblings of this build crate.
    for directory in source_directories(&metadata, config.source_packages)? {
        println!("cargo::rerun-if-changed={}", directory.display());
    }
    let source = SpirvSource::new(&metadata, None, None)?;
    let backend_target = source.install_dir()?.join("target");
    let shader_target = metadata.target_directory.as_std_path().join("spirv-builder");
    run_source_child(&shader_crate, &backend_target, &shader_target)
}

fn source_directories(metadata: &CrateMetadata, packages: &[&str]) -> Result<Vec<PathBuf>, Box<dyn std::error::Error>> {
    packages.iter().map(|name| {
        let manifest = metadata.find_package(name)?.manifest_path.as_std_path();
        Ok(manifest.parent().ok_or("shader manifest has no parent")?.canonicalize()?)
    }).collect()
}

fn run_source_child(shader_crate: &Path, backend_target: &Path, shader_target: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut child = Command::new(std::env::current_exe()?);
    child.arg("--compile-source-shader").arg(shader_crate).arg(shader_target);
    build_support::isolate_codegen_target(&mut child, backend_target);
    let status = child.status()?;
    if !status.success() {
        return Err(format!(
            "source shader build failed ({status}); codegen cache {}, shader target {}",
            backend_target.display(), shader_target.display()
        ).into());
    }
    Ok(())
}

fn compile_source_shader(shader_crate: PathBuf, shader_target: PathBuf, config: &Config<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let backend = Install::from_shader_crate(shader_crate.clone()).within_build_script().run()?;
    let mut builder = backend.to_spirv_builder(shader_crate, "spirv-unknown-vulkan1.2").target_dir_path(shader_target);
    builder.build_script.defaults = true;
    builder.spirv_metadata = SpirvMetadata::NameVariables;
    builder.shader_panic_strategy = ShaderPanicStrategy::SilentExit;
    let result = builder.build()?;
    emit_shader(&result.module.unwrap_single(), true, config)
}

fn emit_shader(path: &Path, compiler_output: bool, config: &Config<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let mut dual = bytes.clone();
    let mut fallback = bytes;
    for entry in config.dual_source_entries {
        dual = build_dual_source::decorate(&dual, entry)?;
        fallback = build_dual_source::fallback(&fallback, entry)?;
    }
    let output = if compiler_output { path.to_owned() } else {
        PathBuf::from(std::env::var_os("OUT_DIR").ok_or("missing OUT_DIR")?).join(config.output_name)
    };
    let fallback_path = output.with_extension("fallback.spv");
    std::fs::write(&output, dual)?;
    std::fs::write(&fallback_path, fallback)?;
    println!("cargo::rustc-env={}={}", config.spirv_env, output.display());
    println!("cargo::rustc-env={}={}", config.fallback_env, fallback_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shader_dependencies_are_watched_at_their_resolved_manifest_paths() {
        let root = std::env::temp_dir().join(format!("shader-build-sources-{}", std::process::id()));
        let shader = root.join("first-repository/shaders");
        let shared = root.join("another-repository/shared");
        for directory in [&shader, &shared] { std::fs::create_dir_all(directory.join("src")).unwrap(); }
        std::fs::write(shader.join("Cargo.toml"), "[package]\nname='shader-source-fixture'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[dependencies]\nshader-types-fixture={path='../../another-repository/shared'}\n").unwrap();
        std::fs::write(shared.join("Cargo.toml"), "[package]\nname='shader-types-fixture'\nversion='0.0.0'\nedition='2024'\n").unwrap();
        std::fs::write(shader.join("src/lib.rs"), "pub use shader_types_fixture::*;\n").unwrap();
        std::fs::write(shared.join("src/lib.rs"), "pub struct Types;\n").unwrap();
        let metadata = CrateMetadata::query(shader.clone()).unwrap();
        let directories = source_directories(&metadata, &["shader-source-fixture", "shader-types-fixture"]).unwrap();
        assert_eq!(directories, [shader.canonicalize().unwrap(), shared.canonicalize().unwrap()]);
        assert!(source_directories(&metadata, &["missing-shader-package"]).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
