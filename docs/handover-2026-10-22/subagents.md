# Subagents for unfinished work

Only unfinished work is listed here. Completed and accepted agents are omitted. When the successor resumes one of these tasks, it must spawn a fresh subagent for that task using the brief below. Do not recreate agents for completed implementation, review, preparation or installation work.

Sound and the 240 FPS target are P3. Six-bit and seven-bit pixel attribution is P4. None of these tasks is a deployment gate. Reserve one owner for any VM GUI/input session and one owner for AMD measurements; do not start all backlog agents automatically.

| Priority | Unfinished task | Fresh subagent when resumed |
| --- | --- | --- |
| P1 | [Camera arrival and accelerated combat](#combat_validation) | `combat_validation` |
| P3 | [Sound and loaded-save audio follow-up](#audio_loaded) | `audio_loaded` |
| P3 | [CPU busy attribution](#cpu_timing) | `cpu_timing` |
| P3 | [Native 4K performance and candidate attribution](#performance_240) | `performance_240` |
| P4 | [Six-bit and seven-bit pixel attribution](#wider_color) | `wider_color` |

<a id="combat_validation"></a>

## Camera arrival and accelerated combat

When this unfinished task is resumed, spawn a fresh subagent named combat_validation. Respect its P1 priority.

Verify visible battles at 1x and 4x, camera arrival before animation, minimum keep-visible AI focus, army presentation and movement bonuses. Use matched initial state and compare RNG plus stable unit outcomes.

First action: Build the reviewed f510 combat diagnostic successor and run its pending 11-test gate. Prepare matched battles and the corrected offline comparator; check actual event provenance before interpreting timings.

Done when: Matched battles have equivalent RNG and stable unit results at both speeds; camera movement completes before visible combat begins; no per-tile recentering, hidden-unit disclosure, stuck arrival or unintended global/audio clock change appears.

Resources: Owned CPU diagnostic build, then one scheduled game/VM GUI owner. Match source/artifact identities with the parent.

Starting material:

- [README.md](evidence/external/combat-diagnostics-army/f510-outcomes-rebind/README.md)
- [verification-manifest.json](evidence/external/combat-diagnostics-army/f510-outcomes-rebind/verification-manifest.json)
- [proof.json](evidence/external/combat-diagnostics-army/offline-comparator-successor2/proof.json)

<a id="audio_loaded"></a>

## Sound and loaded-save audio follow-up

When this unfinished task is resumed, spawn a fresh subagent named audio_loaded. Respect its P3 priority.

Continue the unfinished loaded Mal3 no-device test and music/CPU soak at P3. Fix the owner-driver gaps before running that qualification. Preserve the eight-minute watchdog, identity-safe cleanup and exact audio-fixture restoration. This work does not block installation.

First action: Restore the ce5e104 private QA checkpoint. Fix bridge-log truncation, title adoption incorrectly satisfying loaded-map readiness, and capture occurring before readiness. Extend offline checks, then separately calibrate the normal-Quit radio selection before a new qualified arm.

Done when: Fresh post-adoption full-HUD/unit images and a current loaded native-audio trace establish the intended state; ordinary Quit produces a natural success exit without external signals or survivors. Restore the original audio/VM environment after every outcome. Music soak on physical Neo is recorded separately.

Resources: Sole VM GUI/input and audio-fixture owner; coordinate physical Neo checks with the parent. No VM operation while performance timing owns a quiet interval.

Starting material:

- [REVIEW.md](evidence/external/quit-audio-sol/loaded-owner-driver-sol/REVIEW.md)
- [README.md](README.md)

<a id="cpu_timing"></a>

## CPU busy attribution

When this unfinished task is resumed, spawn a fresh subagent named cpu_timing. Respect its P3 priority.

Add actual CPU busy measurement separately from host encode/build wall time. Preserve the existing counter design and GPU-gap meaning. The accepted GPU-worker and frame-ownership implementations are complete and are not reopened by this task.

First action: Locate the existing viewer timing counters and add a measured thread/process CPU-time source. Use a controlled check that distinguishes blocked wall time from CPU execution.

Done when: The CPU busy metric has a tested meaning and bounded overhead without changing the counter design or attributing all GPU gaps to CPU starvation.

Resources: Owned CPU build target; coordinate affected shared viewer files and any GPU stall test with the parent.

Starting material:

- [actual-App-f510-timing-attribution-audit.json](evidence/handover/creative-qa/text-perf-sol/actual-App-f510-timing-attribution-audit.json)
- [verification.md](verification.md)

<a id="performance_240"></a>

## Native 4K performance and candidate attribution

When this unfinished task is resumed, spawn a fresh subagent named performance_240. Respect its P3 priority.

Continue the unfinished 240 FPS native-4K target and controlled thin-view attribution on AMD RADV. Preserve the counter design and use the accepted f510 moving full-UI baseline. Qualify any optimization with matched correctness and timing. This P3 task is not a deployment gate.

First action: Inspect the accepted f510 baseline and thin-view plan. Bind the exact source, shader pair, 18-camera workload and attachments. Run controlled free/text/glyph-metadata axes under an exclusive AMD measurement reservation.

Done when: The actual full-UI workload meets the 240 FPS target in repeated sustained moving runs, with correct pixels and separately reported full submissions, cache reuse, presentation rates and timing loss. Host-load and frame-gap limits remain explicit.

Resources: Exclusive AMD GPU measurement; no competing builds, captures, hashing or VM activity during measured intervals.

Starting material:

- [sol-root-f510-native4k-baseline-review.json](evidence/handover/sol-root-f510-native4k-baseline-review.json)
- [actual-App-f510-native4k-results.json](evidence/handover/creative-qa/text-perf-sol/actual-App-f510-native4k-results.json)
- [actual-App-f510-camera-stage-map.json](evidence/handover/creative-qa/text-perf-sol/actual-App-f510-camera-stage-map.json)
- [thin-view-diagnostic-plan.json](evidence/external/binary-cell-blend-sol/actualapp-f510/thin-view-diagnostic/thin-view-diagnostic-plan.json)
- [binary-cell-recorded4k-proof.sol.json](evidence/handover/binary-cell-recorded4k-proof.sol.json)

<a id="wider_color"></a>

## Six-bit and seven-bit pixel attribution

When this unfinished task is resumed, spawn a fresh subagent named wider_color. Respect its P4 priority.

Resolve the strict wider-depth subpixel rounding failure and qualify the isolated wider-integer implementation. Keep RADV on its proven UNORM path until a replacement passes the numerical oracle. Do not broaden the Microsoft Basic Render Driver FP16 workaround.

First action: Reproduce bits=6, material=0, dst=0, x=5, y=0 returning [0,0,251,37] instead of [0,0,255,37] in both dual-source and fallback paths. Inspect the private wider compiler/qualification package before building it.

Done when: Strict six-bit and seven-bit tests pass while RGB444/RGB555, alpha, clipping, glyphs and ordinary rendering remain correct. Continuation/retirement bounds and worker behavior pass compiler and runtime checks. The inventory-only switch is never counted as a correctness pass.

Resources: Owned CPU target followed by a separately scheduled exclusive AMD GPU gate.

Starting material:

- [verification.md](verification.md)
- [cpu-gate-plan.json](evidence/external/wider-integer-sol/compiler-gate/cpu-gate-plan.json)
- [package-manifest.json](evidence/external/wider-integer-sol/gpu-qualification-v2/package-manifest.json)

The parent owns integration, acceptance, commits and off-host backups. Rebind any old runtime-owner labels through the reviewed ownership procedure; historical leases are not current authorization.
