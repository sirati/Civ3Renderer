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
| `tile-viewer` | no | window that draws scenes and reports input |
| `civ3-assets` | format reader | PCX, FLC, INI readers; game file lookup; `render-desc` image loader |
| `inproc` | no | safe abstractions for injected code: guarded memory reads, hooks, DLL entry |
| `civ3-sys` | facts only | Civ III binary facts: build detection, addresses, generated struct layouts |
| `civ3-bridge` | **yes** | runs in the game (as `ifc23.dll`), reads its state, drives the renderer |

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

`tile_render::reference` renders the same thing on the CPU; the GPU output is
tested byte for byte against it (`cargo test -p tile-render`).

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
