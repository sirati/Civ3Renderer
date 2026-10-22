# Native application qualification status

The 240 FPS goal at native 3840×2160 on Papa's AMD RADV iGPU remains unverified and unmet by the retained observations. All three old App866 repetitions fail sustained-motion qualification. The corrected durable summary is `/mnt/datasets/c3r-build/handover/creative-qa/actual-App866-dynamic-summary.json`. Its classification supersedes the weaker historical motion test without changing raw evidence.

## Shipping f510 actual App baseline, 2026-10-20

The fresh ordinary native 4K run `finalf510-native1-dual0-plain-v3-r2` completed three 60-second sustained-pan repetitions after backup clearance. Actual App presentation rates were 43.22, 43.99 and 43.00 FPS. Strong A motion, fixed B, inverse-label anchoring, full UI, zero reuse/replay, native AMD geometry and zero timestamp drops passed. The 240 FPS threshold fails in every repetition. No hardware-limit or quiet-host claim follows from the retained unrelated load. All owned processes were cleaned and the lease released.

Authoritative evidence is `actual-App-f510-native4k-results.json`, with per-camera/per-window stages in `actual-App-f510-camera-stage-map.json` and a concise status in `actual-App-f510-native4k-status.md`. Host encode is wall time, not CPU busy time. GPU gap is not attributed solely to CPU waiting. Endpoint query accounting is not called a matched per-frame cohort. Interrupted r1 is preserved separately and has no gameplay/timing result.

## Corrected native 4K geometry, 2026-10-20

The actual frozen Layout/pan oracle proves that the old right target (1918, 800) hits the open divider instead of asking A to scroll. In the same test binary, the closed-wall target (1919, 360) reaches Main, moves A right, preserves the label anchor, and leaves B fixed. Only an appended cfg(test) module differs from frozen 8b. The initial harness viewport setup failure and its logs remain preserved separately.

A fresh actual 8b native 1x/dual 0 untimed live preflight passes with only that right target changed. The original three 96-pixel relative pulses suffice. A has four positive and four negative complete observed intervals, three reversals, and fixed inverse-label anchoring; B stays fixed. Retained incoming Wine/C Wayland protocol records independently show both right target deliveries at (1919, 360) on surface 28. The original native Rust-only parser's zero-event result remains intact. Event timestamps use the helper's elapsed clock, so no host-clock latency estimate is made.

Evidence is `pointer-geometry-cpu-proof.json` and `pointer-geometry-live-proof.json`. All owned runtime processes are absent and both leases are released. This was a diagnostic launch with WAYLAND_DEBUG enabled, with no accepted timing or 240 FPS result. The source-only successor fixture at `/mnt/datasets/c3r-build/pointer-v3-geometry-sol/fixture-v3` makes ordinary preflight, reset, CLI measurement and the timed worker share one oracle-bound geometry plan. Its 32 CPU checks now pass, including six new equality/debug gates. Historical v2 scripts remain byte-verified unchanged. Final source binding is `pointer-geometry-policy-proof.json`. No ordinary fixture-v3 runtime has started; it waits for final PE binding and VM lifecycle acceptance. A fresh unlogged launch and another strong preflight are mandatory before timing. Logical 1080p mode still needs its own oracle. The macOS VM loaded-game Quit blocker remains open.

## Actual 8b preflight, 2026-10-20

The first native1x/dual0 actual8b pointer-v2 preflight ran and failed before timing. A moved left and stopped instead of moving right. Its inverse city-label mapping stayed correct, but positive legs and reversals were zero. All owned processes were cleaned up and the lease was released for audio controls. No other matrix case or accepted timing started. Exact evidence and disclosed host/GPU context are in `actual-App8b-v2-preflight-proof.json`.

The old target (1918,800) lies in the open divider hit band of the actual [0,0,1920,2160] A pane. Frozen Layout/pan code therefore does not request A's rightward scroll there. Actual OS pointer coordinates were not retained, so the failed run does not prove a product barrier fault. A test-only actual Layout geometry oracle and target-only external preflight wrapper are prepared in `/mnt/datasets/c3r-build/pointer-v3-geometry-sol`. CPU compilation and a later bounded live test must wait for broker gates after audio controls. Original helper/fixture sources and motion requirements are unchanged.

The actual8b artifact bytes were matched to the test-only6029 successor carry. These remain application diagnostics under unrelated host load, not release acceptance. The loaded-game Quit failure in the macOS VM remains a release blocker.

## Historical runtime and workload

- Source `86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e`.
- Runtime manifest `/mnt/datasets/c3r-build/release-1019/artifacts/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/linux-runtime-manifest.json`.
- Full shader `f9968edd2867ca9857731cd0b1c09d2172cb019e5523065b942507b249d7dc75`; fallback `7ff6d943a54e7255df2cd160e7239aee067f89470156e5a50b3c1e5079857a5d`.
- Fixture `/mnt/datasets/c3r-build/native-app-qualification-sol`.
- Started case `final866-native1-dual1-r1`; the other native1/2 dual0/1 cases were only prepared.

The readonly source save is `/mnt/datasets/winvm/civ3-src/Civilization III Complete/Conquests/Saves/WW2.SAV`, 684111 bytes, SHA256 `6c1405206c2ece1c03d7b58bcc8ccfe586eb485d451168f562cddd11a0134632`. The loaded game was Monarch, Germans, Week24 1941AD, native map 256×256, wrap 0,0. The nonwrapping SceneMap adds two exterior rows, giving 128×258 TileWorld cells. The original engine camera was parked 0,0 with visible tiles -1..5. `workload.json` binds current-load verification and the untimed summary/world/trace evidence.

At native4K, the actual Load Game menu item was around 1667,1114. The filename editor ignored Ctrl+A and appended a long path to its initial `..`. The verified recovery closed the error with Enter, sent 90 Backspace down/up pairs, typed `Saves\WW2.SAV`, pressed Enter, waited for the saved-game summary, then pressed Enter. Future runtimes need their own untimed coordinate verification. Private saves are links to the existing install. No game files were copied. Any `civ3-trace.request` belongs only in the run's private `home/game/Civilization III Complete/Conquests` directory and must be removed before timing.

## Bounded observations

| Repetition | App/full-submit FPS | Full submits | Reuse | GPU samples | Readback-busy drops | Sustained-motion gate |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | 45.4216 | 2504 | 0 | 2032 | 472 | Failed, initial motion then static boundary |
| 2 | 47.1064 | 2595 | 0 | 2124 | 471 | Failed, static world camera |
| 3 | 47.5075 | 2617 | 0 | 2104 | 513 | Failed, static world camera |

Repetition1 primary-camera x observations were 6821,5704,4132,3000,1449,301,0,0,0,0,0,0. It moved initially, reached the boundary and remained there. Its 45.4216 FPS is a bounded actual-App observation across mixed motion/static work. It is not sustained-pan evidence. No pooled dynamic average is valid. GPU coverage was 2032/2504 = 81.15%, 81.85%, 80.40%, respectively, so complete GPU timing coverage was also not established.

The capture-free Rust helper was active and RFB/WayVNC were absent during every timed repetition. Readonly host snapshots preserve CPU/memory/IO pressure, load/stat/meminfo and AMD clocks/power/temperature/memory. Unrelated ML and long-lived processes remained active, so these observations do not establish an isolated hardware limit.

The actual App had 18 cameras, roughly 7316 sprite source commands and 17168 outline-glyph source commands. The reconstructed backend had 6 cameras and 690 glyphs. Source counts include ordinary/zoomed twins and do not prove shader visibility. MainA camera xy and label screen_shift changed by equal/opposite amounts during initial motion, with source-anchor sum 0,0. B stayed fixed.

`timing-window-summary.json` retains separate log-file/report windows. Wine and bridge logs duplicate the same reports, so they must not be pooled. Across repetition 1's mixed motion/static work, per-window GPU-span p50 ranged 18.60..19.27ms, p95 20.00..21.53ms and p99 20.71..22.73ms, at only 81.15% coverage. Worker draw(all) wall p50 ranged 19.84..20.63ms and includes surface acquisition. It is not pure CPU work. Main draw(all) p50 was near zero because rounds include attempts while a worker packet is pending; p95 was 2.68..4.01ms. No global quantile was derived by averaging report quantiles.

The historical `dynamic-rep1-pass-summary.json` filename reflects the weaker old gate. Its costs remain useful only within the observation scope above. Main world views v0/v1 each cost roughly 4.23..4.67ms at per-window p50. Four 2-pixel-wide world seam views v2..v5 each cost 1.20..1.29ms, about 5 ms together. Output cost 1.79..1.90ms, glyph cache fill 0.01 ms and publication rounded near zero. Root received these findings. No seam or glyph-preparation optimization is claimed from this agent's observation alone.

## Capture-free pointer v2

Source review proved WayVNC 0.10.2 captures internally after a client connects, regardless of RFB framebuffer requests. RFB is restricted to untimed keyboard/menu setup. Timed input stops both RFB and WayVNC after creating a direct Wayland virtual pointer.

The old helper emitted absolute warps only. Its inability to reverse after reaching a pointer barrier is a fixture diagnosis; no production barrier defect has been established. The original fixture sources are preserved in `baseline866-fixture/source-sha256.json`. Original repetition data and reports remain unchanged.

The external, nonshipping Rust helper now emits genuine `zwlr_virtual_pointer_v1.motion` relative events, opcode 0 with signed 24.8 deltas. Absolute `move` remains opcode 1. The helper validates every request batch before emitting an event and forbids unsafe code. `--capabilities` reports protocol 2 and capture_backend=none without connecting to Wayland. Its binary is `direct-pointer-target/release/c3r-qualification-pointer`, SHA256 `25b7a6040e20bd35757c17dc32252a829e9b814423781983bbb0fa0e6ce32cc9`.

The motion policy sends three separate 96-pixel relative escape samples at 20 ms input cadence before an absolute edge position. A neutral reset exercises both signs when the current wall is unknown. These delays are test-input cadence outside shipping code. They are not renderer waits. `switch_to_direct_pointer.py` requires protocol 2, records source/binary/lock hashes, creates the new device first, then uses owned pidfds to stop RFB/WayVNC and verifies their absence.

The strengthened fixture requires primary cells=true world cameras of sufficient size. Thin seam/UI cameras cannot pass. Every complete adjacent observation interval must move. The same primary camera must have at least two positive and two negative x legs, at least three direction reversals, and equal/opposite label movement around a fixed source anchor. First/last partial intervals remain explicitly unobserved.

`preflight` uses normal input to put A inside x 6000..10000, verifies two fresh neutral reports, then verifies left/right/left/right motion with two fresh reports per leg. `reset` returns to a stable interior before every repetition. `measure` requires a matching, unconsumed reset no older than 30 seconds and records its input transitions. `run_three.py` resets before each repetition. Runtime preparation binds `timing.rs` to its immutable source-manifest SHA and parses that source's readback-ring capacity. It does not assume the historical ring 4 applies to a successor with ring 32.

These CPU fixture changes alone do not prove live input behavior. The first actual8b preflight is described above; it failed, and no successor timing run has executed.

## Completed CPU-only verification

The helper's 3 Rust tests passed. They retain signed/fractional relative axes, reject invalid batches before event emission, and use an independent Unix-socket fake Wayland peer to verify actual opcodes, fixed-point values and frame events. The Python fixture's latest 26 tests passed, including the locked-pointer model, old-helper rejection, sustained-motion/reset rejection, partial-interval limits, timing parser and source-bound ring handling. Earlier 23-test results were superseded by the final 26-test run. Logs are `direct-pointer-v2-test.log`, `direct-pointer-v2-build.log` and `test-fixture-v2.log`.

Historical diagnostics checks extracted the exact App866 Rust observation and App formatters, compiled std-only fixtures, and checked valid bounded JSON/count behavior. Evidence is `diagnostics-formatter-final866/result.json` and `test-diagnostics-final866.log`. Those checks are not full workspace or successor runtime acceptance. Root now owns shipping diagnostics and any successor changes.

`fixture-v2-binding.json` in this handover binds current fixture/helper sources, binary, logs, docs and corrected ledgers. External Python orchestration was explicitly approved by root and remains outside the repository and shipped packages. No shipping source changed during the pointer v2 work.

## Ownership and next runtime gate

All old App/compositor/Wine/helper processes were stopped. `final866-native1-dual1-r1/shutdown.json` reports all_owned_processes_absent=true, no direct/descendant survivors, no forced direct children and unrelated_processes_touched=false. The temporary CPU-only helper lease was used and released. No current GPU/App lease is held by this agent. Do not restart App/Sway/Wine or run builds until root and `/root/mac_fps_music` grant a new lease and select a coherent successor runtime.

Under that new lease, first prove v2 barrier release and bidirectional sustained motion with no capture process. Then run three 60-second repetitions for native1 dual0/1 and scale2 crisp UI dual0/1 with full UI and zero reuse. Preserve source/workload matching and all timing/drop counters. Use successor encode/acquire/submit/present/gap diagnostics only when the selected binary and independent timing gate bind them. A GPU inter-frame gap includes uploads and pacing and cannot by itself be called CPU-caused waiting. Never force timing coverage by blocking the GPU.

New user feedback is tracked separately in `/mnt/datasets/c3r-build/handover/feedback-20261020-live.sol.json`. This handover does not mark it implemented or runtime-passed.
