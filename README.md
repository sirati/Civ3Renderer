# Civ3Renderer

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
renderable atlas textures let wgpu use attachment clears for it. It keeps drawing with
the active art until the transfer finishes, then adopts its pixels, tables
and scene generation together. One candidate allocation or retired atlas
may exist alongside the active one. The combined atlas allocation limit is
2 GiB in a 32-bit process and 4 GiB in a 64-bit process by default,
configurable with `TILE_RENDER_ATLAS_PEAK_MIB`; optional reservations shrink
before required art is refused. Live pixel and palette updates remain
immediate, including copies into an uploading atlas, and are outside the
background transfer allowance. Startup and offscreen callers can still
use the synchronous `set_atlas`, `grow_atlas` and `reserve_atlas` methods. At the start of a
game the bridge makes art ahead (`civ3_bridge::preload`): every animation
of the units on screen, then of the other units present, the interface's
art of a built-in list (`assets/civ3/preload-art.txt`, written by the
bridge with `CIV3R_ART_LIST=<file>`), then every animation of every unit
type in every era, in additions small enough that art the screen needs
now waits for one at most, up to a budget of atlas memory
(`CIV3R_PRELOAD_MIB`; `CIV3R_PRELOAD=off`). A unit whose new animation has
no art yet goes on showing its last frame; nothing waits for art (no
scene is held back, fonts are loaded in the background, text whose glyphs
are not in the art yet is drawn from its face's outlines meanwhile). When a scenario brings its own
art, a whole description is compiled again against its folders.

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
  the height follows at the window's aspect ratio); a box at the pointer,
  1:1:1 or 1:3:1 across and `zoom_strip` of the height, above the pointer
  (below it where there is no room); or a strip across the top,
  `zoom_strip` of the height, that the views give room to. `zoom_strip`
  is 5-50 %. In a side-by-side split `zoom_place` puts it over both
  views, A or B, and keeps it inside that view. Ctrl+wheel over it zooms
  it.
- **Screens**: the advisors F1-F8, the city screen, the palace, espionage,
  the game menu and the Civilopedia (the bridge names the open one,
  `scene::Interface`, docs §16.11) go over the window, or the game's whole
  screen is fitted into A or B (left or bottom, right or top), one place
  per orientation.
- **Roles**: `Free` (A shows the game's view, B is moved by hand), or
  `UnitMouse`: A follows the focus of interest (the active unit; while a
  popup is open the middle of the game's view), B follows the world cursor
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
- **Smooth world movement**: a view following the game's view or the focus
  glides there by pixels (a critically damped spring that keeps its
  velocity when the target moves) instead of jumping a tile at a time.
- **Map movement** (`input.map_movement`): `EdgeScroll` (the default):
  with the mouse free, the pointer at a window edge or corner scrolls the
  view it is in wherever that view reaches the edge, diagonally in a
  corner, faster nearer the edge: the game's view (A following it, at any
  zoom) by the game itself (`scene::Input::Edge`), the viewer's views (B,
  A following anything else, insets) by the viewer, up to 600 pixels a
  second. `Drag` (Civilization IV style): the left button held on the map
  and moved more than 4 pixels grabs it, the map follows the pointer 1:1
  and slides on when let go (with the pointer's velocity, slowing by
  friction as Apple's scroll views do); a shorter press is a click, sent
  on when the button comes up. No edge scrolling then. The middle button
  drags in either mode. Dragging the game's view, A is moved by the viewer
  meanwhile and the game is asked to look where it comes to rest (the
  bridge clicks the minimap; the game centres on a tile), then A follows
  the game's view again. While the mouse is captured neither applies.

While the mouse is free the divider between A and B is a wall except in
its middle third (drawn without the line) and where the unit's order
buttons are: the pointer pushed against it stays in its view (the system
pointer is moved back where the platform allows; on Wayland the compositor
holds it out of the walls through a pointer confinement, which also keeps
it in the window while the mouse is free) and scrolls that view
instead: the game's own view as at its screen edge (`scene::Input::Edge`;
the game polls the pointer, `inproc::pointer`), the viewer's views by
itself. Through the gaps it passes freely. The game's top-left menu,
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
view set up before the game shows a map is placed at the middle of its view
once it does. Clicks are routed to the game where its view shows the same
world pixel; elsewhere the game is asked to look there (the bridge clicks
its minimap). The main pane following the game's view shows its middle in
the pane's middle (a split or a zoom strip crops it round its middle), so
what the game centres on (the active unit, a popup's subject, a place
looked at) is in the middle of A, and the minimap marks what A shows.
Where the main pane follows the game's view (at any zoom), the pointer
resting at the window's edge scrolls the game's map as at the game's own
screen edge; the viewer's own views scroll at the edges they reach as
well. `TILE_VIEWER_LAYOUT`
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
pointer. A click clicks the element, never the map; the view following the
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
view, the pointer resting at the window's edge scrolls the
game's map as at the game's own screen edge (`scene::Input::Edge`; the
game polls the pointer, `inproc::pointer`). `TILE_VIEWER_LAYOUT` sets it up at start,
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
its middle), the system pointer shows at the world cursor and moves it
until the view can centre it again. The cursor shows once per place at
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
or a double-click goes back).
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
fractional scales; split, zoom box, where each screen goes, roles, smooth
movement; the snapping cursor's update rate) are edited on tabs of the
game's Preferences screen (`civ3_bridge::prefs`, docs §16.11): "CivIII" is
the game's own screen, mirrored as before; "Resolution" and "Layout" are a
panel of the `menu` crate (`assets/civ3/preferences.toml`) laid over it,
drawn from the game's art (its parchment buttons, checkboxes, slider bar
and thumb) with the split pictures generated (`panes:` sources of the
render description) and the game's font. Pointer input the panel takes
never reaches the game; the game's OK button stays usable. A change is
saved at once (`Settings::default_path()`: `$CIV3R_SETTINGS`, else
`civ3r/settings.toml` under `$XDG_CONFIG_HOME`, `%APPDATA%` or
`~/.config`; under Wine the bridge takes the Linux user's path, through
the prefix's `Z:` drive, so that it and a native viewer share one file)
and sent to the viewer (`scene::Update::Settings`, the wire carries them
as TOML); the bridge sends its settings with the first scene, so the
viewer follows the bridge's file even where it would find another.

Resolution (`tile_viewer::scaling`, `civ3_bridge::game_size`): our
rendering resolution is apart from the game's screen. The game's screen
is fixed when it starts, at the desktop's logical size (3840x2160 at
200 %: 1920x1080; at least 1024x768): it is only where the game's
interface is and where its input goes. The viewer draws the world itself
at the window's size divided by the pixel scale n, a whole number, each
rendered pixel an exact n×n box (nearest pixel, never fractional; what is
left of the window, under n pixels, is a border). Auto, the default, is
the desktop's scale rounded down (2 at 200 %, 1 at 150 %). A larger
rendering resolution shows more of the world about the same middle, not
the same world larger. The game's interface is drawn at its own pixels
times the frame scale (the frame's panels, each kept at its side or
corner: bottom-left, bottom-right, top-left) or the menu scale (menus,
screens and popups, about the middle; none larger than the picture), and
pointer input is mapped back through the same pieces; the world scale
scales the world about the middle of the game's view. All of these change
live; the Resolution tab shows the resolution rendered at. The system
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

Profiling: `TILE_VIEWER_PROFILE=1` logs every 5 s where the viewer's frames
go (CPU per step, GPU per view from timestamp queries, read back without
waiting; `TILE_RENDER_GPU_TIMING=passes`: per render pass), how often the
driver's scenes come, the native side's decoding and the publisher's
rounds (`CIV3R_PROFILE=1`: the publisher alone). `civ3-tool bench TRACE
[--frames N] [--rebuild]` draws a trace's scene offscreen the same way
(with `TILE_RENDER_GPU_TIMING=1`, and `TILE_RENDER_REUSE=0` for whole
frames). `civ3-tool render TRACE OUT.png --reference` uses the CPU reference
with the same current scene and runtime pixels, for exact comparisons
against the optimized GPU output. The `atlas_transfer_profile` example
brackets the first transfer with timestamp submissions, including driver
initialization before the render passes; run it with
`TILE_RENDER_GPU_TIMING=1 cargo run --release -p tile-render --example
atlas_transfer_profile -- 2048 4096`.

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

In native mode the game also skips the rest of its pixel work
(`CIV3R_GAME_RENDER=off`, the default with `CIV3R_VIEWER=native*` unless
`CIV3R_GAME_WINDOW=show`; `=on` keeps it, and in-process it is on unless
set). The interface mirror records each drawing it hooks and then skips the
game's own: sprite blits, image copies (`draw_onto`, `copy_*`, GDI's
`StretchBlt` behind them), fills, lines (the game's and C3X's line drawer),
translucent shades, `TextOutA` of the game's text functions, unit pictures
the mirror shows as animation frames, and `present`'s `BitBlt` to the
window; the map's blits go as with `CIV3R_GAME_MAP=off`. What decides what
is drawn runs as before: forms redraw, compose and present, text is measured
(`GetTextExtentPoint32`), animations and the map pass pick their frames and
sprites, and the mirror follows all of it. Drawing goes on where the game
reads its pixels back: images of palette indices (its maps of the regions
under the mouse), the canvases of colour-keyed forms (a click on a
transparent pixel of the HUD goes to the map), the minimap's images (built
from each other pixel by pixel), and any other image the game drew itself
once it reads a pixel of it or cuts a sprite from it. Not skipped:
blits the mirror does not hook (the scaled wrappers), FLC frame decoding,
the minimap's per-pixel builders. `CIV3R_TRACE_PIXELS=1` logs any code
taking the pixels of an image a skipped drawing went onto.

```
tools/install-native                 # build + lay out for WINEDLLPATH
tools/civ3-native start --map-off    # inside nix develop; then civ3-session newgame/trace/shot
tools/civ3-native start --game-render   # the game drawing its own window as well
```

To play on your own Wayland desktop, `tools/civ3-play` starts the game the
same way without `nix develop`. Run `tools/civ3-play setup` once: it takes
the runtime libraries (vulkan-loader, wayland, libxkbcommon, libglvnd) from
Wine's closure, keeps them as GC roots and adds a `.desktop` entry. Wine,
the prefix and the native dir go under `~/.local/share/civ3r` (`wine-wow64`,
`wine-civ3-wow64`, `native`). The game's map is off unless `--map`, the
renderer uses the Intel iGPU unless `--nvidia` (if that GPU cannot show
the viewer's window, the renderer falls back to the default one and the
launcher says so), and `--window` shows the viewer in a window.

The viewer is the only window: the game's own is a real window (input is
posted to it; it asks for the focus and the pointer, which the bridge
answers) but kept hidden from its start (`civ3_bridge::game_window`),
so it is on no screen, task bar or switcher. `--show-game-window`
(`CIV3R_GAME_WINDOW=show`) keeps it shown, for testing; the test tooling
(`tools/civ3-session`, `tools/civ3-native`) shows it by default.

On a scaled desktop the game runs at its logical size: `civ3-play` reads
the desktop's scale (2 at 200 %, 1.5 at 150 %; `--scale S` overrides it)
and passes it on (`CIV3R_SCALE`): the bridge reports the display that
many times smaller to the game (the nearest of its display modes of that
shape), and the viewer's pixel scale Auto is it rounded down.

On Windows (and in the Windows VM, DX12 WARP) the bridge runs the viewer as
a window of the game's own 32-bit process instead; `vendor/wgpu-hal` carries
a fix that DX12 needs in 32-bit processes.
