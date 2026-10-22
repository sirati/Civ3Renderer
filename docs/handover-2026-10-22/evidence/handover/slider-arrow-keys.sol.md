# Slider arrow keys Sol continuation

Original handover preserved at slider-arrow-keys.md. No commits, stash, reset, protocol changes, GUI actions, image inspection or Mac builds.

## Changes

Inherited arrow stepping, native transactions, Preferences draft delivery and Step wire representation remain intact.

- Added SnapInput::keyboard_focus_rect in tile-viewer/src/snap.rs. Native scroll bars without nonempty hover art now have keyboard focus bounds. Uses the same transformed/clipped targets as snapping so moved popups and hidden controls receive correct bounds.
- Added a localized block beside snap_sprites in tile-viewer/src/lib.rs. Reuses the existing divider sprites for a two-pixel focus border in the overlay camera. Preserves existing marker/cursor order and camera limits. Controls with hover art keep that feedback.
- Refreshes the shown keyboard slider after snapper.tick, eliminating a one-tick focus delay when Alt navigation changes the snapped target.
- Added regression tests for focus border selection/removal, deferring to hover art, same-tick Alt target focus changes, and persistent hover after pointer departure followed by deselection.

## Validation

Own target: /mnt/datasets/c3r-build/slider-arrow-keys-sol/target, through nix develop, CARGO_INCREMENTAL=0, TILE_RENDER_ADAPTER=radv. No GPU tests.

Initial compile stopped on another agent's in-progress Scene::sprite_lifetimes additions. That agent completed the additions; the rerun compiled tile-viewer and passed:

- snap::sliders 12/12
- looks::tests 11/11
- menu keyboard_steps_use_the_slider_step_clamp_and_join_an_open_alt_edit 1/1

Remaining checks are recorded below when complete.

Logs: /mnt/datasets/c3r-build/slider-arrow-keys-sol/validation.log and snap-tests.log.
Source hash manifests: source-hashes.before.txt and source-hashes.after.txt in the same directory. Own snap.rs/sliders.rs/looks.rs sources stayed stable across the successful focused tests. Shared lib.rs and scene files changed concurrently, so root's frozen aggregate compilation is required. Root owns Windows/Wine tests and deployment.

## Final results

Validation script exited 0.

- scene --lib: 14/14, including wire roundtrips.
- civ3-bridge --lib arrow_: 2/2, covering native arrow stepping and SnapInput to Preferences draft end to end.
- civ3-bridge --lib native_sliders::tests: 10/10, including exact cancel restoration, another-control restoration, clamping and noncoalesced step queue order.
- civ3-bridge --lib native_slider_steps_take_the_native_delivery_path: 1/1.

Final exact source hashes are in /mnt/datasets/c3r-build/slider-arrow-keys-sol/source-hashes.final.txt. Stable owned files:

```
189aef49d63efbe4c2c1d39f0282ae2d67ae8eaac4da18d6dc501656947a7fd5  crates/tile-viewer/src/snap.rs
e197ba3f47e9170b65b9e2e0ff06d85ac0f62608d29b14ea53fbcce96a492327  crates/tile-viewer/src/snap/sliders.rs
d9c07d8384e2870ae5cd8a75861f2be28ab4d1cd946aec8112c491da70733093  crates/tile-viewer/src/looks.rs
c1645d3aa4a31b37b704ac276c3dee736cba2df3999316d67a45c5cbce9c0c3d  crates/menu/src/panel.rs
eb51b193ce92a7fc073825f15df3f566c64f5a75e871673b3937fb9c020dfc3f  crates/civ3-bridge/src/native_sliders.rs
4a96e43d239d5554881041d3824d9711b4d5ac50a0b8058151216718313c4c85  crates/civ3-bridge/src/native_sliders/tests.rs
51374ae73cce2c515621ce9b587a1cef5e7b556fdde35741f0f62b88bf1505be  crates/civ3-bridge/src/prefs.rs
ee585c6568cd4a25b7bad09c0cd6a2d63d4b0b239b8cf08a4682f799aad34254  crates/civ3-sys/src/sliders.rs
```

Shared tile-viewer/lib.rs final-observed hash 1e9c0c9c27f7e71e0e026d8e2ed55c7a82cf1ccd35402b29b80dbd35a67062c1 is a post-test observation, not a frozen-tree validation claim. Root must run its final aggregate compile. No pending slider deficiencies found in focused host tests. Native border appearance still needs root's platform/GUI validation workflow.

## Follow-up normal OS cursor audit and fix

Root assigned the ordinary uncaptured cursor audit while startup_progress_sol and tile_popup_render_shadow implement the GPU worker split. Root clarified ordinary uncaptured includes go-to targeting, so physical OS pointer remains visible with logical tile targeting preserved. Captured and Alt virtual pointers retain their handoff barriers.

Concrete old gates:

- driver_cursor read shown_scene and required shown_scene_matches_art.
- draw_frame adopted shown_scene after software frame_done, surface acquisition, renderer control-frame availability and upload_art.
- pending_cursor_art was created only after GPU atlas adoption, then activate_cursor_art required the shown scene's art match.
- cursor_snapping was produced by draw and unconditionally hid the OS pointer even when uncaptured, including free go-to targeting.
- CursorBuilds worker completion sent mpsc data without waking the event loop.

Implemented, agreed with startup_progress_sol and primary App integration owner tile_popup_render_shadow:

- New system_cursor.rs owns CPU cursor metadata and generation matching, independent of renderer cursor_art.
- Whole.cursor_art keeps an Arc of CPU cursor images from Assets::publish before GPU operations. uploaded() drops compiled atlas pixels while retaining this art.
- update_system_pointer calls refresh_system_cursor, reading received display.scene() cursor/assets and CPU art. It no longer reads shown_scene or requires GPU atlas activation.
- GPU activate_cursor_art no longer clears OS custom-cursor caches. CPU art generation changes clear them.
- Assets publication publishes Whole before advancing latest; cursor snapshots read latest before Whole to avoid exposing future sprite ids against previous CPU art during replacement.
- Normal Default/System Arrow OS cursors use our arrow art when available. Busy and invalid system icons retain their shapes.
- Removed draw-produced cursor_snapping visibility field. Draw tile snapping only applies while captured. Logical targeting does not hide or warp the ordinary free cursor.
- CursorBuilds stores a wake callback, installed from cloned EventLoopProxy at both run entrypoints. Completion directly wakes main after publishing the source.
- Existing gpu.window access is intentionally preserved for the App integration owner, who creates main-thread window metadata before asynchronous GPU initialization. about_to_wait already applies cursor/layout requests before render frame gating; owner must preserve this ordering.

CPU tests in /mnt/datasets/c3r-build/os-cursor-sol:

- Initial cargo test system_cursor::tests passed 3/3 with integrated pre-worker App.
- The resulting binary cursor filter passed 34/34, including prepared_cursor_wakes_the_event_loop_without_a_rendered_frame and generation matching. Log cursor-existing-binary-tests.log; binary hash in test-binary-hash.txt.
- Fresh cargo cursor-filter run stopped on the in-progress render_worker.rs unclosed impl State delimiter. Worker owner notified; latest-source rerun still needed.
- New arrow-art and CPU-art-retention tests were added after the initial binary build and require that rerun.
- CPU-only checks use approved unchanged precompiled shader path from root. Dedicated target os-cursor-sol/target. No GUI/screenshots/Mac compilation/protocol changes.

Source hash observations in os-cursor-sol/source-hashes.txt. Shared App integration remains in flight, so these are not a frozen aggregate-source claim.

Additional cursor follow-up:

- Suppressed the ordinary free-map capture hint while latest received interface.targeting is true. Previously cursor::system_pointer gave the capture hint priority over the actual valid/invalid target cursor. SystemCursor::shown now keeps target icon priority while respecting explicit virtual-pointer hiding. Added regression coverage.
- Worker owner reported ArtCommitted carries Compiled without pixels. Building render cursor Art from that metadata loses blank-cell flags and pixels. Recommended actor include cursor_art:Arc<cursor::Art> extracted from full source before pixel release; this is render-only and remains distinct from independent OS state. Newest Assets.whole must not be used blindly for older GPU acknowledgments. startup_progress_sol and tile_popup_render_shadow are coordinating this actor/App fix.

## Latest integrated cursor verification, stable worker and scene prefix

No source changes made in this verification pass. CPU-only Papa nix develop, CARGO_INCREMENTAL=0, own os-cursor-sol/target, RADV only, canonical final shader /mnt/datasets/c3r-build/army-solid-artifacts/shader.spv SHA256 67f3cefc9e905d7404baf63a3d0a3b54ca775bddc61612f52aa57c9b7de1ceb0.

- cargo test -p tile-viewer --lib cursor -- --nocapture: 39 passed, 0 failed. Includes CPU art retention, our arrow and busy/invalid icons, direct cursor worker wake, CPU scene/art generation matching, target icon priority, and blocked-GPU cursor updates.
- cargo test -p tile-viewer --lib snap::targeting -- --nocapture: 7 passed, 0 failed. Covers captured/Alt recapture, attractor and target handoff paths.
- Before-source manifest latest-before.sha256 checked successfully after each run, all 9 entries unchanged. Binary hash latest-test-binary.sha256 and test logs latest-cursor-tests.log / latest-targeting-tests.log retained in /mnt/datasets/c3r-build/os-cursor-sol.

Source audit verdict for this exact manifest:

- Ordinary uncaptured pointer is OS-rendered. pointer_hidden depends only on explicit capture/edge, Alt and deliberate placement/release handoff state; no draw-produced targeting/snapping flag remains.
- about_to_wait receives the newest scene and calls apply_layout_requests/update_system_pointer before frame_clock.ready and any draw submission. refresh_system_cursor reads CPU Whole art plus received display.scene, independent of worker readiness, renderer art acknowledgment, shown_scene adoption, acquire, frame_done or upload.
- Gpu now contains main-thread OS window metadata, so update_system_pointer can set OS icon while worker is initializing. Cursor preparation sends a direct event-proxy wake.
- Ordinary free go-to leaves physical cursor location untouched. Render tile snapping is guarded captured; cursor::Machine only enters edge repositioning when captured. Valid/invalid target icons override ordinary map capture hints.
- Captured and Alt virtual cursor paths remain separate; their deliberate warp/release/presentation barriers are preserved.
- ArtCommitted now includes cursor_art from full pixel source for captured render cursor flags. Main stores it in pending_cursor_art, separate from OS cursor state.

One startup-window observation was reported to root, outside normal visible-window cursor behavior: init still uses with_visible(false), and worker Ready first sets the window visible. This does not gate the OS cursor methods once a window is available, but window visibility itself remains GPU-init gated. No edit was made without coordination.

Source-stable CPU/static verdict: PASS for the cursor lane and preserved explicit virtual-pointer modes. No GUI, screenshot or platform runtime claim is made. Root owns aggregate platform validation.
62172b4301ffb38e4f66b4404d0eb6f32370ea25872986f17b85d06cc89c7bf6  crates/tile-viewer/src/lib.rs
9400cee46f52f84734bf1cb7b2e8846afb8a2a9f120d113c0159cdce952b69c7  crates/tile-viewer/src/cursor.rs
3fed8b6390960fc0a3786eee1c12268db85f7c26e37d0b579570b719d11992ec  crates/tile-viewer/src/system_cursor.rs
6505aa6bc4c81f980d1b62e4221419bd0858f4314398f8cbf6a981709eb82d65  crates/tile-viewer/src/render_worker.rs
bc0a264aae530dbf6d80b14c51cda2a30a917e8f29ef1c9c49dd9f3195aebe7a  crates/tile-viewer/src/layout.rs
1eba0b3dcfd01a82fd665fd2d8c05c65641e8327959c325745cfc32a3e69cb17  crates/tile-viewer/src/input.rs
98c1401a5e1e55795af0aaad00788ba0de14eb0fd9dce8b15b493046774bb73d  crates/tile-viewer/src/constraints.rs
fc9fae3996b4e2cc80a7b4b0274ed16657b76151f6f0c2fd8e5da7302a6fd7fe  crates/scene/src/lib.rs
1cfbf95e54d771b1e3493aafd7af634dc1287babf8afc06432827f8e6208aab1  crates/scene/src/wire.rs

## Launcher escaped-debugger cleanup handoff

Integrated exact random CIV3R_SESSION_TOKEN ownership into ManagedChild Wine commands. The new civ3r-process crate acquires identity before environment inspection and signals through Linux pidfd or macOS audit-token pidversion validation. Cleanup additionally requires the automatic winedbg numeric PID/event role. Shared wineserver/services, unmarked clients and unsupported identity APIs fail closed. Original process-group cleanup remains intact.

Host tests passed with 14 launcher tests and 5 process tests, plus one ignored subprocess fixture. The direct ManagedChild test verifies escaped debugger cleanup while preserving an unmarked debugger and marked wineserver. ARM64 macOS launcher test sources passed cargo check. Intel Sonoma final v3 native suite passed 5 tests with one ignored fixture, exit 0, binary SHA256 9a3ba405c57b78917461b3c47e1bbba202126c5bc2e7bfa078063ac99ce33d27. Native tests cover environment/argv discovery, unmarked-client preservation, unreaped exit detection and atomic stale-identity rejection.

Actual Wine evidence confirms marker inheritance by the faulting game and escaped debugger. A marked shared wineserver does not mark a newly launched unmarked client. The macOS backend queries kern.argmax for KERN_PROCARGS2 capacity because larger buffers fail EINVAL. API absence disables escaped-helper cleanup safely.

All ten source hashes in launcher-cleanup-sol/final-source.sha256 matched after final tests. Logs, native binaries and audit.json remain under /mnt/datasets/c3r-build/launcher-cleanup-sol; VM evidence remains under /mnt/datasets/c3r-build/vm-qa31b-sol. No guest operations, broad process cleanup, commits, protocol changes or new locks were made in this lane.

Root/VM owner still owns actual newly built launcher fault/shutdown Wine acceptance. Audio owner confirmed upcoming pre-exec private DLL preparation preserves ManagedChild and CIV3R_SESSION_TOKEN; that combined source needs aggregate build/runtime validation. Heavy builds remain stopped for the FPS quiet window.

## F1 full-table smooth scrolling and wheel sensitivity

Source lane complete and CPU verified. SmoothMenus no longer narrows the native scroll rectangle to clickable ListItem name fields. Domestic native scroll bounds cover every header/table column. Generic list-box content excludes sidebars through native item-layout bounds. Input routing maps the visible native row across all table columns, preserves independent multicolumn hit targets and suppresses clicks through animated row gaps. Nested list ownership and excluded world sprites retain their separate behavior.

Native disassembly at 0x5228D0 confirmed one row per signed message regardless of magnitude. InputState now accumulates fractional motion, emits one 120-unit message per complete notch, normalizes physical trackpad pixels by monitor scale with forty logical pixels per notch, and caps one event at 32 immediate notches without an excess backlog. Fractions follow the native screen/popup/scroll target and reset on focus/gesture/mode/modifier/scale changes. The App synchronizes against latest received scene before translation. Preview modal owner has physical window_pointer() and reset_wheel() hooks.

Broker-approved CPU checks passed with 43 controls tests, 22 SmoothMenus tests and 7 InputState tests. One GPU test remained ignored. All seven source hashes matched after the final suites. CPU lease released. Source findings, disassembly, logs, hashes and copied test binaries remain at /mnt/datasets/c3r-build/smooth-menu-sol. Root/VM owner retains actual platform acceptance for whole-table midpoint alignment, rapid scrolling/endpoints, production-cell row selection and other native lists. No GPU, guest or GUI operation was performed in this lane.
