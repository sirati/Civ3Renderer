# Handover: arrow keys on selected sliders and scroll bars

## Task
User: "When you have selected a scroll bar you should be able to use the left
and right arrow keys to move the value." This covers our TOML Preferences
sliders and the game's native Scroll_Bar controls (audio volumes, list scroll
bars). A selected slider has to be visible, arrows must not leak into the
world while one is selected, and arrows keep their normal meaning when none is.

## Design (done)
- New `scene::SliderAction::Step`. Its value is a signed count of the
  control's own steps. Wire tag 4. I bumped wire VERSION 44->45; another agent
  has since bumped it to 46, which is fine.
- The viewer owns the selection (`tile-viewer/src/snap.rs`, `SnapInput`):
  - `selected`: set by a left press that reaches the driver and lands on a
    Slider interactable (hit-tested in driver coordinates, as `looks::hit`
    does), by an Alt-mode `begin_slider`, or by a step. Cleared by Esc (the
    Esc is consumed, along with its release), by any other press (a non-left
    press on the same slider keeps it), by a WorldButton press, or when the
    slider is no longer among the targets (tab or screen change).
  - Effective slider (`keyboard_slider`): the open Alt edit, else in snap mode
    the snapped target if it is a slider (none if it is not), else `selected`.
  - Left/Up step -1, Right/Down step +1, Shift x10. Ctrl and Alt are left
    alone (Ctrl+Shift+arrows still pan). The press is consumed and sent as
    `Input::Slider{Step}`; its release is swallowed (`stepped_keys`).
  - Inside an Alt edit, any pending motion preview is flushed first, then the
    Step is sent. `SliderEdit.resync` adopts the first changed scene value so
    later motion continues from the stepped value.
  - `route()` now wraps the old body, renamed `route_input()`. Only keys and
    presses take the new path. `tick()` refreshes `shown`.
- Look: `looks::Feedback::keyboard_slider(id, now)` forces Hover on the
  selected slider. It is wired in `tile-viewer/src/lib.rs`
  `show_keyboard_slider()` after input routing and after `snap.tick`, and it
  requests a redraw. TOML sliders have hover looks (knob_hover). Native
  scroll bars have no looks, so nothing is shown for them.
- Menu (`crates/menu/src/panel.rs`): Step uses the slider's own step (or
  range/20 without one) through the new `step_slider()`, which the wheel now
  shares. Inside an Alt edit, Cancel restores. An edit on another slider is
  cancelled first.
- Prefs (`civ3-bridge/src/prefs.rs`): Step is counted as `mutating` for native
  delivery and the receipt fence. Panel steps go into the draft as mouse edits
  do, and the dialog does not resize.
- Native (`civ3-bridge/src/native_sliders.rs`, `Transaction::step`): steps
  one stored unit (the game's arrow-button step, `Slider::stepped` in
  `civ3-sys/src/sliders.rs`), clamped. Writes go through `set_exact`, which
  uses notify command 0, the same path as drags. Inside an open edit of the
  same control, Cancel restores. An edit of another control is restored first.
  Steps are never coalesced. List scroll bars are Scroll_Bar too, so advisors,
  the Civilopedia and scenario lists are covered generically.

## Files touched
scene/src/lib.rs, scene/src/wire.rs, menu/src/panel.rs,
civ3-sys/src/sliders.rs, civ3-bridge/src/native_sliders.rs,
civ3-bridge/src/native_sliders/tests.rs, civ3-bridge/src/prefs.rs,
tile-viewer/src/snap.rs, tile-viewer/src/snap/sliders.rs,
tile-viewer/src/looks.rs, tile-viewer/src/lib.rs

## Tests (host: all passed, log /mnt/datasets/c3r-build/kbd-sliders/host-tests.log)
- menu: keyboard_steps_use_the_slider_step_clamp_and_join_an_open_alt_edit
- native_sliders: arrow_steps_move_one_native_unit_clamp_and_stay_inside_an_open_edit,
  a_step_on_another_slider_restores_the_open_edit_first,
  steps_are_never_coalesced_and_keep_their_queue_order
- prefs: arrow_keys_step_the_clicked_menu_scale_slider_in_the_draft_with_the_dialog_fixed
  (viewer SnapInput -> Prefs end to end), native_slider_steps_take_the_native_delivery_path
- snap::sliders: arrows_without_a_selected_slider_keep_their_meaning,
  a_clicked_slider_takes_arrow_steps_until_a_press_elsewhere,
  esc_and_the_slider_leaving_the_screen_deselect,
  the_alt_mode_slider_takes_arrows_snapped_to_and_inside_its_edit
- scene wire roundtrip now includes a Step.

## In progress / tree state
- Nothing of mine is half-done. All edits are complete.
- The i686 check (`cargo check -p civ3-bridge --target i686-pc-windows-gnu
  --tests`) has NOT passed yet. The shared tree currently fails in another
  agent's in-progress work: trace.rs:636 has a non-exhaustive match missing
  `ScreenDraw::Indexed`. Earlier failures were in cursor.rs (snap_to_tiles)
  and scaling.rs (ShownPiece.modal). None of them are in my files.
- Wine bridge tests (`cargo test -p civ3-bridge --target x86_64-pc-windows-gnu`)
  have not been run yet.

## Next steps
1. Once the tree compiles, run the i686 check and the Wine bridge tests. Use
   your own CARGO_TARGET_DIR under /mnt/datasets/c3r-build/ and
   CARGO_INCREMENTAL=0. I removed my target dirs.
2. Deploy as usual. Never compile on TS Neo: cross-compile on papa for
   aarch64-apple-darwin.
3. Optional: a looks.rs unit test for the forced hover; a highlight for native
   scroll bars (they have no Looks).

## Gotchas
- Esc on a selected slider only deselects. In Preferences, after clicking a
  slider, the first Esc deselects and the second closes or asks to discard.
  This is deliberate, per "deselect on Esc".
- In snap mode, snapping onto a non-slider hides the old selection. It comes
  back after leaving the mode if that slider is still shown.
- The native step is one stored unit (Shift: 10). If the volume ranges turn
  out to be large, consider a larger unit in `Slider::stepped`.
- Wire VERSION is shared with other agents. Check it before bumping again.
