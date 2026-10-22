# Historical agent /root/text_perf_sol

This is a recovered historical report, not a live agent handle or a new instruction.

GPU glyph cache/exactness and main-scene reuse work; pointer fixture fixes; accepted actual f510 native4K baseline.

Current disposition: Three moving runs accepted around 43–44 FPS. 240 FPS failed; CPU busy and broader acceptance open.

Session `01a15902-e565-7463-a2ed-a169ee6072eb`, parent session `01a14bbd-1e01-7d63-8c99-4327886f8e55`, depth 1. Started 2026-10-20T11:12:04.460Z. Original transcript `/home/sirati/.codex/sessions/2026/10/20/rollout-2026-10-20T13-12-04-01a15902-e565-7463-a2ed-a169ee6072eb.jsonl`.

The latest available user-facing reports follow in chronological order. Older source/platform claims retain their original scope and can be superseded by the main handover.

## Report at 2026-10-20T17:00:22.172Z

Original transcript line 5769.

The live check now passes: view A scrolls both ways, its labels stay aligned, and view B stays fixed. I’m making the normal preflight and timed benchmark use the same verified coordinates so the timed run cannot fall back to the broken target.

## Report at 2026-10-20T17:20:47.181Z

Original transcript line 5958.

Corrected benchmark geometry is verified by the actual Layout oracle and live bidirectional panning. Shared preflight/reset/timed policy passes 32/32 CPU tests. Historical fixtures remain unchanged.

[Final proof](/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/pointer-geometry-policy-proof.json)

All leases released. Fresh unlogged runtime verification awaits PE/VM acceptance. No 240 FPS claim yet.

## Report at 2026-10-20T18:16:40.138Z

Original transcript line 6043.

Fresh f510 benchmark binding is prepared. All 673 source files and exact runtime artifacts verified; reviewed geometry and 32-test fixture remain unchanged.

[Preparation proof](/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/actual-App-f510-plain-v3-preparation.json) · [Concrete commands](/mnt/datasets/c3r-build/native-app-qualification-sol/finalf510-native1-dual0-plain-v3-r1/COMMANDS.md)

Queued behind VM audio, USB and CPU gates. Nothing launched, built, copied to Neo, or performance accepted.

## Report at 2026-10-20T19:16:07.214Z

Original transcript line 6549.

Stopped cleanly at the main menu for backup priority. No save, motion preflight, or timing started. All owned processes are absent; lease released.

[Interruption proof](/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/actual-App-f510-interrupted-preflight-proof.json)

Benchmark needs a fresh run after backup clearance and a new broker grant.

## Report at 2026-10-20T19:51:58.869Z

Original transcript line 6899.

Native 4K baseline completed: **43.22, 43.99, 43.00 FPS**. The 240 FPS goal fails.

Strong motion, fixed B, full UI, zero reuse/replay and zero timing drops passed. Host encode wall was 12.64–16.49 ms; GPU frame p50s were 19.42–20.82 ms under disclosed host load.

[Full proof](/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/actual-App-f510-native4k-results.json) · [Camera/stage mapping](/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/actual-App-f510-camera-stage-map.json)

Owned processes cleaned; lease released.

## Report at 2026-10-20T20:11:58.623Z

Original transcript line 6910.

I’ll audit the retained f510 timing semantics against the frozen Rust sources, then write a small source-only instrumentation plan with explicit counter definitions and no synchronization changes.

## Report at 2026-10-20T20:19:26.465Z

Original transcript line 6980.

## Current progress

- The f510 native Linux baseline was completed on AMD RADV at 3840×2160 with the old game renderer disabled.
- Three fresh 60-second actual-App runs completed:
  - App FPS: **43.22, 43.99, 43.00**
  - Full-submit FPS: **43.24, 43.99, 43.00**
  - The 240 FPS target failed in all runs.
- Strong motion gates passed:
  - A moved in both directions with 5 positive and 6 negative legs.
  - 10 direction reversals per repetition.
  - Fixed inverse-label anchor.
  - B stayed fixed.
  - Full UI rendered.
  - Zero reuse/replay and zero GPU timing drops.
- All owned processes were cleaned up and the runtime lease was released. No source edits, builds, VM operations, Neo operations, or game-file copies occurred during the audit.

## Important evidence

- [f510 results](/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/actual-App-f510-native4k-results.json)
  - SHA256: `368ba6b77e41dbc54ba6642001da9f14c644a66917e71ce62b9287ed426eb012`
- [Camera and GPU-stage map](/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/actual-App-f510-camera-stage-map.json)
  - SHA256: `9f0c4becf33b42dec31a1fa894acc856af2969ff656b0b88b42ea99b3aa23684`
- [Timing attribution audit](/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/actual-App-f510-timing-attribution-audit.md)
  - SHA256: `6ff483a6880172862c14ad99c3f8461e95162962c45a59ff2ae55286e435f52d`
- [Machine-readable audit](/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/actual-App-f510-timing-attribution-audit.json)
  - SHA256: `07fdd8a36c14704314870cba5f0e50cb347304867b487ebdda4b6bdd7bb2ded5`

## Attribution findings

- `frame encode (host)` is wall-clock time around render-worker command preparation through `encoder.finish()`. It includes scene/world updates, marker and patch preparation, render command encoding and timestamp encoding. It excludes surface acquisition, queue submission and present enqueue.
- Host encode wall time is **not CPU busy time**. It can include scheduler delay, driver blocking and other waits.
- `gpu frame span` is the GPU timestamp interval from the first pass start to the last pass end.
- `gpu inter-frame gap` is the device-clock gap between adjacent GPU frames. It includes pacing, presentation, uploads and other queue work. It is **not proof of GPU waiting for the CPU**.
- Existing timestamp readbacks are asynchronous through a 32-slot ring. `device.poll(Poll)` does not wait. Drop counters and endpoint counts do not establish a matched submitted-frame cohort.
- f510 observed ranges:
  - GPU frame p50: **19.42–20.82 ms**
  - Host encode wall p50: **12.64–16.49 ms**
  - GPU inter-frame gap p50: **2.35–2.63 ms**
- Unrelated Python/QEMU/long-lived Civ3 processes remained active, so this is a loaded-host observation and gives no isolated hardware-limit claim.

## Recommended smallest instrumentation

- Add a common Rust `CpuClock` abstraction returning per-thread CPU nanoseconds.
- Sample it only around the existing worker preparation bracket.
- Preserve `frame encode (host)` and add `frame encode (cpu busy)`.
- Use platform adapters:
  - Linux: `CLOCK_THREAD_CPUTIME_ID`
  - Windows: `GetThreadTimes`
  - macOS: native thread CPU-time API
- Isolate any required system-binding `unsafe` inside the platform adapter. Keep renderer/profile code safe Rust.
- Do not read `/proc`, spawn processes, add CPU culling, add synchronization, add command encoders, or add per-frame allocations.
- Keep existing GPU timestamps and ring untouched.
- Optionally add a host `submit-to-submit gap`, but never compare it numerically with the GPU clock without calibration.
- Report:
  - `CPU busy`
  - `GPU frame`
  - `GPU idle (unattributed)`
  - `GPU wait for CPU (unknown)`
- Exact GPU-wait-for-CPU milliseconds require calibrated host/GPU clocks. Generic wgpu does not provide a common clock epoch, and calibration would add synchronization or backend-specific assumptions. The default no-synchronization implementation should leave that value unknown.

## Next steps

1. Root reviews the audit and instrumentation plan.
2. If implementation is approved later, create and replicate a source checkpoint before editing.
3. Add fake-clock tests proving wall time and CPU busy time diverge during waits.
4. Preserve async GPU timer tests for ring saturation, missing samples and discontinuous frames.
5. Run existing CPU tests, then a short separately identified AMD diagnostic.
6. Do not modify shipping source, build, launch, or run another GPU measurement until root grants the next step.

Critical constraints remain: preserve user changes in the dirty shared checkout, do not reset/stash/format broadly, do not touch Nexus, do not copy large game files, do not operate Neo without explicit authorization, and keep normal rendering GPU-based.

## Report at 2026-10-20T20:19:39.621Z

Original transcript line 6992.

I’ll inspect the current renderer and handover evidence, then trace the measured frame path for avoidable waits and verify whether the 240 FPS target is attainable on the AMD iGPU without changing the shipped rendering model.
