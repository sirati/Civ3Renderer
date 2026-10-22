The successor shader pair contains the supported fractional text clip correction. Both half-open clip edges now use the same inverse-nearest membership as native fills, including negative coordinates and saturation. Glyph positioning and sampling phase are unchanged. Its full and fallback SPIR-V hashes are 43961a92338f6b5b1f5ebd12dfdc806057fe43d8bfeb50abdb7872d274b601c4 and bbde10b5e91a3e54c337eab67c2e5acce8abf384657043eb46bbe056ea799281.

The first corrected source used saturating_add, which rust-gpu cannot compile. That failed source freeze and complete compiler log remain at /mnt/datasets/c3r-build/background-upscale-sol/coherent-clip/. The owner replaced it with an equivalent bounded scalar branch, reran its eight CPU tests and supplied final shared/text.rs SHA256 a3929c2ff388df42bbd5afe4a2699e889a9c5c211fb20ee772a2475e6cdba243. The successful successor source is coherent-clip2/source/. Its 669-file SOURCE-MANIFEST.json has SHA256 2d4e226bf02660fc31870e7fc85040ad032ee6c111fcfd47602cf6373b99bc8f. The frozen source had no drift.

The successful full/fallback pair passed strict Vulkan1.2 validation, all five shader build helper tests, a safe 192-byte host uniform layout assertion and independent reflection of all 41 members. host_tick_ms remains at 176, followed by scalar pads at 180, 184 and 188. No uniform or wire schema changed.

All 20 shared CPU tests and four menu background tests passed. All ten sampling GPU tests passed on the dual-source device and the same ten passed on the feature-disabled fallback device. Adapter assertions and logs require AMD integrated 0000:72:00.0, Vulkan and RADV; wgpu validation was enabled. The sampling executable SHA256 is 69a171dc04f412590d774f5d1b5944254bd8f97ecf6f5996963a0d4740bdf6e5. Independent binary inspection found both exact successor SPIR-V byte arrays inside it. Source inputs, retained external libraries and all eight rebuilt workspace dependencies are recorded in sampling-host-build.json.

startup_progress_sol also passed the expanded actual clip regression and retained host-clock animation probe against this pair. Its clip matrix compares 3,374,976 exact pixels across four scales, three origin/screen-shift cases and both clip ends. Its animation probe compares 24,000 pixels over all 30 frames and wrap, checks retained-picture reuse and preserves the game clock. The owner's gpu-results.json and coherent-executable-bindings.json record exact shader bytes embedded in both executables. These are numerical correctness gates.

The adopted seven glyph-cache regressions also passed on both native optional and feature-disabled devices, 14 executions with no failures or ignored tests. Its executable SHA256 is 4e67568fef83bd32a890ea88e39e800405eab9d888881cb70f1c63b422492423; both exact successor SPIR-V arrays are embedded. Two additional renderer eligibility CPU tests passed. Evidence is at /mnt/datasets/c3r-build/glyph-skip-adoption-sol/coherent-clip2/. Its runtime-9b-leaf-binding.json proves all 51 renderer/shared/shader/GPU-text files and both shader modules are identical to corrected release source 9b07836fc7743952d53283e41b41999972a781f42d005be67c31e568cdddc205. This is a leaf-source binding, not application runtime acceptance. Earlier native mixed-art and tiny-strip matrices are preserved as historical evidence for the preceding pair; they were not rerun by this task after the clip correction.

Evidence:

- /mnt/datasets/c3r-build/background-upscale-sol/coherent-clip2/binding.json
- /mnt/datasets/c3r-build/background-upscale-sol/coherent-clip2/source/SOURCE-MANIFEST.json
- /mnt/datasets/c3r-build/background-upscale-sol/coherent-clip2/sampling-host-build.json
- /mnt/datasets/c3r-build/background-upscale-sol/coherent-clip2/sampling-executable-binding.json
- /mnt/datasets/c3r-build/background-upscale-sol/coherent-clip2/sampling-cpu-results.json
- /mnt/datasets/c3r-build/background-upscale-sol/coherent-clip2/sampling-gpu-results.json
- /mnt/datasets/c3r-build/loading-ui-sol/gpu-results.json
- /mnt/datasets/c3r-build/loading-ui-sol/coherent-executable-bindings.json
- /mnt/datasets/c3r-build/handover/loading-ui.sol.md
- /mnt/datasets/c3r-build/glyph-skip-adoption-sol/coherent-clip2/gpu-results.json
- /mnt/datasets/c3r-build/glyph-skip-adoption-sol/coherent-clip2/runtime-9b-leaf-binding.json
- /mnt/datasets/c3r-build/background-upscale-sol/coherent-clip2/completion.json

No performance measurement or 4K 240 FPS acceptance is claimed. No Neo connection, UI operation or app relaunch occurred. The successor pair and sampling compiler/GPU leases have been released. The source and evidence are retained for release adoption, which separately verifies the entire shader/shared/build/toolchain input union against its final source archive.
