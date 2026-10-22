# Shipping glyph-cache camera reuse audit

This is a read-only audit of the accepted f510 shipping source and the retained native 4K measurement. It proposes a bounded diagnostic and a possible private candidate. It does not change source, build artifacts, the wider qualification packages, or the shipping decision.

## Inputs

The renderer source is `/mnt/datasets/c3r-build/release-1019/src-f51034b34b94eeb3bc3a8dcfbd2e4faf8eabc6350721b7f7172da7b406504703`.

| file | SHA-256 |
| --- | --- |
| `crates/tile-render/src/renderer/glyph_cache.rs` | `2aff303f8a579182819019ebee56ce24e54a1f072992375d480e8a3b71cf1246` |
| `crates/tile-render/src/renderer.rs` | `427c3cad90717e5f1277095b9b979479cb6e8cc2cdae9aa2df0becd31f36f57b` |
| `crates/tile-render/src/camera.rs` | `9a5e3e63808d3325c34f1c328f534f29131a28aee82dba38b82863ebd66aec3b` |
| `crates/tile-render-shared/src/text.rs` | `a3929c2ff388df42bbd5afe4a2699e889a9c5c211fb20ee772a2475e6cdba243` |
| `crates/tile-render-shared/src/glyph_cache.rs` | `ff9d5cff6f3114abbbdd8c7d7cf8044e81b89a77dedacf13e5b77430f39faf33` |
| `crates/tile-render-shaders/src/text.rs` | `3648e58f388fad5550505d9a34906926b15d02995f92d89d0ae94a5c69e0c404` |
| `crates/tile-render-shaders/src/glyph_cache.rs` | `b136ddc56b78137abd07d49a5c185c5749c1af8e8be53b1f5827bd7d68e42676` |
| `crates/tile-viewer/src/render_worker.rs` | `8bd2ecd7cba24b30823f7533b30ea2f3562f9402397d8698cddc404f329d3e3b` |

Measurement evidence:

- `handover/creative-qa/text-perf-sol/actual-App-f510-native4k-results.json`, SHA `368ba6b77e41dbc54ba6642001da9f14c644a66917e71ce62b9287ed426eb012`.
- `handover/creative-qa/text-perf-sol/actual-App-f510-camera-stage-map.json`, SHA `9f0c4becf33b42dec31a1fa894acc856af2969ff656b0b88b42ea99b3aa23684`.

The camera stage map records world cameras v0 through v5 at zoom `1/1`, downscale 1, with v0 and v1 full world views and v2 through v5 thin world views. The retained workload has 17,168 glyph commands and 252 screen glyph commands.

## What the current code does

`Steps::draw` calls `prepare_glyph_cache` once for every eligible camera (`renderer.rs:2658-2672`). The eligibility check first looks for a visible text run for that view and art stage (`glyph_cache.rs:402-414`). `State::prepare` then walks the entire `world.text_source` slice (`glyph_cache.rs:248-331`), even when an instance will be culled. It computes camera-dependent placement, viewport and explicit-clip intersection, and stores a 32-byte `CacheGlyph` or the procedural fallback marker. `State::flush` writes every used metadata vector and its fill-job vector to the queue (`renderer.rs:752-760`, `glyph_cache.rs:200-207`).

The coverage atlas itself persists across camera movement. A cache entry key is the face pointer, glyph record, scale, bold amount, italic flag and the two fixed-point phases (`glyph_cache.rs:16-30`, `260-302`). The color, key color, text category, view class, clip rectangle, viewport size, target location, color quantization and dual-source versus three-pass mode do not enter that coverage key. The GPU applies color contrast and keyed composition when it samples coverage (`tile-render-shaders/src/text.rs:74-102`, `228-250`).

The metadata is different from the coverage. The CPU currently makes it visibility-dependent. `placement` rejects the wrong view, wrong art twin, hidden category, distant placement, empty bounds, and an explicit clip or viewport miss (`glyph_cache.rs:432-452`). This means a metadata vector made for one camera is not a complete mapping for another camera. The text vertex shader repeats view, art and category tests and rejects empty bounds, but it does not reject a whole viewport miss before rasterization (`tile-render-shaders/src/text.rs:15-72`). Hardware clipping still removes off-target fragments.

`State::begin` clears the per-control-slot `filled` set and resets the number of preparations (`glyph_cache.rs:186-198`). There are three control slots, selected in round-robin order (`renderer.rs:18-40`). `flush` writes the slot's metadata after encoding and before submission. `controls_submitted` recycles the slot only after the queue submission (`renderer.rs:718-731`). A shared metadata buffer must preserve this queue ordering or keep one safe snapshot per in-flight slot.

An atlas allocation exhaustion or face-budget overflow sets `reset_after_submit`; `State::submitted` then clears entries, shelves and pinned faces at the next submission boundary (`glyph_cache.rs:181-196`, `300-322`). A mapping that points into the old atlas must be retired at the same boundary. The `ready_index` and monotonically increasing `ready_tag` are GPU readiness stamps, not an atlas-generation identity. A candidate must clear or epoch its mapping when the atlas is reset or replaced.

World replacement and free-text updates repack `text_source` and can replace the outline buffer (`renderer.rs:1743-1815`, `1873-1936`, `1944-1988`). The worker chooses `set_world`, `update_cells`, `update_free`, or marker updates using world and free versions (`render_worker.rs:923-944`). Cell-only updates do not change text. Free updates, marker-tail changes, font replacement, atlas replacement and sprite/world replacement can change the glyph mapping or its instance indices.

## Camera-only reuse rules

A camera pan can reuse the coverage shape when all of these hold:

- The zoom ratio and text scale are unchanged. `text::scale_of` derives shape scale from `em`, zoom numerator/denominator and face units-per-em (`text.rs:273-288`).
- The transformed fractional origin is unchanged. Cache phases are `p.x mod 96` and `p.y mod 96` (`glyph_cache.rs:270-275`). At the measured `1/1` zoom and integer camera coordinates, integer panning preserves these phases. At a rational zoom, the camera and origin residue modulo the zoom denominator must match. A wrapped-world copy must also use the same period residue.
- The face, glyph record, bold/darkening amount and italic state are unchanged. `TEXT_BOLD` is represented in `p.bold`; `TEXT_ITALIC` is in the key. Bars are intentionally procedural and are not cached (`glyph_cache.rs:276-281`).
- The text instance identity and index remain stable. The current shader indexes metadata by the `TextGlyph` instance index (`text.rs:29-35`, `76-83`). Reordering, inserting or removing world text, or changing the marker tail requires a new mapping buffer or an explicit indirection table. Painter order must remain the same even though coverage itself is order-independent.
- Atlas rectangles and readiness state remain valid. Any reset, shelf overflow, face-budget reset, atlas replacement or face replacement invalidates the old mapping. `Arc` face pointer identity is used to keep old outline storage alive; `same_fonts` compares pointer identity (`renderer.rs:2991-2994`).

The following changes do not require a new coverage shape when the mapping remains complete: text RGB color, key color, contrast boost, color bit depth, quantized versus float encoding, dual-source versus three-pass output, target clip, viewport/scissor, view class, `FREE_EXCEPT_VIEW`, art-stage selection, hidden-category selection and painter order. They do require the draw controls and, for the current implementation, they still trigger a new visibility-filtered metadata vector.

Downscale and camera position affect the placed pixel and therefore the phase class. A change in `downscale`, zoomed-art ratio, screen origin or screen shift must be treated as a new mapping class unless its transformed phase residue is proven equal. `target_x` and `target_y` only select where the view is written and do not change the cached shape.

## Possible private candidate

The smallest safe design is a two-level mapping:

1. Keep a scene mapping keyed by the immutable text-source identity, font-face identity, atlas epoch, zoom/downscale class and transformed phase residue. The scene mapping contains one `CacheGlyph` per text instance without view, category, viewport or explicit-clip rejection. The shader's existing view/art/category tests remain the draw gate.
2. Bind that mapping for every camera in the same class. Upload only the normal `FrameParams`/output controls. Allocate and fill a new class lazily when a new phase, zoom or font appears. Keep the existing per-slot metadata path as a fallback while validating the new path.

The mapping must have a stable text-source identity and instance order. A scene replacement, `update_free`, marker-tail change, font/outline change, atlas reset, or atlas replacement starts a new mapping epoch. Three in-flight slots need either immutable mapping buffers retired by queue order or one mapping snapshot per slot. New cache fill jobs still require the existing render pass and readiness publication before the first draw uses them. This design does not alter the coverage algorithm or the text output shaders.

## Bounded diagnostic before any candidate adoption

Use the retained f510 world packet and the existing 18-camera arrangement in a private diagnostic. Do not run the full application for this check.

- Render three integer pans of v0/v1 and the v2-v5 thin views at zoom `1/1`, followed by one zoom `3/2` or zoom `2/1` view and one rational camera-origin phase change.
- Record for every camera: `text_source.len()`, `State::prepare` visits, metadata bytes written (`32 * glyph_count`), unique fill jobs (`96 * jobs.len()`), cache hits, atlas allocations, reset count, control slot, and map-class key.
- Repeat after a cell-only update, free-text update, marker-tail update, font replacement, atlas reset and world replacement. The expected safe reuse set is camera-only movement within one phase class and cell-only updates. The expected invalidations are the other four changes.
- Compare output bytes and the existing `glyph_sample_hash`, camera hash and sprite/glyph command counts. Any mismatch rejects the candidate. Keep GPU pass timing separate from host metadata-write timing.

The current code gives a lower-bound metadata-write estimate of `17,168 * 32 = 549,376` bytes for each eligible camera. The six measured world cameras alone therefore prepare at least 3,296,256 bytes of metadata per frame, before any additional eligible UI cameras. This is a byte count, not a causal attribution of the 12.64–16.49 ms host encode or 19.42–20.82 ms GPU frame windows.

No source change or adoption follows from this audit. The 240 FPS target remains unqualified; this report identifies a bounded performance experiment only.
