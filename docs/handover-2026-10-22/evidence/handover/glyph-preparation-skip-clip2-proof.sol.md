# Glyph-preparation regressions on the fractional-clip successor

All seven adopted glyph-preparation GPU regressions pass against the accepted fractional-clip successor shader pair. Every case executes once with the selected adapter's supported optional features and once with a second device requesting no optional features. All fourteen executions pass, with zero failed or ignored tests. The two CPU eligibility regressions also pass.

The test source is the exact 669-file snapshot `/mnt/datasets/c3r-build/background-upscale-sol/coherent-clip2/source`. Its manifest SHA-256 is `2d4e226bf02660fc31870e7fc85040ad032ee6c111fcfd47602cf6373b99bc8f`. All 669 manifest files, and the manifest itself, were reverified after the GPU gate. The permanent regression module is unchanged, SHA-256 `dfc3c3f15d07d9561326170af7a6938b631d8e7c9bc31196db80eeafe47ed2f2`.

The exact unit executable is `/mnt/datasets/c3r-build/glyph-skip-adoption-sol/coherent-clip2/artifacts/glyph-skip-unit-tests`, SHA-256 `4e67568fef83bd32a890ea88e39e800405eab9d888881cb70f1c63b422492423`. It embeds each shader exactly once:

- Full: `43961a92338f6b5b1f5ebd12dfdc806057fe43d8bfeb50abdb7872d274b601c4`.
- Fallback: `bbde10b5e91a3e54c337eab67c2e5acce8abf384657043eb46bbe056ea799281`.

The GPU gate ran serially under WGPU validation on Papa's AMD RADV integrated GPU at PCI `0000:72:00.0`, Mesa 26.2.4. It completed in 2.26 seconds, which is test execution time and carries no FPS claim. Global glyph indices, all three queued control slots, cold preparation skipping, warm preparation count and metadata upload bytes remain covered by the original assertions. The tests impose no vendor, backend, or dual-source requirement and contain no self-skip or environment mutation.

`runtime-9b-leaf-binding.json` proves all 51 files across the renderer, shared, shader, and GPU-text crates, plus the shader pair, are byte-identical to the final runtime freeze `/mnt/datasets/c3r-build/release-1019/src-9b07836fc7743952d53283e41b41999972a781f42d005be67c31e568cdddc205`. That runtime source manifest is `0fc87f45c617bff1e6695f4b8133bf51c594c5e5fc2d4193ed649b9760c33b17`. This permits the renderer proof to carry across those identical files. It is not whole-application runtime acceptance.

Evidence is under `/mnt/datasets/c3r-build/glyph-skip-adoption-sol/coherent-clip2/`. Start with `completion.json`, then `build-manifest.json`, `cpu-results.json`, `gpu-results.json`, and `runtime-9b-leaf-binding.json`. The final log is `logs/durable-seven-native-and-fallback.log`, SHA-256 `f010f334ee5ad0689b9bc69d65a7d256eedd780e6681192a3d394392e346ac3d`.

The serial jobs1 CPU build and exclusive GPU correctness leases are explicitly released. No owned process remains running. This proof changed no production source, performed no Neo work, and made no performance measurement. The 4K 240 FPS goal remains unverified. Earlier acceptance against `cb7214b0`/`3d07b207` remains preserved under `coherent/` as historical evidence for the earlier clip math.
