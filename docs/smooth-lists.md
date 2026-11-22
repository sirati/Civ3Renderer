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
3. The other lists (advisors, Civilopedia, file lists, diplomacy tables),
   one at a time, on the same parts.

`docs/civ3-map-rendering.md` §16 "List boxes" has the list object layout
(G +0x2C first entry shown, +0x30 shown; L +0xC8 first node, +0xD0 count,
+0xF0 selected, +0xF4 hovered; draw 0x6181E0).
