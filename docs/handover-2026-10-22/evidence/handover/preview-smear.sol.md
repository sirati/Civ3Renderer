# Preview smear, Sol continuation

Original `preview-smear.md` preserved. No commits, resets, stashes, deployment, GUI work or image inspection.

## Source changes

The existing renderer fix keeps world extents uncapped and draws small fitted worlds directly at their zoom. The independent test draws a zoom-1 CPU picture and manually samples it, so the old shared camera cap cannot hide the regression.

Continued the extent fix through viewer geometry. `layout.rs`, `pan.rs` and `world_rescale.rs` now use `Camera::world_size()` to report world rectangles, find view centers, capture the cursor, move with arrow keys, clamp pans and rebase centers after Z scale changes. `source_size()` is the GPU picture allocation limit and must not determine these coordinates.

Added regressions for the narrow preview with a driver downscale already applied, driver extent reporting and capture center above 4096 world rows, and rebasing the full camera center during Z changes. Extended `fitted_preview.rs` with static retained-frame reuse and a clipped preview whose negative origin preserves the fractional zoom phase, checked against the independent zoom-1 oracle.

## Coordination

`mac_fps_music` owns renderer/viewer redraw caching and view-aware zoom-art work. That agent may update the `camera::placements` callback signature; this continuation does not edit camera geometry. Scenario generation/lifecycle remains `scenario_crash` work.

## Validation

Passed freshly source-built shader RADV validation with `CARGO_INCREMENTAL=0`, `nix develop`, and no precompiled shader override:

- `cargo test -p tile-render --test fitted_preview`: 3 passed, first run and final integration run after mac_fps_music's view-aware placements callback changes. Logs `/mnt/datasets/c3r-build/preview-smear-sol-tests.log` and `/mnt/datasets/c3r-build/preview-smear-sol-integrated-tests.log`.
- `cargo test -p tile-viewer --lib`: 381 passed, 0 failed, 3 ignored. Includes all new extent/capture/Z regressions and existing GPU city/scaling checks. Log `/mnt/datasets/c3r-build/preview-smear-sol-viewer-tests.log`.

Source hashes are in `/mnt/datasets/c3r-build/preview-smear-sol-hashes.txt`. The tested source-built SPIR-V is retained as `/mnt/datasets/c3r-build/preview-smear-sol.spv`, SHA256 `d19e66b8b204f1d9ee0d8c383e81f6180d5e14fa459f76551d168769d201980b`. Own 2.7 GiB target directory removed after tests.

## Runtime limits

No Apple Metal or live scenario screen verification performed here. Root must deploy the complete matching five-artifact protocol set and measure live scene reuse/frame timing. The original handover's viewer-only deploy suggestion is superseded by orchestrator protocol-44 requirements.


## Native Windows DX12 continuation

Root requested investigation of the frozen tiny-background GPU mismatch. No frozen source was edited. Diagnostics ran through VM owner `targeting_cursors`, with no GUI or image inspection. The native adapter is Microsoft Basic Render Driver, CPU, DX12, driver 10.0.26100.1. Treating this as unsupported software would have hidden a reproducible production geometry defect.

### Proven cause and correction

`scaled_span` obtained ceil endpoints through `-scaled(-value, n, d)`. On native DX12 the scaled quad collapsed to the one-pixel hairline fallback. A gradient fixture rendered exactly at unit zoom and through an ordinary source picture, but direct-at-zoom renders left every pixel except the origin clear. A diagnostic that bypassed atlas fetch/discard still produced only that pixel. Forcing a full viewport quad while encoding vertex values proved vertex n/d was correct but computed right endpoint was 1 instead of 32 at zoom 2/1 or 8 at zoom 1/2.

Only `scaled_span` now computes signed ceil directly. For a product p >= 0 it uses `(p + d - 1) / d`; for negative p it uses `-((-p) / d)`. This is ceil for both signs under the same integer bounds as the original calculation. No backend bypass, feature disable, CPU reference change or Metal `box_mean` change. `shader-correction.diff` records the exact 19-line source diff against frozen ef.

Persistent regression `direct_scaled_quads_keep_their_full_extent` is appended to `crates/tile-render/tests/fitted_preview.rs`. It compares GPU pixels against manually sampled nominal source pixels and the unchanged CPU reference across 24 combinations of positive/negative sprite positions, negative clip bounds and zoom 2/1, 1/2, 2/3, 3/7.

### Evidence

All native logs are `/mnt/datasets/c3r-build/windows-vm-sol/evidence/`:

- `tiny-mask-isolated.log`, `tiny-mask-validation.log`: frozen tiny-mask mismatch reproduced serially, with validation enabled and no validation errors.
- `gpu-integrations.log`: original background suite failed 7/7; ordinary orthogonal world smoke passed.
- `coordinate-probe-patched.log`: independent compute and raster fast rounding/FragCoord arithmetic passed.
- `output-probe.log`, `dynamic-output-probe.log`: exact frozen production output shader passed gradient copies and zooms, including dynamic uniform offsets 0, 256, 1024, 4096.
- `renderer-probe.log`: actual frozen renderer only failed direct-at-zoom geometry.
- `renderer-scaled-diagnostic.log`, `renderer-vertex-diagnostic.log`: fragment discard excluded; vertex endpoint collapsed despite correct zoom values.
- `renderer-ceil-correction.log`: all 12 Direct/Quantized x zoom 1/1, 2/1, 1/2 x hairline false/true cases exact with the narrow correction.
- `ceil-correction-tests.log`: original tiny-mask test passed, all 7 background tests passed. Two unrelated quantized shaded-world tests failed identically before/after; `mac_fps_music` owns that separate blend/rounding investigation.
- `golden-probe.log`: all 24 persistent regression combinations passed native DX12 against manual nominal sampling and the unchanged CPU reference.

Initial scratch graphics pipeline creation failed because the standalone diagnostic Cargo root omitted the frozen workspace's vendored wgpu-hal patch for 32-bit pipeline stream alignment. Adding the exact frozen vendor patch fixed the probe itself. This was a diagnostic setup issue, separate from the frozen test failure.

### Canonical pair

`/mnt/datasets/c3r-build/dx12-coordinate-sol/canonical.spv` was freshly built from the live source and has the exact same hash as the corrected external shader tested natively:

`1e8f22da55bf9ff6855ca9da0724da95388b7456e94dd22a5bb0be8ca73fa6ef`

Exact source hashes are `/mnt/datasets/c3r-build/dx12-coordinate-sol/canonical-hashes.txt`:

- shader `lib.rs`: `c6430da0ac9ba27050cf0c42112b1130573a5f70a5c271e59506db7805d2b8fd`
- `fitted_preview.rs`: `78299499d5b54ddacc54c64324abeed6d2d84ce9e84bb87bbe0654b6ff006e42`
- shared `lib.rs`: `9017c7bc4871922d6cb431f27590dd5e151e5f74e71e72d55e4f9183b1b9c243`

RADV matching-source validation completed through `mac_fps_music`: fitted_preview 4 passed, gpu_matches_reference 38 passed, ui_background_sampling 7 passed. The complete latter suites ran serially after an initial concurrent attempt produced an unexplained all-zero 768-pixel image in `nonwrapping_candidate_bounds...`. The full clean rerun is evidence of a passing final run; the first anomaly has not been assigned a cause. The final serialized matching-source RADV gate passed all 62 tests: fitted_preview 4, gpu_matches_reference 38, ui_background_sampling 7, text_at_target 3, text_matches_reference 10. Persistent log `/mnt/datasets/c3r-build/perf-agent/geometry-final-radv.log`; exact initial anomaly retained in `/mnt/datasets/c3r-build/perf-agent/geometry-initial-anomaly.log`. Initial failure was `nonwrapping_candidate_bounds_preserve_tall_wide_and_fixed_anchor_pixels`, Orthogonal, scaling 0, downscale 1, zoom 1/1, camera (-70,-135), all 768 pixels returned zero including alpha instead of the clear color. Its cause remains unexplained. The final source/SPV hashes match the canonical pair above. Scratch shader build caches were removed; source, logs and SPIR-V artifacts remain at their documented paths. No Mac test or deployment performed in this continuation. Root owns the next immutable paired snapshot and all five matching production artifacts.
