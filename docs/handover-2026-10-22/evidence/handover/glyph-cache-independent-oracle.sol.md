# Independent glyph-cache GPU oracle

Final v2 correctness gate passed on Papa AMD RADV. No production source edits. No guest builds. The dedicated correctness lease was granted after shader_perf_sol and explicitly released after all four GPU processes exited zero. This is not a 240 FPS or full-application acceptance result.

Source: `/mnt/datasets/c3r-build/text-perf-sol/source-cache-v2`.
All 647 manifest files verified with zero drift. SOURCE-MANIFEST.json SHA256 `671a6746d7925fc18b958dc6ebc84c4f8c8edc27ae6a97ac8abfe4764b473514`.
Embedded dual shader SHA256 `a6c0df03eab40fd5c08eda9dfac9ac929c48629552e16895a61d1a7268c92ced`.
Embedded fallback SHA256 `2c54027c3067c72ac7f651fc6ca5122fb4e636710b095f80e62b494aaebb9155`.
Oracle executable SHA256 `1d20c810fa64c78c70ea236f21d7099743853bb86ceab4d71ee5427dcf8b232e`.

Durable test source, driver, comparison script, lock, binary, SPIR-V and hashes are under `/mnt/datasets/c3r-build/glyph-oracle-sol/`. `binding.json` binds all exact files. Final outputs are `results-v2-r2/`; earlier failed setup attempts remain separate.

## Results

Cache enabled and `TILE_RENDER_GLYPH_CACHE=0` ran in separate processes. One pair used actual dual-source blending; the other explicitly requested a device without that feature to use the three-channel fallback. WGPU_VALIDATION=1 was set. Adapter identity was asserted AMD/Vulkan/RADV and printed: PCI 0000:72:00.0, vendor1002, device13c0, Mesa26.2.4, IntegratedGpu.

Each pair compared 375 images and 11,274,176 pixels. Every RGBA byte matched exactly, totaling 22,548,352 pixels across the two cache/direct comparisons. A separate cross-comparison also found dual-source versus three-pass outputs exact for both cache modes, 750 images and another 22,548,352 compared pixels. The latter is a separate comparison of the same outputs, not additional unique test scenes.

Each process additionally compared 114 scenes against the existing CPU text reference. Across four processes these cover 35,475,200 pixel comparisons. Maximum channel difference was one, with no pixel beyond that existing hardware-composition tolerance. The large-glyph eviction stress uses exact direct-GPU comparison, not the CPU reference. Output equality is asserted by compare.py; no pass relies on screenshot interpretation or printed reference lines alone.

## Coverage

- Existing read-only Lucida Sans outlines, Direct and RGB555 composition, colors, keyed text, translucent varying backdrops, bold, italic and bars.
- Fractional em64 and eight rational scales, including1777/4096 and4096/1777; negative/positive camera and screen phases, clipping, three simultaneous view zooms and clipped source copies.
- Warm updates through update_free with changing color/key/em/position; set_world and changed face at the same index, including a different synthetic outline; simultaneous nonzero packed real-font bases.
- Three encoded snapshots before one submit, plus safe fourth-frame refusal. Separate set_world preparations retain the same real face Arc as font arrays change [A,B] to [B] to [C,A,B], with different filler-face words, live colors/keys and distinct retained GPU outline buffers. One cached allocation and two retained-key hits were observed across those three preparations.
- Dropped encoder A, submitted B, then revisited A. A second cancellation case reuses canceled slot storage with fewer jobs before revisiting its stale-tail glyph. GPU readiness publication must correspond only to coverage work actually submitted. Independent source review confirmed exact-size publish binding excludes padded/stale job capacity.
- 260 large input variants plus an early-key revisit in the default 2048-square integer atlas. Observed atlas bytes16777216, reset_count1, allocation_count207, hits53, one procedural fallback and87 entries after the run. Some distinct em64 inputs reduce to the same effective coverage key. Every stress image had visible ink and matched the direct path. Atlas memory is16MiB; readiness/metadata add storage, with final stress resident GPU bytes16844288. Face words retained84432bytes. The test does not claim total cache storage is exactly16MiB.
- Timing enabled for the three prepared font-repack frames on the timestamp-capable device, validating fill/publish timestamp commands under WGPU validation.

## Provenance and limits

The first attempt exposed a test assumption: this real LSANS cmap lacks Euro. The fixture now records that fact and uses its native notdef glyph0 for that slot. It is not evidence that Euro has a supported glyph. A second attempt's world stress used zoom-filtered art at native zoom and correctly produced no glyph; the fixture now uses unfiltered outline commands so native-scale stress has visible ink. Both setup failures are preserved; neither is attributed to production code.

Earlier v1 source-cache-qa had only a Cargo.lock drift caused by its diagnostic scaffold dependency edges. The owner preserved that provenance. Final v2 freezes the resolved lock, verifies all647files, and the final executable builds with --locked.

CPU cache statistics prove allocations, retained keys and eviction, not actual GPU texel-fetch counts. GPU speedup and sustained dynamic4K240 performance remain separate gates. This test does not cover Metal, Windows drivers, full application UI onset or uncaptured OS cursor latency. Frozen31b aggregate counts do not include these new cache tests.

Exact result files:
- `results-v2-r2/dual-source/oracle-result.json`
- `results-v2-r2/three-pass/oracle-result.json`
- `results-v2-r2/dual-versus-three-pass.json`
- `results-v2-r2/execution.json`
- `run-v2-r2.log`, with individual `on.log`/`off.log` files in each pair directory.

Later read-only scope audit: warm update_free cases run only between submissions. Three prepared source-changing cases use set_world replacement buffers. The gate does not cover in-place update_free while older encoders remain unsubmitted. See prepared-update-scope.sol.md. Production worker ordering avoids that interleaving; root was notified of the public API precondition issue. No GPU rerun was performed during the FPS quiet window.
