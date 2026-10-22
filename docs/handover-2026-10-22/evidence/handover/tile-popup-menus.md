# Handover: tile info box + right-click menus (civ3-renderer)

Repo: /home/sirati/devel/rust/civ3-renderer (shared dirty tree, other agents active). Build dir used: /mnt/datasets/c3r-build/popup-capture-target. Delete it when you're done. Never build on the Mac (TS Neo).

## Task
1. Tile info box (form 0x66F5C8, open_tile_info 0x5AA530) was the wrong size. User hint: "the world is scaled but the dialogue is not". The settings are World 1.5x and Frame 1x. Box and picture must have one consistent scale.
2. The cattle resource shadow and other drawing bugs inside the box's tile picture.
3. Block map interaction (edge scroll, drag, wheel, captured motion) while a context menu or tile box is open.
4. Recapturing the mouse, or clicking outside the box or menu, dismisses it and does nothing else. No "Not while this window is open" notice for these popups.
5. "Terrain details" from a city menu opened at the wrong tile, because of the parked camera.

## DONE (my part; host-tested)
- Scale choice: the WHOLE box is drawn at the world zoom of the view it opened in, floored at the HUD (frame) scale, then capped to fit the window. The box picture is a 1:1 tile, so frame, text and picture all scale together. Context menus stay at the menu scale.
  - scene: `PopupAnchor.world_scaled: bool` (crates/scene/src/lib.rs). The wire encodes it (wire.rs); VERSION is bumped 43 -> 44.
  - tile-viewer layout.rs: `hud_scale` field (set from `plan.frame` in lib.rs next to menu_scale). `popup_zoom(size, world: Option<camera.zoom>)` is called from `anchored_place`.
  - bridge interface_parts.rs: the TileInfo anchor has world_scaled: true.
- Terrain details tile:
  - Cause: open_tile_info takes its tile from the mouse point through get_tile_coords_under_mouse (0x4E3D20, hooked to return the virtual tile). The world_input interceptor cleared the virtual tile on any client mouse message (moving over the menu or clicking its entry). A WorldMove under the modal context menu also cleared it (screen_gate refusal -> clear()). The game then fell back to the parked camera and got a random tile.
  - Also, the anchor required the press to be less than 2 s old on both sides. A menu read for longer gave no anchor.
  - Fix in world_input.rs: `click_popup_open(game)` checks the modal form's vtable against click_popup(). While it is true, the virtual tile is not cleared (interceptor and dispatch). `popup_tile()` returns the virtual tile.
  - live.rs: `click_popup_seen` chains a popup opened from an anchored one, at any age. The anchor uses the new `interface_parts::click_anchor(kind, at, tile, game_tile, zoomed_out, size)`: TileInfo uses the game's tile.
  - Viewer layout.rs: `chain_press` (set in popup_closed) plus the anchored press of the popup being replaced is used when the world press is stale.
- Dismissal and blocking:
  - New `scene::Input::DismissPopup` (wire tag 17).
  - Viewer: `Layout::click_popup_open()` = popup shown with a driver anchor. In `button()`, a press on the world while it is open (or any press while captured) returns only [DismissPopup], its release is swallowed (`dismissed[3]`), and a left click in a capture view also recaptures. Ctrl+Shift+Space and capture_at queue DismissPopup via `unit_order_events_push` (unit_orders.rs). The world wheel, `motion()`, edge_scroll and the slide (pan.rs) are blocked while it is open.
  - Bridge: forward_input maps DismissPopup to `world_input::post_dismiss`. On the game thread `dismiss_click_popup` posts Escape only if the popup is still open (no race to the map, where Escape opens REALLYQUIT). Fallback: a world press under a click popup dismisses it, swallows the release (SWALLOWED bitmask) and shows no notice. The live.rs world_buttons refusal path skips the notice for click popups.
  - Verified from the exe: Context_Menu key_down 0x61AA80 on Escape clears the selection and closes. The tile box key_down 0x5AAFD0 closes on Esc/Enter.
- docs/world-input.md: new paragraphs on the above.
- Tests:
  - layout.rs: a_tile_box_is_drawn_at_its_views_world_scale_and_a_menu_at_the_menus, a_popup_opened_at_a_world_click_holds_every_view_still, a_press_beside_a_popup_opened_at_a_world_click_only_dismisses_it, terrain_details_from_a_city_menu_open_at_the_menus_tile_however_long_it_was_open.
  - interface_parts.rs: terrain_details_from_a_city_menu_anchor_at_the_city_tile_at_both_zooms.
  - wire round trip includes DismissPopup and world_scaled.
- Results:
  - Host: scene and tile-viewer all pass (369). civ3-bridge: everything passes except units::tests::armies_draw_in_their_owners_colours, which is the army-colours agent's work in progress, not mine.
  - `cargo check -p civ3-bridge --target i686-pc-windows-gnu` passed.

## IN PROGRESS / NOT DONE
- Wine bridge tests (`cargo test -p civ3-bridge --target x86_64-pc-windows-gnu`) are NOT run. The bridge stopped compiling because of OTHER agents' in-flight edits: `missing field indexed in ScreenArt` (menu crate, likely the shadow subagent below) and a `Vec<(Instant, Notice)>` type mismatch. My files compiled before that. Rerun once the tree compiles.
- Resource shadow and drawing bugs inside the box: delegated to a subagent, which was told to stop. Its notes should be at /mnt/datasets/c3r-build/handover/tile-popup-render-shadow.md (if it wrote them). Its build dir is /mnt/datasets/c3r-build/tileinfo-render-target and PNGs go under /mnt/datasets/c3r-build/tileinfo-render/. It appears to be adding `Element::Indexed` / `ScreenArt.indexed` in the menu crate; that is the compile break. A regression test for cattle + road + river is still needed.
- Not tested on the Mac. The orchestrator deploys.

## Gotchas
- `modal_form()` is a form object pointer; its vtable is at +0. CONTEXT_MENU_VTABLES = 0x671AC0, 0x66B240.
- A right press in the viewer may be held until release (go-to drag preview). In tests, look for the WorldButton in the down+up output.
- The `Pan.coast` field is private to pan.rs.
- Wire Input tag 16 is Slider; 17 is the new DismissPopup.
- A WorldMove under any modal used to call clear(). Keep the click-popup early return before the screen_gate refusal.
- The game exe for disassembly: /mnt/datasets/winvm/civ3-src/Civilization III Complete/Conquests/Civ3Conquests.exe. C3X headers: /mnt/datasets/winvm/c3x-src. The host has no python; use `nix shell nixpkgs#python3`.
