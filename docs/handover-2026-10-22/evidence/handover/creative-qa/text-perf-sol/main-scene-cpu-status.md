# Main-thread scene and metadata CPU work

The Neo main-thread sample had 60 of 78 observations under draw/prepare_frame. The retained 1-second/10-ms sample is `/mnt/datasets/c3r-build/perf240-sol/evidence/neo-live-3be/game-sample-1s-10ms.txt`. It showed full Scene/LayerCells copies, Compiled metadata copies and cell setter/dirty dispatch. This bounded sample identifies work to remove; it does not establish the steady-state CPU budget.

The patch incrementally extends future metadata, retains immutable committed metadata by Arc, copies only source cell deltas, and updates sanitized command snapshots without recopying unchanged world arrays. New descriptor prefixes recheck used cell mappings and restore affected layers when omitted art becomes valid. Pinned readers retain immutable data. A skipped predecessor causes a full source refresh.

Initial world cells now pack directly into allocated upload records. The full branch no longer clones a pinned old world before discarding it, and a clean shared world keeps its identity when Dirty::None is consumed. Real record changes and binding-only Dirty::Records([]) notifications retain their behavior. Full redraw/rebuild and worker ownership gates remain. No waits, locks, unsafe code, rendered-frame cache or quality change was added.

The CPU filters passed 13 in final-incremental-tests.log, 2 in scene-snapshot-final.log, 1 in shared-world-dirty-tests-complete.log, 7 in shown-scene-tests-final.log, 1 in bulk-cell-tests.log. These are separate scopes and repeat earlier tests. Bulk packing covers 64 combinations of record widths, phase/offset fields and input lengths, including missing data and zero replacement. Prefix tests compare incremental output with the full-sanitization oracle and cover pinned readers, newly available art, generation changes and skipped predecessors.

Each of five paired synthetic CPU repetitions performs 16 iterations. Median totals were:

| Probe | Before, ms | After, ms | Deterministic work change |
| --- | ---: | ---: | --- |
| future_metadata | 28.629570 | 1.259486 | 16 complete table copies to 1; 16 growths applied once |
| scene_snapshots_and_prefix | 77.433838 | 0.027260 | 25,362,432 full cell copies to 0; 32 changed-cell copies |
| initial_cell_packing | 387.279824 | 232.145884 | 1,056,768 packed bytes exactly equal per pair |
| clean_shared_world_snapshot | 0.374655 | 0.000570 | 16 pinned world copies to 0; bytes equal |

These are CPU metadata/packing probes, not release App FPS, GPU timing or 240 FPS acceptance. The optimized helper linked the cached dependency libraries recorded in the proof. The host remained contended, and each pair used fixed before/after order. Raw output, frozen source, helper bodies, executable and dependency hashes are bound by `main-scene-cpu-proof.json`. The three before helper bodies match installed3be exactly.

Evidence is `/mnt/datasets/c3r-build/main-scene-cpu-sol/{before,after,proof.json,cpu-probe-final-results.jsonl}`. The CPU test executable is retained as `tile-viewer-cpu-tests`; run named CPU filters only without a new GPU lease. The duplicated owned Cargo cache is removed after proof capture.

Optional-art asset/command generation changes still force full world reconstruction. Bulk packing reduces its cost but retains that trigger. The render-prefix certificate allows submitted worlds to own prior LUTs; it cannot alone certify interchangeable current world tables. A safe continuation needs record-format checks, table/font/set synchronization, sanitized-cell remapping invalidation and pinned-frame ownership tests. Requirements are in `/mnt/datasets/c3r-build/main-scene-cpu-sol/remaining-world-rebuild.md`.

No Neo, App, compositor, Wine or GPU test ran for this patch. The jobs1 CPU lease is released. Root must bind this code to a coherent successor, run affected GPU/runtime acceptance and measure sustained full-quality 4K work before a 240 FPS claim.
