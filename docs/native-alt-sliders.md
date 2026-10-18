# Native Alt slider edits

Native sliders publish their actual value and knob travel as slider targets.
Selecting one starts an edit without a native click, held button, or value write.
Relative mouse movement sends normalized previews while the pointer stays at
its selected target. Commit keeps the preview. Cancel restores the exact integer
read at entry, without converting it through the normalized wire value.

The bridge posts a registered window message to the game's existing window.
Its shared interceptor dispatches slider requests on that window's game thread.
Pending adjacent previews coalesce; begin, commit, cancel, keys and pointer
buttons stop coalescing across their queue positions. All native callbacks run
without a Rust state lock. The driver restores before foreign mouse-down,
key-down, focus loss or window closure can replace the form. If a control becomes
hidden or disabled, it restores only when its identity still matches. A replaced
or destroyed control receives no writes.

Control identity includes its address, canvas, parent address and vtable,
callback identifier, range and reverse flag. Ordinary scene filtering determines
which control may begin or preview an edit. Cancellation can restore a hidden
control when its memory still describes the same object.

## Binary evidence

The native implementation supports the PCGames Conquests 1.22 layout. The
existing test executable is C3X-patched; the Scroll_Bar code below remains native.
No additional game file or asset copy is required. Its SHA-256 is
`66d0ae91de6f4a2376e03aa74eb29a8256f02108ca3a4c024a4857337bc45ee6`.

- `Scroll_Bar` constructor `0x60CAE0` writes vtable `0x6660A8` at `0x60CB24`.
  The C3X `Civ3Conquests.h` also identifies its `0x13C0` object layout.
- `Parameters_Form` constructs its three audio sliders at `+0x1213C`,
  `+0x134FC`, and `+0x148BC` through that constructor at `0x587850` onward.
- `0x60D1F0` sets ascending bounds at `+0x580` and `+0x584`; `+0x588`
  records reverse notification order. `0x60D270` clamps a value into `+0x58C`
  and reflects it when the reverse flag is set.
- Drawing at `0x60DFC0` derives the knob rectangle from `+0x5AC..+0x5B8`.
  At `0x60E048`, flags `+0x574 & 2` determine whether arrows consume track
  space; `+0x57C != -1` adds the border. The exported travel reproduces that
  arithmetic, and uses the full form dimensions and translated native origin.
- `0x60DEB0` is the arrow-button notification function. Its command branches
  only change the stored value for command 1 or 2. Command 0 skips those
  branches, then invokes parent vtable slot `+0xE4` with callback identifier
  `+0x578` and the actual value. It also runs optional callback `+0x13B8`,
  the parent's notification object and the native redraw. Reversed sliders
  reflect the notification value through `minimum + maximum - stored`.
  The driver writes its validated exact position and invokes command 0.
- For Preferences, parent slot `+0xE4` is `0x588DB0`, which updates the audio
  values and sound engine. Its continuous-change slot `+0xE8` is `0x623710`,
  a jump to that same `+0xE4` method. Preview and rollback therefore use the
  game's existing audio update path, including the all-sound multiplier.

Focused tests cover horizontal and vertical geometry, arrows and borders,
reverse flag handling, unreadable and malformed controls, exact cancellation
for a range larger than the normalized wire precision, unchanged entry,
commit, stale requests, hidden controls, replaced objects and queue barriers.
