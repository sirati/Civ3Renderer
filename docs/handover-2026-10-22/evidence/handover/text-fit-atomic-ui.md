# Handover: text overflow + atomic UI updates (Civ III renderer)

Repo: /home/sirati/devel/rust/civ3-renderer (shared, uncommitted tree; nothing committed).
Build dir used: /mnt/datasets/c3r-build/ui-atomic-target (CARGO_TARGET_DIR, CARGO_INCREMENTAL=0, `nix develop`). Delete it when finished.

## Task
1. Tooltip/popup text: the last letter or two sticks out past the white background (any scale).
2. One frame of flicker when a UI appears: background missing for a frame, or text that rearranges a frame later. UI changes must be atomic (world may update progressively). Keep a 150 ms fallback so loading never blocks for long.

## Causes found
- Overflow: the pens came from the glyph sheets' advances. Those sheets were measured on Windows. Under Wine (the Mac) the game sizes the background with Wine's GetTextExtentPoint32A, which differs from Windows by a few %, so the error adds up along the run. On top of that, each outline glyph is centred in its cell, so a glyph wider than its cell spills over (up to about 2 px at 1:1 for Lucida 14/16 px).
- Flicker: `live.rs` published every screen at once and drew art that was not resident yet as nothing ("no screen held back"). Text whose font was still loading was drawn from outlines with the advances of a font of another size, then moved once its own font arrived. Text whose font was loaded but whose glyph sheets were not in the art yet was drawn from outlines at 1:1. Screen-art builds also waited 35 ms of request quiet before starting.
- Checked and not a cause: the viewer already holds a scene until the art generation it names is uploaded (`tile-viewer` `uses_art`, `retain_pending_scene`). The outline buffer is created synchronously. The UI model snapshot only reflects completed presents: the tooltip overlay is committed at `Event::Present`, and `presenting` is never in a snapshot. The opacity GC only drops ops that are already hidden.

## DONE (all compiled and tested on the host)
- `crates/civ3-bridge/src/text.rs`
  - `Fonts.widths` + `measures` fields. `Fonts::game_width(run)` is the system `GetTextExtentPoint32A` measure (`civ3_assets::fonts::measure`), cached and capped at 16384. It is only used when `measures` is set, i.e. `Fonts::in_background()` (the live publisher), so `Fonts::default()` in tests is unchanged. `set_game_width` exists for tests.
  - `fit()`: spreads the per-glyph advances so they sum to the game's measure. Used in `run_sprites` (1:1 atlas), `outline_fallback` and `run_glyphs`.
  - `fitting_em()`: shrinks (never grows) the outline em when the face's natural run width plus the bold allowance (43/64 px per glyph) exceeds the width.
  - `cells()`: when there is a game measure, the first and last visible glyphs get a cell at least as wide as the glyph, aligned to the run's start and end.
  - `run_provisional(run, compiled)`: the text is drawn provisionally (its font is still loading, or its sheets are not in the art yet).
  - Tests: `fit_tests::{tooltip_and_popup_text_stays_within_the_games_measure_at_every_scale (real LSANS + real sheet cache, scales 1, 3/2, 2, 3, Windows width and a 3 % narrower width), advances_are_spread_over_the_games_measure, text_with_another_sizes_advances_holds_the_interface_until_its_font_arrives}`.
  - Existing test `prune_tests::outline_glyphs_follow_the_one_to_one_pens`: its em assertion is relaxed to `9*64 < em <= 12*64`, because the fixture's fake advances are narrower than the face.
- NEW `crates/civ3-bridge/src/ui_transaction.rs` (registered in lib.rs as `pub mod ui_transaction;`): `HOLD_MAX = 150 ms`, `RETRY = 4 ms`, `readiness()` (ui_screen readiness plus provisional text that is still coming) and `UiHold` (publish or hold; `released()` once it timed out). Tests: art arriving late is never shown without its background, the 150 ms fallback still publishes, and text whose font is not final is not ready.
- `crates/civ3-bridge/src/live.rs`: `ui_hold` state. After the `publication` match, while the interface is Pending (at most 150 ms), the round does `last = None; wait = RETRY; continue;`, so the viewer keeps the previous scene. One info log is written when the hold is released without all of its art. Outdated comments are updated.
- `crates/civ3-bridge/src/art_schedule.rs`: new `Timing.screen_quiet` (GROWING: 0, test T: 70 ms). A pending screen request now starts its build at once (prefetch).
- `crates/civ3-bridge/src/ui_screen.rs`: doc comments only, plus the test `new_world_notice_requests_required_art_before_the_batch_starts`, which now expects an immediate start for screen art.

## Test status
- Bridge-targeted host tests: 58/58 pass (log: handover/text-fit-atomic-ui-bridge-tests.log).
- Full host workspace (`cargo test --workspace --no-fail-fast`, incl. GPU): everything passes except `civ3-bridge units::tests::armies_draw_in_their_owners_colours`. That test belongs to the army-colours agent and is not touched by these changes (log: handover/text-fit-atomic-ui-host-tests.log).
- NOT RUN (stopped for the handover): `cargo check -p civ3-bridge --target i686-pc-windows-gnu` and the Wine bridge tests `cargo test -p civ3-bridge --lib --target x86_64-pc-windows-gnu`. The known baseline Wine failures are the 2 `city_screen` tests.

## IN PROGRESS
Nothing half-done. The tree compiled at the last host run, but other agents keep editing it (earlier, an in-progress `ScreenDraw::Indexed` broke compilation for a few minutes).

## Next steps
1. Run the i686 check and the Wine bridge tests. Under Wine, `game_width` is live only for `in_background` Fonts, so the test fixtures stay deterministic.
2. Deploy and check on TS Neo: tooltips and popups at pixel scale 2 (no letters past the box), and no first-frame flicker. In `CIV3R_PROFILE` output, look for the event "held for interface art" and the log line "interface published without all of its art".
3. Optional: `ink_bounds` (Alt target anchors) still uses unfitted pens.

## Design decisions
- The hold is per scene, not per UI layer. The publisher skips publishing, so the viewer keeps the previous complete scene; a brand-new tooltip shows nothing until it is complete. The world in the viewer keeps animating and panning on its own. Only scene state changes wait, for at most 150 ms per waiting episode. The timer resets only once the interface is Ready or Unavailable, so a stuck art build cannot hold scenes repeatedly.
- Unavailable art (absent or failed) never holds a scene.
- The game's measure comes from the system text routine in our own thread of the game process, not from the TextOutA hook. This avoids adding a field to `TextRun`, which has about 35 literals across other agents' files.
- The em only shrinks, so the ascent and baseline stay the game's (`|lfHeight|` is the em; all game fonts use negative heights).

## Gotchas
- Never compile on TS Neo. Cross-compile on papa.
- The tree is shared: re-read files before editing, and never revert others' changes.
- In tests, text on a black reference render must not be black (the first failure of my test).
