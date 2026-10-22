# Bounded terrain-region GPU cache design note

This is source-only investigation. No terrain cache has been implemented or tested. Root reprioritized the glyph-preparation skip candidate before this design reached a prototype.

## Evidence

- GPU attribution: `/mnt/datasets/c3r-build/resolve-sol/diagnostic/attribution.json`
- Native inventory: `/mnt/datasets/c3r-build/perf240-sol/evidence/inventory-native4k-v2.log`
- Scale-2 inventory: `/mnt/datasets/c3r-build/perf240-sol/evidence/hd-ww2-production-scale2-inventory.log`
- Recorded scene: `/mnt/datasets/c3r-artifacts/continuation-20261018/performance-22cd2e4ece06/evidence/recorded-traces/hd-ww2.txt`
- Performance helper: `/mnt/datasets/c3r-build/perf240-sol/source/crates/civ3-bridge/src/bin/perf240.rs`

The recorded HD-WW2 world is a nonwrapping 68 by 122 staggered grid, step 64 by 32. Native camera position is 64,580 with a 3840 by 2160 viewport, zoom 1, downscale 1, no cull, no hidden categories, binding 0, and 128-pixel overhang in each direction.

| GPU pass | Layers | Contents | Recorded native 4K GPU time |
| --- | --- | --- | --- |
| 0 | 0..2 | Terrain, polar cap, floodplain | about 3.343 ms |
| 1 | 3 | Rivers under | about 0.062 ms |
| 2 | 4..52 | Map objects, borders, roads, resources, cities | about 2.643 ms |
| 3 | 53 | Rivers over, then free UI labels | about 0.437 ms |
| 4 | 54..55 | Fog and fog edge | about 2.173 ms |
| 5 | 56..75 | Descending units | about 0.199 ms |

Additional recorded costs were free UI about 1.23 ms, output about 1.762 ms, and clear about 0.403 ms. The whole frame was about 12.327 ms. Values are historical attribution, not a current acceptance result.

## Safe initial scope

Cache the exact colored prefix through passes 0 to 2, layers 0 to 52, excluding all free sprites. Initially accept only the recorded native zoom-1/downscale-1, no-cull, binding-0, nonwrapping case. Other views and unsupported cases retain normal GPU rendering.

The first 53 base layers have no per-cell phases or offsets and use static single-frame PCX sets. `render-desc` creates implicit single-frame sprite sets; explicit animations remain separate. Animated volcano smoke and effects are free sprites after fog or units. `ui_screen::under_pass` places dynamic city/map labels after pass 3 and before fog. The city binding only replaces fog/fog-edge layers, but clean-map masks still affect prefix categories.

Replay the original prefix order, row/column/layer painter order, biased quantized target format, clear color, and alpha. Do not flatten shadow or fog factors approximately. `Blend::Scale` rounds after every sprite, so algebraically combining independent factors changes pixels.

Rebuild a dirty cache tile from clear and replay all prefix contributors with conservative overhang. Invalidate the old and new art footprints using per-prefix cell versions, layer/LUT/set changes, atlas/palette mutation, masks, binding, encoding, downscale and clear color. Reject reachable animated sets unless their phase/version is explicitly tracked. Dynamic units, labels, UI and fog still render each frame.

The viewer already distinguishes changed cell/free records and `Dirty::Records` identifies region and cell. A change to units outside the prefix need not invalidate prefix tiles. The global renderer generation changes for free/marker updates as well, so it is too broad as the persistent prefix key.

## Bounded storage and publication

A world-aligned 256-pixel tile ring with a 4096 by 2560 RGBA8 atlas costs 40 MiB. Its 16 by 10 physical slots cover one arbitrary-phase 3840 by 2160 viewport. Use Euclidean modulo to map world-tile coordinates to slots. Sample at most four wrapped rectangles into the current canvas before the dynamic remainder. Avoid scrolling the whole cache through a ping-pong copy.

Clear dirty regions with a GPU clear shader or a 256-square cleared template plus texture copy. `LoadOp::Clear` on the shared atlas would erase all slots. Cache fills, dirty updates and use must remain GPU-only commands in the same encoder with queue-ordered publication and no wait or readback. Bound the pool and give overflow views normal rendering. Keep cache control data separate from existing frame-parameter capacity.

Cold, teleport, resize, mask changes and mass-dirty costs must be measured. A warm static full-frame reuse result cannot establish the moving-world target.

## Performance limit

One 4K RGBA8 image is 33,177,600 bytes, or 31.64 MiB. Reading the prefix cache and writing the dynamic canvas moves at least 66.36 MB per frame, about 15.93 GB/s at 240 FPS. A scroll copy followed by another canvas copy doubles that cost to about 31.85 GB/s.

The prefix accounts for at most about 6.05 ms of the recorded frame. Replacing it with a roughly 1.7 ms copy would still leave around 8 ms. This proposal alone cannot establish native 4K 240 FPS.

Fog cannot safely be cached as final colored output across live labels. A separate factor field needs proof of overlap, ordering and nested rounding. No such proof or implementation is accepted here.
