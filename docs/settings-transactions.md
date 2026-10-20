# Settings acceptance and cancellation

Preferences keeps renderer settings in a draft. View placement, menu size,
combat speed and frame pacing take effect on Accept. Moving the menu size
slider therefore leaves the dialog and its pointer mapping stable.

Background sampling, cursor size and captured-mouse sensitivity preview
immediately. Cancel restores the settings from when Preferences opened.
The render multiplier also previews immediately, with a separate ten-second
Keep/Revert dialog. A fresh Enter press or Keep confirms the preview; Escape,
Revert or expiry restores the previous multiplier. The countdown runs in the
viewer even when the publisher stops sending frames. A later Preferences
Cancel restores the opening multiplier even after Keep.

Accept and Cancel are the game's own buttons: the round Accept button and the
X button in the screen's bottom-right corner, drawn and hovered with the
game's art. The bridge finds them among the native controls (`Button`
objects at Parameters_Form +0x5A0, id 0, and +0xC74, id -1, created by
`0x588370`). It keeps the left press and release of a click on them from the
game and runs the transaction instead, so plain clicks, captured-mouse clicks
and Alt-snapped clicks behave the same.

Accept, or Enter outside a confirmation dialog, applies the draft and saves it once. Cancel (X or Escape)
closes an unchanged dialog immediately. If renderer or original Civ III settings changed, it asks
whether to discard them and shows a summary. Yes discards; No returns to the
draft. Y selects Yes. N, Enter and Escape select No. While this question is
open, Alt offers only its Yes and No buttons.

Potential native keyboard and mouse edits remain pending until a private fence
posted after them is acknowledged by the game's window thread. A memory read
before that acknowledgment cannot declare the dialog unchanged. This also
covers a native Space or arrow key followed immediately by Escape.

A native popup over Preferences receives its current gesture, including its
key or mouse release. Later actions wait for the same queue acknowledgment
and a fresh native modal lookup. If the popup closed, queued Cancel reaches
the Preferences draft and asks before discarding it. If it stayed open,
input continues to that popup. Window metadata and application Quit still pass
through while interactive input waits.

Scale menus enables the menu size slider, which selects 25% to 100% of the
available screen height. The checkbox and percentage follow the same draft
rules. The original checkbox groups and audio controls remain native Civ III
controls.

The PCGames Preferences audio callbacks normally preview changes immediately.
The bridge now stages their numeric values and uses the game's own checkbox
state and redraw function. Original sound callbacks run at Accept, before the
game saves its options and destroys the dialog. Cancel skips its four direct
audio rollback calls, including channel enable/disable, because active audio
was never changed. Playing sound nodes remain active.

Accept (the round button, or Enter in the dialog) runs the round button's own
callback `0x587E70` with id 0 on the game's window thread, posted after the
forwarded input. It sets the result byte `+0xEC61` and then closes, so the
close hook sees Accept, applies staged audio once, and the original closure
saves the options to conquests.ini. Native Return is never sent: the game
closes on it before `0x587EA0` sets the result byte, which discards Civ III
options and audio (seen with the original IFC23.dll too). Cancel (X or Escape)
sends native Escape; the form clears the result byte when it opens, so a
cancelled close cannot commit. Audio
callback contracts are `0x588AD0`, `0x588C20` and `0x588DB0`, and native closure is
`0x588040`. Patches check their original bytes. The native visibility test at
`0x609AC0` prevents a duplicate close or static destruction from applying sound
again. These hooks affect the Preferences form only.

The emergency Ctrl+Shift+Backspace shortcut resets the draft while Preferences
is open. Outside Preferences, it still restores and saves layout and resolution
immediately so an unusable configuration can be recovered. Closing the renderer
application follows its normal owned-game shutdown path.

The Layout tab includes Combat animation speed, from 1x to 4x in quarter
steps, defaulting to 1x. It changes combat visuals and their native animation
pauses after Accept. Camera-arrival waits keep their own deadline. Sound
playback retains its original pitch and timing, so sounds may outlast an
accelerated fight. The setting does not change simulation clocks or combat
results.
