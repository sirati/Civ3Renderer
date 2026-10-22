# Renderer subagent inventory

The original Codex session has 96 recovered descendant sessions: 66 direct agents, 28 children of those agents and two deeper descendants. All 96 have a local report record. The initial root transcript exposed only 71 authors; session metadata recovered the remaining descendants. One duplicate `unit_notices` spawn attempt was rejected and does not represent another agent.

Task names below are historical identities. Do not send continuation messages to these paths without checking the current orchestration system. Historical retry-limit failures do not establish completion or live availability. The latest user request is documentation; no agents were restarted or created for this handover.

The inventory distinguishes completed implementation/review/preparation from current runtime acceptance. Every row links the agent's latest retained reports and original transcript location. Source implementations are committed in the main or private development checkpoint as described in the main handover; a historical report saying "uncommitted" describes its own date.

## Inherited Claude handovers

These eleven task reports and the old orchestrator report were present when the Sol continuation resumed. They preserve the earlier Claude/Opus work, including test recipes, source changes and remaining issues. Later Sol reports and user instructions supersede their no-commit rules, old source identities and obsolete readiness policy.

| Inherited task | Original report | Sol continuation |
| --- | --- | --- |
| army-units | [Original](evidence/handover/army-units.md) | [Continuation](evidence/handover/army-units.sol.md) |
| mac-fps-music | [Original](evidence/handover/mac-fps-music.md) | [Continuation](evidence/handover/mac-fps-music.sol.md) |
| preview-smear | [Original](evidence/handover/preview-smear.md) | [Continuation](evidence/handover/preview-smear.sol.md) |
| scenario-crash | [Original](evidence/handover/scenario-crash.md) | [Continuation](evidence/handover/scenario-crash.sol.md) |
| slider-arrow-keys | [Original](evidence/handover/slider-arrow-keys.md) | [Continuation](evidence/handover/slider-arrow-keys.sol.md) |
| targeting-cursors | [Original](evidence/handover/targeting-cursors.md) | [Continuation](evidence/handover/targeting-cursors.sol.md) |
| text-fit-atomic-ui | [Original](evidence/handover/text-fit-atomic-ui.md) | [Continuation](evidence/handover/text-fit-atomic-ui.sol.md) |
| tile-popup-menus | [Original](evidence/handover/tile-popup-menus.md) | [Continuation](evidence/handover/tile-popup-menus.sol.md) |
| tile-popup-render-shadow | [Original](evidence/handover/tile-popup-render-shadow.md) | [Continuation](evidence/handover/tile-popup-render-shadow.sol.md) |
| unit-notices | [Original](evidence/handover/unit-notices.md) | [Continuation](evidence/handover/unit-notices.sol.md) |
| release-deploy | [Original](evidence/handover/release-deploy.md) | [Continuation](evidence/handover/release-deploy.sol.md) |

[Old orchestrator](evidence/handover/00-orchestrator.md) is historical. The [current summary](README.md) resolves superseded instructions and release identities. The [evidence index](evidence-index.md) includes additional GPU, UI, Windows, audio, loading, cache and independent-review reports produced after the initial task files.

## Recovered branches and Linux features

| Agent | Work delivered | Current disposition |
| --- | --- | --- |
| [advisor_portrait](agent-records/advisor_portrait.md) | Transfers popup-owned portraits with their parent position, scale and bounds. | 14 interface-parts tests passed; live popup acceptance open. |
| [alt_controls](agent-records/alt_controls.md) | Native Alt targets for status orb, unit portrait and cargo icons. | Focused source/platform tests passed; current full-game Alt acceptance open. |
| [alt_prefs_tabs](agent-records/alt_prefs_tabs.md) | Suppresses hidden Preferences targets and underlying controls under popups. | Focused tests passed; later aggregate source supersedes unfinished-peer compile warnings. |
| [alt_setup](agent-records/alt_setup.md) | Alt navigation for title/New Game groups and editable seed using native hit maps. | Focused tests passed; live setup acceptance open. |
| [alt_sliders](agent-records/alt_sliders.md) | Alt slider preview/commit/rollback and cancellation behavior. | Focused transaction/navigation tests passed; current settings runtime acceptance open. |
| [camera_decouple](agent-records/camera_decouple.md) | Parks native world camera, publishes semantic AI focus and keeps animations independent; restores native fallback on viewer loss. | Source tests and binary audit passed; live AI/fallback/populated-map acceptance open. |
| [capture_edge](agent-records/capture_edge.md) | Captured-edge confinement, corners and single-axis lines with normal release. | Viewer/check tests passed; platform runtime acceptance open. |
| [capture_sensitivity](agent-records/capture_sensitivity.md) | Configurable captured relative sensitivity with fractional preservation and scale tests. | Targeted settings/Preferences tests passed; live ratios/focus boundaries open. |
| [city_fit](agent-records/city_fit.md) | City UI fitting and isolation from A/B clean-map filters. | Eight focused tests passed; live current city acceptance open. |
| [civilopedia_alt](agent-records/civilopedia_alt.md) | Civilopedia/scenario anchors and clipping that excludes hidden controls across panes. | Focused package suites passed; later wire revisions supersede historical version 35. |
| [clean_map](agent-records/clean_map.md) | Per-camera Game/Normal/Custom masks, 13 categories and separate district-resource layers. | Renderer/scene/bridge checks passed; PCGames offsets verified, other executables have scope limits. |
| [edge_brightness](agent-records/edge_brightness.md) | Outer fog/terrain boundary brightness and false-coastline fixes. | 11 focused tests and i686 check passed; broad visual acceptance open. |
| [edge_scroll_speed](agent-records/edge_scroll_speed.md) | Edge-scroll behavior and focus arbitration so stale AI focus cannot override later selection. | Viewer tests passed; later captured AI keep-visible policy belongs to army_units. |
| [effects](agent-records/effects.md) | Fixed smoke/effects palette growth and tested simultaneous views and paired infantry combat. | Targeted checks passed; nuclear/SAM/SDI and exact presentation latency open. |
| [integration_qa](agent-records/integration_qa.md) | Combined control/camera/notice/boundary plans, artifact verifier and signed-artifact launch gate. | Preparation passed; final recorded report did not execute the combined runtime matrix. |
| [menu_active_view](agent-records/menu_active_view.md) | Menus retain their opening view, scale and clipped hitboxes while oversized panels can pan. | Focused tests passed; full current UI acceptance open. |
| [minimap](agent-records/minimap.md) | Independent Geography/Territory sources and immediate minimap invalidation for HUD/advisor. | Targeted bridge/sys tests passed; current feature acceptance open. |
| [native_alt_sliders](agent-records/native_alt_sliders.md) | Native slider transaction routing clears stale world input before callbacks. | Seven tests and i686 check passed at its checkpoint. |
| [nvidia_launcher](agent-records/nvidia_launcher.md) | Explicit NVIDIA PCI selection and an asset-free headless verification helper. | Historical preparation only; current AMD-only constraint supersedes any NVIDIA run plan. |
| [pending_input](agent-records/pending_input.md) | Recovered pan, hover and targeting branches and signed integration into zoom. | Integrated; desktop HiDPI and exact origin still lacked live checks. |
| [perf_review](agent-records/perf_review.md) | Independent source review of performance/failure-latch changes at 22cd2e4. | Review passed for its source scope; runtime qualification separate. |
| [performance](agent-records/performance.md) | Recovered performance pipeline, async art growth, failure latching and benchmark helpers. Historical 22cd2e4 measurements exceeded 240 FPS in some workloads, with long tails and lower advisor rates. | Historical benchmark complete; steady 4K at 240 FPS and current workload acceptance open. |
| [pointer_constraints](agent-records/pointer_constraints.md) | Wayland walls, gap crossing, lock hints, cancellation and hotplug recovery, merged via signed fixes. | Targeted live checks passed; GNOME and live fractional scaling remain unverified. |
| [qa_prepare](agent-records/qa_prepare.md) | Added exhausted-movement notices and active-view menu/Preferences reopening checks to combined plan. | Plan prepared; runtime evidence is separate. |
| [retirement](agent-records/retirement.md) | Retirement/render-owner fixes and cleanup audit. Preserved unique saves and all 35 retention targets; final report identified six old task-owned prefixes. | Source recovery recorded; cleanup inventory is historical, not a new deletion instruction. |
| [scale_review](agent-records/scale_review.md) | Menu-pan cancellation, release hover mapping and camera redraw/presentation fixes. | Focused scaling passed; early zoom-box failures handed to owners, not a current full pass. |
| [scenario_preview](agent-records/scenario_preview.md) | Scenario cancellation and stale-result protection; later raised the view budget to 32 with a worker-layout regression. | Implementation recorded; current scenario roundtrip acceptance open. |
| [settings_buttons](agent-records/settings_buttons.md) | Preferences placement inherits Menu panel, clips its background and retains tabs/popups. | Implementation recorded; later successor settings work supersedes early test blockers. |
| [smooth_menus](agent-records/smooth_menus.md) | Optional clipped intermediate row scrolling with correct visible-row hit mapping and release cancellation. | Focused CPU/GPU checks passed; later F1 user regression requires full-game acceptance. |
| [unit_drag_orders](agent-records/unit_drag_orders.md) | Civ IV style order gestures and rescaling of pending destinations without synthetic confirmations. | 19 order tests passed; current runtime acceptance open. |
| [unit_notices](agent-records/unit_notices.md) | Native message expiry audit and Windows-1252 Unicode correction. | Continued by unit_notices_sol; older concurrent build blockers are historical. |
| [unit_visibility](agent-records/unit_visibility.md) | Collects units outside the native A camera through native selection, fog and detection rules. | Targeted tests passed; full current B-view runtime acceptance open. |
| [vm_qa](agent-records/vm_qa.md) | Windows VM combined matrix and save-preserving capture/trace helpers. | Prepared, unlaunched in final report; native guest matrix remains open. |
| [world_input](agent-records/world_input.md) | Independent world input and stationary-pointer refresh after accepted presentation. | Focused tests and viewer suite passed at its checkpoint; live input matrix open. |
| [zoom_detail](agent-records/zoom_detail.md) | World magnifiers use full-resolution art after Z zoom-out while preserving input/coverage. | Focused tests passed; current runtime acceptance open. |
| [zoom_pointer](agent-records/zoom_pointer.md) | Top-strip barriers, floating magnifier placement, capture release and smooth-menu setting hook. | Implementation and focused tests recorded; combined live geometry acceptance open. |

## Platform integration and settings

| Agent | Work delivered | Current disposition |
| --- | --- | --- |
| [macos_bridge](agent-records/macos_bridge.md) | Single-process bridge integration, camera/live-source checks and Wine platform tests. | 443 Wine tests passed at 3a91 source; current matched macOS acceptance remains separate. |
| [macos_icon](agent-records/macos_icon.md) | AppKit icon integration and corrected color-managed native icon oracle. | Targeted native checks passed; installation identity tracked separately. |
| [macos_pointer](agent-records/macos_pointer.md) | Common physical-pixel pointer/keyboard API, platform backends and Preferences fence/drag review. | Targeted checks passed; live cross-platform pointer and stalled-GPU cursor acceptance open. |
| [macos_transport](agent-records/macos_transport.md) | Native macOS transport, release helpers and read-only SMB recovery. | Multiple historical source preparations; latest current identities are in the release section. A later retry-limit failure is not task completion. |
| [macos_vm](agent-records/macos_vm.md) | macOS VM setup and verified complete game transfer to Neo with fonts/mods/saves retained. | Transfer verified; execution acceptance handed to native/runtime owners. |
| [macos_vm_presentation](agent-records/macos_vm_presentation.md) | Native presentation, ARM64 cross-build validation and corrected Metal art/mask tests. | 774 tests passed at historical candidate; later f510 acceptance is separate. |
| [macos_vm_templates](agent-records/macos_vm_templates.md) | Prepared reusable macOS snapshots and verified stopped clone instances. | Template work covered with scope limits; liveness must be rechecked. |
| [pointer_vm_qa](agent-records/pointer_vm_qa.md) | VM pointer/UI tests and original vendor-only audio control after VM audio correction. | Vendor control exited 0; later retry failure is historical, not a current live handle. |
| [rust_launcher](agent-records/rust_launcher.md) | Rust native Wine/FEX launcher, recovery/comparison behavior and Neo/Intel VM guide. | Documentation and targeted implementation settled; current runtime acceptance separate. |
| [settings_transaction](agent-records/settings_transaction.md) | Deferred persistence, preview/cancel/accept behavior and native slider transactions. | 51 focused tests and Windows check passed at freeze; live UI/audio sequence open. |
| [ui_background_dither](agent-records/ui_background_dither.md) | Texture-only background sampling with alpha masks and later preload overflow saturation/admission fix. | Focused CPU/GPU tests passed; later upscaling/preview behavior needs current acceptance. |
| [ui_feature_audit](agent-records/ui_feature_audit.md) | Menu scale/fit and texture-only background behavior; scenario preview lifecycle coordination. | Focused evidence and recipe prepared; current UI acceptance matrix open. |

## Sol continuation and later investigations

| Agent | Work delivered | Current disposition |
| --- | --- | --- |
| [army_units](agent-records/army_units.md) | Army membership/national colors/aggregate stats; atomic UI review; captured AI keep-visible and protocol 50 camera-arrival backpressure; combat diagnostics and outcomes. | Implemented fixes and targeted tests recorded. f510 diagnostic PE build/11-test gate and matched battles remain open. |
| [glyph_cache_sol](agent-records/glyph_cache_sol.md) | Glyph-camera/cache audits and isolated wider integer path with continuation, retirement and worker integration. | Source/preparation reviews passed; private compiler/numerical GPU/adoption checks remain open. |
| [mac_audio_sol](agent-records/mac_audio_sol.md) | Miles/native audio ownership, no-device causality, loaded/quit controls and independent driver review. | Vendor/title passes scoped valid. Latest review finds log truncation, title-vs-map readiness and pre-adoption capture blockers. |
| [mac_fps_music](agent-records/mac_fps_music.md) | Mac frame/music diagnosis, narrow DX12 FP16 compatibility fix, GPU timing/counter work and controlled culling preparation. | Narrow compatibility tests passed. 240 FPS, CPU-busy attribution and physical music soak remain open. |
| [preview_smear](agent-records/preview_smear.md) | Fractional preview geometry, native DX12 signed-ceil correction and later PE art-loading attribution. | Targeted correctness passed; full rapid scenario switching and load-cost acceptance open. |
| [release_deploy_sol](agent-records/release_deploy_sol.md) | Source freezes, cross-builds, paired artifacts, signed app staging/install, identity-safe cancellation, cleanup inventories and off-host Git backups. | Neo 3be installed, f510 staged only. Source backups closed; release runtime acceptance open. |
| [scenario_crash](agent-records/scenario_crash.md) | Atlas-generation/readiness/cancellation fixes plus independent wider-candidate orchestration reviews. | Targeted source work recorded; final wider review clears preparation only, not compiler/GPU/runtime acceptance. |
| [shader_perf_sol](agent-records/shader_perf_sol.md) | Shader audit, dual-source subpixel, binary-cell recorded4K gate and private actual-app/thin-view preparation. | Recorded cell-only correctness passed; strict wider rounding, app A/B/adoption and 240 FPS remain open. |
| [slider_arrow_keys](agent-records/slider_arrow_keys.md) | Selected custom/native slider stepping, preview rollback, startup/failure cleanup reviews and acceptance-ledger reconciliation. | Targeted source tests recorded; ledgers retain open runtime checks and were later superseded in specific facts. |
| [sparse_cells_sol](agent-records/sparse_cells_sol.md) | Read-only sparse-cell compaction audit with painter-order, invalidation and retirement risks. | Design delivered; implementation deferred pending current-app attribution. |
| [startup_progress_sol](agent-records/startup_progress_sol.md) | Native startup status/error/cancel UI and readiness; Intel CPU tests; audio lifetime/cancellation/combat diagnostic reviews; corrected offline comparator. | Scoped tests passed. Comparator successor2 has 27 pure tests and independent review; real matched battle evidence open. |
| [targeting_cursors](agent-records/targeting_cursors.md) | Move-only tile snapping, invalid-target game icon, native Windows numeric tests and animated busy cursor wake scheduling. | Focused checks passed; full current cursor/platform acceptance open. |
| [text_fit_atomic_ui](agent-records/text_fit_atomic_ui.md) | Native GDI text metrics/sign, final command transactions, independent glyph oracle and Preferences screenshot disposition. | Targeted evidence recorded. Art-wait UiHold design superseded; full UI/fidelity acceptance open. |
| [text_perf_sol](agent-records/text_perf_sol.md) | GPU glyph cache/exactness and main-scene reuse work; pointer fixture fixes; accepted actual f510 native4K baseline. | Three moving runs accepted around 43–44 FPS. 240 FPS failed; CPU busy and broader acceptance open. |
| [tile_popup_menus](agent-records/tile_popup_menus.md) | Popup sizing/placement/freezing/dismissal and opaque modal painter order over Preferences. | Targeted final9b AMD modal gate passed; live current popup/C3X acceptance open. |
| [tile_popup_render_shadow](agent-records/tile_popup_render_shadow.md) | Indexed shadow capture/preload/trace support, real-art pixel correction, compatibility review and native combat wait scaling. | Targeted pixels/checks passed; resized indexed smoke gaps and actual 4x combat runtime proof open. |
| [unit_notices_sol](agent-records/unit_notices_sol.md) | Bounded notice deduplication and viewer-clock leases; canonical scene remains immutable; later combat comparator review. | Focused tests passed; final comparator successor2 review passed offline, native battle/persistence proof open. |
| [vm_snapshot_sol](agent-records/vm_snapshot_sol.md) | macOS snapshot/restore/GUI ownership, no-device causal controls and restoration; later runnable loaded-audio owner driver ce5e104. | Template/restoration closed within scope. Loaded no-device arm incomplete and latest driver has three review blockers. |

## Nested agents

| Agent | Work delivered | Current disposition |
| --- | --- | --- |
| [alt_setup/setup_decode](agent-records/alt_setup--setup_decode.md) | Verified native editable-seed vtable, callbacks and buffer fields. | Read-only evidence delivered to parent; no live setup acceptance. |
| [camera_decouple/ai_focus_evidence](agent-records/camera_decouple--ai_focus_evidence.md) | Disassembly and callsite evidence for native AI/move/bombard camera events and visibility. | Evidence delivered; parent owns final implementation. |
| [camera_decouple/animation_independence](agent-records/camera_decouple--animation_independence.md) | Keeps off-camera units/effects in native animation membership without changing elapsed timing. | 11 bridge, 2 Wine/i686 tests and 36 binary checks passed; live acceptance open. |
| [clean_map/clean_engine](agent-records/clean_map--clean_engine.md) | Audited native flags and district/resource category separation. | Parent corrected category ownership; no runtime claim from review. |
| [clean_map/clean_render](agent-records/clean_map--clean_render.md) | Per-camera 13-category filtering and cache invalidation in renderer/reference paths. | Strict AMD suites passed at its checkpoint; current feature acceptance separate. |
| [glyph_cache_sol/wider_retirement_design](agent-records/glyph_cache_sol--wider_retirement_design.md) | Resumable reach discovery preserves the current cell and counts metadata visits across yields. | Parser checks passed; compiler/GPU gates open. |
| [glyph_cache_sol/wider_worker_integration](agent-records/glyph_cache_sol--wider_worker_integration.md) | Separates wider acquisition from CPU preparation and restores periodic diagnostics. | Rustfmt parsing passed; build/GPU gates open. |
| [macos_bridge/scaled_ui_capture](agent-records/macos_bridge--scaled_ui_capture.md) | Captures resized native blits, clipping, palette/opacity and fixed-point sampling with cache identity. | Focused model/screen tests passed; generated-surface resizing remains unsupported. |
| [macos_pointer/wayland_backend](agent-records/macos_pointer--wayland_backend.md) | Wayland physical-pixel capture, exclusions, lock hints and focus-aware constraints. | Six focused tests passed; live compositor coverage separate. |
| [macos_pointer/window_backends](agent-records/macos_pointer--window_backends.md) | Window backend support and forwarded-key ordering through translated WM_CHAR. | 20 unit and 2 Wine hook tests passed; current platform acceptance separate. |
| [pending_input/hover](agent-records/pending_input--hover.md) | Recovered context-row geometry and local feedback across native/in-process views. | Signed c5035cb integrated; small tooltip differences and combined QA limits retained. |
| [pending_input/hover/ui_verify](agent-records/pending_input--hover--ui_verify.md) | Compared context rows, hover/press states and Preferences tabs against game/mirror images. | Context rows exact; small menu tooltip/reference differences retained; not isolated performance. |
| [pending_input/pan_live](agent-records/pending_input--pan_live.md) | Native/in-process pan, momentum, walls, capture and fractional-scale live checks. | Checks passed; desktop HiDPI and exact origin untested; view-budget defect handed to owner. |
| [pending_input/targeting](agent-records/pending_input--targeting.md) | Recovered targeting fixes including settler radius and cursor capture evidence. | Signed source integrated; unique saves/evidence preserved, owned scratch stopped/removed. |
| [pending_input/targeting/live_tester](agent-records/pending_input--targeting--live_tester.md) | Live native/in-process target and cursor checks plus resource shutdown. | Artifacts preserved; VNC/sessions stopped; exact evidence in parent report. |
| [performance/async_growth_design](agent-records/performance--async_growth_design.md) | Async art-growth and startup-failure latch with negative readiness controls. | Signed 22cd2e4 and 194 bridge tests recorded; no false readiness after capture failure. |
| [performance/frame_cap](agent-records/performance--frame_cap.md) | Reviewed strict cap semantics and draw-start pacing overhead. | Explained avoidable capped shortfall; current behavior must be rechecked before changes. |
| [performance/live_tester](agent-records/performance--live_tester.md) | Measured recovered source panning/advisor/drag workloads. | Some rates exceed 240; p99 gaps and advisor rates leave steady 240 FPS unproven. |
| [performance/recover_benchmarks](agent-records/performance--recover_benchmarks.md) | Built immutable 22cd2e4 benchmark helpers and retained source/binary manifests. | CPU build passed; no GPU or output-equivalence claim from helper preparation. |
| [performance/review_performance](agent-records/performance--review_performance.md) | Independent hash and metric audit of 757 run files, 72 review artifacts, 36 phase rows. | Historical evidence accepted; moving-frame and cold-feedback limits preserved. |
| [pointer_constraints/live_verification](agent-records/pointer_constraints--live_verification.md) | Live walls, scrolling, City focus, modifiers and hotplug checks. | Passed on signed fixes; private sessions stopped. |
| [pointer_constraints/protocol_review](agent-records/pointer_constraints--protocol_review.md) | Fixed pending lock hint commitment after Locked and cancellation behavior. | Seven tests passed; integrated by parent. |
| [retirement/retire_crash_review](agent-records/retirement--retire_crash_review.md) | Reviewed underlay ownership/filtering across recursive surfaces/copies. | No concrete findings in final ownership fix; no runtime claim from review. |
| [retirement/retire_inproc_test](agent-records/retirement--retire_inproc_test.md) | In-process retirement tests and ownership/shutdown inventory. | Retained evidence; no broad ownership claim for older prefixes. |
| [retirement/retire_window_test](agent-records/retirement--retire_window_test.md) | Native retirement tests, prefix ownership and save-preservation audit. | Unique saves hash-matched retained copies; older prefix ownership remains separately documented. |
| [scenario_preview/preview_review](agent-records/scenario_preview--preview_review.md) | Shared cancellable image loading, native compilation cancellation and stale-result rejection. | Signed fixes and focused CPU checks recorded; no live claim. |
| [ui_background_dither/background_metadata](agent-records/ui_background_dither--background_metadata.md) | Identified 55 pure texture rectangles across 13 sources, excluding controls/bevels/text. | Read-only metadata delivered; title art remains nearest. |
| [ui_background_dither/source_alpha_split](agent-records/ui_background_dither--source_alpha_split.md) | Added complementary alpha/background-mask and nearest/dither/clip/move regressions. | 28 menu tests passed at checkpoint. |
| [ui_feature_audit/scenario_preview_lifecycle](agent-records/ui_feature_audit--scenario_preview_lifecycle.md) | Publishes complete scenario menu before optional preview work; cancels/drains stale selection and clears absent world. | 9 preview and 4 scheduler tests plus i686 check passed; rapid-switch runtime acceptance open. |
| [world_input/engine_mouse](agent-records/world_input--engine_mouse.md) | Native independent-camera input hooks, key dispatch, city-focus protection and stale-target clearing. | 48 sys tests passed; parent completed later integration. |
