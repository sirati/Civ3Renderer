# Sol continuation of tile-popup-render-shadow

Original handover preserved at tile-popup-render-shadow.md. No commits, stash, reset, GUI, images, Mac operations, or locks added.

## Changes

- menu/src/screen.rs now serializes/parses indexed art with source rectangle, palette range, transparent indices and shade levels. This fixes indexed UI art disappearing from recorded preload lists. Malformed shade levels greater than 16 are rejected. Regression checks palette/color and shade variants roundtrip and produce identical loader descriptions.
- civ3-bridge/src/preload.rs now groups indexed variants into per-file preload batches and accounts for each atlas region's pixel cost. Regression covers parse plus cost.
- civ3-bridge/src/ui.rs IndexedBlit now inherits the sprite's own palette unless the call supplies an override. Regression checks both branches.
- civ3-bridge/src/ui_trace.rs installs the existing direct sprite_draw_blend hook (0x5F8620), using on_map_blend with identity scale. trace.rs guards the native map-blend wrapper original using a scoped thread-local flag, so nested direct hooks neither duplicate capture nor alter the wrapper's original native draw path. Windows-only test covers nested guard restoration and panic unwinding.

## Verification

Host tests running on papa with nix develop, CARGO_INCREMENTAL=0, TILE_RENDER_ADAPTER=radv, own target /mnt/datasets/c3r-build/tileinfo-sol-target.
- menu --lib: pending.
- civ3-bridge --lib: pending; output /mnt/datasets/c3r-build/tileinfo-sol-bridge.log.
- Windows checks delegated to root aggregate. ui_trace is cfg(windows), so host checks do not validate hook code.

## Remaining known limitations

Non-FLC color-table smoke indices 224..239 are still omitted. Resized blend/color-table blits still use the plain resized path. These need proper indexed resized support plus the ExplosionPalette smoke rules; neither was replaced with guessed colors. Existing real-art tile-info CPU regression remains intact.
8ccc4bf09c82e6c31dcbc3102145c7f48e01fc8a7a0142e714a1a09d3b184829  crates/menu/src/screen.rs
7d62b229fb2dc4749abce21a5d973c796d24c6905edf767576850478d3bab293  crates/civ3-bridge/src/ui.rs
7f4afcdefd2083ad390a53190ea43a6cb92f2021a59f9913f2615ec31be8064c  crates/civ3-bridge/src/preload.rs
9f1371c7806d1ba430ba666011f9caa788728f94c6401540fbe6682750efc633  crates/civ3-bridge/src/ui_trace.rs
b426896a363e9fc39682cf0388da54b5158d9a8af4225891d326c288e03e5f3d  crates/civ3-bridge/src/trace.rs
b3fdd3dc4d7e649505389d01ed789e6105190f4aec3b3a43d0db1e47707ee765  crates/civ3-bridge/src/world.rs
55ddb68f2fd96a9f5acd0f132b6301a63c273bf0ed706590f51de4bac5f27fa6  crates/civ3-bridge/src/ui_screen/tile_info_tests.rs

Further fix: trace.rs lacked the ScreenDraw::Indexed match arm, a Windows-only compile error not reached by host tests. It now writes ordinary ui_image geometry/palette followed by ui_image_indices metadata. world.rs replays that metadata into ScreenDraw::Indexed. Regression covers palette/rules/path with spaces and empty transparent list. Invalid rule metadata leaves the ordinary image intact.

Host menu --lib completed: 30 passed, 0 failed. Includes indexed roundtrip regression.

Latest source hashes are in /mnt/datasets/c3r-build/tileinfo-sol-source.sha256, refreshed after the Arc fixture fix and trace replay support. Earlier inline hashes above predate those final edits. Real Civ III assets exist at the regression's default install path. Full bridge and focused picture logs remain authoritative for test completion.

## Final host verification

- menu --lib: 30 passed, zero failures.
- civ3-bridge --lib: process exited 0. Full outcome in /mnt/datasets/c3r-build/tileinfo-sol-bridge.log.
- Focused real-art regression: /mnt/datasets/c3r-build/tileinfo-sol-picture-direct.log, 1 passed in 0.58 s. 44,288 pixels total, 91 shadow pixels. Before: 2,107 differing pixels including 89 shadow pixels. After: zero. Unframed before: 3,249 differences; after: zero.
- New bridge preload indexed-cost/list, inherited/override-palette and indexed trace replay tests all passed.
- Windows-only direct blend hook/guard remain for root's aggregate Windows/Wine compile/tests. Host checks do not validate cfg(windows).

Full bridge outcome confirmed: 401 passed, 0 failed, 1 ignored in 104.20 s. The previously reported army owner-color failure passes in the current shared tree. Queued focused cargo regression also completed successfully in 1.47 s at tileinfo-sol-picture.log. Source hashes refreshed at completion; shared files may continue changing under other agents.
