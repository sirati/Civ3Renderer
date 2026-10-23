# Settings acceptance and cancellation

Preferences keeps renderer settings in a draft and saves only on Accept.
While the dialog is open, the game shows the draft: every renderer control
on every tab previews as soon as it changes. That covers checkboxes,
choices, colour swatches, sliders (on every drag update, including Alt
slider Preview events), the split, roles and screen places, zoom modes and
placement, clean map, accessibility, mouse and Alt mode settings, FPS
display, VSync and the FPS limit, and combat speed. Game frame scales the
minimap, current-unit panel, top buttons and unit orders, including orders
placed in the active view. Zoom sliders affect the drafted zoom mode. A
split or screen place change moves Preferences, like any screen, to its new
place; its size stays.

Three controls wait for Accept: Base UI scale, Scale menus and Menu size.
They set the size of menus, the Preferences window among them. Keeping
their accepted values while the dialog is open keeps its size, slider
tracks, native pointer mapping and Alt targets stable.

Base UI scale has a Preview button beside its slider. Pressed, it reads
"Stop preview" and shows the drafted base on the game behind the dialog:
the frame and the world map scale with it (and follow the slider while the
preview is on), while menus keep the accepted base, so the dialog keeps its
size. The viewer gets this as `Resolution::menu_base_ui_scale`, a transient
field that is never saved. Pressing it again, choosing another tab, Cancel
or closing the dialog ends the preview; Accept keeps the base. The base's
undo button still restores the accepted base, and an open preview then
shows that.

Cancel or Escape asks before discarding changes. Keeping the draft keeps its
preview; confirming discard, or an external close without Accept, restores
all previewed settings to their accepted values. Accept keeps and saves them.
Each renderer slider has a small curved-arrow undo button to the right of its
value while it differs from the accepted setting. It restores just that
slider, including its preview, without saving or resetting other edits. The
button remains available if a slider becomes disabled. Its tooltip reads
"Undo to current state". It participates in ordinary and Alt hit testing.

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
available screen height. The checkbox and percentage take effect on Accept.
The original checkbox groups and audio controls remain native Civ III controls.

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
pauses as soon as it changes; Cancel restores the accepted speed.
Camera-arrival waits keep their own deadline. Sound
playback retains its original pitch and timing, so sounds may outlast an
accelerated fight. The setting does not change simulation clocks or combat
results.

The Accessibility tab groups its controls by feature: Active unit (the
arrow and its bobbing), Unit markers (their size and stack counts),
Diplomacy rings (on/off, colours, invert, ring thickness) and Mouse (Alt
mode in menus and snap updates per second, formerly on the Layout tab;
their setting keys are unchanged).
