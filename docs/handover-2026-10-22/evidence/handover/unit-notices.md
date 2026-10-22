# Handover: map notice tooltips stacking / never disappearing

## Task
User spam-clicked a unit that already moved; "This unit has already moved" plates stacked on the map
and never went away. Required: (1) dedupe identical notice (same text + tile), restart its timer
instead of adding; (2) robust expiry independent of game frames, bound count (one per tile, small
global cap); (3) tests: 50x spam shows one with extended life; all gone after expiry with no game
progress; different notices on different units each show once.

## Findings (source)
- The notices are the game's own MapMessage records (Main_Screen_Form +0x590, 50 x 0x170), read
  every publisher round by `unit_notices::read` (live.rs, ~line 1150), plus the renderer's own refusal
  notices from `modal::notices` (2.5 s, already one per tile).
- They are NOT mirrored UI ops: MapMessage::draw text/fills are dropped from the UI mirror
  (ui_trace.rs `emit`, trace.rs fallback) via `unit_notices::drawing()`. So no popup canvas / occlusion
  GC involvement.
- Before the fix every live game record became one plate, 1:1. Each press makes the game add another
  record (new slot, `below` toggled), so spam = stack. Visibility depended solely on the game's
  alive byte +0x26, cleared only by the game's animator expiry pass; any record the animator does not
  expire stays visible forever. The viewer replaces world sprites wholesale (scene Update::Sprites),
  and the publisher polls every 33 ms independent of game presents, so the stale copies were not a
  viewer/diff issue.
- Could not confirm in the exe why the game's records stay alive (no exe/disassembler found on papa
  quickly). The fix does not depend on it.

## DONE (uncommitted, in shared tree)
- crates/civ3-sys/src/map_messages.rs: `Message` gains `started` (+0x168, timeGetTime) and
  `duration` (+0x16C, ms); decode fills them; test asserts them.
- crates/civ3-bridge/src/unit_notices.rs: new `Shown` tracker (`update(now, game, ours) -> Vec<Notice>`):
  key (tile, text); a game record is "told" when its (slot, started, tile, text) is new; repeat
  tells only extend `until`; one per tile (newer text replaces; `below` forced false); CAP = 8 newest;
  dropped when `now >= until` (publisher clock; game duration if 1..=5000 ms, else 2.5 s) OR when the
  game/modal no longer lists it. `synthetic` Message gets started/duration 0.
  Tests: spamming_one_notice_shows_it_once_for_longer, notices_go_on_time_even_if_the_game_never_frees_them,
  different_notices_on_different_units_each_show_once_and_are_capped.
- crates/civ3-bridge/src/modal.rs: `notices()` now returns `Vec<(Instant, Notice)>` (when told).
- crates/civ3-bridge/src/live.rs: `notices_shown` tracker next to `last_notices`; notice_state uses
  `notices_shown.update(Instant::now(), unit_notices::read(&game), modal::notices(&game))`.

## Test status
- Host: `cargo test -p civ3-bridge -p civ3-sys --lib`: all unit_notices and map_messages tests pass.
  3 unrelated failures from other agents' in-progress work: text::prune_tests::outline_glyphs_follow_the_one_to_one_pens,
  text::fit_tests::tooltip_and_popup_text_stays_within_the_games_measure_at_every_scale,
  units::tests::armies_draw_in_their_owners_colours.
- i686 check (`cargo check -p civ3-bridge --target i686-pc-windows-gnu`): fails ONLY on another agent's
  in-progress `ScreenDraw::Indexed` non-exhaustive match at trace.rs:636; no errors in my windows-only
  edits (live.rs, modal.rs) reported. Re-run once that is fixed.
- Wine bridge tests (x86_64-pc-windows-gnu via tools/wine-run): NOT run (blocked by the same break).

## NOT done / next
- Re-run i686 check and Wine bridge tests after the trace.rs break is fixed.
- Optional: update docs/unit-map-notices.md (it says no time-based cache extends notices; now the
  renderer applies its own lifetime, dedupe and cap).
- Not covered: if the publisher hits the snapshot-error `continue` (live.rs ~1048) for a long time, the
  viewer keeps the last scene. True viewer-side expiry would need a scene wire change (FreeSprite has
  ~70 construction sites); not done.
- Live check on Mac: spam-click an exhausted unit -> one plate, disappears ~2 s after last click.

## Gotchas
- Shared tree, many agents; re-read before edits. Own CARGO_TARGET_DIR under /mnt/datasets/c3r-build/
  (mine, notice-dedupe, deleted). Never compile on TS Neo; cross-compile on papa.
