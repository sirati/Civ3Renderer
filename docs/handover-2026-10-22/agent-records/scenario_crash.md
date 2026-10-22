# Historical agent /root/scenario_crash

This is a recovered historical report, not a live agent handle or a new instruction.

Atlas-generation/readiness/cancellation fixes plus independent wider-candidate orchestration reviews.

Current disposition: Targeted source work recorded; final wider review clears preparation only, not compiler/GPU/runtime acceptance.

Session `01a1581c-745a-7a40-b5a5-f9ad067f8654`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-20T07:00:22.236Z. Original transcript `/home/sirati/.codex/sessions/2026/10/20/rollout-2026-10-20T09-00-22-01a1581c-745a-7a40-b5a5-f9ad067f8654.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-20T13:46:39.151Z

Original transcript line 3495.

## Identity and current task

We are subagent `/root/scenario_crash`, parent `/root`, shared repo `/home/sirati/devel/rust/civ3-renderer`.

Current authorized task: implement an **isolated native1x per-pass overhang optimization prototype based on immutable3be**, preserving original shader cell/layer design and all cameras/input mappings. Need:

1. Cached geometry metadata only on world/LUT/set/atlas descriptor identity changes, never renderer global generation/free/marker updates.
2. Preserve existing global dynamic offset reach and all bindings.
3. CPU geometry enumeration.
4. Exact whole-target GPU paired oracle before adoption.
5. Report counts/cost before seeking performance lease.
6. No changes to live tree, physical install, or immutable3be.
7. GPU must have explicit broker lease; **currently waiting for one**. No FPS timing during Intel/VM load.

## Current prototype state

Everything under `/mnt/datasets/c3r-build/filler-cameras-sol/`.

- Immutable base:
  `/mnt/datasets/c3r-build/release-1019/src-3be94d8ba5388f1759cf425d28c771631a71cef336351b3df032afc86018764c`
- Owned source snapshot: `filler-cameras-sol/src`
- Owned Cargo target: `filler-cameras-sol/target`
- Binary: `target/release/filler240`
- No production edits. No active build/GPU process.
- Shader unchanged, verified identical canonical3be SHA256:
  `f9968edd2867ca9857731cd0b1c09d2172cb019e5523065b942507b249d7dc75`
- Canonical precompiled shader:
  `/mnt/datasets/c3r-build/release-1019/artifacts/3be94d8ba5388f1759cf425d28c771631a71cef336351b3df032afc86018764c/shader.spv`

Artifacts:

- `status.json`
- `renderer-only.patch` — focused host renderer/helper change, preferable for review.
- `candidate.patch` — also includes probe binary/Cargo changes.
- `source-hashes.json`
- `binary-sha256.txt`
- `real-art-inventory.json`
- `geometry-tests2.log`
- `build-oracle3.log`
- `oracle.py` — ready, **not run yet**.
- `findings.md` — earlier source investigation.
- `reach-candidate.rs` — earlier sketch, actual updated code is in owned snapshot.
- `prepare.py`, `integrate.py`, `prepare-oracle.py` are historical one-shot scripts; **do not rerun blindly**.

### Implementation

New owned source module:
`src/crates/tile-render/src/overhang_experiment.rs`.

Per-pass certificate unions **full placed sprite rectangles for every animation frame of every LUT entry** plus layer static offset. No alpha/palette assumptions. First implementation only affects native zoom1/1, downscale1, nonstacked world cameras. Unsupported or missing metadata falls back unchanged.

For extent `[l,t,r,b)`, grid step `sx,sy`, column step `C=sx` orthogonal or `2sx` staggered, maximum stagger row x-shift `H=0` or `sx`, certificate:

```
(max(0,-l), max(0,-t), max(0,r+H-C), max(0,b-sy))
```

Existing `Camera::frame_params` already extends left by `C`, top by `sy`. Intersect each certificate component with original camera overhang, then let existing frame_params add **unchanged global dynamic reach**.

Only cell candidate `first_col/first_row/cols/rows` change. All original camera coordinates, viewport, screen_shift, view index, cull, bindings, palettes, free/glyph filtering, shader and draw ordering remain.

`WorldBuffers` owns a small `Cache` with bounds, atlas geometry revision, build count/time. New world means fresh cache. Existing `same_shape` already compares all layer/LUT/set render metadata, so changed structural metadata calls `set_world`. A separate `TileRenderer.reach_revision` advances at actual sprite descriptor ownership/replacement sites, including prepared atlas adoption/transfer commits. Alpha/palette-only in-place updates do not change full geometry and need no invalidation. **Global `generation` is not used.** No GPU buffer retained by cache, no new GPU allocation/binding/wait/lock.

Runtime selector:

- baseline: `TILE_RENDER_PASS_REACH` **unset**
- candidate: `TILE_RENDER_PASS_REACH=1`
- force redraw: `TILE_RENDER_REUSE=0`

Probe exposes `pass_reach_build_stats()` for assertions.

### CPU proof completed

Three tests passed, no GPU executed:

- `complete_removed_geometry_enumeration_preserves_negative_wrap_stagger_and_order`
  - **399,535,488 rectangle checks**, **126,116 removed cells**.
  - Orthogonal/staggered, wrap/nonwrap, descending/ascending, negative/positive cameras, tiny strip viewports, all animation frames, layer offsets, all bounded per-cell offsets.
  - Includes bound slot offsets in global reach.
  - Tests retained cell sequence exactly equals filtered original sequence.
  - Byte-compares normalized FrameParams to prove **only four cell enumeration fields change**.
- `all_animation_frames_and_descriptor_revision_are_cached_without_generation_key`
  - 100 same-key updates do not rebuild; changed atlas revision does.
- `unsupported_geometry_and_missing_descriptors_keep_original_camera`
  - zoom/downscale/stacked/nonworld fallbacks and missing descriptor fallback.

Log `geometry-tests2.log`; first `geometry-tests.log` failed only due test fixture adding layers after slots, corrected.

### Real-art counts completed, before integration

Actual App retained trace:
`/mnt/datasets/c3r-build/native-app-qualification-sol/final866-native1-dual1-r1/untimed-loaded-trace.txt`

Actual six camera metadata from:
`.../untimed-pan-preflight.json`.

Geometry scan cost **44 μs once**. Global reach **[128,0,128,0]**, grid `[128,258,64,32]`.

- Main camera0: 122,892 → 110,780 instances, 9.86% fewer.
- Pane camera1: 122,892 → 110,564, 10.03% fewer.
- Large fillers2/3 each: **15,048 → 10,496**, **30.25% fewer**.
- Small fillers4/5 each: **5,928 → 3,321**, **43.98% fewer**.

Global reach does not erase the gain, so parent authorized continuing prototype.

## GPU oracle ready, not yet run

`/mnt/datasets/c3r-build/filler-cameras-sol/oracle.py`

Run only after explicit GPU grant:

```
nix develop -c python3 /mnt/datasets/c3r-build/filler-cameras-sol/oracle.py \
  > /mnt/datasets/c3r-build/filler-cameras-sol/oracle-run.log 2>&1
```

Uses AMD RADV headless, no compositor needed, no FPS claim.

17 scenarios × baseline/candidate × 3 redraw packets = **51 paired whole-target 4K frames**, 102 rendered frames. Raw targets saved under `oracle/`; compares all 3 packet outputs byte-for-byte and records SHA256s/result JSON incrementally.

Scenarios:

- Vertical/horizontal split, actual camera placement, origin placement, negative wrapped placement.
- View-specific/except-view sprite and glyph fixtures.
- Clean-map filtering.
- Atlas replacement.
- Zoom0.5/1.5/2 fallback arrangements.
- Pixel-scale2 and single-view arrangement.

Holds public layout unchanged. `FILLER_LAYOUT_SPEC=split=vertical|horizontal|off`. Fixture `FILLER_REACHABLE` supplies open reachable-control span so there are **four filler cameras**, as actual App. Actual vertical main sample uses post-pan `(9088,1800)`, starting main `(960,0)` → `(10048,1800)`, pane `(962,0)` → `(10050,1800)`. Main-derived screen_shift is adjusted by pan; pane shift stays0. These reproduce actual first six camera sampling fields.

Three packets call `update_free` and change invisible EMPTY markers to alter global generation. Oracle asserts metadata build count remains1. Atlas-replacement scenarios assert counts `[1,2,2]`. Baseline asserts0 builds. Images read back only for correctness, not normal production path.

**Caveat:** retained trace/public-layout replay includes126 glyphs, whereas live App packet may differ. It reproduces actual six world camera fields with reachable-span fixture but is not a serialized exact live packet. Report this honestly. No adoption/performance claim before parent review.

## Broker and scheduling

GPU/compile broker: `/root/mac_fps_music`.

Latest broker message:

> User-feedback fixes have priority. Overhang numeric gate remains reserved after active VM/userfix priority. Glyphskip seven cases ready, sampler/clipping builds pending. No timing active. Explicit GPU grant required.

We sent readiness:

- Immutable3be isolated binary ready.
- CPU tests/counts passed.
- Canonical shader SHA verified.
- No build/GPU process.
- Requested correctness-only102-render gate.

Broker acknowledged readiness but **has not granted GPU yet**. Do not poll for progress or start GPU based on elapsed time.

Compile constraint while Intel deploy/VM work: **`CARGO_BUILD_JOBS=1`**. Initially mistakenly used4 jobs, broker corrected; stopped own Cargo with SIGINT and resumed1. All subsequent builds/tests use1 and are complete.

## Why camera merge was rejected

Parent discovered actual App four2px filler cameras each cost~1.2–1.29ms, ~5ms total. Naively expand main/pane by half-gap and drop fillers is not exact:

- Fillers have their own renderer view IDs. View-specific overlays excluded from fillers would appear in expanded parent; except-parent sprites/glyphs currently visible in fillers would disappear.
- Fractional zoom origins reset at filler span start. Parent/filler lattice agrees only if offset*d divisible by zoom numerator, per axis.
- Extending camera origin can change wrapped free sprite copy selected by `wrap_distance`.
- Free/cell pass interleaving prevents moving filler free draws wholesale to end.
- Later UI overlapping small fillers is not proof of opacity, especially native alpha/256.

Overhang candidate avoids all these semantics.

## Prior completed task: dense GPU resolve, rejected

All under `/mnt/datasets/c3r-build/resolve-sol/`.

Built isolated canonical31b dense compute resolve, original instance order/count retained. 12 paired4K real-art captures byte-exact.

Three native4K WSI dynamic-pan A/B pairs,720 forced-whole frames per variant:

- baseline GPU p50/p95/p99 **12.2332 /12.494658 /12.62711ms**
- resolve **12.09416 /12.439822 /12.7314684ms**
- only1.14% median improvement, worse p99 and CPU encode/submit.

Focused per-pass diagnostic:

- Terrain mean3.351128→3.329841
- Detail49layers2.628655→2.424141
- Fog2.176981→2.149312
- Units20layers0.199559→0.124920
- Whole12.322031→12.256237
- Span minus summed render means0.044921→0.283306ms, rough added compute/barrier residual.

Root rejected production adoption and agreed no large compaction rewrite justified. Artifacts preserved; no active process. `resolve-sol/conclusion.md`, `compaction-plan.md`.

Future fragment/overdraw candidates must use **new union actualApp profile**, not old baseline. Root suggested exact predecoded base palette cache only as candidate, not authorization to speculate/implement.

## Other recent independent reviews completed

Parent diagnostics source reviews, no edits/build/GPU:

1. `timing.rs` overflow omission:
   Entire sample dropped on query exhaustion; reset/discard/pending/ring invariants approved.
   `/mnt/datasets/c3r-build/handover/timing-overflow-independent-review.sol.json`

2. TimerStats and ordered FAILED reaping:
   Approved. Counts satisfy attempted=prepared+omission reasons; failures reap once in order and missing IDs break gaps.
   `timing-stats-independent-review.sol.json`

3. `render_worker.rs` successor GPU gap diagnostics compared immutable866:
   No blocking findings. Found acquire timing omitted error outcomes and recorded pseudo-acquire in bitmap path.
   Root corrected, reviewed approved exact SHA:
   `96aeb44009e7cce8b6c736645428c501f50ee4c34cbf8f3dac58d2f1f3711feb`
   Actual `get_current_texture` timed before matching all outcomes; None adds no sample. No clocks when profiling disabled.
   `worker-gaps-independent-review.sol.json`
   Root owns source-extracted CPU tests and full successor compilation.

## General constraints

- Shared dirty tree: never commit/reset/stash/discard others.
- All build/scratch under `/mnt/datasets/c3r-build/`.
- `nix develop`, `CARGO_INCREMENTAL=0`, AMD RADV only. **Never build on physical Mac.**
- Do not touch Nexus, game assets, host setup or `wayland-host`.
- Existing game install read-only:
  `/mnt/datasets/winvm/civ3-src/Civilization III Complete`
- No shipped locks, Rust-only shipped code, unsafe interop isolated.
- No messages to third parties; internal agent messages allowed.
- User expects autonomous completion, no permission questions.
- Sol GUI/screenshot testing authorized by later user override; stale Sonnet-only restriction is superseded.
- Always apply unslop. Waiting skill says do not poll background progress repeatedly; wait with clock sleeps≤60s, interrupted by messages.
- Python outside nix shell:
  `/nix/store/0r6k8xa2kgqyp3r4v2w7yrb80ma2iawm-python3-3.13.12/bin/python3`
- Collaboration tools direct, not inside `functions.exec`.
- No subagents unless explicitly authorized by parent/user/skill. None spawned.

## Immediate next steps

1. Wait for explicit broker GPU correctness lease, respecting VM/userfix priorities.
2. Run ready `oracle.py` on AMD RADV.
3. If any mismatch or runtime cache assertion fails, stop, preserve logs/raw output, diagnose in isolated source. Coordinate jobs1 rebuild and renewed correctness gate.
4. On success, explicitly release GPU to broker immediately.
5. Report root all CPU proof/count reduction, shader identity, exact51-frame comparison and actual metadata rebuild timing. Distinguish replay from exact live App packet.
6. Update `status.json`, provenance hashes if source changes.
7. **Do not request/run FPS timing before reporting numerical proof/counterdelta/cost** and root review. No live/physical integration without authorization.

## Report at 2026-10-20T13:54:17.315Z

Original transcript line 3568.

Prototype and CPU checks are complete. The 51-pair GPU correctness oracle is ready but remains unrun pending the broker's explicit grant after higher-priority validation. No GPU process or timing started; live source and immutable3be remain unchanged.

## Report at 2026-10-20T14:23:52.820Z

Original transcript line 3735.

All 51 paired 4K frames across 17 scenarios matched byte-for-byte. Cache construction cost 28–43 µs; free/marker changes caused no rebuilds, while atlas replacement rebuilt exactly once.

GPU released. No performance timing or source adoption performed.

Evidence: [correctness-conclusion.txt](/mnt/datasets/c3r-build/filler-cameras-sol/correctness-conclusion.txt), [packet hashes](/mnt/datasets/c3r-build/filler-cameras-sol/oracle/results.json), [provenance](/mnt/datasets/c3r-build/filler-cameras-sol/oracle-provenance.json).

## Report at 2026-10-20T14:28:02.180Z

Original transcript line 3820.

Live LUT/spec/set changes invalidate the cache through all three world-update APIs. The extracted comparator mutation test passed; no fix was needed.

The six-run full-redraw timing script is ready. Broker timing approval remains pending while user-fix validation and host load take priority. No benchmark started.

## Report at 2026-10-20T15:30:34.982Z

Original transcript line 3872.

Timing remains queued. The broker reports active VM validation, GPU correctness checks, launcher/Wine checks, and compilation, so no quiet performance window exists.

The qualified binary, fixtures, and six-run timing script are preserved. No benchmark or production changes started.

## Report at 2026-10-20T16:10:54.726Z

Original transcript line 3949.

Timing completed and GPU released. Median GPU time improved 1.24%, from 8.694 to 8.586 ms; individual pair gains ranged from 0.30% to 1.57%.

Host contention and much cheaper reconstructed filler passes prevent an actual App performance claim. No adoption or shared edits.

[Timing evidence](/mnt/datasets/c3r-build/filler-cameras-sol/timing-conclusion.txt) and [distributions](/mnt/datasets/c3r-build/filler-cameras-sol/timing/summary.json).

## Report at 2026-10-20T18:39:02.094Z

Original transcript line 4062.

Review recorded in [handover](/mnt/datasets/c3r-build/handover/wider-gpu-orchestration-independent-review.sol.json).

Three blockers concern enforced source, shader, and build provenance. Current files match their recorded hashes, but the scripts do not enforce all advertised bindings. Corrections sent to `glyph_cache_sol`; original package preserved.

No build, GPU, VM, or shared edits performed.

## Report at 2026-10-20T18:55:10.483Z

Original transcript line 4125.

V2 resolves all three provenance blockers. Independent hash-anchored preflight passed; no blocking orchestration findings remain.

[Review report](/mnt/datasets/c3r-build/handover/wider-gpu-orchestration-v2-independent-review.sol.json). CPU build evidence still needs review before a separate GPU grant. No build, GPU, or VM execution performed.
