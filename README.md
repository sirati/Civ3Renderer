# Civ3Renderer

The [22 October 2026 handover](docs/handover-2026-10-22/README.md) records open and closed work, unfinished tasks and subagent continuation instructions, acceptance checks, and source backups. An [offline reading copy](docs/handover-2026-10-22/handover.html) is also available.

A modern, GPU-driven renderer for tile worlds — and a driver that makes
Civilization III: Conquests render its map through it.

The renderer knows nothing about Civilization III. Everything game-specific is
either *data* (a TOML render description plus the game's own art files, read
by an asset loader) or lives in exactly one crate, `civ3-bridge`, which runs
inside the game and turns its state into game-agnostic scene snapshots. Another
game could drive the same renderer, with or without Civ III's art.

## Crates

| crate | game specific? | what |
| --- | --- | --- |
| `tile-render-shared` | no | GPU data model shared by host and shaders (`no_std`) |
| `tile-render-shaders` | no | rust-gpu shaders (built by `tile-render`'s build script) |
| `tile-render` | no | wgpu renderer for layered tile worlds; CPU reference renderer |
| `render-desc` | no | TOML render descriptions → atlas, sprite sets, layers |
| `scene` | no | world snapshots and input events between driver and renderer |
| `menu` | no | TOML menus (images, buttons, glyph text) as screen-space sprites, hit testing; panels of controls (choices, tabs, checkboxes, sliders) bound to named values, drawn as run-time screen elements and text; run-time screens (image regions, fills, clipped) |
| `viewer-settings` | no | the viewer's own settings (resolution, layout, input) saved as TOML, and as named controls for a preferences screen |
| `tile-viewer` | no | window that draws scenes and reports input |
| `pointer-api`, `pointer-control` | no | physical-pixel pointer contract and backend selection |
| `pointer-wayland`, `pointer-x11`, `pointer-windows`, `pointer-macos` | no | display-specific capture, barriers and pointer placement |
| `pointer-fallback` | no | portable window grabs and warps used by the backends |
| `viewer-transport` | no | bounded authenticated loopback transport for a separate native viewer |
| `civ3r-viewer` | asset loader only | standalone native viewer, with Wine path mapping |
| `civ3-assets` | format reader | PCX, FLC, INI readers; game file lookup; `render-desc` image loader |
| `inproc` | no | safe abstractions for injected code: guarded memory reads, hooks, DLL entry |
| `civ3-sys` | facts only | Civ III binary facts: build detection, addresses, generated struct layouts |
| `civ3-bridge` | **yes** | runs in the game (as `ifc23.dll`), reads its state, drives the renderer |
| `gdi-text` | no | safe wrapper for GDI text rasterisation (glyph sheets matching old games' text) |
| `gpu-text` | no | font outlines (TrueType/OpenType, `ttf-parser`) packed for drawing text on the GPU at any size |
| `wine-unixlib` | no | safe abstractions for Wine dual-stack programs (PE code calling a native Linux unixlib) |
| `civ3r-native` | asset loader only | native Linux side of the renderer inside the Wine game process |

All `unsafe` interop is confined to `inproc`; `civ3-sys` and the renderer
crates forbid unsafe code, `civ3-bridge` denies it except for the exported DLL
entry symbol.

## The renderer

Ported from the 2024 prototype (branch `legacy`) to rust-gpu 0.11 and wgpu 30.
The prototype's idea is kept — tile data packed per layer at byte granularity,
resolved on the GPU through *layer table → sprite set → animation frame* — but
its compute pass that wrote sprite indices into a vertex buffer became vertex
pulling: one instanced draw where instance *i* is layer *i mod L* of visible
cell *i / L*. All layers of a cell are drawn before the next cell, row by row,
which is the order an isometric map needs for tall sprites to overlap the rows
behind (a pass may sweep each row right to left instead, as Civ III draws
its units). Sprites are anchored quads fetched texel-exact from an atlas.
A layer may store, after the value and the phase, a signed pixel offset per
cell, for the few cells whose sprite is between cells (a unit on its way);
only layers that use it pay for it, and the renderer widens the window of
drawn cells by how far the offsets reach. A layer whose `ticks_per_frame`
is 0 shows the frame its cells' phases say, whatever the clock.

Beyond the grid, a world has *free sprites* (effects, labels, UI): animated
sprite sets placed in world or screen space and drawn after any pass. A free
sprite may be clipped to a viewport rectangle (the quad is shrunk in the vertex
shader). Translucent texels in a quantized model mix in 1/32 steps and round
down, like a game averaging two RGB555 values.

Views can be zoomed by any ratio without blur: a zoomed view is drawn at
zoom 1 into a texture of its own and copied into its rectangle by nearest
pixel, at its place in the view order (later views blend over it). From the
first sprite made for its zoom on (text drawn from outlines at the zoom), it is drawn
at its zoom straight into the target instead, every sprite scaled exactly
as the copy would show it, so such text stays in its pass (under the fog of
war); a *hairline* view (an interface shown smaller) keeps sprites the
scaling would lose one screen pixel wide. Screen copies magnify a rectangle
of the finished picture the same way. Free
sprites may also invert what is under them (`Blend::Invert`).

A view can show the world `n` times smaller (`Camera::downscale`, a game's
zoomed-out view) from the same atlas: the art is held once, at full size.
The shaders place each sprite shrunk and fetch every n-th texel of every
n-th row of its *sample box*; steps, offsets and world positions are
divided, per-sheet `scaling` keeps anchors or screen art as they are.
The sample box is per sprite, found when the description is compiled
(`sample_from`): the whole cell, or — for games that keep sprites cropped
to the box of their non-blank pixels and shrink them from there — that
box, which makes the sampling phase depend on the sprite (`blank_loader`
and an image's `blank` say which pixels are blank). Civ III's map is drawn
that way. Art a game keeps in a second place for its zoomed-out view
(`scaled_origin`) is a sprite's `downscaled` twin. The viewer's own zoom
applies on top (a zoomed-out view shown at 2/1 in a pane).

Colour: a world may compose in a *quantized colour model* (e.g. RGB555, as
Civ III does). The target then stores `q + bias` per channel in 8 bits, and
*scale* sprites — the blend tables games use for fog of war and shadows,
`c' = floor(c·k/den)` — are ordinary premultiplied blends whose offset makes
the 8-bit rounding land exactly on the floor, after every sprite, in drawing
order. A final pass expands the channels to 8 bits.

Sprites come in two kinds, drawn side by side in the same passes and
blends: colour sprites (RGBA texels) and *indexed* ones (one byte, a
palette index, per texel, on index pages of their own), drawn through
palette rows: the sprite's own row (an 8-bit art file's palette with its
transparent, translucent and shadow entries) and, for the indices in its
`remap` range, a *named* palette the cell (by bits of its value, layers
with `palettes = true`) or the free sprite picks — a nation's colours, say.
So a unit's frames are in the atlas once for every nation, its body and
its shadow share them, and 8-bit art takes a quarter of the memory. A
render description's images are indexed wherever the loader can load them
so (`ImageLoader::indexed`; `indexed = false` says otherwise); generated
pictures, glyph sheets and pixels computed per pixel (alpha masks) stay
in colour. `[palettes]` names the rows draws pick (from the loader's
palette files, or another palette with entries copied over).

`tile_render::reference` renders the same thing on the CPU; the GPU output is
tested byte for byte against it (`cargo test -p tile-render`), including an
exhaustive test of the rounding.

What costs the GPU is shading fragments, so none is shaded only to be
discarded where that is known ahead: a compiled atlas notes each sprite's
blank margins and blank isometric corners (`SpriteDesc::blank`, `cuts`),
and quads leave them out (cells of passes of diamonds are octagons); a
free sprite drawn again later the same, all opaque or clear, is drawn once
(a mirrored interface repeats pictures many times). Changes are uploaded
alone: the records of changed cells (`TileWorld::take_dirty`,
`TileRenderer::update_cells`), the free sprites, and the *markers* that
change every frame (`set_markers`: a cursor, a frame counter). A frame
whose views, world and tick are the last frame's shows the picture drawn
then, its overlay views (`render_layered`) drawn anew over it
(`TILE_RENDER_REUSE=0`: every frame drawn whole).

## In the game

`civ3-bridge` is installed as the game's `ifc23.dll` (a proxy forwarding to
the original). With `CIV3R_VIEWER=1` (or `=fullscreen`) it opens the reimplemented renderer in a
window of its own inside the game process. The renderer runs on its own
thread and owns the whole world: a publisher thread snapshots the game's
memory (`civ3-sys`), maps it to a scene (`rules`, `scene_map`) and sends
only what changed (`scene::Update` batches: cells, sprites, views,
overlays, run-time sprite pixels) through an mpsc queue; input flows back
through another, which any number of views or windows may feed. The tile
renderer draws any number of views of the one world into a window, each
with its own camera and screen-space overlay: the game's view (its
camera), the city screen's view of the city's surroundings while it is
open, and the mirrored interface on top. Mouse and keyboard input in the
window is forwarded to the game's window; `CIV3R_GAME_MAP=off` stops the
game drawing its own map.

The art grows as the game shows more (`render_desc::Growing`): the first
art is a whole description compiled to be appended to; what the game
shows later (unit animations, regions of interface screens, fonts,
districts) goes into additions of only the new art, compiled on an art
thread of the bridge (and by the native viewer itself, in the order sent).
A growth keeps every sprite id, set id, palette row and layer value there
and puts its sprites where the atlas is free. The live viewer transfers
whole replacements, additions and reservations with a 4 MiB explicit
transfer allowance per frame, including GPU copy payloads and aligned CPU
staging for uploads. Driver texture initialization is additional GPU work;
renderable atlas textures let wgpu use attachment clears for it. During
replacement work it keeps drawing the previous compatible scene with its
active art, including cursor, hover, layout and frame-counter updates. A
separate preparer uploads the replacement and its required additions, then
adopts its pixels, tables and compatible scene together. Cancelled uploads
retain their allocations until the GPU finishes with them. The combined
GPU atlas allocation limit covers the active art, the preparer's resources
and retired allocations. It is 2 GiB in a 32-bit process and 4 GiB in a
64-bit process by default, configurable with `TILE_RENDER_ATLAS_PEAK_MIB`.
Optional reservations shrink before required art is refused. Live pixel and palette updates remain
immediate, including copies into an uploading atlas, and are outside the
background transfer allowance. Startup and offscreen callers can still
use the synchronous `set_atlas`, `grow_atlas` and `reserve_atlas` methods. At the start of a
game the bridge makes art ahead (`civ3_bridge::preload`): every animation
of the units on screen, then of the other units present, the interface's
art of a built-in list (`assets/civ3/preload-art.txt`, written by the
bridge with `CIV3R_ART_LIST=<file>`), then every animation of every unit
type in every era, in additions small enough that art the screen needs
now waits for one at most, up to a budget of atlas memory
(`CIV3R_PRELOAD_MIB`; `CIV3R_PRELOAD=off`). Another preload chunk starts
only after the preceding art reaches the drawable GPU atlas. A unit whose
new animation has no art yet keeps its last frame. Fonts load in the
background, with outline text where a complete fallback is available.
A screen with unresolved mandatory images or text keeps the last complete
screen visible while input, cursor feedback and layout updates continue.
Terminal art failures show a separate generated "missing art" screen,
without depending on game fonts or publishing a partially drawn interface.
The viewer retains one future scene while newer updates coalesce, so
optional growth cannot keep moving its adoption target. When a scenario
brings its own art, a whole description is compiled against its folders.

The game's interface is mirrored (`CIV3R_UI=off` turns it off): hooks on
its UI drawing (image loads, sprite slices, blits, copies, fills, lines,
text, and the composition of forms onto the screen) keep a display list per
canvas (scrolling, opaque covers, clip rectangles and colour keys
followed); the screen is composed from the canvases' current content —
regions of the game's art files (alpha-masked where the game masks them),
fills, translucent shades, lines, text and unit animation frames, clipped —
that the `menu` crate's run-time screens draw. The map canvas is mirrored
without the map itself (ours): the city labels on it go under our fog of
war; the unit overlay is left out (we draw units, on layers of their tiles: docs §10.13). Pictures the game makes
pixel by pixel (the minimap) are generated from the world state and sent
as run-time sprite pixels. See `docs/civ3-map-rendering.md` §16, §17.
While a unit command waits for its tile (bombard, the air missions, go-to)
the bridge draws what the game shows from the game's state
(`civ3_bridge::targeting`, docs §10.14): the outline of the tiles in range,
the frame on a tile the command takes, and the go-to path (the game's own
path finder, run on the game's thread) with its marker and turn count; the
command's pointer is the game's cursor.

The wheel (and trackpad scrolling) also scrolls what the game itself
never let it: the Civilopedia's articles and lists, the Foreign advisor,
the Hall of Fame, the load/save file list, text boxes, any vertical
scroll bar under the pointer, the advisors' lists with the pointer on one
of their buttons, and diplomacy's trade trees. The bridge does each notch
as the game's own arrow click on that control (`civ3_bridge::ui_wheel`);
these step without the smooth slide.

The Layout tab's **Smooth scrolling in menu lists** option animates rows
in scrollable lists, including the city's production chooser. It is off
by default and independent of smooth world movement. The game receives
wheel input immediately; confirmed row changes slide through the list's
bounds, and clicks follow the visible row. Lists without matching rows
across an update change immediately. Alt button snapping keeps its own
scroll behavior. During a slide the list shows the game's newest drawing
except for rows that have already left the game's list. The game draws
its highlight under the pointer into the list (a list box's selection
that follows the pointer). The viewer learns that highlight from the
game's drawing when the game's pointer moves onto a row and lights it
with no click or key: what the highlight adds and which colours it
changes must turn the row's plain drawing into its lit one exactly and
back. A learned highlight is drawn on the row under the pointer in every
frame of a slide, and for up to 0.3 s after it until the game shows the
same row lit.

The game decides what its interface shows; how a control looks under the
pointer the viewer draws itself, at its own frame rate
(`tile_viewer::looks`, `scene::Looks`). With each scene the bridge sends,
per control the game draws by its state (buttons, the advisors' tabs,
check boxes, popups' choice rows, context menu rows; our preferences
tabs and title menu too), its sprites in its other states: its own draws
with the game's art for that state swapped in (the sprites its fields
name for normal, rollover, pressed, selected; text and fill colours),
and where the interface's views draw it. The moment the pointer reaches
the game over a control (also the snapping pointer once it rests), the
viewer draws its hover look in place of its sprites; pressed while the
left button went down on it; normal once the pointer leaves; selected (or
a check box flipped) at a click, for up to a second until the game shows
it. Once the game's own redraw shows the same, its sprites are drawn
again, which look the same; a control the game shows otherwise (latched,
drawn without its state's art) keeps the game's look.
`TILE_VIEWER_LOOKS=off` leaves them to the game (to compare);
`CIV3R_HOLD_POINTER=<file>` keeps the pointer from the game while the
file exists (to see the viewer's looks alone).

The viewer arranges the world beyond the views the driver asks for
(`tile_viewer::layout`; the driver's interface stays on top, unscaled), as
the user's settings say (`viewer_settings`, `$CIV3R_SETTINGS` or
`~/.config/civ3r/settings.toml`, the preferences screen's *Layout* tab):

- **Split**: one view, views A and B side by side (1:1, or A twice as
  wide), or A above B.
- **Zoom box**: off; a world magnifier in a box in the middle of the
  window sized 1:3:1 or 1:7:1 across the width (margin : box : margin;
  the Box height slider sets 5-100% of its region's height, with Auto
  keeping the window's aspect ratio); a box at the pointer,
  1:1:1 or 1:3:1 across and `zoom_strip` of the height, above the pointer
  (below it where there is no room); or a strip across the top,
  `zoom_strip` of the height, that the views give room to. `zoom_strip`
  is 5-50 %. In a side-by-side split `zoom_place` puts it over both
  views, A or B, and keeps it inside that view. Floating boxes move below
  an approaching pointer; **Keep mouse out of the floating zoom box**
  instead blocks entry. Top strips always block entry and scroll the map
  upward at their lower boundary. Controls over a zoom area remain
  reachable. World magnifiers keep full-size artwork when Z zooms the
  game's map out. Ctrl+wheel over a zoom view changes its magnification.
- **Screens**: one setting places every screen and popup: the advisors
  F1-F8, the city screen, the palace, espionage, diplomacy, the game menu,
  dialogs, popups and the Civilopedia (the bridge names the open one,
  `scene::Interface`, docs §16.11). They go over the window (Floating), or
  their actual panel fills A or B as far as its aspect ratio allows (left
  or bottom, right or top), one place per orientation. Context menus and
  the right-click tile information box are the exceptions: they open at
  the click. A dialog's advisor head (some dialogs draw it on a form of
  its own above them, e.g. the Science Advisor's "need direction") goes
  with the dialog, and the list a combo box in a dialog opens (a modal
  form of its own) is drawn over that dialog, moved and scaled exactly as
  it, and takes the clicks on it (`scene::Interface::modal_attached`).
  The tile box does not block the other view: the game closes
  it at once and the viewer keeps its picture in the clicked view until a
  press or Esc there (docs/world-input.md, "Popups the viewer keeps").
  Both map views keep their own cameras. The city panel's
  internal map follows its panel. The explicit `ui=pane` shortcut still
  fits the entire game window. **Open menus in mouse view** is enabled by
  default and applies with Floating: the game menu, Preferences and
  generic popups open in the view that held the mouse, including the
  captured world cursor's view, and stay there until closed. Their UI
  keeps the selected menu and pixel scale. Middle-drag inside that view to
  reach controls in an oversized menu; the other view keeps showing its
  own world. Turn the option off to float them over the whole window.
- **Clean map**: the Clean map tab has separate A, B and Zoom columns.
  Game follows the game's Clean Map command and its thirteen preferences;
  Normal shows every category; Custom hides selected categories in that
  view. Changing a checkbox selects Custom and saves immediately. Fog and
  player visibility still apply. Both terrain details and city/unit labels
  follow the selected view's mask.
- **Roles**: `Free` (A follows game focus requests and can be panned,
  B is moved by hand), or `UnitMouse`: A follows game focus requests for
  the active unit, visible AI actions and popup subjects; B follows the world cursor
  and captures the mouse when clicked into, so the unit stays in sight in
  the other view. An open popup moves fully into A where it fits; one
  the game opens at a click (a context menu) opens where that click was
  made, e.g. at the world cursor in B. While a popup is open the mouse is
  let go, so the pointer can reach it; it is captured again when the
  popup closes. A screen (an advisor, the city screen, the menu, the
  Civilopedia) opening lets go of the mouse the same way, the pointer put
  on its main control where the screen shows (the city screen's production
  box; else the screen's middle; on Wayland through a pointer lock and a
  position hint; where the system pointer cannot be
  moved at all it stays where it is), and the mouse is captured again,
  the world cursor where it was, when the screen closes.
- **Smooth world movement**: a view following game focus requests
  glides there by pixels (a critically damped spring that keeps its
  velocity when the target moves) instead of jumping a tile at a time.
- **Map movement** (`input.map_movement`): `EdgeScroll` (the default):
  with the mouse free, the pointer at a window edge or corner scrolls the
  view it is in wherever that view reaches the edge, diagonally in a
  corner, faster nearer the edge. Every world view, including A and B,
  uses the same speed, up to 600 window pixels a second regardless of pane
  size or zoom. `Drag` (Civilization IV style): the left button held on the map
  and moved more than 4 pixels grabs it, the map follows the pointer 1:1
  and slides on when let go (with the pointer's velocity, slowing by
  friction as Apple's scroll views do); a shorter press is a click, sent
  on when the button comes up. The middle button
  drags in either mode. Panning A preserves its exact pixel position
  after release, including when smooth movement is enabled. A follows
  new game camera requests, including AI movement and combat, without
  moving B. In UnitMouse roles, selecting another unit resumes following
  that unit. The native game's parked camera does not pull A back to a
  tile. Toggling Z keeps the views and captured cursor on the same map
  locations; the magnifier keeps full-detail art. The mouse wheel pans the
  view under the pointer using the game's scroll-speed setting, vertically
  by default and horizontally with Shift. Ctrl+wheel changes that view's
  zoom. Map dragging is disabled while the mouse is
  captured. In `Drag` mode, a short right click opens the unit menu. Drag
  the right button to preview the selected unit's native go-to route, then
  release it and left click the destination tile to confirm. While the
  right button is held, the preview scrolls the map at its edge. Release
  stops scrolling and keeps the selected destination fixed. Releasing the
  right button never moves the unit. Clicking another tile, dragging the
  confirmation, pressing Escape, or leaving the map cancels the order.
  Unit-order dragging also works with the captured world cursor.

The native main-map camera stays at (0, 0), with a small range covering
five tile coordinates per axis. It supplies native bookkeeping only.
Game camera requests reach A as explicit events, including another request
for the same tile. Visibility and show-moves preferences still decide which
AI actions request focus. B and the zoom view keep their own positions.
Native animation updates include visible units and effects across the map,
using the original elapsed time once. Window size, UI canvases and the city
screen's internal map retain their normal geometry. Parking activates only
when the checked PCGames hooks and native world-input endpoint are ready;
an unsupported executable or failed dependency keeps the original camera.
See [the camera contract](docs/independent-camera.md) for diagnostics.

While the mouse is free the divider between A and B is a wall except in
its middle third (drawn without the line) and where the unit's order
buttons are: the pointer pushed against it stays in its view (the system
pointer is moved back where the platform allows; on Wayland the compositor
holds it out of the walls through a pointer confinement, which also keeps
it in the window while the mouse is free) and scrolls that view
instead. A and B use the same pixel-based movement; the game's separate
edge scroller stays disabled. Through the gaps it passes freely. The game's
top-left menu,
Civilopedia and advisor buttons (and their tooltips) are drawn in the view
the pointer is in, only there. While the mouse is captured they cannot be
used: in their place the view following the world cursor shows a small
parchment box saying "Press CTRL"; while button snapping is on the buttons
show instead, to be snapped to.
While the mouse is captured the world cursor sits in the middle of B (off
the middle only at the edge of a map that does not wrap), is drawn in every
view that shows its pixel, and the whole divider is a wall. Ctrl tapped on
its own (no other key, button, wheel or mouse motion while it is down) lets
the mouse go (Alt is button snapping's); the divider opens only
then. Over B the pointer is then a crosshair, and a click there captures it
again: only that (nothing goes to the game), the world cursor in B's middle
where it was, B staying put. While the mouse is free B keeps looking where
it was let go.

Ctrl+Shift+Backspace, anywhere in the game, puts the layout and the
resolution settings back to their defaults (one view, no zoom box, pixel
scale 1) and saves them: the way out of settings that make the game hard to
use. The pointer settings stay. The keys do not reach the game.

The Layout tab's **Captured mouse sensitivity** multiplier changes only
captured relative movement, from 0.1 to 10 times its normal speed. The
default is 3. Normal pointing, menu snapping and dragging keep their own
speed. At a captured view's edge, the system pointer stays inside that
view; when only one axis is unlocked, it stays on a line along that axis.

The Ctrl+Shift keys change the arrangement for the session on top of the
settings: S cycles the split, I adds a floating inset at the pointer (X
removes it), M cycles a magnifier beside the pointer (everything on
screen, the world only), F cycles what the view under the pointer follows
(the game's view, the active unit, the world cursor, nothing), Space lets
go of the mouse (and takes it over a view following the world cursor;
each change is logged), A toggles a blinking arrow over the active unit,
+/−/0 and arrows zoom and pan the view under the pointer; a middle-button
drag pans a view (and a left one, with map movement `Drag`), a left drag on
the divider's open middle moves it. A free
view set up before the game shows a map is placed at its initial focus.
World input supplies the selected tile directly to the game's native hover,
selection and command handlers, including tiles outside its original camera.
Each renderer view keeps its own camera, and the minimap marks what A shows.
The pointer resting at an edge scrolls that renderer view at the same speed
in A and B. `TILE_VIEWER_LAYOUT`
overrides the settings at start, for testing, e.g.
`split=vertical,main=focus,pane=pointer,pane_zoom=2`.
(`tile_viewer::layout`; the driver's interface stays on top, unscaled):
the main view zoomed (Ctrl+wheel), a second pane beside or below it
(Ctrl+Shift+S, a draggable divider), up to three floating insets
(Ctrl+Shift+I), and a magnifier beside the pointer showing everything or
the world only (Ctrl+Shift+M). Each view follows the game's view, the
driver's point of interest (the active unit), the world cursor, or nothing
(Ctrl+Shift+F); a view following the world cursor keeps it in its middle
and captures the mouse when clicked into (Ctrl+Shift+Space lets go, and
takes it over such a view; each change is logged), the cursor drawn in
every view that shows it. Alt pressed on its own turns on button snapping
(`tile_viewer::snap`; on every screen, the mouse captured or not), and it
stays on: Alt again, Esc or Ctrl turn it off (Alt with another key is the
game's shortcut). The pointer then goes from one thing the interface can
be clicked on to the next (the driver lists buttons, list entries and
lists, `scene::Interactable`), anywhere in the window, across the divider
and the views: on Alt to the one nearest the pointer (or, turned on again,
the one it last rested on), then a few times a second to the one the
mouse moved towards (within 45° of its direction, the closest to that
direction first; small jitter moves nothing). Each move is a short spring
flight (1.33 update intervals, a slight overshoot) that lands exactly on
the element; the game gets the pointer once it rests, and a translucent
ghost shows the next target meanwhile. While it is on, its pointer is the
only cursor. In a list, moving up or down scrolls the list under the
pointer. Selecting a slider starts editing without changing its value.
Mouse movement then adjusts that slider while the pointer stays still; the
next left click accepts the preview and a right click discards it. Esc,
Alt, losing focus, or closing its tab also restores its original value.
A click clicks the element, never the map; the view following the
world cursor is a target too, and a click on it captures the mouse again.
Turned off, the pointer stays where it ended: on the interface the system
pointer shows there (a captured mouse is let go), on the capture view the
world cursor goes on.
(`snap_updates_per_second`, `snap_ghost_fps` in the viewer's settings.)
A free view set up before the game shows a map
is placed at the middle of its view once it does. Ctrl+Shift+A toggles a blinking
arrow over the active unit. Clicks are routed to the game where its view
shows the same world pixel; elsewhere the game is asked to look there
(the bridge clicks its minimap). Where the main pane follows the game's
view, the pointer resting at the window's edge scrolls it locally at the
same window-pixel speed as B. `TILE_VIEWER_LAYOUT` sets it up at start,
e.g. `split=vertical,main=focus,pane=pointer,pane_zoom=2`.
The mouse cursor is the game's (`Art/cursor.pcx`, `scene::Cursor`: the
bridge reads which one the game shows, e.g. its targeting cursors, or the
system's busy pointer): while the mouse is free the system draws it (a
custom system cursor made from the art; the crosshair cell where a click
captures the mouse), smoother than anything drawn in a frame; the viewer
draws it itself, as sprites in a camera on top that change without the
world being rebuilt, only where the system cannot: while the mouse is
captured or snapping (the system pointer hidden, never seen jumping), and
in the other views showing the world pixel under the pointer (the other
pane, insets, a world zoom box). While the view following the world
cursor is held at the edge of a map that does not wrap (the cursor off
its middle), the system pointer shows at the world cursor and stays in
that view. If only one camera axis reaches the map edge, it moves along
a one-pixel horizontal or vertical line; the other axis keeps moving the
camera through relative mouse motion. At a corner it moves within the
view, with the zoom strip and blocked zoom boxes still reserved. Moving
inward captures the pointer again once the view can centre it. Ctrl,
focus loss, menus and button snapping release these edge constraints.
The cursor shows once per place at
any moment (`tile_viewer::cursor::Machine`); no world cursor is drawn
once the mouse is free. Each pointer event redraws at once
(`TILE_VIEWER_LATENCY=1` logs the latency from motion to the drawn
cursor; `TILE_VIEWER_FRAME_TIMES=1` how far apart frames are, every 2 s).
`vendor/winit` carries a fix for custom cursors on a 16-bit
Windows desktop (the game's mode).
On the Load Scenario screen the highlighted scenario's world shows beside
the dialog as its players start with it (what they have explored, the
rest black), fitted to what is revealed (docs §19); a double-click on it
explores it twice as big with the mouse captured (Ctrl lets it go, Esc
or a double-click goes back). The dialog's small map of the scenario is
made from the same world, as the game makes it (bright terrain, all of it).
With a split, `ui=pane` (Ctrl+Shift+U) fits the game's whole screen, its
menus too, into the main pane as one picture (views stacked into one
scaled picture, `Camera::stacked`; its one-pixel lines kept); `ui=window`
keeps the menus (and a city screen's map) 1:1 over the window and resizes
only the world. The game's minimap also marks what the split pane and the insets show
(`CIV3R_VIEW_RECT_COLOR`, `R;G;B`, yellow by default). Zoomed views show
text drawn at their zoom from the font's outlines (`text=sharp`, Ctrl+Shift+T;
`pixel`: the 1:1 text's pixels scaled). `CIV3R_UNIT_RINGS=on` (or e.g. `war=invert,peace=0;200;0,width=3`) draws a
second ring around foreign units by diplomacy: war, mutual defence,
peace.

The renderer's own settings (`viewer-settings`: pixel scale and
fractional scales; split, zoom box, where screens and popups go, roles, smooth
movement; the snapping cursor's update rate) are edited on tabs of the
game's Preferences screen (`civ3_bridge::prefs`, docs §16.11): "CivIII" is
the game's own screen, mirrored as before; "Resolution" and "Layout" are a
panel of the `menu` crate (`assets/civ3/preferences.toml`) laid over it,
drawn from the game's art (its parchment buttons, checkboxes, slider bar
and thumb) with the split pictures generated (`panes:` sources of the
render description) and the game's font. Pointer input the panel takes
never reaches the game. Edits stay in a draft until Accept, so the menu
size slider cannot resize its own dialog while it is being edited. Accept
applies and saves once. Cancel or Escape asks before discarding changed
renderer or native Civ III settings; No keeps the draft. The original
audio controls also defer their sound changes until Accept. See
[settings acceptance and cancellation](docs/settings-transactions.md).

Accepted settings are saved at `Settings::default_path()` (`$CIV3R_SETTINGS`,
else `civ3r/settings.toml` under `$XDG_CONFIG_HOME`, `%APPDATA%` or
`~/.config`; under Wine the bridge uses the Unix user's path through the
prefix's `Z:` drive). The bridge sends them to the viewer as TOML in
`scene::Update::Settings`, starting with the first scene, so the native
viewer follows the same accepted file.

The first run (no settings file yet, `civ3_bridge::first_run`): once the
title menu is up, settled and nothing loads, the bridge opens Preferences
by clicking the title's Preferences row, on the Resolution tab, and saves
`[first_run] preferences_shown = true` as they open, whatever the user
does then; they never open by themselves again. A settings file without
that marker (an existing install) or one that cannot be read counts as
done.

Resolution (`tile_viewer::scaling`, `civ3_bridge::game_size`): our
rendering resolution is apart from the game's screen. The game's screen
is fixed when it starts, at the desktop's logical size (3840x2160 at
200 %: 1920x1080; at least 1024x768): it is only where the game's
interface is and where its input goes. The viewer draws the world itself
at the window's size divided by the pixel scale n, a whole number, each
rendered pixel an exact n×n box (nearest pixel, never fractional; what is
left of the window, under n pixels, is a border). ×1, the default, on a
high-density (Retina) display too, renders every window pixel; the
frame, world and menu scales size things there. The whole frame,
interface and text included, is rendered at the window's size over n
and enlarged once onto the window, so ×n renders n² times fewer pixels
(`tile_viewer::scaling::drawn`). The game's text (Lucida Sans, the face
the game installs) in menus and views zoomed by the menu scale or to fit
is drawn from its outlines at its size in that picture, at the game's
pens and advances scaled; at 1:1 the game's own glyph pixels are drawn
as before. Only the few seconds' confirmation of a new pixel scale is
composed at the window's resolution, so it stays readable at ×6. A larger
rendering resolution shows more of the world about the same middle, not
the same world larger. The game's interface is drawn at its own pixels
times the frame scale (the frame's panels, each kept at its side or
corner: bottom-left, bottom-right, top-left) or the menu scale (menus,
screens and popups, about the middle). With "Scale menus" off, menus use
their native size; the title and Preferences always fit the visible
canvas or assigned view, including at pixel scale ×3. With it on, "Menu
size" selects 25–100% of the canvas height for a standard 1024×768 menu;
width and aspect ratio can limit the result. The title and setup screens
count as their 1024×768 menu, not the game's cleared full-window
background, so at 100% they touch the top and bottom of the canvas
(bars only at the sides) like other menus. A modal screen whose opaque
canvas covers the game's whole window around a 1024×768 board (the Hall
of Fame, the victory screen) counts as that board, not its black clear,
and shows at the title's size. Small popups use that same
reference scale. Existing per-view advisor and City placement still
applies. The defaults are off and 100%; old saved menu multipliers are
removed without being interpreted as percentages. Pointer input and Alt
targets use the same transforms as the pixels. The world scale scales the
world about the middle of the game's view. Other oversized menus retain
their native size with scaling off; hold the middle mouse button and drag
to reach controls outside the window. Changes apply when Preferences is
accepted. The Resolution tab shows the rendering resolution. The system
pointer is the game's cursor art at the pixel scale (each of its pixels
n×n, also over menus at a menu scale of their own), as are the cursors
and markers the viewer draws.

The `menu` crate's panels (`menu::panel`) are game agnostic: TOML styles
(face, size, colours), art (regions of image files cut or repeated to
size, ends kept with `slice`, fills, tints and borders, sprites, pictures
the render description generates, with text labels), and items — images,
text with `{value}` substitution, choices (button groups and tabs: a value
set to an option), checkboxes, sliders (range, step, drag and wheel) —
shown and enabled by conditions on the values (`tab=layout & !locked`),
repeated over lists (`repeat`, `{item}`). The driver supplies the values
and draws the text with its own fonts; the panel reports what changed and
which pointer input was its own.

Verification tools (`tools/civ3-session`, `civ3-tool`): the tracer hooks the
game's sprite blits and records one redraw — every sprite, position and
order — plus the world state; `civ3-tool drawdiff` compares our draw list
with it, `civ3-tool pixeldiff` compares our image with a screenshot (the
whole frame, and the terrain alone: what no unit, label or interface sprite
of ours covers), `civ3-tool labels` compares the city labels
`civ3_bridge::city_labels` makes from the world (name, size, production and
its turns, plates, frame, icons: docs §10.8, so that any view can draw them,
not only the game's own) with the operations the interface mirror recorded
for the game's own.

The districts of the C3X mod (`civ3_bridge::districts`, docs §18) are read
from C3X's state in the game and drawn by C3X's rules; `tools/district-oracle`
runs C3X's own choice functions, cut out of its source, over all their inputs
for the tests.

## Building

```
nix develop                     # rustup, mingw cross compilers, Vulkan
cargo test                      # host
cargo test --target x86_64-pc-windows-gnu    # runs under Wine (tools/wine-run)
cargo build -p civ3-bridge --target i686-pc-windows-gnu
tools/install-bridge            # into the Wine prefix's game copy
```

The first build compiles rust-gpu's codegen backend (several minutes, cached
per user). Set `TILE_RENDER_ADAPTER=radv` to pick the AMD iGPU.

Frame rate: the viewer's `[display]` settings `present` (`Vsync`, the
default: frames at the display's refresh rate; `Mailbox`, `Immediate`: as
fast as they are drawn, three in flight) and `max_fps` (0: no cap), or
`TILE_VIEWER_PRESENT=vsync|mailbox|immediate`, `TILE_VIEWER_MAX_FPS=N`; the
frame counter is `show_fps`. A positive `max_fps` limits every presented
frame, including scene updates and pointer redraws. The event loop keeps
receiving input and scenes while waiting for the next frame. With no cap,
vsync uses the display's refresh for animation and requests input frames
immediately; mailbox and immediate draw without waiting for compositor
redraw callbacks.

The optional `fps_detailed` setting adds rolling one-second timing averages.
`CPU build` is wall time spent preparing the accepted frame on the main
thread and GPU worker. It excludes surface acquisition, queue submission,
presentation and time waiting in the worker mailbox; it is not CPU usage.
`GPU` spans the first to last timestamped render pass. `GPU gap ~` measures
the interval between adjacent instrumented frames on the GPU clock. That
interval includes pacing, presentation, uploads and other queue work, so it
cannot identify time spent waiting for the CPU alone. GPU results arrive
asynchronously without waiting for readback; unsupported or stale samples
show `n/a`. The original `show_fps` counter remains available on its own.

Profiling: `TILE_VIEWER_PROFILE=1` logs every 5 s where the viewer's frames
go (CPU per step, GPU per view from timestamp queries, read back without
waiting; `TILE_RENDER_GPU_TIMING=passes`: per render pass), how often the
driver's scenes come, the native side's decoding and the publisher's
rounds (`CIV3R_PROFILE=1`: the publisher alone). `civ3-tool bench TRACE
[--frames N] [--rebuild]` draws a trace's scene offscreen the same way
(with `TILE_RENDER_GPU_TIMING=1`, and `TILE_RENDER_REUSE=0` for whole
frames). Its output includes the fixed scene tick, reuse setting and the
counts of whole redraws and reused pictures. `civ3-tool render TRACE
OUT.png --reference` uses the CPU reference
with the same current scene and runtime pixels, for exact comparisons
against the optimized GPU output. The `atlas_transfer_profile` example
brackets the first transfer with timestamp submissions, including driver
initialization before the render passes; run it with
`TILE_RENDER_GPU_TIMING=1 cargo run --release -p tile-render --example
atlas_transfer_profile -- 2048 4096`.
The GPU worker reports atlas-transfer work separately from main-thread work.
GPU execution summaries give each delayed timestamp result its own sample;
readback arrival intervals are not reported as GPU frame intervals.

## Native macOS viewer

For reusable test instances, snapshots and login instructions, see
[the macOS VM guide](docs/macos-vm.md).

The Windows game runs under Wine. The shipped launcher and viewer are native
Rust executables. `civ3r-viewer` owns AppKit on its main thread and uses Metal
by default. The macOS adapters use `objc2` for Objective-C framework bindings;
shared application code uses safe Rust and the platform adapters contain the
small unsafe contracts required by Apple APIs. No C or Objective-C application
source or shell launcher is installed.
The Windows bridge sends scenes and asset descriptions over an authenticated
loopback connection. The native process loads the existing art directly.
The renderer replaces the original map, UI and window pixel drawing by
default. `--show-game-window` changes only visibility. `--compare` enables
original pixels in a second window for comparison; `--map` enables only the
original map pixels for diagnostics. `--compatibility-fallback` keeps normal
replacement rendering while permitting original-renderer recovery after
viewer loss. Without that option, closing or losing the viewer stops its
owned game. A viewer that fails before game startup starts no game.

The game still updates the data needed by its simulation and input handling.
Hit-test masks, transparent-control masks and images the game reads back
remain available. These semantic readbacks do not enable the original map,
UI or window pixel output. The replacement renderer owns visible frames.

`CIV3R_GAME_RENDER=off` is the default. Explicit `on` or `compare` selects
comparison; `compat` or `compatibility` permits fallback. An explicit CLI
mode overrides the environment. `--compare-game` is an alias for `--compare`.

Build the Windows bridge on Linux:

```sh
nix develop -c cargo build --locked --release -p civ3-bridge --target i686-pc-windows-gnu
```

On macOS, from this source checkout with Rust and Apple's command-line tools:

```sh
CIV3R_BRIDGE=/path/to/ifc23.dll \
CIV3R_FONT_CACHE_SOURCE=/path/to/generated/windows-fonts tools/build-macos
"$HOME/Library/Application Support/civ3r/bin/civ3-play-macos" \
  --game "/path/to/Civilization III Complete/Conquests" \
  --wine /path/to/wine --window
```

The runtime also needs generated Windows GDI glyph sheets and their TOML
metrics. `CIV3R_FONT_CACHE_SOURCE` installs the `Lucida_Sans-*.png` and
`Lucida_Sans-*.toml` data from an existing cache into `CIV3R_HOME/fonts`,
plus the cache manifest and provenance JSON when present.
Generate that cache with `tools/gen-civ3-fonts` on the Windows build/test host;
the macOS launcher does not regenerate it. This is required runtime data
alongside the Rust executables. Original game assets remain in place.
The launcher selects this private font directory for both Wine and the native
viewer using a shared `Z:` path. An explicit `CIV3R_FONT_CACHE` is preserved.

`tools/civ3-play` also selects this launcher on macOS. `CIV3R_HOME` changes
the installation directory. The launcher links game assets, including the
required original `conquests.biq` rules and its `conquests.mb` companion, into
a private game directory and keeps its renderer DLL, settings, Wine prefix,
logs and new saves there. It seeds the small initial game configuration and
existing campaign/high-score histories into private files. Later launches preserve the
private histories. Generated top-level `.tmp` files, including the load-side
`bic__in_.tmp`, stay private. The launcher replaces its older direct-to-source
temporary/history links without following those links for writes and rejects
links to unrelated paths. Existing saves are linked for loading; use a
read-only source mount to protect them against overwriting. Each private installation permits one active launch.
The Rust launcher creates a private session token with operating-system
randomness and cleans up owned process groups and session files on exit or
interruption. It also initializes the game's 32-bit Wine install registry entries with the private install paths,
so a new Wine prefix does not depend on a previous game installation. It has
`forbid(unsafe_code)`.
The shell/Python scripts in `tools/` are development and VM-management tools.

To check launcher isolation and lifecycle behavior without a Mac:

```sh
nix develop -c cargo build --locked -p civ3r-launcher
nix develop -c python3 tools/test-civ3-play-macos
```

Set `CARGO_TARGET_DIR` for both commands if build output lives elsewhere, or
set `CIV3R_TEST_LAUNCHER` to the compiled `civ3-play-macos` executable.

When building the rust-gpu compiler on the guest is impractical, set
`TILE_RENDER_PRECOMPILED_SPV` to an absolute path to the SPIR-V module built
from the same shader sources on Linux. The viewer embeds that module, so
it is not required beside the installed executable. `CARGO_TARGET_DIR`
selects the renderer build location. Without this override, the build uses
rust-gpu's required nightly and compiler components, with its codegen backend
cached per host user. Shader output stays under the selected target directory's
`spirv-builder` subdirectory. A global Cargo `build.target-dir` setting is
supported: the nested codegen build selects its own cache target instead of
inheriting the renderer's locked target. This does not remove rust-gpu's
platform toolchain requirements; the precompiled module remains the supported
alternative when those requirements cannot be met in a macOS guest.

The nested-target regression can run without a GPU or rust-gpu toolchain:

```sh
nix develop -c rustc --edition 2024 --test crates/tile-render/build_support.rs -o /tmp/c3r-build-support-test
/tmp/c3r-build-support-test
```

Set `C3R_SHADER_BUILD_TEST_DIR` to keep its temporary, dependency-free Cargo
fixtures on a chosen build volume. The test bounds both nested and outer builds
and removes its fixture after success.

The common pointer API uses physical window pixels for capture, placement,
confinement bounds and excluded rectangles. Only its backend crates know
the display APIs. Wayland uses compositor constraints and lock-and-hint
placement. Windows uses ClipCursor plus layout warps for exclusions. X11
uses window grabs and layout warps. macOS uses public CoreGraphics APIs,
with AppKit releasing constraints when focus is lost or a native menu opens.

A macOS VM without a Metal device can run the production shaders through
SwiftShader Vulkan, then present their completed output through CoreGraphics:

```sh
export DYLD_LIBRARY_PATH=/path/to/software-vulkan
export VK_DRIVER_FILES=/path/to/software-vulkan/vk_swiftshader_icd.json
export WGPU_BACKEND=vulkan TILE_RENDER_ADAPTER=swiftshader
export TILE_VIEWER_SOFTWARE_PRESENT=1
```

This mode uses bounded readback storage and software-renderer backpressure.
It tests the native application and production rendering path, but does not
measure Apple GPU performance. For the physical Apple ARM64/Metal setup,
see [the TS Neo run guide](docs/macos-tsneo.md).

## Civilization III facts

* `docs/civ3-map-rendering.md` — how the game draws its map.
* Struct offsets are never typed by hand: `tools/gen-civ3-layout` compiles
  C3X's `Civ3Conquests.h` with the 32-bit mingw compiler and writes
  `crates/civ3-sys/src/layout.rs` from `crates/civ3-sys/layout.fields`.
* The game composes in 15-bit colour (RGB555); the render description asks
  the loader to quantise accordingly, which is what makes pixel-exact
  comparison possible.

## Native Linux renderer inside the game (Wine "dual stack")

Under Wine's new WoW64 mode (`wineWow64Packages`) the 32-bit game and our
32-bit bridge share one 64-bit process with native Linux code. The builtin
DLL `civ3r_native.dll` carries a unixlib, `civ3r_native.so` (crate
`civ3r-native`), which runs the tile viewer natively (Wayland, Vulkan):
the bridge sends it the render description and scenes and takes input
back through `__wine_unix_call` (`wine-unixlib`, `scene::wire`). With
`CIV3R_GAME_MAP=off` the game no longer draws its map at all.

With any renderer backend the game skips its original pixel work by default
(`CIV3R_GAME_RENDER=off`). Showing its window does not change this policy.
Explicit `=on` enables comparison; `=compat` enables fallback recovery while
keeping original pixels suppressed during renderer operation. The interface
mirror records each drawing it hooks and then skips the game's own: sprite blits, image copies (`draw_onto`, `copy_*`, GDI's
`StretchBlt` behind them), fills, lines (the game's and C3X's line drawer),
translucent shades, `TextOutA` of the game's text functions, unit pictures
the mirror shows as animation frames, and `present`'s `BitBlt` to the
window; the map's blits go as with `CIV3R_GAME_MAP=off`. What decides what
is drawn runs as before: forms redraw, compose and present, text is measured
(`GetTextExtentPoint32`), animations and the map pass pick their frames and
sprites, and the mirror follows all of it. The game retains semantic
readbacks needed by its input and simulation: images of palette indices
(its maps of the regions under the mouse), canvases of colour-keyed forms
whose native hit tests read pixels (a click on a transparent HUD pixel goes
to the map), the minimap's images (built
from each other pixel by pixel), and any other image the game drew itself
once it reads a pixel of it or cuts a sprite from it. These images are data
inputs and do not restore original visible frames. FLC frame decoding and
the minimap's per-pixel builders also remain active as required data work.
The five known scaled sprite wrappers are hooked, including team discs,
fog, colored animation frames, shadows and scaled interface icons. The
interface mirror keeps each file icon's native crop, destination size,
16.16 sampling, palette and mask through copies and clipping. Lazy file
sources retain their source-loading callback; their nested pixel copy is
captured and suppressed. The direct-call audit covers all 70 callers of
these wrappers in the checked executable. Scaled generated surfaces and
unknown indirect or modified-executable writers are outside that audit.
The summary replay uses
our terrain sampler and follows the game's tile tint, restore and city-marker operations. Its
terrain rasterizer and pixel writers are skipped with game rendering off;
its geometry and playback logic still run. `CIV3R_TRACE_PIXELS=1` logs any code
taking the pixels of an image a skipped drawing went onto.

```
tools/install-native                 # build + lay out for WINEDLLPATH
tools/civ3-native start --map-off    # inside nix develop; then civ3-session newgame/trace/shot
tools/civ3-native start --game-render   # the game drawing its own window as well
```

To play on your own Wayland desktop, `tools/civ3-play` starts the game the
same way without `nix develop`. Run `tools/civ3-play setup` once: it takes
the runtime libraries (vulkan-loader, wayland, libxkbcommon, libglvnd) from
Wine's closure, keeps them as GC roots and adds a `.desktop` entry. It also
installs Microsoft's DirectPlay into the prefix (`tools/install-directplay`,
files from the flake's `directplay` package with a fixed hash). Civ III's LAN
and Direct IP games go through DirectPlay 8, and Wine's builtin `dpnet`
stubs `Host`, `EnumHosts` and `Connect`. The macOS launcher does the same
from `runtime/directplay` ([TS Neo](docs/macos-tsneo.md)). Steam's
multiplayer is a separate problem; see [Steam multiplayer](docs/steam-multiplayer.md). Wine,
the prefix and the native dir go under `~/.local/share/civ3r` (`wine-wow64`,
`wine-civ3-wow64`, `native`). Original pixels stay off by default. `--compare`
shows the original renderer for comparison, `--map` enables only map pixels,
and `--compatibility-fallback` permits original-renderer recovery. The
renderer uses the Intel iGPU unless `--nvidia` (if that GPU cannot show
the viewer's window, the renderer falls back to the default one and the
launcher says so), and `--window` shows the viewer in a window. Explicit
`--nvidia` or `--intel` overrides an inherited `TILE_RENDER_ADAPTER`.
The NVIDIA path allows both installed EGL vendors: forcing Mesa alone
also breaks NVIDIA's Vulkan initialization and can crash Wine. Intel
remains visible to Vulkan for the presentation fallback. Launcher GPU
selection can be checked without starting Wine or loading a driver with
`nix develop -c python3 tools/test-civ3-play`.

The viewer is the only window: the game's own is a real window (input is
posted to it; it asks for the focus and the pointer, which the bridge
answers) but kept hidden from its start (`civ3_bridge::game_window`),
so it is on no screen, task bar or switcher. `--show-game-window`
(`CIV3R_GAME_WINDOW=show`) keeps it shown for diagnostics without enabling
original pixels; use `--compare` to see original rendering. The test tooling
(`tools/civ3-session`, `tools/civ3-native`) shows it by default.

On a scaled desktop the game runs at its logical size: `civ3-play` reads
the desktop's scale (2 at 200 %, 1.5 at 150 %; `--scale S` overrides it)
and passes it on (`CIV3R_SCALE`): the bridge reports the display that
many times smaller to the game (the nearest of its display modes of that
shape). The viewer's pixel scale does not follow it (×1 unless chosen);
on the first run the frame and world scales start at it rounded down,
and menus at the share of the window a standard menu had at that scale.

On Windows (and in the Windows VM, DX12 WARP) the bridge runs the viewer as
a window of the game's own 32-bit process instead; `vendor/wgpu-hal` carries
a fix that DX12 needs in 32-bit processes.
