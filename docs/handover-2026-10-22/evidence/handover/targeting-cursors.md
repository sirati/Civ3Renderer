# Handover: targeting cursors (Civ III renderer)

Repo: /home/sirati/devel/rust/civ3-renderer (shared dirty tree, many agents; nothing committed by me).

## Task
User report: "Relocate this aeroplane" (air rebase) snaps the cursor to tiles like Ctrl+G/Ctrl+X go-to; it should not. Over tiles where the order is impossible the game's "not allowed" cursor (cell 5 of Art/CURSOR.PCX, red circle with a slash) must show; it did not.
Required:
1. Tile snap only for move targeting (go-to mode 1, stack go-to 0x12, which covers Ctrl+X and stack-type moves), decided by the targeting mode.
2. "Not allowed" cursor over invalid tiles for every targeting order.
3. Tests.
4. Run host tests, the i686 check and the Wine bridge tests; then the orchestrator deploys to the Mac. Never build on the Mac.

## DONE (all edits in place)
- crates/scene/src/lib.rs: new field `Interface::snap_to_tiles: bool` after `targeting`.
- crates/scene/src/wire.rs: the field is encoded and decoded right after `targeting`, and the round-trip test literal sets it. **Wire VERSION bumped 45 -> 46**, so bridge and viewer must be deployed together.
- crates/tile-viewer/src/cursor.rs:
  - new `pub fn snaps_to_tiles(&scene::Interface, &Facts) -> bool`: the old inline condition plus `interface.snap_to_tiles`;
  - new test `only_a_moves_target_snaps_the_cursor_to_tiles`.
- crates/tile-viewer/src/lib.rs: the snap call site (around line 3280, `let snapping = ...`) now calls `cursor::snaps_to_tiles`, and its comment says go-to.
- crates/tile-viewer/src/cursor_screen_tests.rs line 187: the same replacement.
- crates/civ3-bridge/src/targeting.rs: `go_to(mode)` is now `pub`, with a doc comment.
- crates/civ3-bridge/src/cursor.rs:
  - new `pub const NOT_HERE: u32 = 5` and `pub fn targeting(game, mode, target_ok, on_world) -> Option<GameCursor>`. With an active non-go-to mode and the pointer on the world, it returns Sprite(mode) when target_ok and Sprite(5) otherwise; in all other cases it returns the game's cursor unchanged.
  - new test `a_tile_the_command_does_not_take_shows_not_here` covering rebase 0xA/0x13, bombard 7, air drop 4, airlift 8, precision 0xB, 9, go-to and no mode.
- crates/civ3-bridge/src/live.rs:
  - right after `let game_cursor = ...` (around line 1121): over `Shown::Map(world, _)`, `game_cursor` is passed through `crate::cursor::targeting(game_cursor, world.targeting.mode, world.targeting.target_ok, crate::world_input::pointer_tile().is_some())`;
  - in the `scene::Interface { ... }` literal: `snap_to_tiles: matches!(shown, Shown::Map(world, _) if crate::targeting::go_to(world.targeting.mode))`.
- docs/civ3-map-rendering.md §10.15:
  - line 963: corrected the mode table from the disassembly (recon is 0x16, not 9; rebase is 0xA and 0x13; every other mode, go-to included, always passes);
  - line 965: documents why the bridge applies the cursor rule itself.

## Tests run (CARGO_TARGET_DIR=/mnt/datasets/c3r-build/cursor-target-agent)
- `cargo test -p scene -p tile-viewer --lib`: all pass (scene 13, tile-viewer 371), including the new test.
- `cargo test -p civ3-bridge --lib`: 387 pass, including both cursor tests and the targeting tests. 3 fail in other agents' areas, not mine:
  - text::prune_tests::outline_glyphs_follow_the_one_to_one_pens
  - text::fit_tests::tooltip_and_popup_text_stays_within_the_games_measure_at_every_scale
  - units::tests::armies_draw_in_their_owners_colours (army colours)
- Logs are in /mnt/datasets/c3r-build/cursor-target-agent/*.log.

## IN PROGRESS / NOT DONE
- The i686 check (`cargo check -p civ3-bridge --target i686-pc-windows-gnu`) has NOT passed yet. It is blocked by another agent's in-flight `ScreenDraw::Indexed` variant: there is a non-exhaustive match at crates/civ3-bridge/src/trace.rs:636. That was the only error, none in my files. Earlier, city_screen.rs and ui_screen.rs had the same kind of error; those were fixed by their owner. Re-run once trace.rs is fixed.
- Wine bridge tests (`cargo test -p civ3-bridge --target x86_64-pc-windows-gnu`, run through tools/wine-run) have not been run.
- The tree compiles for my parts: host test builds of scene, tile-viewer and civ3-bridge succeed.
- Nothing has been tested live in the game.
- Optional: a test with real game rules for rebase and bombard validity. No trace has targeting lines for those modes (I searched /mnt/datasets/c3r-artifacts). Validity comes from the game's own field_2E1AC, so the tests feed that flag.
- When finished, delete /mnt/datasets/c3r-build/cursor-target-agent (target dir) and the scratch disassembly in /mnt/datasets/c3r-build/scratch/c3x-text.txt and cursor-4dc360.txt.

## Findings (from objdump of Civ3Conquests.exe at /mnt/datasets/winvm/wine-civ3-ptrconstraint/drive_c/Games/Civilization III Complete/Conquests/)
- 0x4DC360 computes field_2E1AC, the validity of the tile under the pointer.
  - It is 0 when there is no unit (+0x4D74), and 0 when out of range (Mode_Action_Range >= 0 and distance > range).
  - Otherwise the case table at 0x4DC524 is indexed by mode-4: 4 air drop; 6, 7, 0xF, 0x10 bombard; 8 airlift; 0xA, 0x13 rebase; 0xB, 0x11 precision; 0x16 recon; every other mode is 1.
  - So go-to (1 and 0x12) is always valid, and the game never shows "not allowed" for go-to.
- 0x4DC540 switches the cursor: when field_2E1AC is 0, it sets field_4ED0 and cell 5 (0xA522DC) through 0x604350; when valid again, it switches back to cell Mode_Action. 0x604350 sets the form's +0x208 and calls 0x606A70.
- 0x606A70(0, 1, -1, -1) hit-tests the game's own last client mouse position and copies that form's sprite to 0xCAD2D0, which is the value the bridge reads as the cursor id.
- The hover 0x4EB7D0, which the bridge calls with the virtual tile for every world move, calls 0x4DC360 and **skips 0x4DC540** when the tile converts. 0x4DC540 runs from the animator tick (0x4EF1AC, non-go-to modes) and after commands.
- While the viewer's pointer is on the world, the game's real client pointer stays where it last was on the interface, typically the order button. Probable cause of the missing "not allowed" cursor: the game's cursor id goes stale or comes from the wrong form, even though field_2E1AC is correct for our virtual tile.
- The fix therefore mirrors the game's rule (0x4DC540) from field_2E1AC instead of reading the game's cursor id.
- Snap cause: rebase is mode 0xA (lit_orders calls it AIR_GO_TO). The old viewer snapped for every `targeting` mode.

## Gotchas
- `world_input::pointer_tile()` is Some only while the viewer's last pointer event was on the world. Any real WM_MOUSE* message (pointer over the interface) clears it, and it returns None while a modal screen is open. So the cursor override applies over the world only.
- The wire VERSION bump to 46 may collide with another agent bumping it at the same time. Check the final value before deploying.
- Other agents keep editing lib.rs, scaling.rs, trace.rs, ui_screen.rs and city_screen.rs, so compile errors from those files are transient. Re-read before editing.
- No python3 on this host; use nix shell or Edit.
- Never build on the TS Neo Mac. Cross-compile on papa.
