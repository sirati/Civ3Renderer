# Tile popup menus, Sol continuation

Original handover preserved at tile-popup-menus.md. Shared dirty tree preserved, no commits/stash/reset. No Mac builds or GUI inspection.

## Changes in this continuation

- Retained world-scaled tile box, HUD scale floor and fit-to-window cap. Added cap regression for tall and wide bodies.
- Found and fixed outside-click leak through HUD controls. Store the current popup body and use the rendered modal body when available, otherwise its anchored world placement. Any press outside dismisses once and swallows release. Popup clicks continue to reach the game.
- Found Ctrl+wheel over HUD bypassing popup map freeze. Blocked that path and added regression alongside HUD outside-click test.
- Found a concrete queued Escape race. DismissPopup checked modal on game thread but Window::key posted the key afterwards. Two dismissals could queue two Escapes and the second could open quit after the popup closed. Added Window::key_now, guarded to the owning UI thread and using synchronous SendMessageW. Popup dismissal uses it, so subsequent queued dismissals observe the closed modal.
- Reviewed terrain-details chain. Bridge refreshes click_popup_seen throughout anchored popup lifetime; viewer retains anchored press for direct replacement and briefly after close. Client mouse/world moves preserve virtual tile while a click-popup modal is open. Existing city tile regression retained.
- DismissPopup remains wire input tag 17. Shared wire VERSION is now 46 due to other agents, not this continuation.

## Validation

Host scene/tile-viewer unit suites running through nix develop with CARGO_INCREMENTAL=0, TILE_RENDER_ADAPTER=radv, own target /mnt/datasets/c3r-build/tile-popup-menus-sol-target. Log /mnt/datasets/c3r-build/tile-popup-menus-sol-tests.log. Final results to follow below.

Windows/Wine checks and deployment belong to orchestrator. In particular the Windows-only key_now method needs aggregate Windows build/runtime verification. Resource rendering and cattle shadows belong to tile_popup_render_shadow.

Windows regression added to existing inproc keyboard fixture: `immediate_keys_finish_before_return_and_reject_other_threads`. It verifies Escape down/up reach original handler before pumping, a foreign thread is rejected without UI pumping, and subsequent pumping delivers no extra Escape. Orchestrator must run it under Wine.

Host log already confirms passing popup tests: `a_press_on_the_hud_outside_a_click_popup_only_dismisses_it`, `a_world_scaled_popup_fits_the_window_even_when_the_hud_is_larger`, `a_popup_opened_at_a_world_click_holds_every_view_still`, `a_press_beside_a_popup_opened_at_a_world_click_only_dismisses_it`, and `terrain_details_from_a_city_menu_open_at_the_menus_tile_however_long_it_was_open`. Scene suite passed 13 tests. Viewer exhaustive unrelated city/scaling tests still running at this update.

Source hashes after continuation edits:
```
b306546343da95bc7e8729048863df16dc1d3a8c7c303b3a1e3629bf3a486f30  crates/tile-viewer/src/layout.rs
ac900b031a5f5288f296457a71a35a59742112e605c48bfec9bc5ab1c5301f26  crates/civ3-bridge/src/world_input.rs
2f181aa75fe08795b67a47ae41ceceaf71871fc143dbd2a2b2e1dc23d9fcd932  crates/inproc/src/window.rs
9287ff02dbb978c4585f732dd79c0c8dfb55506ad2aff113b1870e50bcb00087  crates/inproc/src/keyboard.rs
f9054ebdea55b251faa97dfa1e68db09a0a67136f2f71204e1264c6896af9ee9  docs/world-input.md
```

## Final host result

Host command cargo test -p tile-viewer -p scene --lib through nix develop exited 0. Scene: 13 passed. Tile-viewer: 381 passed, 3 ignored, no failures, 181.64 seconds execution. Windows-only immediate-key regression awaits orchestrator aggregate Windows/Wine checks. Source hashes above rechecked after host tests and unchanged.

No further concrete popup input/layout defects found. In-game modal close, terrain-details selection and recapture still need runtime verification. Cattle shadows/rendering remain separately owned. Original Opus target directories left untouched because current ownership/use was not established.

Own scratch target directory removed after successful host tests.

## Extended menu/input QA continuation

Root requested broad menu/input adversarial and actual GUI QA beyond release31b. VM GUI input remains coordinated exclusively by vm_snapshot_sol. Runtime pending cases listed individually in /mnt/datasets/c3r-build/tile-popup-menus-extended-qa.json; pending is not a pass claim.

Found new input defect: an arrow arriving after a tab/modal removed the edited slider but before animation tick produced Cancel(oldID) followed by Step(oldID). Recorded failing-before test log /mnt/datasets/c3r-build/tile-popup-menus-extended-before.log with exact events. Fixed SnapInput::route to validate/retire the slider transaction before keyboard target resolution. New tests cover disappearing target, same-ID replacement by non-slider, and held-arrow release after tab replacement. All14 snap::sliders tests pass after fix, log /mnt/datasets/c3r-build/tile-popup-menus-extended-after.log.

Added integration test an_arrow_before_the_next_tick_restores_a_hidden_native_sliders_exact_entry_value in native_sliders/tests.rs. It routes real SnapInput events into native Transaction with a fake native memory-backed slider, previews a value, hides target, then verifies immediate arrow emits no Step/Commit and restores exact original integer through Cancel. First compile attempt blocked by unrelated in-progress unit_views.rs fields native_address/max_move_points absent from civ3_sys::DrawnUnit. Owner and root notified. Native focused rerun pending schema completion.

Unit owner completed DrawnUnit fields. Rerun focused native sliders group PASSED 11/11, including new exact rollback integration. Log /mnt/datasets/c3r-build/tile-popup-menus-extended-native-after.log. Input fix source stable:
```
a09de2f1878c406fb6fd5108726ed4df369c32b4a58573fae938e9bb300ce3b4  crates/tile-viewer/src/snap.rs
88cb9524480042339b97325f0bc4885c41b12419fad8c5d1bb144b1a7e8302ed  crates/tile-viewer/src/snap/sliders.rs
d63f3fed676f5e39e43f3ceab3d5d38d3383ddec0f708f5924b96572b1bc8e8f  crates/civ3-bridge/src/native_sliders/tests.rs
```

## Second extended input defect, stale Alt menu target

Root authorized a separate fix. Failing-before logs:
- /mnt/datasets/c3r-build/tile-popup-menus-empty-transition-before.log: after an empty menu transition tick, target_id still Some(1), leaving its old button actionable.
- /mnt/datasets/c3r-build/tile-popup-menus-menu-replacement-before.log: click after replacement before tick delivered old (130,535), while new menu target was (540,535).

Fix: Snapper clears vanished target/ghost/rested state when no current targets remain and holds visible pointer stationary. It reacquires the nearest current control when controls return. SnapInput refreshes current target on new down/wheel gesture before routing it, rather than depending on next animation tick. Release continues to use original press identity. Added same-ID moving/in-flight button, held-release-after-replacement and wheel-before-tick regressions. Updated old empty-scene test to distinguish preserved pointer position from actionable target.

All80 focused snap tests pass. Log /mnt/datasets/c3r-build/tile-popup-menus-alt-transition-after.log. New snap.rs hash supersedes prior continuation hash:
```
ba3582269d05e65ac296371755c8039306f48c525c3ded3697b009cd3399bc90  crates/tile-viewer/src/snap.rs
88cb9524480042339b97325f0bc4885c41b12419fad8c5d1bb144b1a7e8302ed  crates/tile-viewer/src/snap/sliders.rs
0acdf605cd24c4077048893a799301b68f15af39c3a6e24e83eb7c88d7675a0a  crates/tile-viewer/src/snap/tests.rs
d63f3fed676f5e39e43f3ceab3d5d38d3383ddec0f708f5924b96572b1bc8e8f  crates/civ3-bridge/src/native_sliders/tests.rs
```

VM actual-flow QA is currently blocked before input by Wine mmdevapi stack overflow and orphan debugger windows. vm_snapshot_sol owns GUI lease/recovery and startup owner/root were notified. No VM input injected by this agent. Runtime QA statuses remain pending, not passed.

Final-source native slider rerun PASSED 11/11 after second Snapper fix. Log /mnt/datasets/c3r-build/tile-popup-menus-extended-native-final.log. Both small input fixes are ready for root source integration. Own extended target retained temporarily for any concrete runtime-discovered follow-up; only this agent may clean it.

## Generic popup underlay invariance, new user screenshot

The supplied Science Advisor screenshot exposed a general geometry policy error. In live.rs, all non-Preferences named-screen popups were merged into screen_items and their bounds unioned with screen_part. That union changed the fitted parent screen. Removed this policy entirely: every detected popup remains an independent view and parent bounds come from the separated underlay.

Two additional generic paths were fixed: named_screen_content used popup bounds as a fallback for City/full-window screens, and the City's named-camera group could include modal views. Popups are now excluded from parent content and parent groups; any named screen establishes an independent modal overlay.

C3X actual VM evidence confirmed a separate painter-order defect, beyond geometry. The profile showed the popup at (236,105), viewport(540,143), camera2, then opaque Preferences at(84,0), viewport(843,632), camera3. frame_without_world retained Scene view ID order; Scene IDs put popup257 before screen258. The popup therefore rendered under Preferences. No-world frames now explicitly order modal layers after their parent; world floating/pane overlays are deferred to on_top regardless of placement. Legacy ui=pane stacked views also put modal layers last. Preview Beside menus keep their fixed menu/world rectangles, with independent modal views appended and arranged separately rather than cropped into the menu's source bounds.

Root authorized bounded typed modal-stack support inside the already-unfrozen protocol50. Scene::ModalStack is a four-slot typed array of view IDs in paint order, carried within Interface update11. Existing popup remains the top ID. No new Input or Update tag was claimed. Native modal stack sampling occurs with NativeUi at Present, on the game thread, and participates in captured metadata equality. The native addresses were verified by disassembling ShowDialog605312: this is stored at CACD20[depth], depthCAD388, maximum4; current topCAD384. Invalid depths, holes/duplicates and inconsistent top are rejected.

Extraction splits native modal canvases top-down, measuring each after removing its children, then removes all of them before measuring the named parent. Separate layer IDs and scene views preserve lower popup geometry and pixels across pushes/pops. Mandatory assets include all layers, and hover looks now include each modal view rather than only constant popup257.

Validation so far:
- Native bounded-stack test passes, including depths0..4 and torn/invalid state rejection. tile-popup-native-stack.log.
- All24 interface_parts tests pass, including all14 native named classes with prompts outside their bounds, and simultaneous nested full-canvas modal extraction. tile-popup-stack-extraction.log.
- Geometry regression passes288 configurations with4 push/replacement/dismissal stages each. Source/world cameras and underlay affine/bounds remain identical.
- Opaque pixel regression passes F6, City, Menu, title-Menu, preview Beside and legacy ui=pane, in floating/pane placements. Exact purple popup pixels win over opaque parent; exact yellow nested pixels win over the lower popup; pixels outside top overlay are byte-identical, lower affine is unchanged, and pop restores exact prior composition. It includes actual C3X source1280x800 in1012x632 canvas with Preferences128,16,1024,768 and popup312,100,656,174. tile-popup-pixel-invariance.log.
- First full viewer run passed471, failed3 unrelated scene_prefix metadata fixture tests and ignored3. text_perf_sol identified/fixed its empty-sprite fixture and was notified. Final full CPU rerun underway.

CPU checks use paired3be precompiled shader f9968edd as a type/reference gate, without executing the GPU. A new ignored GPU companion opaque_modal_stack_gpu_matches_the_independent_cpu_pixels compares exact GPU/reference pixels through the complete opaque/push/pop matrix. Broker queued its2min RADV slot, not yet granted. Final GPU acceptance awaits a fresh coherent clock/background sampler pair from root. Actual matched-successor VM C3X and Science Advisor flow acceptance remains required and is not claimed by these CPU tests.

## Final modal GPU and Windows acceptance

Final full host rerun passed scene18 and viewer480 with4 ignored, no failures,95.81s. Log tile-popup-stack-full-host-final.log. After retained world-anchor and arranged-body click regressions, fresh coherent-pair focused popup group passed17/17, log tile-popup-coherent-viewer.log.

Lower world-anchored modal layers retain their anchors and temporary world pan through child push/pop. Cache only an old top becoming a visible lower layer, so direct replacement still clears anchoring. Modal view IDs use native canvas identity. adopt_presented_mapping copies new anchor fields. Arranged popup body input accepts the exact popup body source even where final interface_shown uses a non-modal anchored piece, preventing false outside dismissal.

Exact immutable /mnt/datasets/c3r-build/tile-popup-modal-gpu-sol/opaque-modal-tests SHA256 6ee34b1bb033f8234f7e82b872565a58f2845df95af1b4719c230c024711f112 passed opaque_modal_stack_gpu_matches_the_independent_cpu_pixels on RADV Vulkan PCI0000:72:00.0, Mesa26.2.4, validation environment enabled. One passed, zero failed,3.20s. Log tile-popup-modal-gpu-sol/gpu-check.log; binding.json records embedded fresh coherent primary/fallback shaders and source hashes. Broker GPU explicitly released immediately after completion.

Windows live-path cargo check -p civ3-bridge --target i686-pc-windows-gnu passed exit0 in3m33s. Log tile-popup-windows-check.log. Windows-only live.rs/ui_trace.rs metadata extraction typechecks. No runtime acceptance is inferred.

Root notified source integration is ready. Actual matched-successor C3X prompt visibility and Science Advisor geometry/push/pop remain pending with vm_snapshot_sol, sole GUI owner. No VM input injected by this agent.
