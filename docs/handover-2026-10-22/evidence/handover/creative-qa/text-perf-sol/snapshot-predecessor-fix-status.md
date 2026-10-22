# Snapshot predecessor correctness fix

A rejected frame preparation can consume Display deltas and then restore an older presented scene. Re-adopting from that older scene with an empty or later-only delta left stale cells, phases, offsets, sprites, views and slots. The minimal fix records the starting sequence of accumulated Display changes. Adoption uses that certificate and falls back to complete source invalidation when its retained scene has a different predecessor.

Only `crates/scene/src/lib.rs` and `crates/tile-viewer/src/lib.rs` changed. The fallback sets world, sprites, views and slots and clears partial cell lists, so downstream world/prefix preparation also refreshes. Matching predecessors retain incremental flags. Pending snapshot handling, presentation-time Mapping activation, shaders and GPU ownership are unchanged.

The exact final four-test module has SHA256 `8df44b5730d8eb2c299d277798fd1bde13d3e46c5066a1684b2aad2025bdc8b1`. Against original b41 adoption helpers it produced one pass and three failures. Two failures are actual stale data. The pending/latest test has correct values under the old code but lacks the conservative complete-invalidation contract. With the fix, all four tests pass. The full Scene suite passes22, and the related viewer filters pass. All counts/log hashes and original-helper identities are in `snapshot-predecessor-fix-proof.json`.

The older `before-regressions.log` used an overly strict assertion on pending adoption itself. Its existing world=true already rebuilds slots. That assertion was corrected before the final identical-fixture rerun. Use `before-final-fixture-regressions.log` and `after-regressions-corrected.log` for the comparison.

The before extraction links the tested Scene library whose new private counter is unused by the old raw take_changes caller. The adoption helper bodies match frozen b41 exactly. This establishes original helper behavior; it is not an execution of the full b41 application.

Source hashes for the approved successor are:

- `crates/scene/src/lib.rs`: `c6a9aa47ff38e7e4615ee1246a1c223d4dad81af22d2bf0a7c7d903d59d862ac`.
- `crates/tile-viewer/src/lib.rs`: `04cd00a4d70a3ac25ba93835e7478c2f55610d54de72e750b9a0e4bc1a7a0db0`.

The root review is `/mnt/datasets/c3r-build/handover/sol-root-snapshot-predecessor-review.json`. Release was told to block b41 deployment and freeze/build a coherent successor with the unchanged shader pair after root approval. No shared-source edits remain planned.

These are CPU correctness checks, with no GPU, App, compositor or Neo activity. They do not prove a performance gain. Actual Mapping.capture pins shown_scene, so the earlier exclusive-helper zero-copy measurements must not be presented as actual App copy counts. Optional world-table retention remains an isolated candidate task.
