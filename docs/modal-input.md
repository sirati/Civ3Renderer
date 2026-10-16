# Playing in one pane while a menu is open in another (research notes)

Addresses are for the PCGames build. They were read from `Civ3Conquests-Unmodded.exe` and `jgl.dll`, and checked against the C3X-patched exe. "C3X: jmp" marks functions that C3X redirects.

## How modal screens run

### Input path

The message pump is `0x624B90`. It calls jgl's slot +0x124 (0x1003D430) on `[0xCAB28C]`, which takes one message or waits with `WaitMessage()`. The message goes to jgl's window procedure (0x1003C730) and then to the controller object 0xCACD30 (vtable 0x67138C). The controller's slots:

| Slot | Address | Input |
|---|---|---|
| +0x0C | 0x62C590 | key |
| +0x10 | 0x62C610 | char |
| +0x14 | 0x62C630 | mouse move |
| +0x18 | 0x62C6D0 | wheel |
| +0x1C | 0x62C720 | left button |
| +0x24 | 0x62C850 | right button |
| +0x30 | 0x62C930 | game timers (WM_USER+1) |

### Routing

- Keys go to `0x608C20`. While a modal form is set in `[0xCAD384]`, only that form gets them.
- The mouse goes through the hit test at `0x609030`. While a modal form is set, it tests only that form's tree and the companion form `[0xCAD380]`. A click that hits neither still goes to the modal form.

So the map form (0x9F86C0) gets no input while a menu is open.

### The modal loop

The modal loop is `Base_Form::m68_Show_Dialog` at 0x6052F0, with arguments `(this, flags, idle_fn, companion)`. It sits in vtable slot +0x110, and C3X does not touch it.

1. It pushes the form on the stack at `0xCACD20`. The depth counter is `0xCAD388`, the limit is 4, and a fifth dialog returns 0 without opening.
2. It sets `[0xCAD384] = this`.
3. It loops. Each pass runs the m70 check and the optional idle callback, then calls `0x624B90` at **0x6053B4** (bytes `E8 D7 F7 01 00`). The loop ends when `[0xCAD384] != this`.

Close is m69 at 0x6053F0. Flag 2 skips the loop but still sets `[0xCAD384]`.

### The human-turn loop

The human-turn loop is `0x4DFC50`. It runs while the turn flag `0xA26858` is set and pumps at 0x4DFE24 and 0x4DFE5C. It also steps the selected unit's go-to: the unit is `[0x9FD434]`, the check is `0x5BE680`, and the step goes through unit vtable +0x54. The active player is `[0x9FD47C]`.

The player's menus open from inside this loop: pump → map handler → `Advisor_GUI_open` 0x49D100 or `City_Form_open` 0x421720 → modal loop. So no end of turn, interturn or AI turn can run while such a menu is open (single player). Menus opened at the start of a turn or during the interturn do not have that guarantee.

### Other pumps

- `0x4DBF29` and `0x4D7AE8` pump in the middle of animations, and the game is re-entrant there. Never act from these sites.
- Animation loops use the filtered pumps `0x624C20` and jgl slot +0x12C.

### Map handlers

- **Left click:** `0x4EB290`. It finds the tile through the game camera with `0x4E3D20`.
  - In go-to mode it calls `perform_action_on_tile`.
  - Otherwise it calls `bring_tile_into_view` 0x4DFAA0, then `find_visible_unit` 0x4E3F50, then `Unit_select` 0x5C5FA0.
- **`perform_action_on_tile`:** 0x4E6E70 `(this, action, x, y)`, C3X: jmp. Action 1 is go to.
  - Next to the unit, it moves now through `0x4DE7B0`, which may ask to confirm war.
  - Further away, it only sets go-to state 0x10, with the destination at unit+0xB0/+0xB4.
- **`issue_command`:** 0x4DAB30 `(this, command, Unit*)`.

### Forms

| Form | Address |
|---|---|
| main screen (map) | 0x9F86C0 |
| Advisor_GUI | 0x9AFD58 (kind at +0x2510) |
| City_Form | 0x7409C0 (current city at 0x7423AC; while it is open, `0x9CB3F4` is the city and the game camera is on it) |
| popup | 0x9B7510 |
| DiploForm | 0xA351C0 |
| Civilopedia | 0x9E85B0 |

## Approaches

### (a) Act from inside the modal loop (chosen for advisors and the Civilopedia)

Patch the call at 0x6053B4 to call a function of ours. It runs one queued action and then calls 0x624B90. After queueing an action, post `WM_NULL` to wake `WaitMessage`.

Call the game's tile-level functions rather than faking clicks:

| Action | Functions |
|---|---|
| select | `0x4E3F50` + `0x5C5FA0` |
| move / go to | `0x4E6E70` (1) |
| command | `0x4DAB30` |

Only act when all of these hold:
- depth == 1;
- the modal form is on the allowlist;
- the game is inside 0x4DFC50 with the turn flag set;
- it is not a network game;
- no action is running.

Risks:
- Prompts open a second dialog level, so hold no lock while acting.
- Advisors cache lists, so do no attacks for now.
- `bring_tile_into_view` moves the game camera, which is fine under full-screen advisors but not under the city screen.
- Far go-to orders only walk once the menu closes.
- Keys never reach `handle_key_down`, because of end turn and the F-keys. Use an allowlist of commands instead.

### (b) Make screens non-modal (rejected)

The opener's code would run too early. The turn loop would also keep going under the screen, so end of turn and AI turns could run.

### (c) Close, act, reopen (for the city screen)

Close the screen with m69, keep `Advisor_GUI+0x251C` at 0, and act at the next human-loop pump. Then reopen with `Advisor_GUI_open(0x9AFD58, kind)` or `City_Form_open(0x7409C0, [0x7423AC], 0)`.

This loses scroll and tab state, so the mirror should hold the last frame while it happens.

## Plan

**v1:** approach (a) for advisors and the Civilopedia.
- Add `Input::WorldButton` and `Input::WorldKey` to the wire.
- The viewer sends them for presses in panes that don't route into the game window.
- The bridge stops its `Look` minimap clicks while `[0xCAD384] != 0`.

**v2:** the city screen through (c), go-to walking inside (a), and attacks.
