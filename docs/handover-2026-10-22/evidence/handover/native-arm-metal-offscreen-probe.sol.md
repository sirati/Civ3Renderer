A native Apple ARM64 Metal diagnostic is compiled and ready. It has not been executed and no Neo connection was made. Root must decide when the VM checks and idle installation allow a bounded run. The prepared device artifact is a Rust Mach-O executable; all Python scripts are Papa development tooling and are outside the production package.

Executable: /mnt/datasets/c3r-build/metal-offscreen-sol/c3r-metal-offscreen-probe
SHA256: bf7f77b520e3b9c0fe7b1ea13d0d4bb56f4cc76c6282c9f50cf349dea6881b56
Size: 9,405,056 bytes.
Frozen Rust source: /mnt/datasets/c3r-build/metal-offscreen-sol/probe-f0e8f61c9592f753517dd4d457f4be27184621f2a36267fff761cb5c3c4d07a2.rs
Source SHA256: f0e8f61c9592f753517dd4d457f4be27184621f2a36267fff761cb5c3c4d07a2.

The probe renders four tiny GPU images, one 96 by 64 and three 16 by 16. It checks actual set_markers text when the scene contains fonts but no scene text. Its synthetic rectangular glyph crosses the 27*3/2 text clip boundary; 108 interior pixels and the two boundary columns have independent known white/blue results. A patterned background then checks five known bilinear colours and fixed enlarged dither at two clock values. It asserts native Apple ARM64, the Metal backend and a hardware adapter. No game assets, Wine, window or CPU reference rasterizer are used.

A separate watchdog exits the process with 124 after 9 seconds, independent of GPU polling. Root must also enforce an external 10-second process deadline when executing. A run can prove only these shader/backend pixels. It does not accept the game release, Wine integration, input handling, GUI behavior or application frame rate.

The direct-rustc compile reused the release owner's explicitly released read-only b41 ARM target. The exact tile_render, tile_render_shared and wgpu rlibs were approved by hash. build.json records their hashes, all resolved transitive library/fingerprint hashes, compiler/linker/SDK identity and the exact command. The initial setup omitted the host proc-macro dependency search directory; its compiler failure and build manifest are preserved separately as attempt1. The corrected command compiled successfully without changing the Rust source or any shared target. All retained dependencies were rehashed unchanged after linking. CPU lane and target hold were explicitly released.

Independent binary inspection found both final shader arrays inside the executable:

- Full SPIR-V 43961a92338f6b5b1f5ebd12dfdc806057fe43d8bfeb50abdb7872d274b601c4, 485,164 bytes at offset 4,914,746.
- Fallback SPIR-V bbde10b5e91a3e54c337eab67c2e5acce8abf384657043eb46bbe056ea799281, 484,900 bytes at offset 4,429,846.

macho-structure.json independently verifies a native ARM64 Mach-O executable, macOS minimum 14.0, only system dylibs and no rpaths. Its ad-hoc signature is present and all 2,279 SHA256 code pages verify. These are static preparation checks, not a Metal runtime result.

Evidence is in /mnt/datasets/c3r-build/metal-offscreen-sol/:

- build.json and build-driver.log.
- logs/compile.log, with the initial setup failure retained as logs/compile-attempt1.log and build-attempt1.json.
- macho-structure.json.
- completion.json.
- The frozen Rust source and executable above.

The b41 archive identifies shader and dependency provenance. Its application scene behavior was superseded by another owner's correctness work. This probe makes no application runtime claim from b41. No physical Mac UI, app relaunch or performance test occurred.
