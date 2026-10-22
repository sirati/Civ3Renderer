# Glyph preparation skip candidate

The isolated candidate skips glyph-cache preparation when a camera stage cannot draw outline text. It remains unapplied to production. Isolated numeric correctness validation against immutable 866 plus only this patch passed. Shared source and the 3be freeze remain unchanged. No performance benchmark ran. Full evidence is in `/mnt/datasets/c3r-build/handover/glyph-preparation-skip-866-validation.sol.md`.

Patch: `/mnt/datasets/c3r-build/glyph-preparation-skip-sol/candidate.patch`

Source hashes and checks: `/mnt/datasets/c3r-build/glyph-preparation-skip-sol/source-review.json`

## Change and safety argument

`Steps::draw` passes its selected step range to `prepare_glyph_cache`. Preparation returns before cache borrowing, metadata assembly, buffer allocation, and upload preparation if there are no fonts/text, the art stage is neither 0 nor 2, or there is no outline-text run in the selected steps for this camera's view class.

A shared `free_run_visible` predicate applies the same coarse eligibility to ordinary free draws, native smoke draws, and their pass-presence checks. Both text kinds skip invisible art-stage draws before the cache bind group is required. The ordinary draw signature now carries the art stage; native smoke uses `FrameParams::zoom_art`.

The source proof for skipping art stage 1 is `tile_render_shared::text::visible_at_art`: stage 0 permits ordinary and low twins, stage 2 permits ordinary and zoomed twins, and every other stage returns false for every glyph. This candidate removes only such guaranteed invisible text draws. Native `Subpixel` sprite text and all other free-sprite kinds keep their existing eligibility.

Eligible preparations still create the existing complete metadata array with one entry per global glyph. There is no metadata compaction or instance-index remapping. Painter order remains unchanged for all retained draws. Cache filling, exact-length readiness publication, fallback coverage, frame-slot claims, and submitted-batch eviction are unchanged. `claim_control_frame` still calls `State::begin` before preparation, and `flush` still uploads only the preparations used by the same slot. The candidate adds no shaders, CPU rasterization, locks, waits, unsafe code, protocol changes, or control slots.

This gate is deliberately coarse. It can still prepare text hidden by per-glyph categories, twins, clipping, or viewport bounds. Avoiding those calls requires a separate proof and is outside this patch.

## Motivation and limits

Root's actual-application diagnostic reported 18 cameras, 17,168 glyph commands, and 7,316 sprites. It was an untimed diagnostic and is not a qualifying performance result. An unconditional preparation builds 549,376 metadata bytes for 17,168 glyphs at 32 bytes per entry, even when the stage cannot display text. Repeating that work across camera stages is avoidable. This candidate does not yet have a measured CPU or upload reduction.

The accepted glyph coverage cache already passed the independent real-font oracle and the three durable cancellation, prepared-snapshot/font-repacking, and eviction regressions. Those results apply to the prior source, not this candidate. Historical 4K cache results remain around 19.6 ms GPU for sharp scale 2 and around 13 ms for native 1x. The 240 FPS target requires at most 4.167 ms per frame and has not been achieved.

## Source binding

`git apply --check` passed against the shared source during this review. Saved originals and candidates match `binding.json`, and all three shared files still match the saved originals.

| File | Original SHA-256 | Candidate SHA-256 |
| --- | --- | --- |
| `crates/tile-render/src/renderer.rs` | `ff41f555ad55ee90560a50c426a46f762463c066d7ba7dfc0b96d5c02818ea1b` | `ae171fb6aa18b56b79a4fc1ca072f78f536446091759ff5fc6dcc360767fe40b` |
| `crates/tile-render/src/renderer/glyph_cache.rs` | `864cc854201881175045c4e29240da1756d5c98940b7a0a1a4e0b451ae49ebaa` | `e258310b33bc510593a01ee02a8efee5dde965faa7d3a809e622daeb7cbee404` |
| `crates/tile-render/src/renderer/smoke.rs` | `6a4e1acd57152f277aca4899e98637cb457f57b7478e2c3cf3cbc637cd7edb78` | `ec2c8fca7b1a589ad415608780f17b63755106a1394e2e5228b51e967a0a64de` |

Patch SHA-256: `168d9fa1e98d61c839e9a393843e1049f21c8cc78587902c1e7748455b9f20bc`

## Validation and remaining acceptance

Seven meaningful GPU regressions pass with both dual-source and fallback. Original 866 fails the six new preparation-policy assertions. The existing three glyph-cache ownership regressions pass. All 375 real-font cases match original 866 exactly for dual/fallback and cache on/off, with exact cache/direct and cross-backend comparisons as well. Global indices and three prepared slots retain exact output. See the numeric-validation handover for source, dependency, shader, executable, log and result bindings and preserved fixture failures.

The warm fixture uses one preparation and 128 metadata bytes instead of four preparations and 512 bytes. This is a counter regression result, not a frame-time estimate. Actual-application performance comparison awaits a coherent successor freeze and the broker's quiet window. Root review and adoption remain pending.

The earlier bounded terrain-prefix design remains in `/mnt/datasets/c3r-build/handover/terrain-region-cache-design.sol.md`. It is a design note without an implementation or performance acceptance.
