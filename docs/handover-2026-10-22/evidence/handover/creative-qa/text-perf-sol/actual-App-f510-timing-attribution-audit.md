# f510 timing attribution audit and smallest safe instrumentation plan

## Audit result

The accepted f510 run proves actual App presentation and full-submit rates, native AMD/RADV use, strong bidirectional A motion, fixed B, inverse-label anchoring, full UI, and zero reuse/replay. It does not identify CPU busy time or prove that the GPU idle gap was CPU waiting.

`frame encode (host)` is measured in `tile-viewer/src/render_worker.rs` from the `Instant` immediately before target selection and world/marker/patch updates through `encoder.finish()`. The interval includes conditional world replacement or dirty/free uploads, marker and patch preparation, layered or cached-picture command recording, timestamp encoding, optional bitmap readback command encoding, and encoder finalization. It excludes surface acquisition, queue submission, and present enqueue, which have separate host spans. It is a wall interval on the render-worker thread. It includes time while the thread is runnable, scheduler-delayed, or blocked inside a driver call. It is not CPU busy milliseconds.

The existing GPU timestamp path is asynchronous. It records timestamp pairs around render passes, resolves them into a 32-slot readback ring, maps results later, and uses `device.poll(Poll)` without waiting. `gpu frame span` is the GPU device-clock interval from the first pass start to the last pass end. `gpu inter-frame gap` is the current frame's first GPU timestamp minus the previous frame's last timestamp when frame IDs and timestamp periods are adjacent. The frozen source explicitly says the gap includes pacing, presentation, uploads and other queue work and cannot isolate CPU waiting. It is therefore a GPU idle gap with unknown cause, not a CPU-wait measurement.

`timer_submitted_samples`, `timer_collected_samples`, and the zero drop counters describe asynchronous query accounting. They do not create a matched per-frame CPU/GPU cohort. The f510 run correctly reports this limit. The run's 240 FPS failure is independent of this attribution limit.

## Smallest Rust-only change for useful counters

Add a tiny `CpuClock` trait to the profiling/timing common API and sample it only around the existing worker preparation interval. Do not touch scene generation, culling, the submission protocol, or the GPU query ring.

- `CpuClock::now()` returns per-thread CPU nanoseconds for the calling render-worker thread.
- At the existing `prepare = Instant::now()` point, also record `cpu_before = CpuClock::now()`.
- At `worker_prepare = prepare.elapsed()`, record `cpu_busy = CpuClock::now() - cpu_before`.
- Add a second profile span named `frame encode (cpu busy)` beside the existing `frame encode (host)`. Keep the old host span unchanged.
- Optionally add `cpu busy (surface acquire)`, `cpu busy (queue submit)`, and `cpu busy (present enqueue)` only if the same clock wrapper makes those boundaries cheap. They are not needed for the first diagnostic pass.
- Emit numeric aggregate fields in the existing profile report and JSON. Preserve per-window samples and no cross-window averaging.

This gives the exact CPU time charged to the worker thread during the existing encode bracket. It excludes time the worker sleeps, waits for a mutex, waits on a driver or compositor, or is descheduled. It includes user and kernel CPU charged to that thread. It is a CPU-time counter, not total machine CPU utilization and not a promise that every CPU core was idle.

There is no safe, portable `std` API for per-thread CPU clocks. Keep the platform call behind the common API: Linux can use `CLOCK_THREAD_CPUTIME_ID`; Windows can use `GetThreadTimes`; macOS can use its thread CPU-time API. The small platform adapters may use the existing Rust `libc`/system bindings internally, while the renderer and profile code stays safe Rust. A no-unsafe shipping policy would require an external profiler and would not produce an in-app per-frame counter, so that tradeoff must be explicit. Do not read `/proc` or spawn a process per frame; those would contaminate the measurement.

## GPU frame and idle counters

Keep the current GPU timestamp query implementation and names:

- `gpu frame span`: GPU execution duration of one accepted timestamped frame.
- `gpu inter-frame gap`: device-clock idle interval between adjacent GPU frame stamps, with discontinuous/out-of-order samples excluded.
- `gpu timing drops`: the existing ring saturation, map, over-budget and discard counters.

Add no `queue.submit` wait and no `device.poll(Wait)`. The new CPU clock samples are host-only and do not synchronize with the GPU.

Add a host `submit-to-submit wall gap` only as a separate diagnostic span, measured from the end of one `queue.submit` call to the start of the next. This helps explain cadence, but it remains a host interval and must not be subtracted from a device-clock gap. Keep `surface acquire (host)`, `queue submit (host)` and `present enqueue (host)` separate.

## What can and cannot be called GPU wait-for-CPU

With generic wgpu timestamps, the host `Instant` clock and the GPU timestamp clock have no common epoch. A GPU gap and a host submit gap cannot be directly compared to derive an exact CPU wait duration. The safe default report should therefore expose:

- CPU busy ms, per worker frame/profile window.
- GPU frame ms, per timestamped frame/profile window.
- GPU idle gap ms, the existing inter-frame gap, explicitly labelled `unattributed`.
- GPU wait-for-CPU ms, `unknown`/`not calibrated`.

An optional backend-specific timestamp-calibration mode could report a bounded CPU-wait estimate, but calibration reads synchronized host/device clocks and would add backend-specific synchronization or assumptions. It should not be part of the no-synchronization default. If later enabled, it must be opt-in, report the calibration error bound, and keep the uncalibrated gap alongside it.

A CPU-wait candidate can be discussed only when the following independent observations line up: the GPU idle gap is present, the worker's CPU busy/host submission record shows no command submitted during that interval, and no surface/present or upload event explains it. That is a diagnostic classification, not an exact millisecond attribution.

## UI/report shape

Keep the current detailed FPS view and add a compact diagnostic mode with these labels:

`CPU busy`, `GPU frame`, `GPU idle (unattributed)`, `GPU wait for CPU (unknown)`.

Use p50/p95/p99/max per complete report window. Do not sum view p50s, average quantiles from different windows, or report GPU gap as CPU wait. Keep the vN camera-stage mapping separate from the frame-level counters; the f510 supplement already preserves v0 through v5 per-window stages.

## Validation plan before any source adoption

1. Add unit tests for a fake `CpuClock` that advances CPU time independently from wall time, including a blocked/sleep interval where host duration grows but CPU busy does not.
2. Add tests that asynchronous GPU results remain per-frame, ring saturation remains a drop rather than a wait, and missing/discontinuous stamps do not form a gap.
3. Run the existing CPU fixture suite and renderer/tile-viewer tests with the new profile fields; no GPU timing claim follows from CPU tests.
4. On AMD RADV, run a short diagnostic fixture with the same native 4K workload and compare `frame encode (host)` versus `frame encode (cpu busy)`, `gpu frame span`, `gpu idle gap`, and submit/present spans. Keep this run separate from the accepted f510 baseline and disclose host load.
5. Only after those checks should the detailed counter be promoted to the UI. The first shipping implementation should not add GPU/CPU synchronization, CPU culling, per-frame allocations, procfs reads, or extra command encoders.

## Source and evidence binding

Frozen source files reviewed:

- `crates/tile-viewer/src/render_worker.rs`, host spans at the acquire/prepare/submit/present boundaries and asynchronous `device.poll(Poll)`/query collection.
- `crates/tile-viewer/src/profile.rs`, wall-time round/span aggregation and per-window quantiles.
- `crates/tile-render/src/timing.rs`, timestamp queries, 32-slot asynchronous readback ring, frame spans, gaps and drop counters.

The accepted f510 result remains `/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/actual-App-f510-native4k-results.json`. The vN camera-stage supplement remains `/mnt/datasets/c3r-build/handover/creative-qa/text-perf-sol/actual-App-f510-camera-stage-map.json`. No source edit, build, runtime launch, VM operation or GPU timing was performed for this audit.
