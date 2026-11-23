# Smooth lists, rendered from game state

User report (2026-11-22, again): in the city screen's lists, smooth
scrolling is broken every time:
- rows that scroll into view stay cut off and only appear much later;
- the scroll bar changes size as rows arrive;
- the list sometimes gets stuck.

Requirement: smooth scrolling and stepped (one item per notch) scrolling are
the same thing. Only how the position changes differs. A list is never
scrolled by moving what was drawn before. It is drawn at its position.

## Why the current design fails

`tile_viewer::smooth_menus` keeps the drawings the game made at each of its
integer positions and composes the list at its own pixel position, each
row's band taken from the newest drawing that has the row. It sends wheel
notches so the game draws the next rows ahead. Whatever the game has not
drawn yet is missing:
- a cut-off band scrolls in, and the rows appear only when the game's
  drawing arrives (late on a slow machine or under emulation);
- the scroll bar is the game's, drawn for its integer position, so it jumps
  and changes when the drawings arrive;
- a position no drawing covers is waited for, which is when it sticks.

No tuning fixes this, because the content the viewer needs does not exist
until the game draws it. Making the game draw every row in advance would
still be copying its drawing. The project's goal is to render from game
state instead.

## Design

- **Content from game state.** For each list the driver publishes every
  entry, not only those shown: entry `k`'s sprites in the list's content
  space (entry `k` at `y = k * step`). They are built from the game's data
  for the entry (the city's buildings, the units on its tile, the
  production choices) by the same items code that draws everything else,
  in the game's fonts and art. An entry is rebuilt only when its data
  changes.
- **One position.** The viewer keeps the list's position in pixels. The
  wheel, a trackpad, the scroll bar, keys and snapping only change that
  position. Rendering is always the content clipped to the list's
  rectangle at that position, so stepped scrolling is the same path at
  multiples of `step`.
- **Screen pixels.** The position is continuous (pixels of the content,
  fractions kept), and the content is drawn by a camera of its own at
  the list's zoom: between whole content pixels it is moved by screen
  pixels, as a zoomed-in map is (`tile_viewer::layout::FreeView::camera`,
  `screen_rest`). At a 2× or 3× interface a list then scrolls one
  screen pixel at a time, not one game pixel (user, 2026-11-23).
- **Only the foreground scrolls.** An entry's content is its icons, text
  and buttons. The screen's background art (the board, the panel behind
  the list) is not content and never moves: it was not made for
  scrolling (user, 2026-11-23).
- **Partial entries are drawn.** The entries the position puts partly in
  the list's rectangle are drawn, clipped to it. Nothing of an entry
  shows outside the rectangle (no icon hanging below the list, as F1's
  citizens did), and there is no empty gap where the next entry goes.
- **No threshold.** The first trackpad or wheel input moves the list.
  Nothing waits for the game to reach a whole entry, and snapping (only
  with "Snap lists to items") happens after the input stops.
- **Display rate.** The game repaints its interface about 15 times a
  second (76 presents in 5 s on TS Neo; the viewer itself runs at 60+);
  the viewer draws the list at its position every frame from
  the content it has, and the bridge keeps the entries round the position
  published ahead (at least a page each way) so a fast scroll never waits
  for the game.
- **The scroll bar is drawn by the viewer** from position, viewport and
  content height, in the game's art. Its thumb never jumps or changes size
  while scrolling.
- **The game's integer position follows.** The game's first shown entry is
  set (rounded) behind the viewer's position. Clicks and hovers go to entry
  `k` under the pointer: the bridge makes `k` the game's selection or
  hovered entry through the game's own functions, with `k` shown on the
  game's side first if the function needs that. Its hit test is never
  used.

## Order of work

1. Scene: a list description carrying all entries' content, its step,
   count and the game's position, in the engine (game-agnostic). The
   viewer renders and scrolls it as above, with a scroll bar from
   position.
2. Bridge, city screen first:
   - identify each city list (classes 0x665D88, 0x665BF0, 0x665A40,
     0x6665D8; generic 0x6718E8 for the production choices) and the game
     data behind its entries;
   - build each entry's content from that data, matching the game's entry
     layout (taken from its drawing code), checked against the game's
     drawn entries pixel for pixel;
   - publish them, keep the game's position in sync, and route
     clicks/hovers to entries.
3. The domestic advisor's city table (F1) next (user, 2026-11-23: its
   citizens hung below the list, its background moved, and a trackpad
   scroll waited for a whole row before moving).
4. The other lists (other advisors, Civilopedia, file lists, diplomacy
   tables), one at a time, on the same parts. Every scrolling screen is
   in scope; `tile_viewer::smooth_menus`'s composition of the game's
   drawings is removed once no list uses it.

`docs/civ3-map-rendering.md` §16 "List boxes" has the list object layout
(G +0x2C first entry shown, +0x30 shown; L +0xC8 first node, +0xD0 count,
+0xF0 selected, +0xF4 hovered; draw 0x6181E0).

## Engine design (protocol 67, tilerenderer-gameengine b3be47f on)

- `scene::View::list: Option<ListView { id, over, count, step, published }>`:
  an interface-only view holding the entries as world-space sprites in
  content space (entry k at y = k·step, x from the list's left); viewport
  and origin = the list's rectangle (driver pixels); camera_y = the
  driver's offset (content pixels). `over` = the screen view it is in.
- `Input::ListScrolled { id, offset }`: the viewer's offset (rounded); the
  driver sets the game's first entry from it and republishes.
- Viewer (`tile_viewer`):
  - Layout keeps a scroll state per list id: continuous `pos`, a glide
    target for wheel notches, clamped to `0..count·step − viewport`;
    starts at the driver's camera_y; trackpad pixels move it at once
    (window pixels through the list's zoom), wheel notches glide a step.
    Wheel/scroll input over a list's canvas rectangle goes to it and not
    to the driver; `ListScrolled` goes out when the rounded pos changes.
  - `scaling::arrange_panned`: a list view takes the piece of its `over`
    view; its clip is its rectangle through that piece (within the
    over view's clip). Its canvas camera: y = floor(pos), origin moved
    up by the screen pixels of the fraction (`layout::screen_rest`),
    viewport that much taller, target_clip = the clip: by screen pixels,
    entries partly in the rectangle drawn and cut at its edges.
  - Pointer routing over a list maps to the content at the viewer's pos,
    then to where the game's list (at camera_y) has that point.
  - `smooth_menus` leaves lists that have a ListView alone; it is removed
    when no list uses it.

## Bridge design (2026-11-23, from Civ3Conquests.exe PCGames with C3X)

What the game has (read from the binary; `civ_prog_objects.csv` and C3X's
`injected_code.c` in the game source folder name many of the functions):

- `p_city_form` 0x7409C0. Its lists are `Base_List_Box`es (the list
  object G): `Order_List` (production choices) at +0x7C7C,
  `Order_Queue_List` +0x8364, `Improvement_List` +0x8A4C. G+0x2C first
  entry shown, G+0x30 entries shown (`civ3_sys::controls`).
- Every list box draws an entry with 0x617470 (`this` = G + 0x64, the
  argument the entry's index). It calls the list's text callback G+0x50
  (`(control, …, &rect, data)`; nonzero: the default text is skipped) and
  its icon callback G+0x4C
  (`(control, data, index, x, y, …)`, `draw_improv_icons_on_city_screen`'s
  signature). An entry's data is `[[node+8]+4]`; entries are added with
  0x618940 `(text, data, flag)` (text at L+0xE0, lines L+0xE8).
  So one hook round 0x617470 marks every list box row's draws: they are
  the list's rows, everything else the list draws is its background.
- The improvements list (filled by 0x421D30): one entry per improvement
  the city has (`City_has_improvement(city, i, 1)`, 0x4ACBE0), in
  improvement order, skipping BIC flags (+0xF0 & 4, & 8), +0xD8 ≠ −1 and
  +0xEC & 0x80000; data = the improvement's index, text = its name (BIC
  improvement +0x44). Rows 32 px. Callbacks: G+0x50 = 0x421210 (the name
  at x + small sprite width + 2), G+0x4C = 0x4212C0
  `draw_improv_icons_on_city_screen` (C3X wraps it, adding mill resource
  icons, `patch_Sprite_draw_improv_img_on_city_form`): the small sprite
  (`City_Form.Improvement_Images_Small`, 0x746F08, 44-byte `Sprite`s, +0x18
  width, +0x1C height) at (x, y); then the row right of it in quarters:
  upkeep coins (`City_get_improvement_maintenance` 0x4ACE80, sprite
  0x746930), culture (`City_get_tourism_amount` 0x4B07A0, sprite 0x7468AC),
  content faces (0x4F8DA0, sprite 0x74695C), and a fourth group (0x5613F0,
  0x55A790, sprite 0x746988); icons of a group are packed closer when they
  do not fit (0x426790 clamps the step).
- The production list (`Order_List`) draws its background through its
  owner callback (0x41C240 / 0x41C370: `ProductionQueueBox` sprite
  `City_Form+0x64F0`, clipped to the list) and its rows with the entry
  draw above: a 32×32 unit icon (`units_32.PCX`) or the improvement's,
  the name and stats ("Worker  0.0.1") at x+38, the turns ("1 turn") at
  x+133, 42 px rows (run 30 traces).
- Run 30 (/tmp/c3r-run30) has traces of both city lists and F1 at
  several positions; the game draws whole rows only and leaves the part
  row empty (the bug).

Plan:
1. civ3-sys `lists`: read a list's screen rectangle, row height, first
   shown, count and each entry's data (the nodes).
2. On the game's thread (its present), for the open city screen: each
   entry's row data from game state (improvement id, maintenance,
   culture, contentment, …; for production: the order and its turns),
   only when it changes.
3. civ3-ui: a row's items (images by the game's sprites' regions, text in
   its fonts) from that data, a port of the callbacks above, in content
   space (entry k at y = k·step).
4. The publisher: a `ListView` view per list with those sprites; the
   rows the game drew (marked by the 0x617470 hook) left out of the
   mirrored screen; `Input::ListScrolled` sets G+0x2C on the game's
   thread; the scroll bar drawn by the viewer from the position.
5. F1's city table is drawn by the advisor's own code (not a list box):
   its row drawing is found and ported the same way.
