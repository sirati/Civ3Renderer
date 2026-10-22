# Exact GPU text optimization

Status: numeric text gates and independent integrated glyph-cache oracle passed. Relative AMD measurements completed; actual native-App qualification pending. No claim of 240 FPS.

Production changes owned here:

- `tile-render-shared/src/text.rs`: three-subpixel per-channel evaluation and five-subpixel shared RGB evaluation. The original seven-subpixel filter remains the independent oracle.
- `tile-render-shaders/src/text.rs`: narrow RGB fallback draws, optional single-draw dual-source LCD blending, and integer coverage-cache consumption with procedural fallback on missing GPU readiness tags.
- `tile-render-shaders/src/glyph_cache.rs`: safe Rust GPU fragment fill of exact linear coverage bytes and a GPU metadata publication compute shader. No CPU coverage rendering, shader image-write unsafe, locks, or waits were added.
- `gpu-text/tests/channel_coverage.rs`: exact real Lucida and synthetic outline oracle across fractional origins/em/zooms, bold/italic, and decorations.
- `tile-render/tests/text_dual_source.rs`: independent feature-enabled versus feature-disabled devices and exact RGBA comparison on varied backgrounds.

The renderer cache integration belongs to glyph_cache_sol; feature negotiation/SPIR-V postprocessing belongs to release_deploy_sol; renderer pipelines belong to tile_popup_render_shadow.

## Evidence

All evidence below is under `/mnt/datasets/c3r-build/text-perf-sol`.

- CPU oracle: 156,465 exact channel comparisons against the original seven-subpixel function. `current-dual-cpu-oracle.log`.
- Three-tap GPU reference: 18/18 passed serially, including real fonts and target-resolution text. `three-tap-reference-serial.log`.
- Dual-source GPU reference: 18/18 passed. `current-dual-reference-gpu-oracle.log`.
- Dual versus three-draw fallback: 32,400,000 RGBA pixels matched byte for byte over direct color, 4–7-bit quantized color, six zoom ratios, fractional metrics, bold/italic, decoration bars, clipping, and partial-alpha backgrounds. `current-dual-gpu-oracle.log`.
- An earlier concurrent three-tap test run failed one annotation plate test. Unchanged artifact subsequently passed the isolated annotation test and all 18 tests serially. Keep `three-tap-gpu-oracle.log`, `three-tap-annotation-serial.log`, and canonical control evidence. The cause is unproven. Shader owner proved relevant non-text SPIR-V entries semantically identical.

## Current coherent cache prototype

- Frozen source: `source-cache-v2`, 647 files, verified no drift after `cargo build --locked`.
- Source verification: `cache-v2-source-verification.json`.
- Shader: `cache-dual.spv`, SHA-256 `a6c0df03eab40fd5c08eda9dfac9ac929c48629552e16895a61d1a7268c92ced`.
- Fallback: `cache-fallback.spv`, SHA-256 `2c54027c3067c72ac7f651fc6ca5122fb4e636710b095f80e62b494aaebb9155`.
- Both modules passed `spirv-val --target-env vulkan1.2`.
- Diagnostic executable: `perf240-cache-v2`, SHA-256 `78e95589c67bb34950fe84a4647d04c3b0eef4e2b446dce64fff4bc911101dce`.
- Toggle `CIV3R_PERF_DUAL_SOURCE=0/1` for independent devices' capability; toggle `TILE_RENDER_GLYPH_CACHE=0/1` for exact procedural/cache path. The first toggle only exists in the diagnostic binary.
- Uses the retained full production-quality performance workload, all 690 glyphs at pixel scale 2, full UI and cameras. `--mode scene-color` rebuilds the scene as in `scene` and also changes glyph colors every frame. It does not alter glyph metrics or remove UI.
- Frame GPU span includes cache generation and publication because the first clear timestamp precedes them. Separate `glyph cache fill` and `glyph cache publish` timing labels are present in v2.

The earlier `source-cache-qa` prototype acquired a Cargo.lock diagnostic dependency-edge update from adding direct winit/pollster dependencies to the benchmark bin. Its production/shader files matched the original source manifest. The old manifest is preserved; `cache-lock-resolution.json` and `cache-lock-scaffold.diff` record this. V2 freezes the resolved lock and builds with `--locked`.

## Cache correctness contract

The atlas stores precisely the integer 0–255 linear channel results already rounded by the existing filter, in RGBA8Uint. Integer texture loads therefore preserve later contrast-boost and 32-step RGB555 blend thresholds. Colors and backgrounds are not part of the cache. Keys retain immutable font identity/glyph, exact scale, bold/italic and Euclidean baseline phases modulo 96. Glyph coverage and all composition are GPU work.

GPU readiness tags publish after the fill render pass. A canceled encoder cannot publish its shapes; every used shape has a GPU-culled fill job on later uses, so cancellation repairs without readback. Cache metadata lives in the fragment stage to preserve the default eight vertex-storage-buffer limit. The final cache state uses Option pipelines rather than introducing OnceLocks.

## Independent cache gate completed

The final v2 cache matched the direct path exactly for 375 cases and 11,274,176 RGBA pixels in each of two independently created capability modes, dual-source and explicit three-draw fallback. Total: 22,548,352 exact RGBA pixel comparisons. CPU reference comparisons were reported separately and remained within the existing one-level blend tolerance. The gate exercised font-base repacking, three unsubmitted control snapshots, dropped encoder and stale-tail cancellation, and observed eviction/reset in the default 16 MiB atlas.

Durable independent evidence is in `/mnt/datasets/c3r-build/glyph-oracle-sol/results-v2-r2/{dual-source,three-pass}/oracle-result.json`, with `/mnt/datasets/c3r-build/glyph-oracle-sol/binding.json` binding final v2 source, shaders, executable and font hashes. The final cache performance executable remains `perf240-cache-v2`; speed is measured separately by the exclusive FPS owner.

## Measured relative performance

These are reconstructed production-backend workloads, not actual App qualification. All runs retained the full UI and used dynamic full redraws, with no cached-picture reuse.

The three-tap filter changed the scale2-to-4K GPU median from 37.043 to 31.829 ms and throughput from 25.673 to 29.608 FPS. Native 4K remained about 12.2 ms GPU and 71 FPS. Evidence: `/mnt/datasets/c3r-build/perf240-sol/evidence/candidates/candidate-summary.csv`.

A same-source cache v2 scale2 pan matrix measured 32.767 ms GPU and 28.759 FPS with both cache and dual blending off; 19.855 ms and 45.589 FPS with cache only; 25.630 ms and 36.241 FPS with dual only; 19.606 ms and 46.496 FPS with both. The scene-color workload, which also changed glyph colors every frame, improved from 25.662 to 19.888 ms GPU with dual enabled when enabling the cache. Native 4K remained roughly 13 ms GPU and 66 FPS. Evidence: `/mnt/datasets/c3r-build/perf240-sol/evidence/cache-v2/cache-summary.csv`.

The broker later found unrelated machine-learning CPU contention, so CPU and throughput comparisons remain provisional. GPU timing shows the relative shader/cache cost. Warm cache generation was roughly 0.002 ms fill and 0.001 ms publication. Residual camera passes cannot be attributed entirely to text from those measurements.

## Actual application diagnostics and fixture

`tile-viewer/src/render_worker.rs` and `tile-viewer/src/lib.rs` now emit bounded five-second JSON diagnostics under the existing `TILE_VIEWER_PROFILE` flag. They distinguish worker full versus reused submissions from matching UI-committed presentation completions, bind actual physical/logical geometry, record source-command counts and bounded hashes, and expose complete GPU-timer counters. No extra lock, wait, readback or per-frame outline/image hashing was added. Those diagnostics were built in historical checkpoint866. Root now owns these production files and all successor diagnostics; this document does not claim an arbitrary deployed binary includes later changes.

The actualApp fixture is `/mnt/datasets/c3r-build/native-app-qualification-sol`. Its README gives the runtime/source-manifest contract, native4K and scale2 crisp UI cases, untimed RFB setup, capture-free Wayland relative-pointer timing and ownership-safe cleanup. The latest external fixture passed 26 Python tests and 3 Rust helper tests. Live v2 barrier/preflight proof remains pending. The exact Rust observation/App formatters passed standalone execution and JSON/count checks against 600 glyph commands, sample bounds and negative geometry. These checks do not establish final 240 FPS and do not replace final workspace compilation or a representative heavy native-App run.

Actual-App866 observations and the corrected sustained-motion classification are in `native-app-status.md`. All three old repetitions fail sustained-motion qualification. Repetition1 moved initially then reached a static boundary;45.4216FPS is a bounded mixed-motion/static observation with 2504 full/0reuse and 2032/2504 GPU samples,81.15% coverage. There is no pooled dynamic average and no 240 FPS pass. `fixture-v2-binding.json` binds the current nonshipping fixture, helper, CPU-only proof and corrected ledgers.

Main-thread scene/metadata CPU work from the Neo sample is documented in `main-scene-cpu-status.md` and `main-scene-cpu-proof.json`. CPU proof does not establish actual App performance. Optional-art world-table rebuild invalidation remains open.
