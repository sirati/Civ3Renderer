Scaled UI backgrounds now use the selected sampling mode when enlarged and reduced. Nearest and native 1:1 samples stay unchanged. Dither uses a deterministic source-coordinate pattern. Smooth uses the existing area filter when shrinking and four-tap bilinear RGB reconstruction when magnifying. Both retain the nominal alpha mask, clamp samples to the source crop and exclude transparent neighbours. Only explicitly tagged screen-space background art uses this path. Controls, borders, world art and separate outline text retain their previous rendering.

The implementation is in the shared math, Rust shader, camera documentation and numerical test oracle. Settings now say "Scaled menu backgrounds". The wire values Dither/Interpolate/Nearest are unchanged. No production CPU rasterization, new locks or unsafe code was added. Four explicit taps preserve the existing workaround for SPIR-V-to-MSL loop translation. Settings preview/Accept/Cancel remains owned by text_fit_atomic_ui.

The fresh combined shader pair also contains startup_progress_sol's host animation clock. The full and fallback SPIR-V modules pass strict spirv-val for Vulkan1.2 and all five dual-source build helper tests. A safe host program proves FrameParams is 192 bytes, with host_tick_ms at 176 and scalar pads at 180/184/188. Reflection independently requires 41 members with matching offsets in both modules. An old 176-byte pair fails the same interface gate. The existing 256-byte control stride is sufficient.

Validation passed:

- All 19 shared CPU tests, including independent floating-point neighbour/phase/crop calculations and weighted deterministic dither checks.
- All 4 menu background tests. This includes complementary source masks, exact input geometry and background-only colour changes at native, reduced and enlarged scales.
- All 10 GPU sampling tests on a dual-source device and the same 10 on an explicitly feature-disabled fallback device. Each test asserts AMD vendor 1002, Vulkan and RADV. Logs show integrated device 0000:72:00.0 and Mesa 26.2.4. WGPU validation was enabled.
- The original 866 renderer fails both new targeted magnification assertions as expected. These are preserved failures, not candidate acceptance failures.
- The GPU tests check known 2x bilinear RGB and RGB555 values, fractional scaling, indexed palettes, opacity, crop boundaries, static phase, unchanged controls and a real outlined G over an opaque UI plate.
- The full native mixed-art GPU test passed its 60-case matrix over five real PCX sources, two opacity values, Dither/Interpolate and 1/2,2/3,3/2 scales. Baked controls and separate native controls/text remain exact. Illustrated title art remains unchanged. The tiny-mask test also passed its 126-case matrix, including one-pixel strips, trimming, clip phase, hairlines and enlarged views. These two native tests used the default dual-source device.
- Both native capture/preload role-policy CPU tests passed.

The native test required a declared source correction. A first full workspace rebuild caught eight Linux type errors in the then-in-progress combat_speed.rs. That failed driver and compiler log remain in coherent/logs. Its owner corrected the file to SHA256 fc904f01ef1a859872973ce910bb13cabc25ff2b52fe0277ccd09d147521e4d2. A separate coherent-native source snapshot records that single overlay. All 24 workspace dependencies were then rebuilt successfully; no old workspace dependency was silently reused for the native tests. The original shader source manifest remains unchanged.

Artifacts and provenance:

- Working directory: /mnt/datasets/c3r-build/background-upscale-sol/
- Original 667-file snapshot: coherent/source/SOURCE-MANIFEST.json, SHA256 e90c274c286d21ca89b3b79416a0ca20f41d1c612e291939f200c2dccba26558.
- Pair binding and all strict/interface commands: coherent/binding.json.
- Full SPIR-V: coherent/artifacts/shader.spv, SHA256 cb7214b061ef41f2dff7c4a02d8b988c5a29ecc3b09594d27066efde14cad74d.
- Fallback SPIR-V: coherent/artifacts/shader-fallback.spv, SHA256 3d07b207da6054d9647352d4a1500c1ec4e435a807c05778120c1930e8a38adb.
- Core candidate test executable: coherent/artifacts/sampling-candidate, SHA256 bb40fb0312cfcf7ea4e847bc528b2e6c9e8662e2dd31b3af61a262775ae6e49a.
- Original 866 baseline executable: coherent/artifacts/sampling-before, SHA256 6f94d0ebddf884dc545d943a3f89a9d9d26d3476cbb4e0868d0e2aeee1f02789.
- Core test-only adapter helper and its base-source relationship: coherent/test-inputs/ui_background_sampling_modes.rs and coherent/sampling-host-build.json.
- Core results: coherent/sampling-cpu-results.json and coherent/sampling-gpu-results.json.
- Native source overlay: coherent-native/source/SOURCE-MANIFEST.json.
- Native all-workspace build: coherent-native/host-build.json.
- Native test executable: coherent-native/artifacts/unit-ifc23, SHA256 1f5a8cd6ad6b0bfa78cfaf8f2547223705832a7c9d494b44f473a0297dc3bd55.
- Native results: coherent-native/native-cpu-results.json and coherent-native/native-gpu-results.json.
- Machine-readable completion and evidence hashes: completion.json.

These are numerical correctness checks. The approximately 2.87-second core test-suite duration and 35.48-second native-suite duration are not frame-time measurements. No 4K 240 FPS claim is made. No physical MacNeo GUI was operated or relaunched. GPU and compiler leases were explicitly released. Obsolete owned runtime/cache scratch was archived after zstd-test and tar-compare verification. The archive is obsolete-runtime.tar.zst, SHA256 ec37cceb61753a84dc8f0e1aa1147ba015fcd6a44cb8e206fac047c8c380cb0c. It stores 489,390,305 bytes of files in 100,306,680 bytes, freeing 389,083,625 bytes. Source, the current pair, test binaries and logs remain available. cleanup.json and obsolete-runtime-files.json record the exact paths and hashes. Restore with `tar -I zstd -xf obsolete-runtime.tar.zst -C /mnt/datasets/c3r-build/background-upscale-sol`.
