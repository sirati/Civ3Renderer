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
- Keys and characters: jgl's window procedure passes `WM_KEYDOWN` to the controller's key slot (0x62C590, `(vk, down)`), then to the form's key method, Base_Form 0x6087D0, which records the key at `[0xCAD3B8]` and runs 0x605EF0. That gives the key to the form's key handler (vtable +0x8C, through 0x605E00) only when `MapVirtualKeyA(vk, MAPVK_VK_TO_CHAR)` (0x665244) gives no character. Every other key, Escape and Enter among them, reaches +0x8C from its `WM_CHAR` (controller +0x10, 0x62C610 -> 0x608890 -> 0x605E00, called with the character and `[0xCAD3B8]`). So a key sent without its character is lost to these handlers (`inproc::window::Window::key_now` sends both).
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

### The network game's loop

A network game (game type 4 or 5, `0x49A070`; LAN, internet and direct IP) never enters 0x4DFC50. The game's main function calls `0x469670` once at 0x4F67D8 instead, and the whole network game runs in that loop. Each pass calls the network pump `0x495B00` at 0x469685, then steps the selected unit's go-to, then checks for the end of the game (`0x4F1C20`). `0x495B00` handles the network's messages (`0x495B90`) and then pumps one window message without waiting (`0x624BA0`, jgl slot +0x128). The game's own map clicks are handled from there, and the turn flag says whether it is the player's turn. Screens' idle handlers call `0x495B00` too (from inside their modal loops), so only the call at 0x469685 counts as the human-turn pump of a network game.

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

## v1 as built

- `crates/civ3-bridge/src/modal.rs` runs the actions; `crates/civ3-sys/src/modal.rs` has the sites. The pump calls at 0x6053B4, 0x4DFE24, 0x4DFE5C, 0x4DBF29 and 0x4D7AE8, and the network game loop's call of 0x495B00 at 0x469685, are patched after checking their bytes. The human-turn, network-loop and animation sites only count how many of their calls are running, which is how "inside the human-turn loop (0x4DFC50, or 0x469670 in a network game), outside an animation" is checked.
- Advisor_GUI really is the modal form while an advisor is open (F1 sets `[0xCAD384] = 0x9AFD58`).
- The left button selects, the right button sends the unit, and keys act through an allowlist. Outside such a menu, world keys reach the game as plain keys.
- Seen in the game (PCGames, Wine), with the domestic advisor open:
  - A next-tile move happens at once.
  - A farther go-to only sets the order (state 0x10). The unit walks once the menu closes.
  - After sentry or wait, the game does not pick the next unit while the menu is open.
  - `can_perform_command` refuses sentry and fortify for a unit with no moves left.
- Not done: redrawing the open advisor; the city screen (approach (c)); attacks.

## v2 as built: the city screen too, through (a)

Approach (a) turned out to be safe for the city screen as well, so (c) was not needed. The city screen gives orders to the units in it the same way: at 0x422810 and 0x4228B5 it calls `Unit_select` and `issue_command` while it is open, and then redraws itself through vtable +0x58 (`City_Form::draw`, 0x419770). The forms acting is allowed under are now Advisor_GUI, the Civilopedia and City_Form (0x7409C0). The gate is otherwise unchanged: depth 1, opened from the human-turn loop, outside an animation, not a network game, no action already running.

### Virtual map input under a screen

The private-message path (`docs/world-input.md`) runs from the screen's own pump, because the modal loop dispatches the game window's messages. `crate::world_input` used to drop events while a modal form was open. Under an allowed screen it now handles them like this:

| Input | Under a screen |
|---|---|
| Move | The native hover, so the pointer tile, go-to target and path preview update. |
| Left press, nothing armed | `Select`: `find_visible_unit` + `Unit_select`, with no camera move. |
| Left press, go-to armed | If the tile holds no foreign unit or city, the native left click (`perform_action_on_tile`). The unit then walks the shown path (see below). |
| Right press, nothing armed | `GoTo`: `perform_action_on_tile(1)`. A neighbouring tile is moved to at once; a far tile sets the order. |
| Right button, a command armed | The native right button, which cancels it. |
| Releases, middle button | Nothing. |
| G (`WorldKey` 0x47) and drag previews | `issue_command(0x10, unit)` after `can_perform_command`. The map's `handle_key_down` returns at once while City_Form is shown (0x4E06C5), so the key itself does nothing there. |
| The other allowlisted keys | As in v1. |

Native left and right map handlers are never called with nothing armed, and double clicks are never passed on. The reasons:
- A double click zooms into a city (`City_zoom_to` from 0x4EAE60), which would open a second city screen inside the first.
- The right button opens the context menu or the Civilopedia (0x4EB540). Opening the Civilopedia from under itself would re-enter the open form.
- A single left click on a city tile starts a 250 ms timer (0x4EB529 → 0x4EE650) that later calls `bring_tile_into_view`. Under the city screen, offline, that moves the game camera.

After every action under City_Form the screen is redrawn with `City_Form::draw`, as the screen itself does after ordering one of its units. Its garrison list, for example, shows units moving in or out.

### Go-to walking

The human-turn loop does not walk a go-to by itself. `Unit::can_act_now` (0x5BE680) requires state 0, so `update_while_active` is never reached for a unit in state 0x10.

Under a screen the unit therefore walks the path the game's path finder showed (`targeting::GotoPath`):
- One tile per pass of the screen's loop. Each step is `perform_action_on_tile(GO_TO, next tile)` next to the unit, which moves it at once with the game's own rules and animation.
- The walk stops when the unit is out of move points, blocked, no longer selected, or at a foreign unit or city. The far order is then given again from where it stopped, and the game walks the rest on later turns.
- The pump is woken with `WM_NULL` between steps.

### Feedback

A refused press puts a map notice at its tile for 2.5 s. It uses the font and colours of the game's own notice records (`unit_notices::synthetic`), so every view shows it. The texts:

| Situation | Notice |
|---|---|
| Diplomacy | Not during diplomacy |
| Another dialog, such as the quit prompt | Not while this window is open |
| A popup over the screen | Close the dialog first |
| An attack | Close the screen to attack |
| Nothing selected | Select a unit first |
| Another targeting command armed | Close the screen for this order |

Empty tiles and animations give no notice.

### Not done, and why

- **Attacks.** Combat under a screen could capture or raze cities and change what the open screen shows, for example an advisor's cached lists. A press on a foreign unit or city is refused with a notice.
- **Diplomacy.** A move can open contact, make war or start negotiations, which would change the open deal under the player. Map input is refused there with a notice.
- **The context menu, tile information and the Civilopedia from the map.** These open with the right button or a double click. Under a screen the right button sends the selected unit instead.
- **Redrawing advisors.** Advisor_GUI caches its lists, so an advisor still shows the state from when it opened.
- **Choosing the next unit.** As in v1, the game picks the next unit only once the screen closes.

### Viewer and the capture hint

The bridge now sends the "Press CTRL" hint view while a screen is open over the map (`interface_parts::capture_hint_shown`). Before, it was sent only together with the corner buttons, and those are hidden under a screen. The viewer draws the hint at the captured world pane's top-left in both kinds of layout:
- with a placed panel, as before;
- with `ui=pane`, where the whole game window is fitted into A. This case is new.

### Seen in the game (PCGames + C3X, Wine, QA copy of `2024 Rom C XXIV`)

Tested with a stand-in external viewer that sends `Input::World*`, with Rome's city screen open:
- Unit 103 was selected in another city.
- It was moved one tile east.
- With G, the 6-tile path to a third city was shown, and a click picked it as the destination.
- The unit walked 2 tiles. Its move points were then used up, and the order kept state 0x10.
- The city screen was then still in use: its production list opened and closed, and Escape closed it. Afterwards the game selected its next unit as usual.
- With the domestic advisor (F1) open, the same select, move and walked go-to worked.
- A press on a Portuguese tile was refused with a notice, which showed in the scene and expired. So was a press made while the quit prompt was open.

## Reports the game does not wait for

In a single player game every popup is modal. `show_popup` (0x611460) runs the modal loop (0x6052F0) until the popup closes. Many popups only report something that happened, for example "The Babylonians are building the Hanging Gardens", a wonder completed elsewhere, or war declared on us. Loading a scenario shows a long run of them, and the game stops at each one.

### Which popups

The game itself shows these popups without waiting in an online game. Their callers pass `show_popup`'s non-modal flag 0x4000 when `is_online_game` (0x47B5B0, game type 4 or 5) is true. The code before `set_text_key_and_flags` (vtable +0x170, 0x49F120 → 0x610D20) is `call 0x47B5B0; neg al; sbb eax, eax; and eax, 0x4000`. A scan of the PCGames executable finds 42 such calls. All of Main_GUI's advisor summaries (0x556560, called from 30 places) go that way. Main_GUI ignores `show_popup`'s result there (0x5565B7), and so do the other callers, because online they get the default at once.

The bridge accepts a popup at once only when all of these hold (`civ3_bridge::async_popups::classify`):
- Its class is the generic popup (vtable 0x666408).
- Its text key (`PopupForm+0x580`) is a report:
  - one of Main_GUI's `SUMMARY_` keys, which include the dynamic `SUMMARY_` + name keys at 0x4DD2B0 and 0x56212D;
  - the other summary keys: peace, pacts, alliances, and foreign espionage caught or missed;
  - or a key from the list of report callers: `WONDERPRODUCE`, `CULTUREBORDERVERBOSE`, `HAVE_LUXURY`, `HAVE_RESOURCE`, `HAVE_RESOURCE_NO_UNIT`, `LOSTGOOD`, `NOSUPPORT`, `TRADEEMBARGO`, `TRADEEMBARGOENDS`, `THEY_DESTROY_CITY_GOLD` with its sibling keys for cities we lost (`THEY_CAPTURE_CITY_GOLD`, `THEY_RAZE_CITY_GOLD`, `CITY_LOST_WITH_WONDER`, `CITY_RAZED_WITH_WONDER`), `DESTROYEDSPACESHIP`, `THEIRNUKEFAILEDBYOTHERS`, `THEY_RETURNED_FLAG` and `THEY_TOOK_FLAG`.
- As shown, it has one button and nothing else to click: no choice rows, no list and no second button. Links in its text, which open the Civilopedia, do not count.
- The game is not an online or PBEM game.

The remaining online-flag callers answer the player's own action, and they stay modal: not enough gold, a bad city name, too many cities, the start of a revolution, a failed save, the retire warning. So do the game's openings (`DAWN_OF_MAN`, `LOADGAME`). Every popup with a choice also stays modal. Two examples are `VERBOSE_TRAVELERS_REPORT` ("Big Deal." / "What's the big picture?") and `WONDERCHANGE` ("Zoom to City"). The palace upgrade (`PALACE_VIEW_UPGRADE`) stays modal too, because the palace screen follows it. Small wonders such as the Forbidden Palace come through the travelers' report (`SUMMARY_TRAVELERS_REPORT`, 0x4DD4E0), as the great wonders do.

### How it is accepted

The popup opens as always. At the first passes of its modal loop (the pump call at 0x6053B4, `crate::modal`), the bridge keeps the popup's picture as the game presented it, with its frame and advisor head (`interface_parts::split_modal_stack` on the committed frame). It waits up to 3 passes for the present, and then falls back to the canvas alone. While this happens, the publisher leaves the popup out of the scene. The bridge then sends Enter with its character to the popup on the game's thread (`Window::key_now`), as a player accepts it. The loop ends and `show_popup` returns what the default button returns. If Enter does not close the popup, it stays modal as before and the kept picture is dropped.

The picture is the game's draw commands for the popup, kept the same way as the modal popups' (the interface mirror). It is not pixels. Drawing popups from their script and text data is not done yet.

### In the viewer

The bridge keeps up to 64 reports and sends the oldest two as interface-only views (ids 0x1100 and 0x1101, with the oldest on top), together with `Interface::queued` and the number waiting behind them. They are not modal: all other input goes to the game as if they were not there.
- **Where:** they stack at the window's top-right (while a game loads, the top-right of the menus' region). This keeps them away from the play in the middle of the world, and from the game's own modal popups, which go across the middle and would hide them.
- **The stack:** the next report peeks out 10 pixels down and to the right of the first.
- **Reading one:** a press anywhere on the stack, or Enter or Esc with the pointer on it, closes the first report and does nothing else. Its release is swallowed too (`Input::CloseQueued`).
- **Alt mode:** the first report's button is a target.
- **Mouse captured:** a press plays the world as usual.

`CIV3R_ASYNC_POPUPS=off` leaves every popup modal.

### The loading bar

The game's loading bar (class 0x6724F8, Main_GUI+0x138F0) and its hourglass are sent as an interface-only view of their own (`Interface::loading_bar`, view 0xFFFF0100), cut out of the menus or the frame. By default the viewer puts the bar where the menu would. If the bar meets a popup or a queued report over it, the bar moves down below them, or else up above them, by as little as it can within the menu's region (`scaling::bar_clear_of`). If neither fits, it stays where it was. Its camera always comes after every popup's, so in that case it is drawn in front of them.
