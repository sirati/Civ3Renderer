# Civ III renderer handover

Prepared on 22 October 2026 for the user and the next maintainer. The implementation baseline is commit `8106ae5a4debbe247cb5e64523d4972474f4fe39`. This handover adds documentation to that baseline.

The renderer has substantial working implementations and several verified fixes. Full release acceptance is still open. The AMD iGPU has not reached 240 FPS at native 4K. The latest committed release is staged for Neo, while its last recorded installed app is older. Continue with the open work below and retain the existing counter design.

Read [the subagent inventory](subagents.md) for all 96 recovered Codex agents and the inherited Claude reports, [the acceptance checklist](acceptance.md) for all 85 acceptance families, and [the evidence index](evidence-index.md) for preserved reports and independent reviews. [handover.html](handover.html) is an offline reading copy with searchable agent and acceptance sections. [state.json](state.json) records the main identities and blockers.

## What is closed

These are completed implementations or specific checks. Each result retains its original platform, workload and source limits.

| Work | Completed result | Evidence |
| --- | --- | --- |
| Source recovery and integration | The recovered feature branches were integrated into main. The baseline commit preserves the exact 673 source files of the f510 freeze. The old `perf` and `zoom` branches are ancestors, not current optimization candidates. | [Source review](evidence/handover/sol-root-source-backups-review-20261020.json) |
| Source preservation | The main Git tree, private development snapshot and later QA checkpoint have verified off-host copies on Papa's laptop and Neo. Both hosts were reachable and reported the baseline main commit during this handover. | [Backup and restore](backup-and-restore.md) |
| GPU worker and frame ownership | GPU upload/draw/acquire work has a dedicated worker. Submitted UI mappings remain immutable. Small control data uses three slots; the world and atlas are not copied three times. | [GPU worker](evidence/handover/gpu-worker.sol.md), [synchronization](evidence/handover/renderer-sync.sol.md) |
| UI command transactions | UI publication commits commands and final layout together. Optional missing sprites do not hold an entire UI invisible. The earlier art-residency hold and 150 ms timeout were superseded. | [Final transaction design](evidence/handover/ui-commands-atomic.sol.md) |
| Counter semantics | CPU preparation wall time, GPU execution and inter-frame gaps have separate accounting. Timing/gap regressions passed. Keep the user's counter design. | [Timing handover](evidence/external/fps-details-sol/handover.md), [timing attribution](evidence/handover/creative-qa/text-perf-sol/actual-App-f510-timing-attribution-audit.md) |
| Native DX12 compatibility defect | The minimum workaround for the observed Microsoft Basic Render Driver uses a narrow FP16 quantized path. Native focused checks passed. It does not justify enabling that path on RADV or Metal. | [Native Windows report](evidence/handover/windows-vm.sol.md), [color investigation](evidence/handover/mac-fps-music.sol.md) |
| Audio causal controls | The original vendor no-device failure was reproduced. The corrected production title-screen no-audio normal Exit passed. Healthy-audio title and loaded Mal3 load/quit controls also passed historically. | [Audio report](evidence/handover/mac-audio.sol.md), [title review](evidence/handover/sol-root-f510-no-device-title-review.json) |
| Launcher cancellation | Real TERM after a failed worker returned 143; TERM with an active owned game stopped that game cleanly. Identity-scoped cleanup evidence is retained. | [Cancellation review](evidence/handover/sol-root-f510-cancellation-runtime-review.json) |
| VM desktop template | Prepared macOS desktop snapshots and verification clones were created and checked. This closes the template work within its recorded scope. | [VM handover](evidence/handover/vm-snapshot.sol.md) |
| Current performance baseline | Three qualified Linux full-UI, sustained-motion 4K runs completed. Root accepted the measurement evidence after independently checking it. All three failed the performance goal. | [Independent review](evidence/handover/sol-root-f510-native4k-baseline-review.json) |
| Latest viewer verification | On 21 October, `tile-viewer` recorded 495 passing unit tests, 4 ignored tests, and one passing host-status GPU test. No code changes remained from that turn. | [Verification record](verification.md) |

Many other feature implementations have targeted tests. Their current full-game acceptance remains open in the checklist. A completed agent task or an aggregate passing test count does not close those rows.

## What is open

| Priority | Work | What must happen next |
| --- | --- | --- |
| P0 | Native 4K at 240 FPS on the AMD iGPU | Continue from the accepted full-UI moving baseline. Attribute thin-view/free/text costs with controlled diagnostics, then qualify any candidate with matched correctness and full-game measurements. |
| P0 | CPU busy attribution | Add actual CPU busy measurement before calling host encode wall time CPU utilization. GPU gaps also include scheduling and queue behavior. Preserve the counter layout and meanings. |
| P0 | Six-bit and seven-bit pixel rounding | Reproduce the strict AMD `subpixel_coverage` failure and qualify the isolated wider-color candidate. Do not use the diagnostic inventory switch as a passing oracle. |
| P0 | No-audio loaded-save acceptance | Fix the latest driver defects listed below, then obtain a fresh full-HUD loaded trace and ordinary Quit with actual natural exit status. The previous 480-second forced-TERM run was incomplete. |
| P0 | Combat camera arrival and speed | Run matched visible battles at 1x and 4x. Compare RNG state and stable unit outcomes, as well as timing and camera-before-animation behavior. Prepared observers and comparators do not supply this runtime proof. |
| P0 | Neo release acceptance | f510 is staged, not installed. Finish coherent artifact and priority runtime checks, then use the existing installation procedure. Physical Apple ARM64 Metal/FEX fidelity, music and performance soak remain open. |
| P1 | Feature and platform acceptance | Execute the remaining checks for settings previews, C3X modal ordering, F1 smooth scrolling, scenario roundtrips, cursor independence, city/army state, save/reload/end-turn and the native Windows guest matrix. |
| P1 | Loading cost | Instrument repeated per-source FLC metadata scans and PE art-job stages. The observed 30–36 second late HUD interval was inside PE art compilation after GPU upload. Keep loading analysis separate from frame GPU timing. |

The retained acceptance matrix has 85 families: 82 open, two with the performance goal failed, and one VM-template family covered with scope limits. These are acceptance statuses, not a count of missing implementations.

### Latest audio driver blockers

Checkpoint `ce5e104270258e207e16c3a8cf7a0ff5336c41d5` contains the runnable owner adapter and 12 passing offline tests. A later independent review found three gaps those tests miss:

1. Existing bridge-log truncation aborts observation.
2. `guest_probe.py` accepts title adoption as loaded-map readiness.
3. `vm_adapter.py` captures before checking readiness, so the image cannot prove post-adoption state.

Fix and review these before qualification. The retained package also needs a separately checked normal-Quit radio-selection calibration. The older orchestrator entry describing only a six-test draft is stale. See the [audio agent record](agent-records/mac_audio_sol.md) and the [owner package](evidence/external/quit-audio-sol/loaded-owner-driver-sol/REVIEW.md). Do not treat the preparation instructions as runtime results.

The previous no-device Mal3 arm reached terrain/cities near its deadline, but full HUD/units, a fresh loaded audio trace and ordinary Quit were not established. The watchdog forced TERM after about 480 seconds. The owner restored the exact USB-null fixture and audio environment; root accepted that restoration. Vendor/title causal passes remain valid within their narrower scope.

## Source and installed release identities

| Item | Identity and state |
| --- | --- |
| Implementation baseline | Main commit `8106ae5a4debbe247cb5e64523d4972474f4fe39`, clean before the handover documents. |
| Production source freeze | `f51034b34b94eeb3bc3a8dcfbd2e4faf8eabc6350721b7f7172da7b406504703`, 673 source files, scene protocol 50. This is a source-manifest identity, not a Git commit. |
| Shader pair | Primary `43961a92338f6b5b1f5ebd12dfdc806057fe43d8bfeb50abdb7872d274b601c4`; fallback `bbde10b5e91a3e54c337eab67c2e5acce8abf384657043eb46bbe056ea799281`. |
| Last recorded Neo installation | Source `3be94d8ba5388f1759cf425d28c771631a71cef336351b3df032afc86018764c`, protocol 49, installed 20 October at 13:07:54 UTC. Artifact checks passed; runtime acceptance remained pending. |
| Neo app | `/Users/sirati/Applications/Civilization III.app`, bundle ID `org.sirati.civ3r`. Preserve its icon and Launch Services registration. |
| Pending Neo successor | Signed/staged f510 protocol 50 package. No install or automatic launch is implied by its presence. |
| Older feature branches | `perf` and `zoom` both at `22cd2e4ece069f569ecc485ccbc91c04ffd824c5`, already contained in main. Their old FPS results are different workloads and sources. |
| Private development repository | Main snapshot `6162a7debb8adcc78eac147611eeb1539c7e78c8`; QA branch `checkpoints/loaded-owner-driver-20261020` at `ce5e104270258e207e16c3a8cf7a0ff5336c41d5`. These are in the development backup, not main. |
| Configured origin | `/home/sirati/Projects/civ3-renderer-old` is absent. Off-host Git backups provide the authorized fallback. |

The five runtime files are `civ3-play-macos`, `civ3r-viewer`, `IFC23.dll`, `aarch64-unix/civ3r_native.so`, and `i386-windows/civ3r_native.dll`. Deploy a matched set plus the app starter and bundle artifacts. The release manifests track eight artifacts. Never mix protocol 49 and 50 files. Builds for the Intel macOS VM need their own x86_64 native artifacts.

## Performance evidence and candidate work

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

See [the shader audit](evidence/handover/creative-qa/shader-perf-sol/audit.md), [binary-cell proof](evidence/handover/binary-cell-recorded4k-proof.sol.json), [thin-view plan](evidence/external/binary-cell-blend-sol/actualapp-f510/thin-view-diagnostic/thin-view-diagnostic-plan.json), and [sparse-cell audit](evidence/external/sparse-cells-sol/audit.md).

## Constraints to carry forward

- Keep shipped code in Rust. Use the common pointer API and platform backends. Isolate unsafe interop; add no production Rust locks. Use channels, actors and `civ3r-sync` abstractions.
- Let the GPU draw the world, UI and text. CPU state/layout processing and the independent test oracle are appropriate. The launcher's startup status window is separate from the game renderer.
- Preserve the user's FPS counter design and small triple-buffered controls. Do not triple-buffer the whole world or atlas.
- Use the AMD RADV iGPU for GPU work, historically PCI `0000:72:00.0`, vendor `1002`, device `13c0`. Never select NVIDIA for this continuation.
- Cross-build on Papa. Never compile on physical Neo. Use `nix develop`, `CARGO_INCREMENTAL=0`, and owned build/scratch targets under `/mnt/datasets/c3r-build`.
- Reuse existing game assets and read-only mounts. Do not copy large asset trees or touch the Nexus checkout.
- Keep renderer VMs under `c3r-vms.slice` with a combined 10 GB RAM cap. The macOS `work` VM uses SSH port 22390. Never operate `wayland-host`.
- Coordinate one owner for shared VM GUI/input and one owner for AMD GPU measurement. Old agent handles and `owners_running` fields describe history, not current leases.
- Sol GUI/screenshot work was explicitly authorized on 17 October. Older Sonnet-only restrictions are superseded.
- Keep Neo stopped after the emergency rescue until the coherent release and priority checks are complete. Do not automatically launch the game during backup or documentation work.
- Commit new work and preserve it on a working origin. Where origin is missing, replicate the Git tree to Papa's laptop and Neo before deleting source or private experiments.

The old `00-orchestrator.md` says that nothing is committed, prohibits commits, and describes earlier deployment priorities. Those entries are historical and superseded. Likewise, early reports describe an art-readiness timeout which the later command-transaction design removed.

## Restart sequence

1. Read this summary, the latest agent records for the intended work, and the relevant acceptance rows. Check `git status`, `git log -1`, available artifacts and current process/VM ownership. Do not use historical task names as live handles.
2. Confirm backups before source cleanup. Restore private experiments from `development-source-current.bundle` if needed. Its `SOURCE_INDEX.json` maps original locations. Do not reset main to an old performance branch.
3. For renderer correctness, reproduce the known strict 6/7-bit failure first. For performance, start with the accepted f510 native-4K workload, then run the controlled thin-view diagnostic before adopting optimizations.
4. For audio, fix the three latest owner-driver blockers and complete offline review before calibration or a new loaded no-device arm. Preserve the eight-minute watchdog and exact environment-restoration requirements.
5. For combat, build the reviewed f510 diagnostic successor and execute matched 1x/4x battles. Check RNG and stable unit results with the corrected offline comparator, then evaluate camera-arrival and speed behavior.
6. Bind each accepted change to an immutable source manifest, shader pair and matched runtime artifacts. Run the relevant full groups when code changes affect them. Preserve prior failures and do not count repeated tests as unique coverage.
7. Finish priority runtime checks before replacing the staged/installed Neo set. Continue platform and full-game acceptance, including music/performance soak, separately from installation verification.

Useful existing guides are [macOS Neo](../macos-tsneo.md), [macOS VM](../macos-vm.md), [concurrency](../concurrency.md), [settings transactions](../settings-transactions.md), [independent camera](../independent-camera.md), and [modal input](../modal-input.md). VM credentials remain in the local `/mnt/datasets/macvm/LOGIN.md`; this package excludes credential values.

## Evidence dates and limits

Most retained agent work and runtime evidence dates to 17–20 October. The orchestrator snapshot was last updated at 19:44 UTC on 20 October. The independent 4K baseline review at 19:49 UTC and later driver review supersede its pending/draft wording. The 21 October continuation added verification, found the existing wider rounding failure and reverted all experimental code edits. This handover adds documentation only.

The evidence archive preserves small reports and JSON records with source and copied-content SHA-256 hashes. Redactions are listed in its [manifest](evidence-manifest.json). Build products, screenshots, game assets, VM disks and full raw transcripts remain at the indexed original paths; this documentation backup does not claim to contain those bulk artifacts. Historical reports are evidence, not new instructions.
