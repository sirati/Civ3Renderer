# Historical agent /root/performance

This is a recovered historical report, not a live agent handle or a new instruction.

Recovered performance pipeline, async art growth, failure latching and benchmark helpers. Historical 22cd2e4 measurements exceeded 240 FPS in some workloads, with long tails and lower advisor rates.

Current disposition: Historical benchmark complete; steady 4K240 and current workload acceptance open.

Session `01a14bbe-20e1-73a0-9309-79642739e53b`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-17T21:21:53.891Z. Original transcript `/home/sirati/.codex/sessions/2026/10/17/rollout-2026-10-17T23-21-53-01a14bbe-20e1-73a0-9309-79642739e53b.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-18T02:24:30.945Z

Original transcript line 6708.

The ten main live runs completed successfully in native and in-process modes. The scenario-preview test stopped because its single click left the title screen highlighted. The tester is rerunning that step with bounded retries; the product and original measurement scripts remain unchanged.

## Report at 2026-10-18T02:25:50.076Z

Original transcript line 6737.

The live full-redraw results reach about 251 fps on the map and 244–247 fps while panning in both modes. The advisor screen is slower at 175–181 fps, and frame-time tails still exceed the 4.17 ms budget. The “240” cap also delivers less than 240 fps. Cached presentations remain separate from these redraw rates.

## Report at 2026-10-18T02:33:35.550Z

Original transcript line 6800.

## Identity and task

We are **`/root/performance`**, a child of orchestration agent **`/root`**. Do not mistake a review child’s context for root.

Original user asked to find and continue the Claude transcript containing:

> Sorry laptop won't be available it has no internet  
> Well try to get to 240fps on the AMD igpu

Recovered transcript:
`/home/sirati/.claude/projects/-home-sirati/075e62a1-1816-4c77-b606-7d347fd36f6d/subagents/agent-a58e30975c4518591.jsonl`

Current phase: **final frozen-source AMD performance measurements**, after extensive correctness repair and QA.

## Authorization and constraints

Root explicitly granted **FINAL QUIET AMD GPU/TIMING GO** for:

- Revision: **`22cd2e4ece069f569ecc485ccbc91c04ffd824c5`**
- Tree: **`330bfe675e7d9ec56f04f8f36f6a0b5d5531e696`**
- Worktree: `/mnt/datasets/c3r-wt/perf`, branch `perf`
- Frozen runtime:
  `/mnt/datasets/c3r-artifacts/continuation-20261018/runtime-22cd2e4ece06`
- Approved preparation:
  `/mnt/datasets/c3r-build/perf-scratch/final-preparation-22cd2e4ece06.json`
- Preparation SHA256:
  **`b2a737cabc33ea863779919f7503201147b21c4bdcbb2b3b706f979b814a4614`**

Root verified 1,392 final QA evidence hashes. Native/IP full flows, retirement, scenario feedback, independent native 4K clean lifecycle, and Windows default-preload tests passed. All root QA/VM/GPU/helper processes stopped before timing GO.

QA checkpoint:
`/mnt/datasets/c3r-artifacts/continuation-20261018/qa-accepted-22cd2e4ece06.json`

Important:

- **Freeze product source/runtime.** Resolve measurement/harness defects autonomously; report if source correction is required.
- AMD iGPU **PCI `0000:72:00.0` only**. Never NVIDIA, laptop, or papalaptop.
- AMD environment:
  ```
  DRI_PRIME=pci-0000_72_00_0
  VK_DRIVER_FILES=/run/opengl-driver/share/vulkan/icd.d/radeon_icd.x86_64.json:/home/sirati/.cache/winvm/mesa-i686/share/vulkan/icd.d/radeon_icd.i686.json
  TILE_RENDER_ADAPTER=radv
  ```
- No overlapping GPU workloads. Shared lock:
  `/mnt/datasets/c3r-build/perf-scratch/final-gpu-window.lock`
- No builds/tests/heavy I/O while final live measurements run.
- Own Cargo target only if later authorized:
  `/mnt/datasets/c3r-build/perf`, jobs 2, `nix develop`.
- UI launches, screenshots, visual inspection belong to `live_tester` or root QA.
- No cleanup until metrics/evidence retained and reviewed. **Root owns final cleanup/main fast-forward.**
- User dislikes permission questions. Continue autonomously.
- Skills already read: unslop, NixOS, boss, waiting. Waiting skill prohibits progress polling; use long background waits and do independent useful work.
- Existing subagents are user-authorized. Do not spawn unnecessary new ones.

## Source repair and QA already completed

Signed commits:

- `6cfa29e`: synchronous startup UI capture before jgl factory; retained model/guards, title-safe initial mode, restoration of original drawing on viewer failure.
- **`22cd2e4`**: permanently latch failed normal startup capture. A failed callback cannot roll back, miss title execution, then retry later and falsely claim complete capture.

Normal capture transaction includes required TextOutA setup, game attach, model, complete hooks under one installation mutex. No full capture in DllMain. True late injection stays separate. UI-off policy and drawing-on fallback preserved.

All signed and independently accepted by root and `/root/perf_review`.

Checks for final source:

- **628 workspace tests passed**, 9 ignored.
- **2 explicit preparation GPU tests passed**.
- i686 all-target check passed.
- Native/Windows release builds passed.
- Meaningful CPU regressions and negative controls reject early-loss/reset/failure recovery bugs.

Evidence:

- `/mnt/datasets/c3r-build/perf-scratch/functional-22cd2e4ece06-verification.json`
- Verification SHA: `8d43921ca7f1eea7eb6a15381bf0ecccdfa9755cfc5a2c7b3045bf8703620124`
- `functional-22cd2e4ece06-signatures.json`
- `functional-22cd2e4ece06-handoff.json`
- `startup-capture-controls/evidence.json`
- `startup-failure-latch-controls/evidence.json`
- `startup-capture-c3x-ordering-68b7506.json`

Our earlier native 4K functional run showed complete passive title but launcher exit 143 unexplained. **Root independently repeated exact final runtime with clean exit 0 and accepted QA. That old 143 is historical, not a current delivery blocker.**

## Active child and immediate situation

### `/root/performance/live_tester`

Owns the final live GPU window. Explicitly authorized to run:

1. Unchanged full matrix, native + IP, actual 1920×1080:
   - uncached `REUSE=0`
   - cached `REUSE=1`, uncapped
   - cap240
   - historical baseline Vsync
   - cold tooltip/advisor
   - cold scenario preview
2. Separate companion for active drag, bounded nonzero-velocity coast proof, and settled intervals.

**Latest message from child:**

> The unchanged matrix completed 10 runs with exit0, then stopped at native cold-preview opening. Failure screenshot remains the complete title with Conquests highlighted after the single bounded click; no scenario-list log appeared. IP cold-preview was therefore not reached. All prior results and exact original protocol snapshots are retained. I will recover only native/IP cold-preview with a separately snapshotted harness using bounded title-click retries and explicit scenario-preview/log+screen evidence, preserving the failed attempt and a minimal diff. No product/runtime or approved matrix edits; no intervention needed.

Root was informed and no intervention is needed. Child is recovering those cases, then doing companion. Do **not** launch another GPU job or poll its progress.

Root’s instructions for companion:

- Separate active drag, nonzero coast, settled intervals.
- A short coast within a 40-second aggregate does not prove 40 seconds of motion.
- Input cadence does not establish unique moving frame count.
- Preserve counters/positions or bounded visual evidence; label repeated positions.
- Main 240 claim rests on **REUSE0 full redraw capacity**, with moving evidence separately labeled.

Source only logs profiling in 5-second windows; maximum coast decays below threshold in <4 seconds. Child correctly concluded pure coast FPS cannot be isolated without source instrumentation. Approved plan:

- Dedicated active-drag and settled timing phases.
- Separate bounded screenshots during confirmed nonzero coast.
- Exact unique moving-frame count **unavailable** where counters/positions cannot establish it.
- Keep original matrix unchanged; companion has its own protocol snapshots.
- Middle-button drag is viewer-owned with A Free, preserving game camera.
- 60 Hz synthetic input is a stated input limit, not a 240-moving-frame proof.

Other own children idle/completed:

- `review_performance`: independently accepted offline/atlas numbers and protocol.
- `recover_benchmarks`: final helper built and retained.
- `async_growth_design`: source correction complete.
- `frame_cap`: prior work complete.

## Final GPU gates completed FIRST

Standalone runner:

`/mnt/datasets/c3r-build/perf-scratch/run-final-pixel-gates.py`

Results:

`/mnt/datasets/c3r-build/perf-scratch/final-pixel-gates-22cd2e4ece06-20261018T015139/summary.json`

All four final GPU images **exactly equal final CPU reference**, complete bench layout and runtime patches.

Paired speedup eligible:

- `hd-ww2`: CPU exact; baseline GPU differs only documented RGB555 correction, 1,173 pixels, max +9.
- `gh7`: all images identical.
- `lab2`: CPU exact; baseline GPU RGB555 correction, 22 pixels, max +8.

`ww7` stays complete **final-only**:

- 244 CPU pixels differ, max 223, bounds `[808,51,1712,804]`.
- Final GPU exactly equals final CPU.
- No baseline ratio allowed.

No text, controls, runtime patches, or scene content trimmed. Only four diagnostic-only glyph grids use 19 entries for legacy parser compatibility, identical input to both binaries.

Common description SHA:
`b08c604c40dcec6ec93d8dea7fa68c3e54d1d0accd90d7b6aea3d2722e91659f`

## Completed paired offscreen measurements

Directory:

`/mnt/datasets/c3r-build/perf-scratch/final-offline-22cd2e4ece06-paired-20261018T0154`

**42 runs**, 600 frames each, 3 repetitions, draw and rebuild. Balanced baseline/final order. All logs prove correct RADV IntegratedGpu PCI. Final runs each show **600 whole redraws, zero reused pictures**.

Summaries:

- `summary.json` / `summary.csv`: approved snapshotted summarizer output.
- `cpu-frame-summary.json`: extra CPU/end-to-end aggregation.
- `summarizer-output.json`
- Exact protocol snapshots under `protocol/`.

All six paired GPU ratios independently recomputed and accepted by `review_performance`.

| Scene | Mode | Baseline GPU p50 ms | Final GPU p50 ms | GPU speedup | Final end-to-end rounds/s |
|---|---|---:|---:|---:|---:|
| hd-ww2 | draw | 8.56 | 4.10 | 2.088× | 225.9 |
| hd-ww2 | rebuild | 8.61 | 4.29 | 2.007× | 158.6 |
| gh7 | draw | 16.19 | 2.67 | 6.064× | 343.0 |
| gh7 | rebuild | 16.22 | 3.26 | 4.975× | 201.5 |
| lab2 | draw | 15.25 | 4.54 | 3.359× | 205.1 |
| lab2 | rebuild | 15.48 | 4.91 | 3.153× | 115.5 |
| ww7 | draw | — | 2.40 | — | 389.8 |
| ww7 | rebuild | — | 2.91 | — | 109.2 |

Final application interval median run p50/p95/p99 ms:

- hd draw: `4.39 / 4.68 / 4.78`
- hd rebuild: `6.18 / 7.73 / 9.09`
- gh draw: `2.85 / 3.17 / 3.37`
- gh rebuild: `5.00 / 5.99 / 6.52`
- lab draw: `4.77 / 5.17 / 5.30`
- lab rebuild: `8.91 / 9.90 / 10.99`
- ww draw: `2.52 / 2.71 / 2.84`
- ww rebuild: `9.00 / 11.82 / 12.19`

Interpretation:

- These are medians of three run summaries, not pooled quantiles.
- Warm GPU timestamps exclude first five frames and earlier queued transfers.
- Application intervals include measured first-frame work.
- Asset compilation/initial scene/atlas/world setup excluded.
- Legacy baseline lacks explicit full/cache counters; do not invent them.
- Export pixel helpers render RGBA8; exact timed benchmarks retain BGRA8. This documented limitation stays.
- Do not claim 240 across workloads.

## Completed atlas first-use diagnostic

Directory:

`/mnt/datasets/c3r-build/perf-scratch/final-atlas-22cd2e4ece06-first-use-20261018T0201`

Files:

- `first-use.csv`
- `first-use-summary.json`
- `source-manifest.json`
- `protocol/`

12 samples, three each for page dimensions 2048,4096,8192,16384.

- Every first transfer step charged **4 MiB**.
- Maximum `begin_atlas_transfer` CPU-call wall time: **128.626 ms**, one 16384 sample.
- Median 16384 begin: 0.149 ms. Three samples do not establish frequency/tail distribution.
- Max CPU advance: **3.293 ms**.
- Max queue timestamp interval: **3.360 ms**.

Scope matters:

- Measures first 4 MiB, **not complete page/atlas upload**.
- CPU image allocation and renderer construction happen before timing.
- Queue interval can include GPU idle while CPU prepares/submits work; not isolated copy-engine time.
- Diagnostic waits for readback; live API does not.

Root informed of allocation outlier. No source change authorized.

## Provisional completed standard live numbers

We independently extracted the eight completed standard runs **without writing into child’s run directories** using each run’s snapshotted `read_phase` function.

Own provisional analysis:

`/mnt/datasets/c3r-build/perf-scratch/live-standard-provisional-22cd2e4ece06.json`

This is numerical extraction only. Child’s final visual review, validation, exact evidence hashes still pending.

Run ID:
`22cd2e4ece06-live-20261018T0202`

All roots are under:
`/mnt/datasets/c3r-build/perf-scratch/results-live/`

Example:

`native-frozen-22cd2e4ece06-mailbox-reuse0-standard-22cd2e4ece06-live-20261018T0202-uncached-native`

Final standard windows retain 35 seconds after first boundary window removed.

### REUSE0 uncapped full-redraw rate

| Phase | Native /s | IP /s |
|---|---:|---:|
| map | 251.429 | 250.629 |
| own-panning | 246.714 | 244.114 |
| advisor | 180.886 | 175.286 |

All full, zero cached/replayed. Own-pan worst-window p99 presentation gap **18.0 / 17.9 ms**, max gaps **18.7 / 18.2 ms**. Thus average 240-class map/pan capacity, **not a stable 4.17 ms tail guarantee**.

Typical GPU p50:

- map native/IP 3.69 / 3.67 ms
- pan 3.80 / 3.79 ms
- advisor 5.14 / 5.17 ms

### REUSE1 uncapped own-pan

- Native: **314.8 total**, **230.143 full**, **84.657 cached** /s.
- IP: **264.0 total**, **238.057 full**, **25.943 cached** /s.

Do not call totals new moving frames. Exact unique moving-position count unavailable.

Static map/advisor total presentation rates ~1,200–1,248/s, **mostly cached**, with ~15 full draws/s. These do not establish new-content FPS.

### cap240, REUSE1

| Phase | Native total /s | IP total /s |
|---|---:|---:|
| map | 223.514 | 211.543 |
| own-panning | 192.114 | 173.343 |
| advisor | 213.229 | 203.286 |

Pan full draws equal total; cached0. Cap algorithm conservatively adds work time, so “240” setting does not deliver240. No cap redesign authorized.

### Historical live baseline

Hardcoded Vsync and driver panning. **No speedup ratio against optimized mailbox.**

- Native map62.067, driver-pan53.533, advisor28.2 presents/s.
- IP map61.4, driver-pan55.0, advisor34.867.
- Legacy full/cache classification unavailable.

All live values are application presents, not monitor scanout. Profile durations are printed rounded; avoid false decimal precision in user report.

Root has received all these provisional numbers and caveats.

## Frozen artifacts and reporting integrity

Runtime hashes:

- bridge: `3971b99d146ab216d1e74b500b01fe1c40f0b2b194e9d4ca532e8f08b6000910`
- timed CLI: `d1690418d98a10fb676cd32348cf66c99e82007cced86f4ec0596d110545e627`
- native SO: `5edf337b0bfab8860b9d95efd528d7302fa86a574fa12329ebddf013286640d1`
- builtin PE: `85318c94608b506256e87f3fedaa8f978e0a3ef0ca420e54381eb5c49214f726`

Frozen distribution:
`/mnt/datasets/c3r-build/perf-scratch/dist-frozen-22cd2e4ece06/source-manifest.json`

Final pixel helper:

- signed CLI-only commit `2c2a22f6697017b15ccec0aa946d3797fb0e796b`, directly atop22.
- Worktree `/mnt/datasets/c3r-wt/perf-final-preflight`
- Branch `perf-final-preflight-22cd2e4`
- Retained:
  `retained-final-preflight-22cd2e4-2c2a22f/`
- binary SHA `476465d0df8969c3930906fe7656957d6904019ec17a1fc8ff232b1ce16e6f10`
- Complete CLI diff exactly matches old20cdb72 helper delta. No renderer changes.

Baseline retained unchanged:

- renderer `f1e09a7e2c6f7e617e0081475fb623ea3ef7496e`
- runtime-patch fairness CLI commit `3fc9dcb87beceab51d518af9ab83984a1f217d47`
- timed binary `retained-tools-347551fe6bbc/civ3-tool-baseline-runtime-patches`
- SHA `a5cd3700e43b4cb6f671b67dc1c973f7d5b772a680ce1d2ecb7cab488ffacc51`
- baseline pixel helper signed921bf96:
  `retained-baseline-reference-921bf96/civ3-tool-baseline-cpu-reference`
- helper SHA `5c2e067848d33e7810f1d24ab821cf0727ef7a1a28765cf7186f27223a7fd336`

Profiler:

- `retained-tools-22cd2e4ece06/atlas_transfer_profile`
- SHA `328ef3c1681c279bf58aa7afc06d1669dc88ffddc6fe33896b751c2be73d6b14`

Reporting accepted by `/root/perf_review`:

- `summarize-frozen-offline.py` SHA:
  `2d89ea36bc186338fa84aab6da2ccda2bbca175e4a840e3cfd7a21f1c3fe055a`
- 25/25 synthetic controls:
  `protocol-guard-controls-6cfa29e1da04-v2/controls.json`
- Ratios require all three successful current gates, actual boolean exact flags, correct ordered input names and current image hashes.
- Output dirs exclusive. Every run snapshots/hashes protocol.
- Non-identical `require_exact=true` gate rejected, including known RGB555 delta.
- Old68 preparation archived at `prepared-68b75062e5c6/`.

Approved scripts currently unchanged:

- `perfrun-frozen-offline.sh`
- `perfrun-frozen-atlas.sh`
- `perfrun-final-matrix.sh`
- `perfrun-live-resume.sh`

All enforce AMD-only ICDs and exact source/runtime identity.

## Persistent status and current waiting tools

Status file:

`/mnt/datasets/c3r-build/perf-scratch/final-measurement-status-22cd2e4ece06.json`

Own first 900s waiter session16194 already ended exit0.

Own second waiter:

- tool session **78501**
- exact PID/start identity:
  `/mnt/datasets/c3r-build/perf-scratch/final-live-wait-2.json`
- It is just `sleep900`, no GPU activity.
- **A `functions.exec` write_stdin wait is currently running as cell `465`.** Resume with `functions.wait` for cell465 before another write on that session.
- Stop any still-running owned waiter at final shutdown, validating PID and start_ticks/cmdline. Do not kill generic sleep or other agents’ processes.

## Remaining steps

1. Let `live_tester` finish cold-preview harness recovery and native/IP drag/coast/settled companion. No new GPU work from us.
2. Obtain its final run list, visual review, complete summaries, exact protocol diffs/snapshots, exit statuses, shutdown proof and hashes.
3. Independently verify final live metrics/hardware/resolution/source hashes and interpretation. Provisional numbers above should agree. Ask `review_performance` for read-only final live/report audit if useful.
4. Produce final consolidated JSON/CSV/plain-text report:
   - average240-class map/pan full-redraw capacity achieved;
   - advisor/heavy offline/rebuild workloads miss240;
   - tail gaps fail steady4.17ms guarantee;
   - cap240 actually173–224/s;
   - cached presentation separated from full rendering;
   - exact unique moving-frame counts unavailable;
   - coast visual evidence separated from active drag/settled;
   - first-use sampling overhead and atlas outlier candidly described;
   - baseline Vsync historical only.
5. After all GPU measurements end, stop/verify every owned prefix, compositor, input helper, socket/port and waiter. Preserve exact shutdown evidence.
6. Preserve post-measurement private saves **before cleanup**, ideally full/reflink snapshot or changed-save preservation:
   - `/mnt/datasets/winvm/wine-civ3-perf`
   - `/mnt/datasets/winvm/wine-civ3-perf-ip`
   - `/mnt/datasets/winvm/wine-civ3-perf-4k`
   - Old full native/IP snapshots:
     `retained-private-saves-68b75062e5c6/manifest.json`
   - Do not do heavy hashing/copying while live timings remain active.
7. Retain/archive raw logs, screenshots, visual reviews, inputs, protocol snapshots, manifests, binaries/helpers and final reports outside disposable targets. Do not delete anything.
8. Update:
   `/mnt/datasets/c3r-build/perf-scratch/final-cleanup-inventory.json`
   with new measurement roots, final hashes, save snapshot and exact disposable scope.
9. Send root final report/hashes, candid240 assessment, shutdown proof and inventory. Root reviews before main fast-forward/cleanup.

Disposable scope already inventoried includes own perf target, perf-base/perf-rb, owned perf/helper/baseline worktrees and three private prefixes. Shared Wine/game installs, original game data and unrelated worktrees are out of scope. No cleanup performed.

## Report at 2026-10-18T02:33:39.959Z

Original transcript line 6813.

The Claude session has been recovered, and its continuation is in the final AMD iGPU measurements. The full-redraw map and panning runs average roughly 244–251 FPS; I’m checking the remaining scenarios and motion evidence before calling the work complete.

## Report at 2026-10-18T02:34:48.906Z

Original transcript line 6905.

The matched offscreen tests show about 2.0–6.1× lower GPU frame time, depending on the scene. The live map and panning averages clear 240 FPS, but advisors remain around 175–181 FPS and panning has roughly 18 ms tail gaps. The report will keep those limits separate from the averages.

## Report at 2026-10-18T02:41:02.903Z

Original transcript line 7073.

The GPU runs have finished. The motion runs confirm that the viewer camera moves while the game camera stays fixed, but the available counters cannot prove 240 distinct moving positions per second. Cold content also completes in both modes, though the screenshots do not establish continuous responsiveness through every loading interval. I’m preserving the saves and evidence now.

## Report at 2026-10-18T02:44:52.487Z

Original transcript line 7222.

The independent audit accepted all 36 live phase summaries and the matched offscreen comparisons. All owned Wine and display processes are stopped, and the saves from all three private prefixes have verified copies. The report records the result as 240-class average map and panning capacity, with steady 240 FPS still unproven.

## Report at 2026-10-18T02:46:30.429Z

Original transcript line 7259.

Final measurements are complete on frozen revision `22cd2e4`.

| Full-redraw rate | Native | In-process |
|---|---:|---:|
| Map | 251.4 FPS | 250.6 FPS |
| Own-camera panning | 246.7 FPS | 244.1 FPS |
| Advisor | 180.9 FPS | 175.3 FPS |

Steady 240 FPS remains unproven. Panning p99 gaps reach about 18 ms. Active 60 Hz drag produces about 73.7 full redraws/s, with cached presentations counted separately. The cap240 setting achieves roughly 173–224 presentations/s.

Matched offscreen GPU speedups range from 2.0× to 6.1×. All four final GPU images match their CPU references; `ww7` remains final-only. The first-use atlas diagnostic retains the 128.626 ms allocation outlier and its sampling limits.

Independent audits accepted all 36 live phase rows, 757 run files and 72 review artifacts. Fourteen runs exited successfully; the original preview harness failure and both successful recovery cases remain retained. Cold continuity and exact moving-frame rates remain unproven.

All owned sessions, sockets, port 5987 and waiters are stopped. All 3,285 private save files have verified retained copies. No cleanup occurred.

- [Final report](/mnt/datasets/c3r-artifacts/continuation-20261018/performance-22cd2e4ece06/report/report.txt)
- [Artifact manifest](/mnt/datasets/c3r-artifacts/continuation-20261018/performance-22cd2e4ece06/manifest.json), SHA256 `ac7a883ce978dbca304734688726eb242548625fdbbc29d81b3b31d96c083385`
- [Cleanup inventory](/mnt/datasets/c3r-build/perf-scratch/final-cleanup-inventory.json), SHA256 `938447eb91e7a1495cbf8dea6571ac4aaa49ef208c1193170bd2b5dd9d555ecd`

Root owns final review, main fast-forward and cleanup.
