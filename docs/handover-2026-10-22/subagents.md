# Subagents for unfinished work

The successor taking this handover must spawn a fresh subagent for each unfinished task below. Use the task name and brief in each entry, and give every agent the shared constraints in the main handover. These are continuation assignments, not a roster of historical agents.

Spawn the agents to begin source review and preparation. The parent must schedule dependencies and reserve shared runtime resources. Only one agent may control a VM GUI/input session, and only one may own an AMD measurement interval. Dependent agents can prepare their work while prerequisites finish; they must wait before the dependent runtime or installation step.

Rebind any owner identity embedded in a retained runtime script or broker grant to the fresh agent through the reviewed ownership procedure. Never reuse a historical lease as current authorization.

The parent owns integration, acceptance decisions, source commits and off-host backups. A passed review or completed preparation task is not reopened merely because its feature still needs runtime acceptance. Completed tasks are listed only as "All of this task has been done" in the main summary.

## Continuation tasks

| Task | Spawn a fresh subagent | Remaining work |
| --- | --- | --- |
| [Native 4K performance and candidate attribution](#performance_240) | `performance_240` | Reach and verify sustained 240 FPS on the AMD RADV iGPU at native 3840 by 2160 with full UI and realistic motion. Preserve the counter design. Continue the controlled thin-view diagnostic before adopting binary-cell, glyph-skip, sparse-cell or terrain-cache changes. Test candidates against the unchanged baseline with matched correctness and timing. |
| [CPU busy measurement and concurrency acceptance](#cpu_timing) | `cpu_timing` | Measure actual CPU busy time separately from host build/encode wall time. Verify GPU-worker isolation, immutable frame/mapping ownership, three small control slots and bounded transport under resize, cancellation and failures. Keep the current counter layout and existing GPU gap semantics. |
| [Six-bit and seven-bit color correctness](#wider_color) | `wider_color` | Resolve the strict wider-depth subpixel rounding failure and qualify the isolated wider-integer implementation. Keep RADV on its proven UNORM path until a replacement passes the numerical oracle. Do not broaden the Microsoft Basic Render Driver FP16 workaround. |
| [Loaded-save audio failure and normal Quit](#audio_loaded) | `audio_loaded` | Finish the loaded Mal3 no-device causal test and macOS music/CPU soak. Fix the owner driver before running it. Preserve the eight-minute watchdog, identity-safe cleanup and exact USB-null environment restoration. |
| [Camera arrival and accelerated combat](#combat_validation) | `combat_validation` | Verify visible battles at 1x and 4x, camera arrival before animation, minimum keep-visible AI focus, army presentation and movement bonuses. Use matched initial state and compare RNG plus stable unit outcomes. |
| [Settings and UI runtime acceptance](#ui_acceptance) | `ui_acceptance` | Finish current-game acceptance for UI command atomicity, settings preview/Accept/Cancel, scale confirmation/rollback, menu fit, C3X modal painter order, F1 smooth scrolling, text fit, popup identity/freezing/dismissal, loading/status UI and background sampling. |
| [Scenario switching and loading cost](#scenario_loading) | `scenario_loading` | Finish rapid scenario-selection/roundtrip acceptance, optional preview lifecycle, memory bounds, capture entry and fractional preview geometry. Attribute remaining late HUD/art adoption delays without conflating load time and frame GPU cost. |
| [Pointer independence and input acceptance](#pointer_input) | `pointer_input` | Finish common pointer API, cursor/game-asset busy animation, capture/release, barriers, sensitivity, Alt navigation, native slider/City/F1 targets and translated menu cursor acceptance across supported display setups. |
| [World and city correctness in independent views](#world_fidelity) | `world_fidelity` | Finish current-source correctness for terrain/cities/districts, off-camera units, national palettes, crisp zoom, independent panes and fog/clean-map controls, notices, city transitions, diplomacy overlays and indexed smoke/shadows. |
| [Native Windows guest acceptance](#windows_guest) | `windows_guest` | Execute the prepared native Windows guest matrix and complete the small offline-ready ephemeral template requirements. Use Vulkan as the primary backend and preserve the minimum observed DX12 compatibility behavior. |
| [Dual-stack and physical Metal runtime acceptance](#platform_runtime) | `platform_runtime` | Finish current matched Linux/Intel macOS/physical Apple ARM64 runtime acceptance, native resolution and HiDPI behavior. Verify the intended ARM-native Wine/FEX plus Metal setup and explicit unsupported GPU-option failure behavior. |
| [Matched successor release on Neo](#neo_release) | `neo_release` | Finish the coherent release and priority runtime gates, then install and verify the matched staged successor on Neo. Keep source freezes, shader identity, all five runtime files and starter/bundle artifacts coherent. |
| [Save and full-game lifecycle acceptance](#game_soak) | `game_soak` | Finish new-game/save/load/reload/end-turn/scenario/city/retire/restart soak using the current paired runtime. Check bounded memory, responsive input, world-mode reset and owned-process cleanup. |

<a id="performance_240"></a>

## Native 4K performance and candidate attribution

Spawn a fresh subagent named performance_240 to continue this unfinished task.

Brief to give the subagent:

Reach and verify sustained 240 FPS on the AMD RADV iGPU at native 3840 by 2160 with full UI and realistic motion. Preserve the counter design. Continue the controlled thin-view diagnostic before adopting binary-cell, glyph-skip, sparse-cell or terrain-cache changes. Test candidates against the unchanged baseline with matched correctness and timing.

First action: Inspect the accepted f510 baseline and thin-view plan. Bind the exact source, shader pair, 18-camera workload and attachments. Run controlled free/text/glyph-metadata axes under an exclusive AMD measurement reservation.

Completion condition: The actual full-UI workload meets the 240 FPS target in repeated sustained moving runs, with correct pixels and separately reported full submissions, cache reuse, presentation rates and timing loss. Host-load and frame-gap limits remain explicit.

Resource and ownership rules: Exclusive AMD GPU measurement; no competing builds, captures, hashing or VM activity during measured intervals.

Acceptance rows: [performance-4k-static](acceptance.md#performance-4k-static), [performance-4k-dynamic](acceptance.md#performance-4k-dynamic), [performance-delays](acceptance.md#performance-delays).

Starting material:

- [sol-root-f510-native4k-baseline-review.json](evidence/handover/sol-root-f510-native4k-baseline-review.json)
- [actual-App-f510-native4k-results.json](evidence/handover/creative-qa/text-perf-sol/actual-App-f510-native4k-results.json)
- [actual-App-f510-camera-stage-map.json](evidence/handover/creative-qa/text-perf-sol/actual-App-f510-camera-stage-map.json)
- [thin-view-diagnostic-plan.json](evidence/external/binary-cell-blend-sol/actualapp-f510/thin-view-diagnostic/thin-view-diagnostic-plan.json)
- [binary-cell-recorded4k-proof.sol.json](evidence/handover/binary-cell-recorded4k-proof.sol.json)

<a id="cpu_timing"></a>

## CPU busy measurement and concurrency acceptance

Spawn a fresh subagent named cpu_timing to continue this unfinished task.

Brief to give the subagent:

Measure actual CPU busy time separately from host build/encode wall time. Verify GPU-worker isolation, immutable frame/mapping ownership, three small control slots and bounded transport under resize, cancellation and failures. Keep the current counter layout and existing GPU gap semantics.

First action: Locate the viewer profiling and worker timing paths. Add a measured thread/process CPU-time source and a controlled check that distinguishes blocked wall time from CPU execution. Reproduce the stale/failure/resize paths against current source.

Completion condition: The displayed CPU metric has a tested meaning and bounded overhead; cursor independence, command/image pairing and bounded queues hold during GPU stalls, surface loss and worker failure. No world/atlas triplication or production Rust locks are introduced.

Resource and ownership rules: Owned CPU build target; coordinate affected shared viewer files and any GPU stall test with the parent.

Acceptance rows: [architecture](acceptance.md#architecture), [fps-counter-details](acceptance.md#fps-counter-details), [gpu-worker-isolation](acceptance.md#gpu-worker-isolation), [gpu-transactions](acceptance.md#gpu-transactions), [gpu-three-buffers](acceptance.md#gpu-three-buffers), [gpu-no-cpu-render](acceptance.md#gpu-no-cpu-render), [architecture-transport](acceptance.md#architecture-transport).

Starting material:

- [actual-App-f510-timing-attribution-audit.json](evidence/handover/creative-qa/text-perf-sol/actual-App-f510-timing-attribution-audit.json)
- [verification.md](verification.md)

<a id="wider_color"></a>

## Six-bit and seven-bit color correctness

Spawn a fresh subagent named wider_color to continue this unfinished task.

Brief to give the subagent:

Resolve the strict wider-depth subpixel rounding failure and qualify the isolated wider-integer implementation. Keep RADV on its proven UNORM path until a replacement passes the numerical oracle. Do not broaden the Microsoft Basic Render Driver FP16 workaround.

First action: Reproduce bits=6, material=0, dst=0, x=5, y=0 returning [0,0,251,37] instead of [0,0,255,37] in both dual-source and fallback paths. Inspect the private wider compiler/qualification package before building it.

Completion condition: Strict six-bit and seven-bit tests pass while RGB444/RGB555, alpha, clipping, glyphs and ordinary rendering remain correct. Continuation/retirement bounds and worker behavior pass compiler and runtime checks. The inventory-only switch is never counted as a correctness pass.

Resource and ownership rules: Owned CPU target followed by a separately scheduled exclusive AMD GPU gate.

Starting material:

- [verification.md](verification.md)
- [cpu-gate-plan.json](evidence/external/wider-integer-sol/compiler-gate/cpu-gate-plan.json)
- [package-manifest.json](evidence/external/wider-integer-sol/gpu-qualification-v2/package-manifest.json)

<a id="audio_loaded"></a>

## Loaded-save audio failure and normal Quit

Spawn a fresh subagent named audio_loaded to continue this unfinished task.

Brief to give the subagent:

Finish the loaded Mal3 no-device causal test and macOS music/CPU soak. Fix the owner driver before running it. Preserve the eight-minute watchdog, identity-safe cleanup and exact USB-null environment restoration.

First action: Restore the ce5e104 private QA checkpoint. Fix bridge-log truncation, title adoption incorrectly satisfying loaded-map readiness, and capture occurring before readiness. Extend offline checks, then separately calibrate the normal-Quit radio selection before a new qualified arm.

Completion condition: Fresh post-adoption full-HUD/unit images and a current loaded native-audio trace establish the intended state; ordinary Quit produces a natural success exit without external signals or survivors. Restore the original audio/VM environment after every outcome. Music soak on physical Neo is recorded separately.

Resource and ownership rules: Sole VM GUI/input and audio-fixture owner; coordinate physical Neo checks with platform_runtime. No VM operation while performance timing owns a quiet interval.

Acceptance rows: [audio-music](acceptance.md#audio-music).

Starting material:

- [REVIEW.md](evidence/external/quit-audio-sol/loaded-owner-driver-sol/REVIEW.md)
- [README.md](README.md)

<a id="combat_validation"></a>

## Camera arrival and accelerated combat

Spawn a fresh subagent named combat_validation to continue this unfinished task.

Brief to give the subagent:

Verify visible battles at 1x and 4x, camera arrival before animation, minimum keep-visible AI focus, army presentation and movement bonuses. Use matched initial state and compare RNG plus stable unit outcomes.

First action: Build the reviewed f510 combat diagnostic successor and run its pending 11-test gate. Prepare matched battles and the corrected offline comparator; check actual event provenance before interpreting timings.

Completion condition: Matched battles have equivalent RNG and stable unit results at both speeds; camera movement completes before visible combat begins; no per-tile recentering, hidden-unit disclosure, stuck arrival or unintended global/audio clock change appears.

Resource and ownership rules: Owned CPU diagnostic build, then one scheduled game/VM GUI owner. Match source/artifact identities with the parent.

Acceptance rows: [unit-animation](acceptance.md#unit-animation), [unit-army](acceptance.md#unit-army), [unit-movement-bonuses](acceptance.md#unit-movement-bonuses), [turn-focus](acceptance.md#turn-focus), [bombard-range](acceptance.md#bombard-range).

Starting material:

- [README.md](evidence/external/combat-diagnostics-army/f510-outcomes-rebind/README.md)
- [verification-manifest.json](evidence/external/combat-diagnostics-army/f510-outcomes-rebind/verification-manifest.json)
- [proof.json](evidence/external/combat-diagnostics-army/offline-comparator-successor2/proof.json)

<a id="ui_acceptance"></a>

## Settings and UI runtime acceptance

Spawn a fresh subagent named ui_acceptance to continue this unfinished task.

Brief to give the subagent:

Finish current-game acceptance for UI command atomicity, settings preview/Accept/Cancel, scale confirmation/rollback, menu fit, C3X modal painter order, F1 smooth scrolling, text fit, popup identity/freezing/dismissal, loading/status UI and background sampling.

First action: Use the assigned acceptance rows to run cold/warm Preferences and F1 cases at multiple scales, then C3X and terrain/context popups. Check command publication without waiting for every optional sprite. Preserve exact source, scene and screenshot/trace identities.

Completion condition: Each assigned acceptance oracle passes in the current actual game, including Cancel restoration, ten-second scale rollback, coherent first-frame UI, clipped hit targets, full-row F1 scrolling and usable modal placement. Fix and recheck concrete failures before closing rows.

Resource and ownership rules: One scheduled GUI owner; CPU-only recipe/preparation may proceed independently. Coordinate changes to shared UI files.

Acceptance rows: [text-native](acceptance.md#text-native), [text-tooltip-fit](acceptance.md#text-tooltip-fit), [ui-command-atomic](acceptance.md#ui-command-atomic), [ui-settings-transactions](acceptance.md#ui-settings-transactions), [ui-menu-scale](acceptance.md#ui-menu-scale), [ui-c3x-modal](acceptance.md#ui-c3x-modal), [ui-screen-placement](acceptance.md#ui-screen-placement), [ui-other-pane](acceptance.md#ui-other-pane), [slider-edit](acceptance.md#slider-edit), [menu-hover](acceptance.md#menu-hover), [menu-smooth-scroll](acceptance.md#menu-smooth-scroll), [popup-world-scale](acceptance.md#popup-world-scale), [popup-freeze-dismiss](acceptance.md#popup-freeze-dismiss), [popup-tile-identity](acceptance.md#popup-tile-identity), [popup-resource-shadows](acceptance.md#popup-resource-shadows), [host-busy](acceptance.md#host-busy), [startup-window](acceptance.md#startup-window), [leader-blink](acceptance.md#leader-blink), [ui-background-sampling](acceptance.md#ui-background-sampling).

Starting material:

- [acceptance.md](acceptance.md)
- [settings-transactions.md](../settings-transactions.md)
- [modal-input.md](../modal-input.md)

<a id="scenario_loading"></a>

## Scenario switching and loading cost

Spawn a fresh subagent named scenario_loading to continue this unfinished task.

Brief to give the subagent:

Finish rapid scenario-selection/roundtrip acceptance, optional preview lifecycle, memory bounds, capture entry and fractional preview geometry. Attribute remaining late HUD/art adoption delays without conflating load time and frame GPU cost.

First action: Run the rapid-switch/reopen cases with generation and memory tracking. Instrument per-source FLC metadata calls and PE art-job stages around the reported 30–36 second gap after GPU uploads.

Completion condition: Repeated quick switches never leave a black/stale preview, lost menu or corrupt atlas/glyph state; preview visibility follows human-playable nations; memory is bounded and load-stage timings identify any remaining delay. Warm startup behavior is measured.

Resource and ownership rules: Owned instrumentation/build target and one scheduled actual-game GUI owner; reuse read-only game assets.

Acceptance rows: [preview-human-visibility](acceptance.md#preview-human-visibility), [preview-progressive](acceptance.md#preview-progressive), [preview-smear](acceptance.md#preview-smear), [preview-capture](acceptance.md#preview-capture), [scenario-generation](acceptance.md#scenario-generation), [scenario-memory](acceptance.md#scenario-memory), [startup-warm-time](acceptance.md#startup-warm-time).

Starting material:

- [acceptance.md](acceptance.md)
- [civ3-map-rendering.md](../civ3-map-rendering.md)

<a id="pointer_input"></a>

## Pointer independence and input acceptance

Spawn a fresh subagent named pointer_input to continue this unfinished task.

Brief to give the subagent:

Finish common pointer API, cursor/game-asset busy animation, capture/release, barriers, sensitivity, Alt navigation, native slider/City/F1 targets and translated menu cursor acceptance across supported display setups.

First action: Run focus/multiple-window/boundary and sensitivity-ratio checks, then an observable OS-cursor check during a controlled GPU-only stall. Cover captured and free input, magnifier barriers, invalid targeting and right-drag orders.

Completion condition: Assigned input/cursor oracles pass without duplicate native input or leaked slider arrows. The OS cursor remains responsive during the stall; HiDPI/fractional geometry, capture handoff, targeting and Alt actions match their displayed coordinates.

Resource and ownership rules: Exclusive GUI/input during live checks; coordinate GPU stall work with cpu_timing and the AMD owner.

Acceptance rows: [magnifier-layout](acceptance.md#magnifier-layout), [magnifier-hit](acceptance.md#magnifier-hit), [cursor-os](acceptance.md#cursor-os), [cursor-captured](acceptance.md#cursor-captured), [cursor-scale](acceptance.md#cursor-scale), [cursor-target-modes](acceptance.md#cursor-target-modes), [cursor-civ4-drag](acceptance.md#cursor-civ4-drag), [capture-entry](acceptance.md#capture-entry), [capture-sensitivity](acceptance.md#capture-sensitivity), [alt-navigation](acceptance.md#alt-navigation), [alt-f1-city](acceptance.md#alt-f1-city), [platform-pointer-api](acceptance.md#platform-pointer-api), [mirrored-cursor](acceptance.md#mirrored-cursor).

Starting material:

- [acceptance.md](acceptance.md)
- [world-input.md](../world-input.md)
- [native-alt-sliders.md](../native-alt-sliders.md)

<a id="world_fidelity"></a>

## World and city correctness in independent views

Spawn a fresh subagent named world_fidelity to continue this unfinished task.

Brief to give the subagent:

Finish current-source correctness for terrain/cities/districts, off-camera units, national palettes, crisp zoom, independent panes and fog/clean-map controls, notices, city transitions, diplomacy overlays and indexed smoke/shadows.

First action: Use independent A/B and magnifier views of the same real save. Compare the assigned pixel/state oracles, including C3X resource layers, City worked tiles, repeated notices, resized indexed smoke and rapid Next City transitions.

Completion condition: All assigned feature oracles pass with original game pixels disabled. Separate pane settings and unit visibility remain correct outside the native parked camera, and no outstanding smoke/indexed path is silently omitted.

Resource and ownership rules: One scheduled actual-game GUI owner and separate AMD pixel checks as needed; do not duplicate game assets.

Acceptance rows: [world-pixel-fidelity](acceptance.md#world-pixel-fidelity), [districts](acceptance.md#districts), [city-labels-all](acceptance.md#city-labels-all), [unit-visibility](acceptance.md#unit-visibility), [unit-palette-memory](acceptance.md#unit-palette-memory), [zoom-crisp](acceptance.md#zoom-crisp), [split-independent](acceptance.md#split-independent), [split-fog](acceptance.md#split-fog), [smoke-resized-indexed](acceptance.md#smoke-resized-indexed), [city-state-sync](acceptance.md#city-state-sync), [notices-expire](acceptance.md#notices-expire), [diplomacy-ring-arrow](acceptance.md#diplomacy-ring-arrow), [city-worked-alt](acceptance.md#city-worked-alt), [settings-fog-and-territory](acceptance.md#settings-fog-and-territory), [viewport-separators](acceptance.md#viewport-separators).

Starting material:

- [acceptance.md](acceptance.md)
- [clean-map.md](../clean-map.md)
- [per-view-layers.md](../per-view-layers.md)
- [unit-map-notices.md](../unit-map-notices.md)

<a id="windows_guest"></a>

## Native Windows guest acceptance

Spawn a fresh subagent named windows_guest to continue this unfinished task.

Brief to give the subagent:

Execute the prepared native Windows guest matrix and complete the small offline-ready ephemeral template requirements. Use Vulkan as the primary backend and preserve the minimum observed DX12 compatibility behavior.

First action: Inspect the existing VM and matrix without starting unrelated guests. Verify read-only fixtures and matched artifacts, then run the native guest cases. Papa Wine tests are supporting evidence only.

Completion condition: The native guest matrix passes with explicit adapter/backend identity; the required small clones are usable and within resource limits. Keep hardware acceleration claims separate from pure CPU tests and software-adapter results.

Resource and ownership rules: Renderer VMs together remain within the c3r-vms.slice 10 GB cap. Never operate wayland-host.

Acceptance rows: [platform-windows](acceptance.md#platform-windows), [windows-ephemeral-template](acceptance.md#windows-ephemeral-template).

Starting material:

- [acceptance.md](acceptance.md)
- [macos-vm.md](../macos-vm.md)

<a id="platform_runtime"></a>

## Dual-stack and physical Metal runtime acceptance

Spawn a fresh subagent named platform_runtime to continue this unfinished task.

Brief to give the subagent:

Finish current matched Linux/Intel macOS/physical Apple ARM64 runtime acceptance, native resolution and HiDPI behavior. Verify the intended ARM-native Wine/FEX plus Metal setup and explicit unsupported GPU-option failure behavior.

First action: Bind the current source and all native/PE artifacts. Use the Intel macOS VM for permitted preparatory checks; coordinate any physical Neo run with the release owner after priority checks and installation. Recheck adapter identity and game-window suppression.

Completion condition: Current-source startup/input/drawing/exit works on the assigned platforms, renderer output remains native-resolution with supported game modes, and physical Metal/FEX fidelity/performance is recorded. No VM SwiftShader test is claimed as Metal proof.

Resource and ownership rules: Cross-build only on Papa. No physical Neo compilation or automatic game launch. Reserve physical/VM GUI ownership.

Acceptance rows: [platform-linux-dualstack](acceptance.md#platform-linux-dualstack), [platform-mac-dualstack](acceptance.md#platform-mac-dualstack), [platform-metal](acceptance.md#platform-metal), [resolution-native](acceptance.md#resolution-native), [window-hidpi](acceptance.md#window-hidpi), [nvidia-failure-handling](acceptance.md#nvidia-failure-handling), [low-game-resolution](acceptance.md#low-game-resolution).

Starting material:

- [acceptance.md](acceptance.md)
- [macos-tsneo.md](../macos-tsneo.md)
- [macos-vm.md](../macos-vm.md)

<a id="neo_release"></a>

## Matched successor release on Neo

Spawn a fresh subagent named neo_release to continue this unfinished task.

Brief to give the subagent:

Finish the coherent release and priority runtime gates, then install and verify the matched staged successor on Neo. Keep source freezes, shader identity, all five runtime files and starter/bundle artifacts coherent.

First action: Verify that Neo still has the last recorded protocol49 3be set and that protocol50 f510 remains staged. Inspect actual outstanding priority gates before replacing any file. Prepare rollback and verification from the existing release procedure.

Completion condition: The intended source-bound successor is installed as a matching signed set with searchable app/icon, verified runtime hashes and explicit runtime acceptance results. Installation alone does not close music, performance or feature acceptance.

Resource and ownership rules: Parent coordinates publication after relevant priority checks. Cross-build on Papa; no automatic Neo launch.

Dependent execution must wait for the relevant results from `audio_loaded`, `ui_acceptance`, `combat_validation`.

Acceptance rows: [release-pairing](acceptance.md#release-pairing), [settings-chrome](acceptance.md#settings-chrome).

Starting material:

- [README.md](README.md)
- [macos-tsneo.md](../macos-tsneo.md)

<a id="game_soak"></a>

## Save and full-game lifecycle acceptance

Spawn a fresh subagent named game_soak to continue this unfinished task.

Brief to give the subagent:

Finish new-game/save/load/reload/end-turn/scenario/city/retire/restart soak using the current paired runtime. Check bounded memory, responsive input, world-mode reset and owned-process cleanup.

First action: Prepare the recorded save/load/turn cases and run them only after the relevant current-source smoke and input checks. Preserve unique saves, logs and identity-bound failures.

Completion condition: The full lifecycle sequence passes without leaked processes or unbounded memory, state and world mode reset on exit/restart, and no evidence relies only on title-screen or synthetic tests.

Resource and ownership rules: One scheduled game/GUI owner; share the same asset tree and preserve unique saves before cleanup.

Dependent execution must wait for the relevant results from `ui_acceptance`, `pointer_input`, `scenario_loading`.

Acceptance rows: [regression-start-exit](acceptance.md#regression-start-exit), [save-load-input](acceptance.md#save-load-input), [ui-read-only-constraints](acceptance.md#ui-read-only-constraints).

Starting material:

- [acceptance.md](acceptance.md)
