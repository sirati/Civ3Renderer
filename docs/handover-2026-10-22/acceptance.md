# Renderer acceptance checklist

The retained 20 October ledger contains 85 acceptance families: 82 open, two performance goals failed and one VM-template family covered with scope limits. Implementation and acceptance are separate. An open row may already have implemented code and passing targeted tests.

These rows preserve the ledger's requirement, implementation state, next test and evidence. The main handover records later corrections. Root accepted the native 4K baseline at 19:49 UTC, after the 19:44 ledger snapshot. All three runs still failed the 240 FPS goal. The loaded-audio driver advanced to ce5e104, but its later review found three blockers. On 21 October, the strict six-bit and seven-bit rounding checks failed again.

The current release gates and command-transaction design supersede old wording about immediate deployment and art-readiness holds.

The [original JSON ledger](evidence/handover/creative-qa/acceptance-matrix.json) retains all reconciled evidence. The [request coverage index](evidence/handover/creative-qa/request-coverage-index.json) maps original user requests to these families. The [prioritized test plan](evidence/handover/creative-qa/prioritized-test-plan.json) supplies additional sequencing.

## Family status

| Family | Priority | Acceptance | Requirement |
| --- | --- | --- | --- |
| [architecture](#architecture) | P0 | open | Generic tile renderer/menu and safe isolated interop; no game-renderer pixels |
| [world-pixel-fidelity](#world-pixel-fidelity) | P1 | open | Pixel-correct terrain, improvements, resources, cities and borders |
| [districts](#districts) | P1 | open | C3X district graphics render in all views |
| [city-labels-all](#city-labels-all) | P1 | open | Every city has a label in all independently moved views |
| [unit-visibility](#unit-visibility) | P1 | open | Units outside A remain visible in B and other views |
| [unit-animation](#unit-animation) | P0 | open | Unit motion/action/combat/explosions and active highlight are complete |
| [unit-army](#unit-army) | P1 | open | Army national/body/flag colours and aggregate HP/strength/movement |
| [unit-movement-bonuses](#unit-movement-bonuses) | P1 | open | HUD movement remains correct for civ bonuses and altered road movement |
| [unit-palette-memory](#unit-palette-memory) | P1 | open | Indexed national recolouring avoids duplicating all art per nation |
| [zoom-crisp](#zoom-crisp) | P1 | open | Free zoom and Z downsampling remain crisp without per-zoom art |
| [text-native](#text-native) | P1 | open | Native Windows text geometry and outline glyphs including ANSI punctuation |
| [text-tooltip-fit](#text-tooltip-fit) | P1 | open | Tooltip text/background/borders fit across DPI and pane boundaries |
| [ui-command-atomic](#ui-command-atomic) | P0 | open | Commands/layout/controls/sprite sources appear as one committed UI group without waiting for optional art |
| [ui-settings-transactions](#ui-settings-transactions) | P0 | open | Native/custom preferences preview live, Accept persists, Cancel restores all settings |
| [ui-menu-scale](#ui-menu-scale) | P1 | open | Menu checkbox and percentage mean fraction of available height with no black top/bottom at100% |
| [ui-c3x-modal](#ui-c3x-modal) | P1 | open | C3X modal over Preferences keeps settings size and appears centred in top third |
| [ui-screen-placement](#ui-screen-placement) | P1 | open | Shared Screens setting places city/advisor/menu/popup consistently without duplicating world |
| [ui-other-pane](#ui-other-pane) | P1 | open | Other pane remains usable when permissible menu/city open |
| [split-independent](#split-independent) | P1 | open | Split panes have independent movement, focus and layer visibility |
| [split-fog](#split-fog) | P1 | open | Fog/clean-map settings are independent per view while sharing world layers |
| [magnifier-layout](#magnifier-layout) | P1 | open | Top strip/floating/mouse magnifiers respect pane bounds, size sliders and horizontal ratios |
| [magnifier-hit](#magnifier-hit) | P1 | open | Strip barriers and box entry preserve scrolling/cursor behavior |
| [preview-human-visibility](#preview-human-visibility) | P1 | open | Scenario preview reveals only human-playable nations |
| [preview-progressive](#preview-progressive) | P0 | open | Scenario selection switches immediately while optional preview/art loads progressively |
| [preview-smear](#preview-smear) | P1 | open | Fitted preview columns/visible extents never smear under fractional scale |
| [preview-capture](#preview-capture) | P1 | open | Double-click preview enters x2 captured world view with release hint |
| [scenario-generation](#scenario-generation) | P0 | open | Rapid scenario roundtrips preserve atlas/glyph IDs and never hang |
| [scenario-memory](#scenario-memory) | P1 | open | Cancelled art/uploads and repeated scenario menus remain memory bounded |
| [cursor-os](#cursor-os) | P0 | open | Uncaptured physical OS cursor and icon updates stay independent of slow GPU |
| [cursor-captured](#cursor-captured) | P1 | open | Captured/Alt virtual cursor hides physical cursor and hands off safely at edges |
| [cursor-scale](#cursor-scale) | P1 | open | Cursor scales with integer game pixel blocks without blurry/shadow regression |
| [cursor-target-modes](#cursor-target-modes) | P0 | open | Only go-to/stack modes tile-snap; rebase/airlift/bombard invalid target icon works |
| [cursor-civ4-drag](#cursor-civ4-drag) | P1 | open | Civ IV style right-drag moves units, click opens menu |
| [capture-entry](#capture-entry) | P1 | open | Clicking capture pane captures without world click or recenter |
| [capture-sensitivity](#capture-sensitivity) | P1 | open | Only captured mode gets configurable extra mouse sensitivity |
| [alt-navigation](#alt-navigation) | P1 | open | Directional Alt navigation animates at configured cadence and stays in correct bounds |
| [alt-f1-city](#alt-f1-city) | P1 | open | F1 cities/buildings and city production controls are selectable in Alt mode |
| [slider-edit](#slider-edit) | P1 | open | Alt slider drag/arrow edits support Preview/Accept/Cancel and clear focus feedback |
| [menu-hover](#menu-hover) | P1 | open | Hover/selected/pressed states render locally and hotkeys select stack mode |
| [menu-smooth-scroll](#menu-smooth-scroll) | P1 | open | Optional menu list scrolling is smooth without stale hover or click targets |
| [popup-world-scale](#popup-world-scale) | P1 | open | Terrain information popup uses world scale, HUD floor and window fit cap |
| [popup-freeze-dismiss](#popup-freeze-dismiss) | P0 | open | Anchored tile/rightclick popup freezes map and dismisses on outside click/recapture |
| [popup-tile-identity](#popup-tile-identity) | P1 | open | Terrain Details retains original clicked virtual tile with parked game camera |
| [popup-resource-shadows](#popup-resource-shadows) | P1 | open | Terrain detail/resource palette and cattle shadows are correct |
| [smoke-resized-indexed](#smoke-resized-indexed) | P1 | open | Non-FLC colour-table smoke224..239 and resized indexed blend are complete |
| [city-state-sync](#city-state-sync) | P0 | open | Rapid Next City and alternate-camera UI remain responsive with matching city/camera |
| [notices-expire](#notices-expire) | P1 | open | Repeated already-moved notice dedupes and expires by viewer clock |
| [host-busy](#host-busy) | P1 | open | Busy indicator distinguishes500 ms/no response and prolonged stuck host |
| [startup-window](#startup-window) | P1 | open | Native loading window explains Wine/prefix/translation/art stages through first menu |
| [startup-warm-time](#startup-warm-time) | P1 | open | Warm launcher avoids repeated registry writes; cold translation remains observable |
| [audio-music](#audio-music) | P0 | open | macOS music plays continuously without short loops or excessive sound CPU |
| [leader-blink](#leader-blink) | P1 | open | Leader-head animations have intended timing instead of rapid blinking |
| [resolution-native](#resolution-native) | P0 | open | Native-resolution window, integer Auto/x1..x6 pixels and dynamic resizing |
| [window-hidpi](#window-hidpi) | P1 | open | Only renderer window and correct DPI/placement across desktops and task switch |
| [settings-chrome](#settings-chrome) | P1 | open | Settings icons and native Accept/Cancel chrome are correct; launcher app searchable/icon present |
| [fps-counter-details](#fps-counter-details) | P1 | open | FPS details include separate CPU preparation, GPU execution, CPU passes and optional GPU passes |
| [gpu-worker-isolation](#gpu-worker-isolation) | P0 | open | All GPU upload/draw/acquire work is off OS main thread; main cursor remains live |
| [gpu-transactions](#gpu-transactions) | P0 | open | Resize/lost surfaces/cancelled epochs/failure replies preserve image-mapping pairing |
| [gpu-three-buffers](#gpu-three-buffers) | P0 | open | Small frame controls stay immutable across three in-flight GPU submissions |
| [gpu-no-cpu-render](#gpu-no-cpu-render) | P1 | open | GPU shader paths draw solid UI/text/indexed/direct art; CPU only state/layout/oracle |
| [platform-linux-dualstack](#platform-linux-dualstack) | P0 | open | Wine dual-stack native Linux remains correct with game rendering disabled |
| [platform-mac-dualstack](#platform-mac-dualstack) | P0 | open | Actual single-process Intel macOS game/native runtime plus Apple ARM/FEX |
| [platform-metal](#platform-metal) | P0 | open | Physical A18 Metal/FEX rendering, music and performance |
| [platform-windows](#platform-windows) | P1 | open | Windows primary Vulkan correctness with minimum D3D compatibility |
| [platform-pointer-api](#platform-pointer-api) | P1 | open | Portable pointer constraints and keyboard capture behave on Windows/Wayland/macOS |
| [vm-template](#vm-template) | P2 | covered with scope limits | Prepared desktop snapshot clones boot/login/SSH/read-only assets cleanly under10GB shared cap |
| [release-pairing](#release-pairing) | P0 | open | Five matched binaries plus launcher app deploy immediately after build/protocol/shader gate |
| [performance-4k-static](#performance-4k-static) | P0 | failed goal not achieved | Verify 240 FPS AMD iGPU at3840 x 2160, static/cached and forced dynamic work separately |
| [performance-4k-dynamic](#performance-4k-dynamic) | P0 | failed goal not achieved | 240 FPS AMD iGPU target holds under forced world/UI redraw and realistic interactions |
| [performance-delays](#performance-delays) | P0 | open | Audit arbitrary waits/locks without removing necessary pacing/lost-wake safeguards blindly |
| [diplomacy-ring-arrow](#diplomacy-ring-arrow) | P2 | open | Configurable flashing active arrow and diplomacy rings across views |
| [turn-focus](#turn-focus) | P1 | open | AI combat/turn animations focus requested pane without moving independent pane |
| [city-worked-alt](#city-worked-alt) | P1 | open | City worked-tile Alt targets agree with displayed independent city camera |
| [architecture-transport](#architecture-transport) | P2 | open | Viewer transport async ownership does not block game/publisher or grow unbounded |
| [regression-start-exit](#regression-start-exit) | P0 | open | New game/retire/exit/restart clears world mode and resources |
| [settings-fog-and-territory](#settings-fog-and-territory) | P1 | open | Show territory and clean-map presets/categories update intended views immediately |
| [viewport-separators](#viewport-separators) | P2 | open | High-contrast dividers and capture-release hint stay visible over unexplored world |
| [save-load-input](#save-load-input) | P0 | open | Keyboard/mouse operate new-game, load/save and full turn in each supported runtime |
| [ui-read-only-constraints](#ui-read-only-constraints) | P0 | open | Sol visual/GUI acceptance permitted without touching Nexus/assets/system settings |
| [nvidia-failure-handling](#nvidia-failure-handling) | P2 | open | Unsupported explicit NVIDIA option should fail clearly rather than hang invisible |
| [ui-background-sampling](#ui-background-sampling) | P1 | open | Selectable discard/dither/interpolate background sampling and original native settings buttons |
| [bombard-range](#bombard-range) | P1 | open | Bombard command displays correct range/path and action overlays |
| [mirrored-cursor](#mirrored-cursor) | P1 | open | Translated pointer in a menu pane keeps arrow shape while physical pointer outside |
| [windows-ephemeral-template](#windows-ephemeral-template) | P2 | open | Windows golden VM clones are small,2GB RAM,30GB disk,offline-ready and usable for testing |
| [low-game-resolution](#low-game-resolution) | P1 | open | Original game runs at supported low resolution independent of native renderer output |

<a id="architecture"></a>

## architecture

Generic tile renderer/menu and safe isolated interop; no game-renderer pixels.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Trace production game render hooks through title, world, city and modal
2. Request numeric pixel audit while forcing ongoing redraw

Acceptance oracle: Original drawing counters stay suppressed while new renderer draws; game content/animation selection still advances.

Related failure to probe: Hidden original window still burning GDI CPU; audit request never serviced on a static screen.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [ui-commands-atomic.sol.md](evidence/handover/ui-commands-atomic.sol.md)
- [gpu-worker.sol.md](evidence/handover/gpu-worker.sol.md)
- [vm-snapshot.sol.md](evidence/handover/vm-snapshot.sol.md)
- `/mnt/datasets/c3r-build/launcher-cleanup-sol/audit.json`
- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- `/mnt/datasets/c3r-build/root-union-validation-sol/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/root-aggregate-review.json`
- [union-package-test-results.json](evidence/handover/creative-qa/shader-perf-sol/union-package-test-results.json)
- `/mnt/datasets/c3r-build/release-1019/artifacts/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/release-validation.json`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/allocator-results.json`
- `/mnt/datasets/c3r-build/release-1019/artifacts/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/wine-direct-results.json`

<a id="world-pixel-fidelity"></a>

## world pixel fidelity

Pixel-correct terrain, improvements, resources, cities and borders.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa AMD RADV numeric GPU; Papa Wine existing assets.

Next actions:

1. Render edge/interior, wrap/nonwrap worlds at multiple fractional phases
2. Compare sparse labelled golden cells and cropped GPU numeric outputs

Acceptance oracle: Manual expected colours/positions plus unchanged reference; no blank all-zero passes.

Related failure to probe: Nonwrapping edges default to ocean instead of land continuation; darkness seams.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)
- [windows-vulkan.sol.md](evidence/handover/windows-vulkan.sol.md)
- `/mnt/datasets/c3r-build/smoke-resized-sol/handover.md`
- `/mnt/datasets/c3r-build/smoke-resized-sol/smoke-integration-gpu.log`
- [tiles555-oracle-results.json](evidence/handover/creative-qa/shader-perf-sol/tiles555-oracle-results.json)
- [tiles555-artifact.json](evidence/handover/creative-qa/shader-perf-sol/tiles555-artifact.json)
- [union-package-test-results.json](evidence/handover/creative-qa/shader-perf-sol/union-package-test-results.json)
- [native-captured-smoke-union-results.json](evidence/handover/creative-qa/shader-perf-sol/native-captured-smoke-union-results.json)
- `/mnt/datasets/c3r-build/higher-depth-fp16-sol/strict-candidate-results.json`
- `/mnt/datasets/c3r-build/higher-depth-fp16-sol/one_pixel-dual0.log`

<a id="districts"></a>

## districts

C3X district graphics render in all views.

Acceptance: open. Priority P1.

Implementation record: not individually audited.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Load existing C3X district scenario
2. Inspect district art in A/B/magnifier, Z zoom and city overlay

Acceptance oracle: Known district placements and indexed palette colours agree in every view.

Related failure to probe: District art only follows parked game camera.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)

<a id="city-labels-all"></a>

## city labels all

Every city has a label in all independently moved views.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Zoom out until cities outside native camera appear
2. Move B away from A and open magnifier

Acceptance oracle: Expected city name set equals world city set intersecting each view, with no offscreen omission.

Related failure to probe: Labels disappear only at wrap seam or when native camera is parked.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)

<a id="unit-visibility"></a>

## unit visibility

Units outside A remain visible in B and other views.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Place view B around units entirely outside A
2. Move/fight/fortify units through both views

Acceptance oracle: Per-view unit count/position follows world state independently of A.

Related failure to probe: Animation offset or HP overlay clipped by A.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [army-units.sol.md](evidence/handover/army-units.sol.md)

<a id="unit-animation"></a>

## unit animation

Unit motion/action/combat/explosions and active highlight are complete.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Exercise movement, melee, bombardment, bombing and death animations
2. Check active ring across all action frames

Acceptance oracle: Correct frame/order, national colour and persistent active selection colour; explosion/smoke visible.

Related failure to probe: Attack ring flashes incorrect peace colour; smoke missing.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [army-units.sol.md](evidence/handover/army-units.sol.md)
- [tile-popup-render-shadow.sol.md](evidence/handover/tile-popup-render-shadow.sol.md)
- `/mnt/datasets/c3r-build/smoke-resized-sol/handover.md`
- `/mnt/datasets/c3r-build/smoke-resized-sol/smoke-integration-gpu.log`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/units-gpu.log`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/smoke-gpu-final.log`

<a id="unit-army"></a>

## unit army

Army national/body/flag colours and aggregate HP/strength/movement.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Load army with mixed members, damaged member, hidden nationality and transported units
2. Exercise four eras plus scientific and military leaders

Acceptance oracle: Compare exact real-art palette samples and native member records with visible army HUD.

Related failure to probe: Real-owner flag wrongly uses hidden-nationality body palette.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [army-units.sol.md](evidence/handover/army-units.sol.md)
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/units-gpu.log`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/smoke-gpu-final.log`

<a id="unit-movement-bonuses"></a>

## unit movement bonuses

HUD movement remains correct for civ bonuses and altered road movement.

Acceptance: open. Priority P1.

Implementation record: successor source fixed targeted tests passed not frozen.

Venues: Papa Wine existing assets; native record/synthetic fixture.

Next actions:

1. Freeze successor source and shader,rerun affected4K two-camera army GPU regression
2. Run actual native game with road/naval/civ/C3X bonuses,zero movement,and army member changes;record native values and HUD while last-step animation finishes

Acceptance oracle: HUD native maximum/current movement and army aggregates match captured records in each independent view; no publisher native dereference or identity reuse. Targeted67f3 result is not a changed-shader gate..

Related failure to probe: Army min-member movement wrong when member types share base movement but differ bonuses.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [army-units.sol.md](evidence/handover/army-units.sol.md)
- `/mnt/datasets/c3r-build/army-acceptance-sol-evidence/`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/units-gpu.log`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/smoke-gpu-final.log`

<a id="unit-palette-memory"></a>

## unit palette memory

Indexed national recolouring avoids duplicating all art per nation.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Cycle nations and hidden-nationality units while tracking atlas sprite/page totals

Acceptance oracle: Shared body descriptors/palettes; bounded intentional army flag variants; no per-zoom body copies.

Related failure to probe: Scenario roundtrip accumulates unused country-specific atlas pages.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [army-units.sol.md](evidence/handover/army-units.sol.md)
- [mac-fps-music.sol.md](evidence/handover/mac-fps-music.sol.md)

<a id="zoom-crisp"></a>

## zoom crisp

Free zoom and Z downsampling remain crisp without per-zoom art.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Sweep zoom ratios including 1/2, 3/2, 2 and toggle Z
2. Track atlas image identities

Acceptance oracle: Nearest pixels for world art, shader downsampling, stable atlas size; full-resolution text setting independent.

Related failure to probe: Z changes logical layout, icon size or held cursor scale.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)
- [mac-fps-music.sol.md](evidence/handover/mac-fps-music.sol.md)

<a id="text-native"></a>

## text native

Native Windows text geometry and outline glyphs including ANSI punctuation.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Show long accented/punctuation text at native and fractional zoom
2. Compare final pens before/after glyph residency

Acceptance oracle: Windows GDI pen geometry remains stable; actual glyph availability omits only missing glyphs.

Related failure to probe: Glyph0 boxes, raw ANSI/Unicode punctuation drift, positive lfHeight sign inversion.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [ui-commands-atomic.sol.md](evidence/handover/ui-commands-atomic.sol.md)
- [windows-protocol48.sol.md](evidence/handover/windows-protocol48.sol.md)
- [glyph-cache-independent-oracle.sol.md](evidence/handover/glyph-cache-independent-oracle.sol.md)
- [union-package-test-results.json](evidence/handover/creative-qa/shader-perf-sol/union-package-test-results.json)
- [native-captured-smoke-union-results.json](evidence/handover/creative-qa/shader-perf-sol/native-captured-smoke-union-results.json)
- `/mnt/datasets/c3r-build/release-1019/artifacts/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/wine-direct-results.json`
- `/mnt/datasets/c3r-build/higher-depth-fp16-sol/strict-candidate-results.json`
- `/mnt/datasets/c3r-build/higher-depth-fp16-sol/one_pixel-dual0.log`

<a id="text-tooltip-fit"></a>

## text tooltip fit

Tooltip text/background/borders fit across DPI and pane boundaries.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Open long tooltip for first time at each edge of A/B under scale1/2/3
2. Repeat after font cache warm

Acceptance oracle: Last glyph and underline inside background; whole tooltip transforms together.

Related failure to probe: Warm-only success hides first-use font width drift.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [ui-commands-atomic.sol.md](evidence/handover/ui-commands-atomic.sol.md)
- [text-fit-atomic-ui.sol.md](evidence/handover/text-fit-atomic-ui.sol.md)
- [glyph-cache-independent-oracle.sol.md](evidence/handover/glyph-cache-independent-oracle.sol.md)
- [union-package-test-results.json](evidence/handover/creative-qa/shader-perf-sol/union-package-test-results.json)
- [native-captured-smoke-union-results.json](evidence/handover/creative-qa/shader-perf-sol/native-captured-smoke-union-results.json)

<a id="ui-command-atomic"></a>

## ui command atomic

Commands/layout/controls/sprite sources appear as one committed UI group without waiting for optional art.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Next healthy controlled Mac/Linux launch enables CIV3R_TEXT_TRACE=1 and numeric pixel audit
2. Capture timestamped popup/settings onset burst with isolated empty font cache and existing assets

Acceptance oracle: Match sampled plate/text onset and trace coordinates; no layout reflow between unloaded/loaded font states. Claim only sampled presented updates..

Related failure to probe: Fresh native modal targets paired with old draw group; sprite source ID reuse between Presents.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [ui-commands-atomic.sol.md](evidence/handover/ui-commands-atomic.sol.md)
- [ui-present-metadata.sol.md](evidence/handover/ui-present-metadata.sol.md)
- [gpu-worker.sol.md](evidence/handover/gpu-worker.sol.md)
- [mac-audio.sol.md](evidence/handover/mac-audio.sol.md)
- [vm-snapshot.sol.md](evidence/handover/vm-snapshot.sol.md)
- [sol-orchestrator-current.json](evidence/handover/sol-orchestrator-current.json)
- `/mnt/datasets/c3r-build/vm-qa31b-sol/native-ss-preferences-settle/frames.json`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/native-ss-preferences-reopen/frames.json`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/tab-before.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/tab-after.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/ui-readiness.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/source-lookup-test.log`
- [glyph-cache-independent-oracle.sol.md](evidence/handover/glyph-cache-independent-oracle.sol.md)
- [prepared-update-scope.sol.md](evidence/handover/prepared-update-scope.sol.md)
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/cpu-results.json`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/windows-ui-results.json`
- [army-units.sol.md](evidence/handover/army-units.sol.md)
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/toggle-before.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/toggle-after.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/menu-toggle-after.log`
- `/mnt/datasets/c3r-build/release-1019/artifacts/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/wine-direct-results.json`
- [sol-root-macneo-priority-freeze-review.json](evidence/handover/sol-root-macneo-priority-freeze-review.json)
- [priority3be-reconciliation.json](evidence/handover/creative-qa/priority3be-reconciliation.json)

<a id="ui-settings-transactions"></a>

## ui settings transactions

Native/custom preferences preview live, Accept persists, Cancel restores all settings.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Edit every tab including audio/resolution/layout/cursor/clean-map
2. Cancel, reopen, accept, restart
3. Open C3X modal during draft

Acceptance oracle: Draft overlays active settings only until accept; cancel exact original native/custom values; single original round accept/X cancel controls.

Related failure to probe: Changing one control commits other draft; Escape/modal close accidentally saves.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)
- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results.md`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/native-ss-preferences-settle/frames.json`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/native-ss-preferences-reopen/frames.json`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/tab-before.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/tab-after.log`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/cpu-results.json`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/windows-ui-results.json`
- [army-units.sol.md](evidence/handover/army-units.sol.md)
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/toggle-before.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/toggle-after.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/menu-toggle-after.log`
- [sol-root-macneo-priority-freeze-review.json](evidence/handover/sol-root-macneo-priority-freeze-review.json)
- [priority3be-reconciliation.json](evidence/handover/creative-qa/priority3be-reconciliation.json)

<a id="ui-menu-scale"></a>

## ui menu scale

Menu checkbox and percentage mean fraction of available height with no black top/bottom at100%.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Test title, new-world setup and nation selection at100% and smaller percentages
2. Resize window and change pixel scale

Acceptance oracle: Fit ignores version text/bottom decorations, original aspect constraints understood, no prior title behind nation selection.

Related failure to probe: Only title or only setup screen uses wrong popup classification.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [release-deploy.md](evidence/handover/release-deploy.md)
- [ui-layout-4k31b.sol.md](evidence/handover/creative-qa/ui-layout-4k31b.sol.md)
- [army-units.sol.md](evidence/handover/army-units.sol.md)
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/toggle-before.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/toggle-after.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/menu-toggle-after.log`

<a id="ui-c3x-modal"></a>

## ui c3x modal

C3X modal over Preferences keeps settings size and appears centred in top third.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Open Preferences from main menu and in-game A/B
2. Open/close C3X info with draft settings active

Acceptance oracle: Modal top-third centred; Preferences background size unchanged; accepts no through-map input.

Related failure to probe: Pointer pane picks placement rather than stored screen preference.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [release-deploy.md](evidence/handover/release-deploy.md)

<a id="ui-screen-placement"></a>

## ui screen placement

Shared Screens setting places city/advisor/menu/popup consistently without duplicating world.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Place each F-screen/city/menu left/right/floating
2. Change split orientation while open

Acceptance oracle: Only menu content fits chosen pane; whole background world not copied into menu rectangle.

Related failure to probe: Single sub-dialog moves but parent/border stays behind.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [release-deploy.md](evidence/handover/release-deploy.md)
- [tile-popup-menus.sol.md](evidence/handover/tile-popup-menus.sol.md)

<a id="ui-other-pane"></a>

## ui other pane

Other pane remains usable when permissible menu/city open.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Open city in A, hover/pan B and attempt legal unit actions
2. Repeat with truly modal popup

Acceptance oracle: B follows policy: nonmodal navigation remains active; modal popup freezes map deliberately.

Related failure to probe: Global modal workaround disables both views for any screen.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [tile-popup-menus.sol.md](evidence/handover/tile-popup-menus.sol.md)

<a id="split-independent"></a>

## split independent

Split panes have independent movement, focus and layer visibility.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Move A/B to disjoint terrain with different zooms
2. Compare edge scroll and fractional smooth pan

Acceptance oracle: Each pane rectangle and camera are correct; equal configured edge speed; A does not snap to parked game grid.

Related failure to probe: White camera viewport shows whole window; B objects depend on A.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)
- [mac-fps-music.sol.md](evidence/handover/mac-fps-music.sol.md)
- [ui-layout-4k31b.sol.md](evidence/handover/creative-qa/ui-layout-4k31b.sol.md)

<a id="split-fog"></a>

## split fog

Fog/clean-map settings are independent per view while sharing world layers.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Set territory-only A and unit-only B
2. Toggle fog/revealed layers, Z and magnifier

Acceptance oracle: Layer/category masks affect only selected view and retain correct player fog.

Related failure to probe: Global mask hides units in all panes; zoomed-pass camera sees unrelated UI glyphs.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [mac-fps-music.sol.md](evidence/handover/mac-fps-music.sol.md)
- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)

<a id="magnifier-layout"></a>

## magnifier layout

Top strip/floating/mouse magnifiers respect pane bounds, size sliders and horizontal ratios.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Cycle strip/box/at-pointer; heights5%,10%,50%; ratios1:3:1
2. Open title/menu/city with magnifier enabled

Acceptance oracle: No title black box; strip excludes pointer; boxes inside requested pane; ratio horizontal; fallback below pointer only when needed.

Related failure to probe: Strip height control omitted or shared setting silently ignored.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)

<a id="magnifier-hit"></a>

## magnifier hit

Strip barriers and box entry preserve scrolling/cursor behavior.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Edge-scroll upward with strip enabled
2. Cross a floating box and each split separator

Acceptance oracle: Strip does not steal edge-scroll or pointer; floating-box policy distinct.

Related failure to probe: Barrier includes blank box margin or blocks entire top of window.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)

<a id="preview-human-visibility"></a>

## preview human visibility

Scenario preview reveals only human-playable nations.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Compare scenario with AI-only visible territory against allowed human list

Acceptance oracle: Union of human-playable revealed cells only; no hidden AI map leakage.

Related failure to probe: Changing selected nation leaves previous scenario reveal mask.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.md](evidence/handover/preview-smear.md)
- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/ui-readiness.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/source-lookup-test.log`

<a id="preview-progressive"></a>

## preview progressive

Scenario selection switches immediately while optional preview/art loads progressively.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Navigate slow and cached scenarios using key/mouse rapidly
2. Artificially slow optional art read in isolated runner

Acceptance oracle: Selection/menu complete immediately; busy icon while preview pending; no global missing-art screen.

Related failure to probe: Late old preview overwrites current selection or blocks nation selection.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)
- [scenario-crash.sol.md](evidence/handover/scenario-crash.sol.md)
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/ui-readiness.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/source-lookup-test.log`

<a id="preview-smear"></a>

## preview smear

Fitted preview columns/visible extents never smear under fractional scale.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Use tall/narrow/negative-phase preview quads at varied window sizes
2. Compare GPU golden raster extents

Acceptance oracle: No duplicate columns/full-window vertical stripes; exact signed ceil sampling.

Related failure to probe: Direct-at-zoom path differs from source-picture path across Vulkan/Metal/DX12.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)

<a id="preview-capture"></a>

## preview capture

Double-click preview enters x2 captured world view with release hint.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Double-click preview while loading, then after ready; release Ctrl

Acceptance oracle: Only ready world enters correct x2 capture and displays Ctrl-release hint.

Related failure to probe: Doubleclick stale scenario captures old world.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)

<a id="scenario-generation"></a>

## scenario generation

Rapid scenario roundtrips preserve atlas/glyph IDs and never hang.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Alternate ten scenarios100 times; cancel whole builds at several boundaries
2. Return to title, nation selection, then start game

Acceptance oracle: Every frame namespace matches whole epoch; old job/result cannot win; no vanished backdrop/garbage glyphs; watchdog stays progressing.

Related failure to probe: Empty growth leaps pending nonempty growth; abandoned unsent generation stalls preload.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [scenario-crash.sol.md](evidence/handover/scenario-crash.sol.md)
- [gpu-worker.sol.md](evidence/handover/gpu-worker.sol.md)

<a id="scenario-memory"></a>

## scenario memory

Cancelled art/uploads and repeated scenario menus remain memory bounded.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Repeat scenario churn100 times under a bounded memory sampler
2. Close/reopen menu and force worker resize during upload

Acceptance oracle: CPU/GPU resident plus retired allocations return to bounded plateau; no unbounded canceled whole or reply payload retention.

Related failure to probe: Latest slot retains full initial atlas forever or old native source map.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [scenario-crash.sol.md](evidence/handover/scenario-crash.sol.md)
- [gpu-worker.sol.md](evidence/handover/gpu-worker.sol.md)
- [renderer-sync.sol.md](evidence/handover/renderer-sync.sol.md)
- [worker-cpu-prefix.sol.md](evidence/handover/worker-cpu-prefix.sol.md)
- [worker-prefix-independent-review.sol.json](evidence/handover/worker-prefix-independent-review.sol.json)
- [ui-command-transactions.sol.md](evidence/handover/ui-command-transactions.sol.md)
- [glyph-cache-independent-oracle.sol.md](evidence/handover/glyph-cache-independent-oracle.sol.md)

<a id="cursor-os"></a>

## cursor os

Uncaptured physical OS cursor and icon updates stay independent of slow GPU.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Use an owned runtime with permitted thread-debug mechanism
2. Stall only GPU worker,then move OS cursor and change hover shape

Acceptance oracle: Cursor and input continue while presented frame is unchanged; no GPU completion dependency..

Related failure to probe: Cursor-build completion needs another frame to become visible.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- [gpu-worker.sol.md](evidence/handover/gpu-worker.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results-controls.md`

<a id="cursor-captured"></a>

## cursor captured

Captured/Alt virtual cursor hides physical cursor and hands off safely at edges.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Create explicit Drag/captured fixture, record settings and pointer start/end
2. Exercise vertical/horizontal movement independently,66 confirmation,and focus loss

Acceptance oracle: Numeric camera displacement and sensitivity agree with settings;66 is honored; no pointer drift after release..

Related failure to probe: Window re-entry warps pointer from another compositor surface.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- [targeting-cursors.sol.md](evidence/handover/targeting-cursors.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results-controls.md`

<a id="cursor-scale"></a>

## cursor scale

Cursor scales with integer game pixel blocks without blurry/shadow regression.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Switch pixel scale1..6 and cursor sizes including192px
2. Target valid/invalid cells in each pane

Acceptance oracle: Consistent physical hotspot, dark outline/shadow preserved, uncaptured icon remains OS-managed.

Related failure to probe: Icon art cached for wrong DPI generation.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)

<a id="cursor-target-modes"></a>

## cursor target modes

Only go-to/stack modes tile-snap; rebase/airlift/bombard invalid target icon works.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Exercise actual mission legality over valid/invalid tiles and switch modes via mouse/hotkey
2. Do so free, captured and Alt

Acceptance oracle: Native target legality agrees with displayed frame/icon; only move/stack logical targets snap.

Related failure to probe: Policy unit tests pass while actual game field lags or native routine evaluates parked tile.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [targeting-cursors.sol.md](evidence/handover/targeting-cursors.sol.md)
- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results.md`

<a id="cursor-civ4-drag"></a>

## cursor civ4 drag

Civ IV style right-drag moves units, click opens menu.

Acceptance: open. Priority P1.

Implementation record: not individually audited.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Right click without drag, short drag, long drag, release outside viewport
2. Cancel by Escape/Alt/focus loss

Acceptance oracle: Correct disambiguation and destination; no accidental order on capture click.

Related failure to probe: Release swallowed by different pane/magnifier.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results-controls.md`

<a id="capture-entry"></a>

## capture entry

Clicking capture pane captures without world click or recenter.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Click multiple positions to recapture with selected unit and popup closed

Acceptance oracle: No unit action/world click and no unwanted camera jump.

Related failure to probe: Capture gesture propagates release as command.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results-controls.md`

<a id="capture-sensitivity"></a>

## capture sensitivity

Only captured mode gets configurable extra mouse sensitivity.

Acceptance: open. Priority P1.

Implementation record: not individually audited.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Change captured sensitivity through settings draft, free/captured/Alt motions

Acceptance oracle: Scale only relative captured world deltas; cancel restores value.

Related failure to probe: Physical OS pointer or Alt navigation inherits multiplier.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results-controls.md`

<a id="alt-navigation"></a>

## alt navigation

Directional Alt navigation animates at configured cadence and stays in correct bounds.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Navigate title/new-world/options/difficulty/AI aggression/Civilopedia links/status end-turn and unit select
2. Hold/release Alt with mouse direction changes

Acceptance oracle: Directional targets and visible content anchors, smooth transition duration per configured cadence; no reset to capture pane.

Related failure to probe: Nearest Euclidean target ignores requested direction; text-only rows snap into blank center.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- [targeting-cursors.sol.md](evidence/handover/targeting-cursors.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results.md`
- [tile-popup-menus.sol.md](evidence/handover/tile-popup-menus.sol.md)

<a id="alt-f1-city"></a>

## alt f1 city

F1 cities/buildings and city production controls are selectable in Alt mode.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Open city from capture; production default focus
2. F1 left-click selectable right-click-only build entries; switch cities

Acceptance oracle: Correct target, right-click equivalent where requested, physical capture policy respected.

Related failure to probe: City worked-tile targets remain tied to native camera.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- [army-units.sol.md](evidence/handover/army-units.sol.md)
- `/mnt/datasets/c3r-build/city-worked-alt-sol-evidence/`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/cpu-results.json`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/windows-ui-results.json`

<a id="slider-edit"></a>

## slider edit

Alt slider drag/arrow edits support Preview/Accept/Cancel and clear focus feedback.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Select native/custom slider; arrows at min/max; mouse-adjust; click accept; right/Escape cancel
2. Move across another control

Acceptance oracle: Clamped exact native steps, no lost queued steps, restore original value on cancel, visible focus border without hover art.

Related failure to probe: Keyboard edit renews stale slider address after modal closes.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results.md`
- [tile-popup-menus.sol.md](evidence/handover/tile-popup-menus.sol.md)

<a id="menu-hover"></a>

## menu hover

Hover/selected/pressed states render locally and hotkeys select stack mode.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Enter stack type/all via CtrlX/CtrlG and mouse
2. Hover same control from physical and translated pane cursor

Acceptance oracle: Selected state persists after pointer leaves; translated hover positions match art.

Related failure to probe: Live targeting state uses newer sprite source table than committed draws.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [ui-present-metadata.sol.md](evidence/handover/ui-present-metadata.sol.md)
- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- `/mnt/datasets/c3r-build/vm-qa31b-sol/native-ss-preferences-settle/frames.json`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/native-ss-preferences-reopen/frames.json`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/tab-before.log`
- `/mnt/datasets/c3r-build/ui-atomicity-sol-evidence/tab-after.log`

<a id="menu-smooth-scroll"></a>

## menu smooth scroll

Optional menu list scrolling is smooth without stale hover or click targets.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Scroll scenario/Civilopedia/advisor lists while pointer stays still
2. Toggle smooth-scroll and Alt mid-animation

Acceptance oracle: Displayed rows, hit targets, anchors, hover and selected rows stay aligned.

Related failure to probe: MouseMove for obsolete frame delivered after latest pointer.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)

<a id="popup-world-scale"></a>

## popup world scale

Terrain information popup uses world scale, HUD floor and window fit cap.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Open terrain popup under world zoom1/2/3 and HUD factors; test long/tall content

Acceptance oracle: Inside/body scale together; bounded to pane/window, no oversized/shrunken fixed interior.

Related failure to probe: World popup classification accidentally treats advisor UI as tile content.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [tile-popup-menus.sol.md](evidence/handover/tile-popup-menus.sol.md)

<a id="popup-freeze-dismiss"></a>

## popup freeze dismiss

Anchored tile/rightclick popup freezes map and dismisses on outside click/recapture.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Open popup; attempt edge/drag/keyboard/Ctrlwheel over HUD; click outside; recapture twice

Acceptance oracle: All views stationary while modal; one Escape, swallow outside release, no quit dialog/no stale modal.

Related failure to probe: Queued double dismiss posts extra Escape after modal closes.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [tile-popup-menus.sol.md](evidence/handover/tile-popup-menus.sol.md)

<a id="popup-tile-identity"></a>

## popup tile identity

Terrain Details retains original clicked virtual tile with parked game camera.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Click far-from-native-camera tile; keep menu open1 second and30 seconds
2. Choose terrain details from direct and city menu

Acceptance oracle: Detail title/resource/minimap agrees with original clicked tile.

Related failure to probe: Hover moves native camera after menu-open and replaces target.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [tile-popup-menus.sol.md](evidence/handover/tile-popup-menus.sol.md)

<a id="popup-resource-shadows"></a>

## popup resource shadows

Terrain detail/resource palette and cattle shadows are correct.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Open real cattle/airfield/outpost/radar detail art with shade/blend operations
2. Compare independent91shadowpixel fixture and live displayed result

Acceptance oracle: No black/opaque incorrect shadow; inherited palette/blank rules preserved.

Related failure to probe: Resized colour-table path bypasses indexed smoke/shadow handling.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [tile-popup-render-shadow.sol.md](evidence/handover/tile-popup-render-shadow.sol.md)

<a id="smoke-resized-indexed"></a>

## smoke resized indexed

Non-FLC colour-table smoke224..239 and resized indexed blend are complete.

Acceptance: open. Priority P1.

Implementation record: successor source fixed targeted native CPU and RADV numeric passed not final frozen.

Venues: Papa AMD RADV numeric GPU; existing asset fixtures.

Next actions:

1. Construct real/synthetic colour-table sprite covering224..254
2. Render both resized/unresized and colour overrides

Acceptance oracle: All native smoke/colour-table semantics reproduced without guessed colours.

Related failure to probe: Unscaled fixed but scaled shadows still route plain-resized blit.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [tile-popup-render-shadow.sol.md](evidence/handover/tile-popup-render-shadow.sol.md)
- [sol-orchestrator-current.json](evidence/handover/sol-orchestrator-current.json)
- `/mnt/datasets/c3r-build/smoke-resized-sol/handover.md`
- `/mnt/datasets/c3r-build/smoke-resized-sol/smoke-integration-gpu.log`
- [union-package-test-results.json](evidence/handover/creative-qa/shader-perf-sol/union-package-test-results.json)
- [native-captured-smoke-union-results.json](evidence/handover/creative-qa/shader-perf-sol/native-captured-smoke-union-results.json)

<a id="city-state-sync"></a>

## city state sync

Rapid Next City and alternate-camera UI remain responsive with matching city/camera.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Spam next/previous city100 times during redraw and worker delays
2. Navigate other pane while city open

Acceptance oracle: Title/buildings/worked tiles/outline/camera refer same city; game progress and inputs remain live.

Related failure to probe: One-tile offset; UI local hover hides game-thread hang.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [scenario-crash.sol.md](evidence/handover/scenario-crash.sol.md)
- [tile-popup-menus.sol.md](evidence/handover/tile-popup-menus.sol.md)
- [army-units.sol.md](evidence/handover/army-units.sol.md)
- `/mnt/datasets/c3r-build/city-worked-alt-sol-evidence/`

<a id="notices-expire"></a>

## notices expire

Repeated already-moved notice dedupes and expires by viewer clock.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Spam same unit50 times, then stop game updates while viewer continues
2. Repeat across tiles and new game session

Acceptance oracle: One notice per tile/text, bounded total, renewal only new tell; expiration despite frozen publisher.

Related failure to probe: Stale serial revives after removal or expiry clip hides unrelated sprites.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [unit-notices.sol.md](evidence/handover/unit-notices.sol.md)

<a id="host-busy"></a>

## host busy

Busy indicator distinguishes500 ms/no response and prolonged stuck host.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Pause only game thread/publisher separately while pointer/render continue
2. Resume and compare text/icon colour

Acceptance oracle: Correct orange busy/red stuck thresholds, real wait cursor art, no NEW UNIT icon.

Related failure to probe: GPU pending mistaken for game-dead heartbeat.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)
- [gpu-worker.sol.md](evidence/handover/gpu-worker.sol.md)

<a id="startup-window"></a>

## startup window

Native loading window explains Wine/prefix/translation/art stages through first menu.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Launch cold/warm prefix and deliberately slow startup; cancel each stage
2. Force failure before menu then inspect error/log links

Acceptance oracle: Early visible native status plus elapsed reason, closes on first presented menu, clean bounded process teardown.

Related failure to probe: GPU-init-hidden viewer leaves gap after launcher progress closes.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [startup-progress.sol.md](evidence/handover/startup-progress.sol.md)
- [native-thread-init.sol.md](evidence/handover/native-thread-init.sol.md)
- [vm-snapshot.sol.md](evidence/handover/vm-snapshot.sol.md)

<a id="startup-warm-time"></a>

## startup warm time

Warm launcher avoids repeated registry writes; cold translation remains observable.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Time warm and cold launches using stage timestamps and cache sizes
2. Compare moved installation/invalid saved registry

Acceptance oracle: No unnecessary registry spawn; correct initialization when values differ; translation growth measured not guessed.

Related failure to probe: Existing registry contains value in wrong32/64-bit view.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [startup-progress.sol.md](evidence/handover/startup-progress.sol.md)
- [release-deploy.md](evidence/handover/release-deploy.md)

<a id="audio-music"></a>

## audio music

macOS music plays continuously without short loops or excessive sound CPU.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Owned Intel macOS VM null-audio timing (partial); Physical TS Neo ARM/FEX audio when online.

Next actions:

1. Run gameplay music across track changes while scenario switching/redraw stressed
2. Capture audio callback/thread CPU and timeline with process env/cache proof

Acceptance oracle: No audible repeating fragment or underrun; x87/TSO settings reach actual process.

Related failure to probe: Low sound CPU is mistaken for proof no music loop.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [mac-fps-music.sol.md](evidence/handover/mac-fps-music.sol.md)
- [mac-audio.sol.md](evidence/handover/mac-audio.sol.md)
- [vm-snapshot.sol.md](evidence/handover/vm-snapshot.sol.md)
- [sol-orchestrator-current.json](evidence/handover/sol-orchestrator-current.json)
- `/mnt/datasets/c3r-build/vm-qa31b-sol/vendor-offline-v2-verdict.json`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/miles-offline-v2.json`

<a id="leader-blink"></a>

## leader blink

Leader-head animations have intended timing instead of rapid blinking.

Acceptance: open. Priority P1.

Implementation record: unresolved runtime observation.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Record numeric frame IDs/timestamps for10 seconds in leader/nation selection
2. Compare game-native animation rate

Acceptance oracle: Expected frame duration and blink cadence independent of viewer redraw240Hz.

Related failure to probe: Every redraw advances animation rather than time.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)

<a id="resolution-native"></a>

## resolution native

Native-resolution window, integer Auto/x1..x6 pixels and dynamic resizing.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Resize decorated/fullscreen4K window; Auto then1..6; split/menu/magnifier each
2. Record logical/render/window extents

Acceptance oracle: Actual canvas matches displayed realized resolution and integer blocks; UI occupancy responds; no black screen.

Related failure to probe: Windowedx3 resource stale; DPI conversion applied twice.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)
- [release-deploy.md](evidence/handover/release-deploy.md)
- [ui-layout-4k31b.sol.md](evidence/handover/creative-qa/ui-layout-4k31b.sol.md)

<a id="window-hidpi"></a>

## window hidpi

Only renderer window and correct DPI/placement across desktops and task switch.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Switch windowed/fullscreen/DPI monitors and desktop/workspace
2. Open title before/after running map

Acceptance oracle: Native pixel window fits display; no game window, title uses no world split.

Related failure to probe: Nested KDE/GNOME return warps mouse outside owned window.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [vm-snapshot.sol.md](evidence/handover/vm-snapshot.sol.md)
- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)

<a id="settings-chrome"></a>

## settings chrome

Settings icons and native Accept/Cancel chrome are correct; launcher app searchable/icon present.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Launch by desktop search; inspect Dock/app icon
2. Open each settings tab including capture mode

Acceptance oracle: Civilization icon; icon buttons not labels; exactly one native Accept/Cancel pair; no top3buttons leaked into menu.

Related failure to probe: Accessory progress app steals game activation.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [startup-progress.sol.md](evidence/handover/startup-progress.sol.md)
- [release-deploy.sol.md](evidence/handover/release-deploy.sol.md)

<a id="fps-counter-details"></a>

## fps counter details

FPS details include separate CPU preparation, GPU execution, CPU passes and optional GPU passes.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Enable/disable detailed FPS while changing scenes and GPUpending
2. Match frame ID-linked timestamp results to CPU rounds

Acceptance oracle: Submitted/presented/GPU completiond distinguished; no collection-interval-as-GPU-time; stale timing frame discarded.

Related failure to probe: Accumulated readback windows look like per-frame pass timing.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [gpu-worker.sol.md](evidence/handover/gpu-worker.sol.md)
- [mac-fps-music.sol.md](evidence/handover/mac-fps-music.sol.md)
- `/mnt/datasets/c3r-build/root-worker-gaps-sol/extraction.json`
- `/mnt/datasets/c3r-build/root-worker-gaps-sol/test.log`

<a id="gpu-worker-isolation"></a>

## gpu worker isolation

All GPU upload/draw/acquire work is off OS main thread; main cursor remains live.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Delay surface acquisition, art upload and queue completion independently
2. Keep OS cursor/icon/settings input moving

Acceptance oracle: Main processes OS input without GPU waits; GPUactor sole renderer owner; immutable bounded pending world.

Related failure to probe: Blocked window surface binding deadlocks GPU waiting on main.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [gpu-worker.sol.md](evidence/handover/gpu-worker.sol.md)
- [native-thread-init.sol.md](evidence/handover/native-thread-init.sol.md)
- [worker-cpu-prefix.sol.md](evidence/handover/worker-cpu-prefix.sol.md)
- [worker-prefix-independent-review.sol.json](evidence/handover/worker-prefix-independent-review.sol.json)
- [ui-command-transactions.sol.md](evidence/handover/ui-command-transactions.sol.md)
- `/mnt/datasets/c3r-build/root-worker-gaps-sol/extraction.json`
- `/mnt/datasets/c3r-build/root-worker-gaps-sol/test.log`

<a id="gpu-transactions"></a>

## gpu transactions

Resize/lost surfaces/cancelled epochs/failure replies preserve image-mapping pairing.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Inject acquire Retry, Lost, terminal atlas error, resize during bitmap readback
2. Interleave ArtCommitted/Presented at watermark boundary

Acceptance oracle: Only matching frame/config activation; future replies staged; terminal errors surfaced; no old-canvas bitmap installed.

Related failure to probe: Latest progress crosses FIFO ack cutoff; frame Arc retained until main mutates.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [gpu-worker.sol.md](evidence/handover/gpu-worker.sol.md)
- [ui-present-metadata.sol.md](evidence/handover/ui-present-metadata.sol.md)
- [worker-cpu-prefix.sol.md](evidence/handover/worker-cpu-prefix.sol.md)
- [worker-prefix-independent-review.sol.json](evidence/handover/worker-prefix-independent-review.sol.json)
- [ui-command-transactions.sol.md](evidence/handover/ui-command-transactions.sol.md)
- [glyph-cache-independent-oracle.sol.md](evidence/handover/glyph-cache-independent-oracle.sol.md)
- [prepared-update-scope.sol.md](evidence/handover/prepared-update-scope.sol.md)
- `/mnt/datasets/c3r-build/root-union-validation-sol/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/root-aggregate-review.json`
- [union-package-test-results.json](evidence/handover/creative-qa/shader-perf-sol/union-package-test-results.json)
- `/mnt/datasets/c3r-build/release-1019/artifacts/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/release-validation.json`

<a id="gpu-three-buffers"></a>

## gpu three buffers

Small frame controls stay immutable across three in-flight GPU submissions.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa AMD RADV numeric GPU; Native Windows Vulkan and minimum WARP.

Next actions:

1. Submit3different uniforms/cameras/text palettes before completion
2. Try fourth before/after completion poll

Acceptance oracle: Three snapshots render exact independent values; no CPU overwrite or GPU global wait.

Related failure to probe: Static frame passes while dynamic full redraw reuses wrong control slot.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [renderer-sync.sol.md](evidence/handover/renderer-sync.sol.md)
- [windows-protocol48.sol.md](evidence/handover/windows-protocol48.sol.md)
- [glyph-cache-independent-oracle.sol.md](evidence/handover/glyph-cache-independent-oracle.sol.md)
- [prepared-update-scope.sol.md](evidence/handover/prepared-update-scope.sol.md)
- [union-package-test-results.json](evidence/handover/creative-qa/shader-perf-sol/union-package-test-results.json)
- [native-captured-smoke-union-results.json](evidence/handover/creative-qa/shader-perf-sol/native-captured-smoke-union-results.json)
- [native-app-status.md](evidence/handover/creative-qa/text-perf-sol/native-app-status.md)

<a id="gpu-no-cpu-render"></a>

## gpu no cpu render

GPU shader paths draw solid UI/text/indexed/direct art; CPU only state/layout/oracle.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Trace atlas-free solid/text/background draw through production path
2. Compare static and dynamic full redraw CPU profiles

Acceptance oracle: No shipped CPU raster/composition fallback; numeric CPU reference only tests; bitmap copies completed GPU pixels.

Related failure to probe: A new UI fallback synthesizes CPU fill images instead of procedural solids.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [army-units.sol.md](evidence/handover/army-units.sol.md)
- [ui-commands-atomic.sol.md](evidence/handover/ui-commands-atomic.sol.md)
- [glyph-cache-independent-oracle.sol.md](evidence/handover/glyph-cache-independent-oracle.sol.md)

<a id="platform-linux-dualstack"></a>

## platform linux dualstack

Wine dual-stack native Linux remains correct with game rendering disabled.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine existing assets.

Next actions:

1. Run actual game in same process native Linux, open menus, start map, move unit and end turn

Acceptance oracle: PE game/native unixlib share PID; correct input/rendering and clean stop.

Related failure to probe: Standalone external viewer accidentally used while claiming dual-stack.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [release-deploy.sol.md](evidence/handover/release-deploy.sol.md)
- [native-thread-init.sol.md](evidence/handover/native-thread-init.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results.md`

<a id="platform-mac-dualstack"></a>

## platform mac dualstack

Actual single-process Intel macOS game/native runtime plus Apple ARM/FEX.

Acceptance: open. Priority P0.

Implementation record: runtime menu verified gameplay open.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Reproduce31b title runtime then load existing save/start scenario and interact on map

Acceptance oracle: Same PID maps i686 game, bridge, shim, native unixlib; AppKit main+GPUworker; bounded stop.

Related failure to probe: Menu-only startup mistaken for complete gameplay coverage.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [vm-snapshot.sol.md](evidence/handover/vm-snapshot.sol.md)
- [native-thread-init.sol.md](evidence/handover/native-thread-init.sol.md)
- [mac-audio.sol.md](evidence/handover/mac-audio.sol.md)
- [sol-orchestrator-current.json](evidence/handover/sol-orchestrator-current.json)
- `/mnt/datasets/c3r-build/launcher-cleanup-sol/audit.json`
- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- `/mnt/datasets/c3r-build/vm-qa31b-sol/vendor-offline-v2-verdict.json`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/miles-offline-v2.json`
- [sol-root-macneo-priority-freeze-review.json](evidence/handover/sol-root-macneo-priority-freeze-review.json)
- [priority3be-reconciliation.json](evidence/handover/creative-qa/priority3be-reconciliation.json)

<a id="platform-metal"></a>

## platform metal

Physical A18 Metal/FEX rendering, music and performance.

Acceptance: open. Priority P0.

Implementation record: blocked only physical device offline.

Venues: Physical TS Neo only after reconnect; no builds there.

Next actions:

1. After reconnect deploy paired successor and run title/scenario/map/city/quit soak

Acceptance oracle: Production backend Metal on A18; no bitmap/software CPU performance claims.

Related failure to probe: Intel SwiftShader correctness extrapolated to ARM shader/performance.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [release-deploy.sol.md](evidence/handover/release-deploy.sol.md)
- [vm-snapshot.sol.md](evidence/handover/vm-snapshot.sol.md)

<a id="platform-windows"></a>

## platform windows

Windows primary Vulkan correctness with minimum D3D compatibility.

Acceptance: open. Priority P1.

Implementation record: numeric native verified actual gameplay open.

Venues: Owned Windows VM; no game-asset transfers.

Next actions:

1. Run retained synthetic shader suite under Vulkan and minimum WARP
2. Run actual game only if existing guest assets already present

Acceptance oracle: Adapter and backend explicitly logged; numeric fidelity; no D3D-specific optimization spread to Vulkan.

Related failure to probe: CPU adapter software correctness counted as hardware performance.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [windows-vulkan.sol.md](evidence/handover/windows-vulkan.sol.md)
- [windows-protocol48.sol.md](evidence/handover/windows-protocol48.sol.md)
- `/mnt/datasets/c3r-build/release-1019/artifacts/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/wine-direct-results.json`

<a id="platform-pointer-api"></a>

## platform pointer api

Portable pointer constraints and keyboard capture behave on Windows/Wayland/macOS.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Focus-loss/reentry, pointer confinement, single-axis barrier, multi-monitor DPI
2. Exercise all3platform APIs with owned window

Acceptance oracle: No pointer movement outside owned surface, stable barrier and capture state.

Related failure to probe: UI tests only cover simulated layout and miss compositor confinement.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results-controls.md`

<a id="vm-template"></a>

## vm template

Prepared desktop snapshot clones boot/login/SSH/read-only assets cleanly under10GB shared cap.

Acceptance: covered with scope limits. Priority P2.

Implementation record: operational evidence complete.

Venues: Owned Intel macOS VM descriptor and prepared template.

Next actions:

1. Clone prepared template; verify login, help, read-only mounts, manifests and memory scope

Acceptance oracle: Fresh clone reaches usable desktop and tools, existing assets untouched; allVMRSS total bounded.

Related failure to probe: Warm existing work VM mistaken for reusable snapshot proof.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [vm-snapshot.sol.md](evidence/handover/vm-snapshot.sol.md)

<a id="release-pairing"></a>

## release pairing

Five matched binaries plus launcher app deploy immediately after build/protocol/shader gate.

Acceptance: open. Priority P0.

Implementation record: artifacts verified deployment blocked physical offline.

Venues: Papa immutable artifacts now; Physical TS Neo when online.

Next actions:

1. Validate current 31b archive hashes/protocol48/SPV embeds and signature installation record
2. When online stop cleanly and atomic install then test

Acceptance oracle: All5same frozen source, starter installed; previous release backup; no partial3filemix.

Related failure to probe: Offline successful build reported as completed Mac deployment.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [release-deploy.sol.md](evidence/handover/release-deploy.sol.md)
- [native-thread-init.sol.md](evidence/handover/native-thread-init.sol.md)
- `/mnt/datasets/c3r-build/root-union-validation-sol/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/root-aggregate-review.json`
- [union-package-test-results.json](evidence/handover/creative-qa/shader-perf-sol/union-package-test-results.json)
- `/mnt/datasets/c3r-build/release-1019/artifacts/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/release-validation.json`
- [sol-root-macneo-priority-freeze-review.json](evidence/handover/sol-root-macneo-priority-freeze-review.json)
- [priority3be-reconciliation.json](evidence/handover/creative-qa/priority3be-reconciliation.json)

<a id="performance-4k-static"></a>

## performance 4k static

Verify 240 FPS AMD iGPU at3840 x 2160, static/cached and forced dynamic work separately.

Acceptance: failed goal not achieved. Priority P0.

Implementation record: performance goal unverified.

Venues: Papa AMD RADV only, isolated GPU timing session.

Next actions:

1. Measure actual 4K output and internal resolution with no cap/Immediate supported
2. Run static UI/cached-map repeat30 seconds after warmup

Acceptance oracle: 240 actual presentations/s and matching frame latency metrics; report cached status, CPU/GPU ms and power; no arbitrary source delays.

Related failure to probe: 4K window actually3832x2106; empty/cached benchmark reported as full redraw.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [mac-fps-music.sol.md](evidence/handover/mac-fps-music.sol.md)
- [renderer-sync.sol.md](evidence/handover/renderer-sync.sol.md)
- [sol-orchestrator-current.json](evidence/handover/sol-orchestrator-current.json)
- [actual-App866-dynamic-summary.json](evidence/handover/creative-qa/actual-App866-dynamic-summary.json)
- `/mnt/datasets/c3r-build/native-app-qualification-sol/final866-native1-dual1-r1/timing-window-summary.json`
- [native-app-status.md](evidence/handover/creative-qa/text-perf-sol/native-app-status.md)

<a id="performance-4k-dynamic"></a>

## performance 4k dynamic

240 FPS AMD iGPU target holds under forced world/UI redraw and realistic interactions.

Acceptance: failed goal not achieved. Priority P0.

Implementation record: performance goal unverified.

Venues: Papa AMD RADV only, exclusive timing; existing game assets.

Next actions:

1. Run seeded full-world camera pan and animation forcing renderer world changes
2. Add split views/magnifier/sharp text, UI list scroll and tooltip transitions
3. Collect30-second p50/p95/p99 actual-present/GPU completion metrics

Acceptance oracle: Per-frame budget4.167 ms with workload/quality explicit; no fidelity reduction; failures identify real hot path.

Related failure to probe: Scene publisher33ms or worker timeout4ms constrains updates; main static idle cadence hides delay.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [mac-fps-music.sol.md](evidence/handover/mac-fps-music.sol.md)
- [renderer-sync.sol.md](evidence/handover/renderer-sync.sol.md)
- [sol-orchestrator-current.json](evidence/handover/sol-orchestrator-current.json)
- [tiles555-oracle-results.json](evidence/handover/creative-qa/shader-perf-sol/tiles555-oracle-results.json)
- [tiles555-artifact.json](evidence/handover/creative-qa/shader-perf-sol/tiles555-artifact.json)
- [actual-App866-dynamic-summary.json](evidence/handover/creative-qa/actual-App866-dynamic-summary.json)
- `/mnt/datasets/c3r-build/native-app-qualification-sol/final866-native1-dual1-r1/timing-window-summary.json`
- [native-app-status.md](evidence/handover/creative-qa/text-perf-sol/native-app-status.md)

<a id="performance-delays"></a>

## performance delays

Audit arbitrary waits/locks without removing necessary pacing/lost-wake safeguards blindly.

Acceptance: open. Priority P0.

Implementation record: measurement required.

Venues: Papa CPU trace + AMD RADV exclusive timing.

Next actions:

1. Trace wake cause and queue latency for publisher33ms fallback, actor4ms idle and frame-clock cadence
2. Compare active-wakeup versus timeout-only dynamic cases

Acceptance oracle: No active producer held behind arbitrary sleep; no new locks; bounded waits justified by data; timestamps distinguish pacing from GPUwork.

Related failure to probe: Removing lost-wake timeout reintroduces Mac hang; treating defaultvsync60 as GPU limit.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [renderer-sync.sol.md](evidence/handover/renderer-sync.sol.md)
- [gpu-worker.sol.md](evidence/handover/gpu-worker.sol.md)
- [scenario-crash.sol.md](evidence/handover/scenario-crash.sol.md)
- [worker-cpu-prefix.sol.md](evidence/handover/worker-cpu-prefix.sol.md)
- [worker-prefix-independent-review.sol.json](evidence/handover/worker-prefix-independent-review.sol.json)
- [ui-command-transactions.sol.md](evidence/handover/ui-command-transactions.sol.md)
- [actual-App866-dynamic-summary.json](evidence/handover/creative-qa/actual-App866-dynamic-summary.json)
- `/mnt/datasets/c3r-build/native-app-qualification-sol/final866-native1-dual1-r1/timing-window-summary.json`
- [native-app-status.md](evidence/handover/creative-qa/text-perf-sol/native-app-status.md)

<a id="diplomacy-ring-arrow"></a>

## diplomacy ring arrow

Configurable flashing active arrow and diplomacy rings across views.

Acceptance: open. Priority P2.

Implementation record: not individually audited.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Cycle peace/war/mutual-defense and custom/inverted ring settings
2. Move/fight active units in A/B

Acceptance oracle: Correct secondary ring colour/thickness, no primary national highlight regression.

Related failure to probe: War ring becomes peace colour only in animation.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [army-units.sol.md](evidence/handover/army-units.sol.md)

<a id="turn-focus"></a>

## turn focus

AI combat/turn animations focus requested pane without moving independent pane.

Acceptance: open. Priority P1.

Implementation record: not individually audited.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. End turn with visible AI combat away from native camera and observeA/B focus

Acceptance oracle: Requested active pane follows event; other pane retains desired camera.

Related failure to probe: Parked native focus drives wrong viewport.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)

<a id="city-worked-alt"></a>

## city worked alt

City worked-tile Alt targets agree with displayed independent city camera.

Acceptance: open. Priority P1.

Implementation record: successor source fixed targeted CPU tests passed native runtime pending.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Open city far from parked camera; Alt select every worked-tile control in split view

Acceptance oracle: Target tile/citizen change matches displayed city tile and city ID.

Related failure to probe: Old native camera index selects different tile.

Evidence limit: Original handover explicitly lists this open. Current city_drawn_targets covers citizen heads and units only; no end-to-end worked-tile Alt result found. Do not assert absent implementation without completing control-tree/layout audit.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)
- [army-units.sol.md](evidence/handover/army-units.sol.md)
- `/mnt/datasets/c3r-build/city-worked-alt-sol-evidence/`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/cpu-results.json`
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/windows-ui-results.json`

<a id="architecture-transport"></a>

## architecture transport

Viewer transport async ownership does not block game/publisher or grow unbounded.

Acceptance: open. Priority P2.

Implementation record: original todo not a proven defect.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Slow/stop reader, disconnect transport, overload scenes then reconnect isolated process

Acceptance oracle: Latest scenes coalesce, bounded art prefixes preserved, shutdown reports failure rather than hang.

Related failure to probe: Transport remains non-tokio despite recommendation; separate implementation style from proven runtime defect.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)
- [scenario-crash.sol.md](evidence/handover/scenario-crash.sol.md)

<a id="regression-start-exit"></a>

## regression start exit

New game/retire/exit/restart clears world mode and resources.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Start map, retire to title, open new world, cancel, load save, quit
2. Repeat10 cycles

Acceptance oracle: Title no stale world/magnifier, all owned processes stop and resources plateau.

Related failure to probe: Previous session notice serial/asset epoch reused across reconnect.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [scenario-crash.sol.md](evidence/handover/scenario-crash.sol.md)
- [vm-snapshot.sol.md](evidence/handover/vm-snapshot.sol.md)
- [mac-audio.sol.md](evidence/handover/mac-audio.sol.md)
- [sol-orchestrator-current.json](evidence/handover/sol-orchestrator-current.json)
- `/mnt/datasets/c3r-build/launcher-cleanup-sol/audit.json`
- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)
- `/mnt/datasets/c3r-build/army-checkpoint-86663ea8/allocator-results.json`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/vendor-offline-v2-verdict.json`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/miles-offline-v2.json`
- `/mnt/datasets/c3r-build/release-1019/artifacts/86663ea806b7f38ef447a5f4de553adc4b402e7f63e697762e15d0b5d7c9989e/wine-direct-results.json`

<a id="settings-fog-and-territory"></a>

## settings fog and territory

Show territory and clean-map presets/categories update intended views immediately.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Cycle territory-only/normal and per-view custom hidden categories while gamepaused

Acceptance oracle: Exactly intended categories appear per camera; settings Cancel restores previous masks.

Related failure to probe: Category visibility correct in base sprites but wrong in zoomed replacements.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [mac-fps-music.sol.md](evidence/handover/mac-fps-music.sol.md)

<a id="viewport-separators"></a>

## viewport separators

High-contrast dividers and capture-release hint stay visible over unexplored world.

Acceptance: open. Priority P2.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Use all-black unexplored map and captured mode with top buttons hidden

Acceptance oracle: Divider remains visible; correct icon/hint placement, no clickable buttons behind hint.

Related failure to probe: Solid rectangles accidentally clipped/no atlas while text visible.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)

<a id="save-load-input"></a>

## save load input

Keyboard/mouse operate new-game, load/save and full turn in each supported runtime.

Acceptance: open. Priority P0.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine using existing local game assets; Owned Intel macOS VM using existing read-only assets.

Next actions:

1. Use private save slot in existing runtime;issue legal move,end turn,save,reload

Acceptance oracle: Native state,unit location,and turn survive save/reload; owned processes shut down without orphan debuggers..

Related failure to probe: Unit tests pass but launcher/load dialog size mapping prevents reaching map.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [release-deploy.sol.md](evidence/handover/release-deploy.sol.md)
- [vm-snapshot.sol.md](evidence/handover/vm-snapshot.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results.md`
- [mac-audio.sol.md](evidence/handover/mac-audio.sol.md)
- `/mnt/datasets/c3r-build/vm-qa31b-sol/vendor-offline-v2-verdict.json`
- `/mnt/datasets/c3r-build/vm-qa31b-sol/miles-offline-v2.json`

<a id="ui-read-only-constraints"></a>

## ui read only constraints

Sol visual/GUI acceptance permitted without touching Nexus/assets/system settings.

Acceptance: open. Priority P0.

Implementation record: authorized test method.

Venues: Owned local test UI/VM only.

Next actions:

1. Use already-owned test windows and existing assets
2. Capture/review controlled screenshots only as test evidence

Acceptance oracle: Apply later Sol authorization; no user question or stale Sonnet gate; preserve hostsettings and read-only mounts.

Related failure to probe: Mistaken handover restriction prevents the actual visual tests user requested.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)

<a id="nvidia-failure-handling"></a>

## nvidia failure handling

Unsupported explicit NVIDIA option should fail clearly rather than hang invisible.

Acceptance: open. Priority P2.

Implementation record: not individually audited.

Venues: Papa CPU argument/error tests; never run NVIDIA GPU.

Next actions:

1. Read-only inspect launcher error path and unsupported adapter validation; do not execute NVIDIA

Acceptance oracle: Deterministic actionable unsupported/error report without invisible game process.

Related failure to probe: Flag selects unavailableGPU then music-only background process.

Evidence limit: Aggregate unit counts and partial runtime checks do not establish full feature acceptance. New reconciled evidence is scoped individually to source, shader, adapter and workload.

Evidence:

- [00-orchestrator.md](evidence/handover/00-orchestrator.md)

<a id="ui-background-sampling"></a>

## ui background sampling

Selectable discard/dither/interpolate background sampling and original native settings buttons.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine existing assets; Papa AMD RADV numeric GPU.

Next actions:

1. Cycle all background samplers at fractional menu zoom with native accept/cancel
2. Compare corner/edge windows in Vulkan and Apple Metal after reconnect

Acceptance oracle: Exact box mean or documented selection and stable native chrome.

Related failure to probe: Metal Naga loop drops last sample despite RADV passing.

Evidence limit: This read-only audit performed no new execution; final application feature acceptance remains separate from numerical unit coverage.

Evidence:

- [release-deploy.md](evidence/handover/release-deploy.md)
- [preview-smear.sol.md](evidence/handover/preview-smear.sol.md)
- [union-package-test-results.json](evidence/handover/creative-qa/shader-perf-sol/union-package-test-results.json)
- [native-captured-smoke-union-results.json](evidence/handover/creative-qa/shader-perf-sol/native-captured-smoke-union-results.json)
- `/mnt/datasets/c3r-build/higher-depth-fp16-sol/strict-candidate-results.json`
- `/mnt/datasets/c3r-build/higher-depth-fp16-sol/one_pixel-dual0.log`

<a id="bombard-range"></a>

## bombard range

Bombard command displays correct range/path and action overlays.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine existing assets; Papa AMD RADV numeric GPU.

Next actions:

1. Press B/G/CtrlG away from parked native camera in A/B and near wrap edge
2. Cancel and switch selected unit

Acceptance oracle: Correct virtual target/range and no native-camera white debug outline.

Related failure to probe: Native overlay only produced for original game viewport.

Evidence limit: This read-only audit performed no new execution; final application feature acceptance remains separate from numerical unit coverage.

Evidence:

- [targeting-cursors.sol.md](evidence/handover/targeting-cursors.sol.md)
- `/mnt/datasets/c3r-build/input-runtime31b-sol/results.md`

<a id="mirrored-cursor"></a>

## mirrored cursor

Translated pointer in a menu pane keeps arrow shape while physical pointer outside.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine existing assets; Papa AMD RADV numeric GPU.

Next actions:

1. Move real cursor into other pane while translated menu pointer hovers buttons and links

Acceptance oracle: Translated icon remains arrow over menu and action cursor over world.

Related failure to probe: World targeting cursor replaces translated menu arrow.

Evidence limit: This read-only audit performed no new execution; final application feature acceptance remains separate from numerical unit coverage.

Evidence:

- [slider-arrow-keys.sol.md](evidence/handover/slider-arrow-keys.sol.md)

<a id="windows-ephemeral-template"></a>

## windows ephemeral template

Windows golden VM clones are small,2GB RAM,30GB disk,offline-ready and usable for testing.

Acceptance: open. Priority P2.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine existing assets; Papa AMD RADV numeric GPU.

Next actions:

1. Read prepared Windows descriptors and test fresh owned clone boot/help/driver identity with no source assets copied

Acceptance oracle: Independent ephemeral overlay, expected RAM/disk, automatic offline user; original golden untouched.

Related failure to probe: Existing warm VM hides missing first-boot driver/account setup.

Evidence limit: This read-only audit performed no new execution; final application feature acceptance remains separate from numerical unit coverage.

Evidence:

- [windows-vm.sol.md](evidence/handover/windows-vm.sol.md)
- [windows-protocol48.sol.md](evidence/handover/windows-protocol48.sol.md)

<a id="low-game-resolution"></a>

## low game resolution

Original game runs at supported low resolution independent of native renderer output.

Acceptance: open. Priority P1.

Implementation record: reported implemented not feature runtime verified.

Venues: Papa Wine existing assets; Papa AMD RADV numeric GPU.

Next actions:

1. Read game launch configuration; record original game client versus renderer4K output
2. Exercise pointer and menu transforms

Acceptance oracle: Original low resolution supported by actual Civ3 build; renderer remains native-sized and coordinate-correct.

Related failure to probe: Native game client follows desktop4K and consumes translation/memory needlessly.

Evidence limit: This read-only audit performed no new execution; final application feature acceptance remains separate from numerical unit coverage.

Evidence:

- [release-deploy.sol.md](evidence/handover/release-deploy.sol.md)
