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
| `menu` | no | TOML menus (images, buttons, glyph text) as screen-space sprites, hit testing; run-time screens (image regions, fills, clipped) |
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
behind. Sprites are anchored quads fetched texel-exact from an atlas.

Beyond the grid, a world has *free sprites* (units, effects, UI): animated
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

`tile_render::reference` renders the same thing on the CPU; the GPU output is
tested byte for byte against it (`cargo test -p tile-render`), including an
exhaustive test of the rounding.

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
game drawing its own map. When a scenario brings its own art, the
description is recompiled against the scenario's folders.

The game's interface is mirrored (`CIV3R_UI=off` turns it off): hooks on
its UI drawing (image loads, sprite slices, blits, copies, fills, lines,
text, and the composition of forms onto the screen) keep a display list per
canvas (scrolling, opaque covers, clip rectangles and colour keys
followed); the screen is composed from the canvases' current content —
regions of the game's art files (alpha-masked where the game masks them),
fills, translucent shades, lines, text and unit animation frames, clipped —
that the `menu` crate's run-time screens draw. The map canvas is mirrored
without the map itself (ours): the city labels on it go under our fog of
war; the unit overlay is left out (we draw units). Pictures the game makes
pixel by pixel (the minimap) are generated from the world state and sent
as run-time sprite pixels. See `docs/civ3-map-rendering.md` §16, §17.

The viewer arranges the world beyond the views the driver asks for
(`tile_viewer::layout`; the driver's interface stays on top, unscaled):
the main view zoomed (Ctrl+wheel), a second pane beside or below it
(Ctrl+Shift+S, a draggable divider), up to three floating insets
(Ctrl+Shift+I), and a magnifier beside the pointer showing everything or
the world only (Ctrl+Shift+M). Each view follows the game's view, the
driver's point of interest (the active unit), the world cursor, or nothing
(Ctrl+Shift+F); a view following the world cursor keeps it in its middle
and captures the mouse when clicked into (Ctrl+Shift+Space lets go, and
takes it over such a view; each change is logged), the cursor drawn in
every view that shows it. A free view set up before the game shows a map
is placed at the middle of its view once it does. Ctrl+Shift+A toggles a blinking
arrow over the active unit. Clicks are routed to the game where its view
shows the same world pixel; elsewhere the game is asked to look there
(the bridge clicks its minimap). Where the main pane shows the game's view
unmoved (zoom 1), the pointer resting at the window's edge scrolls the
game's map as at the game's own screen edge (`scene::Input::Edge`; the
game polls the pointer, `inproc::pointer`). `TILE_VIEWER_LAYOUT` sets it up at start,
e.g. `split=vertical,main=focus,pane=pointer,pane_zoom=2`.
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

```
tools/install-native                 # build + lay out for WINEDLLPATH
tools/civ3-native start --map-off    # inside nix develop; then civ3-session newgame/trace/shot
```

On Windows (and in the Windows VM, DX12 WARP) the bridge runs the viewer as
a window of the game's own 32-bit process instead; `vendor/wgpu-hal` carries
a fix that DX12 needs in 32-bit processes.
