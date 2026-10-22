# Deferred renderer work

The user's current priorities put sound and the 240 FPS frame target at P3, and six-bit/seven-bit pixel attribution at P4. These are backlog tasks and do not block the installed Neo release. Use [the unfinished-task briefs](subagents.md) when resuming them.

## Sound at P3


Checkpoint `ce5e104270258e207e16c3a8cf7a0ff5336c41d5` contains the runnable owner adapter and 12 passing offline tests. A later independent review found three gaps those tests miss:

1. Existing bridge-log truncation aborts observation.
2. `guest_probe.py` accepts title adoption as loaded-map readiness.
3. `vm_adapter.py` captures before checking readiness, so the image cannot prove post-adoption state.

Fix and review these before qualification. The retained package also needs a separately checked normal-Quit radio-selection calibration. The older orchestrator entry describing only a six-test draft is stale. See the [audio continuation brief](subagents.md#audio_loaded) and the [owner package](evidence/external/quit-audio-sol/loaded-owner-driver-sol/REVIEW.md). Do not treat the preparation instructions as runtime results.

The previous no-device Mal3 arm reached terrain/cities near its deadline, but full HUD/units, a fresh loaded audio trace and ordinary Quit were not established. The watchdog forced TERM after about 480 seconds. The owner restored the exact USB-null fixture and audio environment; root accepted that restoration. Vendor/title causal passes remain valid within their narrower scope.

## Frame target and attribution at P3


The accepted native 3840 by 2160 baseline uses internal and surface resolution 4K, pixel scale 1, full UI, Immediate presentation, no FPS cap, glyph cache enabled, Tiles555 enabled, and the original game renderer disabled. The retained baseline uses the three-pass subpixel path with `TILE_RENDER_SUBPIXEL_DUAL=0`.

| Run | Presentation FPS | Full-submit FPS | Reuse or replay | Timing drops |
| --- | ---: | ---: | --- | ---: |
| 1 | 43.2235 | 43.2384 | 0 | 0 |
| 2 | 43.9911 | 43.9919 | 0 | 0 |
| 3 | 42.9965 | 43.0014 | 0 | 0 |

Each run has 11 complete moving observation intervals, ten reversals and a fixed B view. Individual-window GPU frame p50s range from 19.42 to 20.82 ms; host encode wall p50s range from 12.64 to 16.49 ms. Unrelated host load remained present. These results establish a usable baseline, not the hardware's quiet-host limit or physical scanout rate. Zero drop counters do not turn endpoint counts into an exactly matched submitted-frame cohort.

The approximately 1.2 ms figures belong to individual thin views, including 2 by 720 and 2 by 96 viewports. They are not full-frame durations. Do not add independent quantiles to manufacture a frame total. The [camera and stage mapping](evidence/handover/creative-qa/text-perf-sol/actual-App-f510-camera-stage-map.json) retains the correspondence.

GPU culling already exists. Terrain rejects candidates before sprite-table resolution. Free-sprite/text shaders also cull before rasterization, but some rejection follows sprite resolution. The earlier blanket claim that every path culls before resolution was too broad. CPU culling was not added during the continuation.

| Candidate | Current scope | Remaining gate |
| --- | --- | --- |
| Thin-view diagnostic | Prepared against the exact 18-camera 4K workload, with unchanged shaders and attachments and controlled free/text/glyph metadata axes. | Review the package, reserve AMD GPU use, run the diagnostic and establish causal attribution. |
| Binary-cell blend bypass | Isolated recorded 4K cell-only gate passed 32 paired images and 265,420,800 RGBA pixels without a discrepancy. Free/UI/text/unit overlays were omitted. | Full-app correctness and matched timing; no production adoption or speedup is established. |
| Wider integer color path | Private source and resumable metadata discovery have reviewed preparation. Compiler/GPU qualification and integration are outstanding. | Check the actual build outputs, strict numerical oracle, retirement/continuation bounds and matched app workload. Keep it private until accepted. |
| Glyph preparation skip | Targeted proofs and adoption reports exist for specific earlier checkpoints. | Rebind source and shader identities and satisfy the outstanding numeric gate. Do not infer acceptance from cache-hit rates. |
| Sparse cells and terrain cache | Design/audit work is retained. Sparse metadata can preserve painter order, with invalidation and retirement risks. | Use current-app measurements to justify implementation; full correctness and adoption remain open. |
| Frame cap | Older audit found draw preparation starting after the presentation interval, adding draw time to the cap period. | Recheck current behavior before changing pacing. Uncapped throughput and capped shortfall need separate measurements. |

See the [performance continuation brief](subagents.md#performance_240), [binary-cell proof](evidence/handover/binary-cell-recorded4k-proof.sol.json), [thin-view plan](evidence/external/binary-cell-blend-sol/actualapp-f510/thin-view-diagnostic/thin-view-diagnostic-plan.json). The performance brief also covers whether sparse-cell work is justified.

## Six-bit and seven-bit pixel attribution at P4

The strict AMD `subpixel_coverage` failure remains recorded in [verification](verification.md). The private wider-integer implementation still needs compiler and numerical GPU qualification. Treat the diagnostic inventory switch as an inventory, not a passing strict oracle. See [the continuation brief](subagents.md#wider_color).
