# Replacing jgl.dll: crates, hooks, records

This is the design for step 2 onwards of `docs/rerender-roadmap.md`: the
game keeps running its own interface code, but the drawing library under
it becomes ours. `jgl.dll`'s objects are taken over one method group at a
time, then built by us, then the DLL is not loaded at all. Facts about jgl
come from the binaries named in the roadmap (§1) and are tagged the same
way: **[V]** checked, **[D]** documented in the repo, **[G]** a guess.

## 1. Constraints

- The game decides what to draw. We replace the library, not its callers:
  no game screen is reimplemented.
- The replacement code runs as 32-bit x86. On the Mac that means FEX
  emulation, so a method body must be a handful of instructions: append a
  record, return. No per-pixel loops, no PE-to-unix call per method.
- The game's code and C3X read some object fields directly (roadmap §1.2),
  so our objects keep jgl's sizes, offsets and return values.
- No locks shared between threads (`docs/concurrency.md`). All jgl calls
  come from the game's thread [G, checked at run time by the counting mode,
  §7].
- Unsafe code is confined. Only `jgl-ffi` (new), `inproc` and
  `wine-unixlib` contain it.

## 2. Crate graph

```
             generic, game-agnostic
   scene   tile-render   menu   tile-viewer   inproc   wine-unixlib
     ^          ^          ^                    ^
     |          |          |                    |
   civ3-world   |       civ3-ui                 |        jgl (safe, no Civ3)
   (Civ3 map:   |       (Civ3 interface:        |          ^
   scene_map,   |        HUD, advisors, city    |          |
   units, ...)  |        screen, popups, text)  |        jgl-ffi (unsafe shim,
     ^          |          ^                    |          no Civ3, no logic)
     |          |          |                    |          ^
     +----------+---- civ3-bridge (IFC23.dll: DllMain, live loop, publisher,
                      wires jgl-ffi's takeover to civ3-ui / civ3-world)
```

| Crate | Contains | Depends on | Unsafe |
|---|---|---|---|
| `jgl` | jgl's behaviour as a safe API: object ids, per-surface state (selected font, text colour, later clip, colour key, palette), fonts, colour tables, the record encoding and decoder, the hook traits, the slot catalogue (names, groups) and call counts | nothing game-specific (`log`) | `#![forbid(unsafe_code)]` |
| `jgl-ffi` | `#[repr(C)]` layouts of the five classes with offset tests against `jgl.dll`; the `extern "thiscall"` slot bodies; method-table takeover and the counting stubs; guarded reads of arguments (strings, rects, fields); later our vtables, objects, surface memory as slices and the `get_graphsy_object_ptr` export | `jgl`, `inproc` | yes, all of it here |
| `civ3-ui` | the Civ3 interface consumer of the jgl hooks: turns text, blits, fills of interface surfaces into the interface model and `menu` items | `jgl`, `menu`, `scene` | none |
| `civ3-world` | the Civ3 world: game state to scene (today's `scene_map.rs`, `units.rs`, `effects.rs`, `districts/`, `city_labels.rs`, ...). Uses the jgl hooks only to ignore map-canvas drawing | `jgl`, `scene`, `tile-render` | none |
| `civ3-bridge` | DLL entry, game-window and input plumbing, the publisher; installs the takeover and hands its hooks to `civ3-ui`/`civ3-world` | all of the above | none (`deny(unsafe_code)`) |

Rules:

- `civ3-ui` and `civ3-world` see `jgl` only: ids, plain values, borrowed
  byte slices. They never read game memory for jgl objects; what a hook needs
  is in its record.
- `jgl` knows nothing about Civ3: no form, canvas or screen names. "The
  owner" of an image (its `+0x80` back pointer) is an opaque `u32` it passes
  on.
- `jgl-ffi` has no policy. Each entry point reads its raw arguments, makes
  safe values, calls `jgl`, and turns the answer back into a register value.

### What moves where

The move is incremental. A piece of `civ3-bridge` moves when it is fed by
the new hooks instead of an exe detour or import hook.

| Today (civ3-bridge) | Goes to | When |
|---|---|---|
| `text.rs` `TextRun` | `civ3-ui::text` | text group (this round) |
| `trace.rs` `record_text`, the `TextOutA` import hook, font logging | deleted; `civ3-ui::text::TextCapture` builds runs from hook calls | text group (this round) |
| `ui.rs`, `ui/display_list.rs`, `ui_screen.rs`, `interface_parts.rs`, `ui_opacity.rs`, `looks.rs`, `lit_orders.rs` | `civ3-ui` | with the blit/copy/fill groups, when their `Event`s come from records |
| `ui_trace.rs` exe detours | deleted group by group; the remaining install code shrinks to wiring | per group |
| `scene_map.rs`, `units.rs`, `unit_views.rs`, `effects.rs`, `districts*`, `city_labels.rs`, `targeting.rs`, `minimap.rs`, `revealed.rs`, `world*.rs` | `civ3-world` | with the map-blit group (the map canvas's jgl calls become "ignore") |
| `modal.rs`, `prefs*`, `native_sliders*`, `game_window.rs`, `heartbeat.rs`, `presenter.rs`, `live.rs` | stay in `civ3-bridge` | `modal.rs` is being changed by another branch now; it moves last |

## 3. Object model and layouts

Our objects keep jgl's layouts exactly. Pointers are `u32` so the same
structs compile on the native side and in tests. `jgl-ffi::layout` has
them; each offset has a test, and a second set of tests reads `jgl.dll`
(when present, `CIV3_JGL_DLL` or the dataset path) and checks the
constructor stores, the getter bodies and the factory allocation sizes.

| Class | vtable RVA | Size | Slots | Fields the game or C3X touch |
|---|---|---|---|---|
| graphics | 0x685F8 | 0x14C | 86 | +0x12C/+0x130 screen w/h, +0x134 depth (8), +0x138 DC, +0x140 HINSTANCE, +0x144 HWND |
| `JGL_Image` | 0x68238 | 0x4D4 | 60 | +0x0C critical section (0x18), +0x24 bit count, +0x40 width (row), +0x44 clip RECT, +0x54 image RECT, +0x7C, +0x80 owner (`PCX_Image*`), +0x84 `BITMAPINFO` (header 0x28 + 256 RGBQUAD), +0x4AC region, +0x4B0 previous bitmap, +0x4B4 bitmap, +0x4B8 current DC, +0x4BC DC, +0x4C0 bits, +0x4C4/+0x4C8 DC and bits lock counts, +0x4CC current bits, +0x4D0 colour key |
| `JGLSprite` | 0x68440 | 0x50 | 41 | +0x0C source, +0x10 colour table, +0x14, +0x18 flags, +0x20, +0x28 key (0xFF), +0x2C..+0x34 getters, +0x38 critical section |
| `JGL_Color_Table` | 0x68938 | 0x820 | 9 | +0x0C entries, +0x40C/+0x60C 16-bit tables, +0x80C ctor argument, +0x810/+0x814 fade state |
| font | 0x688AC | 0x24 | 5 | +0x04 metrics (ctor argument: +0x08 height, +0x0C ascent, +0x10..+0x18), +0x08 embedded base object, +0x14 flags (bit 0: loaded from a file), +0x18 HFONT, +0x20 resource path |

Calling conventions [V]: every slot is `thiscall` (`this` in ecx, callee
pops). `inproc`'s hooks model it as `fastcall` with an unused edx; the shim
uses `extern "thiscall"` directly.

## 4. Hook traits

`jgl` turns slot calls into calls on a few traits, one per method group.
Methods have empty defaults, so a consumer implements only what it uses. All
arguments are plain values or borrowed slices; ids are the game-visible
32-bit addresses (stable while the object lives, reused after it dies,
which is why creation and destruction are hooks too).

```rust
pub struct ImageId(pub u32);   // JGL_Image*
pub struct FontId(pub u32);    // jgl font object*
pub struct SpriteId(pub u32);
pub struct TableId(pub u32);
pub struct Rect { pub left: i32, pub top: i32, pub right: i32, pub bottom: i32 }

/// The text group: fonts, the colour and font each surface has selected,
/// text drawing. Image slots +0xA8..+0xB8, font slots +0x00..+0x10.
pub trait TextHooks {
    /// A text run, with the surface's state resolved by jgl.
    fn text(&mut self, draw: &TextDraw<'_>) {}
}
pub struct TextDraw<'a> {
    pub image: ImageId,
    pub owner: u32,            // the image's +0x80 back pointer, opaque
    pub x: i32, pub y: i32,    // the reference point TextOutA gets
    pub bytes: &'a [u8],       // Windows-1252, as passed
    pub font: &'a FontDesc,    // face, lfHeight, weight, italic, underline
    pub color: TextColor,      // Rgb(r, g, b) or Index(i) (DIBINDEX)
    pub clip: Rect,            // the image's clip at the time of the call
}

/// Whether jgl's own pixel work still runs for a call on `image`
/// (migration only: read-back canvases, compare mode). Asked
/// synchronously by drawing slots; removed with step 4 of the roadmap.
pub trait PixelPolicy {
    fn original_pixels(&self, image: ImageId) -> bool { false }
}
```

Groups to come, in the same style (names fixed now, bodies when the group
moves): `SurfaceHooks` (image created/resized/destroyed, clip set/reset,
colour key, palette), `BlitHooks` (sprite init/duplicate, the blit slots
+0x38..+0xA0 with mode, colour table and crop), `CopyHooks` (+0x40, +0x84,
+0x8C), `FillHooks` (+0x44 fill, +0x48 shade, +0x4C..+0x64 lines and
frames: a line is one call with its two ends), `PaletteHooks` (colour table
set/animate), `PresentHooks` (+0x30 invalidate, +0xA4 present),
`WindowHooks` (the graphics window, pump and pointer slots).

jgl's state for a group lives in `jgl`, not in the consumer: for text, the
font each image has selected (+0xA8/+0xAC) and its colour (+0xB0/+0xB4)
are records that `jgl` applies to its own per-image table, so `TextDraw`
arrives complete. The consumer keeps no shadow of jgl state.

## 5. Records

A slot body does three things: read its arguments, append one record to the
game thread's buffer, and (during migration only) ask `PixelPolicy` whether
to run the original. Decoding and the trait calls happen on the reader's
side of the buffer.

Encoding (little endian, 4-byte aligned, `jgl::record`):

```
u16 op | u16 words   (record length in u32 words, header included)
u32 ...              fixed fields of the op
u8  ...              tail bytes (text), zero padded to 4
```

| Op | Fields | Bytes |
|---|---|---|
| `FontCreated` | font, height, flags, face (tail) | 16 + face |
| `FontDestroyed` | font | 8 |
| `SelectFont` | image, font (0: the stock font, +0xAC) | 12 |
| `TextColor` | image, `0x00BBGGRR`, or `0x10FFiiii` for an index | 12 |
| `Text` | image, owner, x, y, clip (4), bytes (tail) | 36 + text |
| `DcState` | image, face/height/weight/italic/underline/colour read from the image's DC (only for images whose state predates the takeover) | 28 + face |

During migration the buffer is drained right after each append (the
consumers interleave with today's detours, which emit synchronously, and
the order of drawing on a canvas matters). The design target is a
single-producer ring in memory the PE side allocates: the native side reads
it in place (the unixlib can read the process's 32-bit memory), the producer
publishes with one release store of its head, the native side drains once
per present. A full ring sets a resync flag instead of blocking; the
consumer then asks the game for a full redraw.

Cost estimate [G until measured on the Mac]: a text call is about 40 guest
instructions (argument reads, a bounded string copy of 10 to 40 bytes, a
header and nine words). Under FEX at 2 to 4 host instructions each that is
100 to 200 ns, against several microseconds for today's import hook (two
GDI queries, an allocation per run, a `String` per font). The measured
numbers under Wine on x86 are in §9.

## 6. Coexistence with jgl.dll: method-table takeover

Until our factories build the objects, the objects are jgl's and so are
their vtables. jgl's vtables are in its `.rdata`, one per class, shared by
every object of the class. The takeover writes our entry points into slots
of those tables (`inproc::hook::SlotPatch`, which unprotects and restores),
per class and per **method group**. The original pointer is kept, and a
taken-over body calls it:

- when `PixelPolicy` says the surface's pixels are still needed (read-back
  canvases, `CIV3R_GAME_RENDER=on/compare`);
- for state slots whose effect a later original call depends on (selecting
  a font into the DC, setting its colour): these record and then always call
  the original, until the factories are ours and the DC is gone;
- when an argument cannot be read (a bad pointer): the original, so the
  game behaves exactly as it did.

A group is installed only if every one of its slots still holds the
function the 2004-03-17 jgl has there (RVA check, as the existing
`jgl_shade_slot` does). Otherwise the group stays original and the old path
for it is used, or the startup fails as today's capture does.

Groups, and the order they move in:

| # | Group | Slots | Replaces today |
|---|---|---|---|
| 1 | text | image +0xA8, +0xAC, +0xB0, +0xB4, +0xB8; font +0x08, +0x0C, +0x00 (font +0x04 and +0x10 counted only: loading from a file ends in +0x08 through the vtable, measuring stays GDI) | the `TextOutA` import hook and the four exe text-function detours |
| 2 | fills, lines, frames, shade | image +0x44..+0x64 | 0x5FFF30.., `jgl_shade` |
| 3 | copies | image +0x40, +0x84, +0x8C | 0x5FC9F0.. detours |
| 4 | sprite blits and colour tables | sprite +0x04, +0x0C..+0x18, +0x38..+0xA0; colour table | the exe wrapper detours (roadmap §2) and the map blit detours |
| 5 | present and invalidate | graphics +0x30, +0xA4 | `jgl_present` slot patch |
| 6 | surfaces | factories +0x74..+0x94, image +0x04/+0x08, pixel access | roadmap step 5 |
| 7 | window, pump, pointer, video modes | the remaining graphics slots | roadmap step 6 |

## 7. Counting mode

`CIV3R_JGL_COUNT=1` replaces every slot of the five vtables (201 slots)
with a counting stub generated at install time: `inc dword [counter]`
followed by `jmp [original]`, 11 bytes, no knowledge of the argument count
needed. It wraps whatever is in the slot at that moment, so it is
installed after the group takeovers and counts our entry points too. Every
60 s, and at exit, the bridge logs the slots called since start, with
names from `jgl::slots`, and the number of calls from threads other than
the first caller (the single-producer assumption). This closes the gap the
static scan of the roadmap (§1.2) leaves: calls through spilled pointers,
C3X's own calls, and any path the scan missed.

## 8. The final switch: jgl.dll not loaded

Once groups 1 to 7 are ours, nothing calls into jgl's code. Then:

1. The bridge exports `get_graphsy_object_ptr` (from `jgl-ffi`: no
   arguments, returns our graphics object with our vtable).
2. The `LoadLibraryA` hook (`inproc::loader`, already installed from
   `DllMain`) answers `jgl.dll` with the bridge's own module handle, after
   one `LoadLibraryW` on its own path (the exe frees the handle at exit,
   0x6306B9). `GetProcAddress` then finds the export with no further hook.
3. The jgl-specific hooks go: `inproc::hidden`, `inproc::paint`,
   `inproc::screen`'s jgl part, `heartbeat`'s import hooks, the slot patches.

Checks: `inproc::pe::module_base("jgl.dll")` stays `None` for the whole
game; Wine's `+loaddll` log has no jgl; the counting mode's report lists
only our vtables.

## 9. State

### Group 1 (first round)

Done:

- `jgl` (safe) and `jgl-ffi` (unsafe shim). The layouts are checked against
  `jgl.dll` (`crates/jgl-ffi/tests/binary.rs`): every slot RVA of the
  catalogue, the factories' allocation sizes, the image constructor's
  stores and the getter bodies of all five classes.
- Group 1 (text) is taken over and feeds `civ3-ui::text::TextCapture`. The
  `TextOutA` import hook and the four exe text-function detours are gone.
- Unsafe code outside `jgl-ffi`: `inproc` gained `alloc_code`/`write_code`
  (the counting stubs' memory), `gdi::stock_font`, `gdi::dc_text_colorref`
  and `process::thread_id`, because Windows calls live there already.
  `examples/jgl_text_bench.rs` (civ3-bridge) uses unsafe code to call a
  vtable slot and set up a DC; it is a measurement program, not linked
  into the DLL.

Equivalence [V]: with both paths running on a loaded game (title screen,
Load Game, map, advisors F1 to F4, F6, F7), every one of 3148 text runs
had the same text, position, font, colour, canvas and clip on the old
and new path, and none was drawn outside the exe's text functions (which
the old path needed for the canvas). Traces and screenshots before and
after differ only in animated and timed content (unit animation frames,
the blinking "next turn" line and button, the advisor's quote, the pointer
and its tooltip): `/tmp/c3r-jgl-shots/{before,after,diff}`.

Cost [V, x86-64 host, Wine 11 WoW64, release build, 200k to 500k calls,
`examples/jgl_text_bench.rs`]: a `text_out` through jgl with the old import
hook took 737 to 981 ns, the taken-over slot 226 to 319 ns (3.1 to 3.5
times less; jgl drawing for real takes 1100 to 1900 ns). The old figure
leaves out the exe text-function detour each run also passed. Under FEX
the old path's two GDI queries and its guarded read are emulated Wine
code and system calls, so the ratio there should be larger [G, not
measured: no Mac in this round].

Counting mode, same session [V]: 100 of the 201 slots were called. Calls
the static scan of the roadmap (§1.2) did not find include the pumps
graphics +0x128 (2.2 M) and +0x150, image +0x38 (a copy, 4020), sprite
+0x44 (2995), +0x50, +0x7C, +0x84 (blits), colour table +0x20, and the
font measure +0x10 (2.2 M calls, kept by jgl). No call came from a thread
other than the game's. A longer session (city screen, diplomacy, combat,
turn change, Civilopedia) should run before group 6.

Display modes (part of group 7, `jgl::display`, `jgl-ffi::display`):
graphics +0x58, +0x5C and +0x60 are taken over, so the game's mode never
switches the display. The game asks for its screen's size at 16 bits
(`KeepRes=1`: the display's size, at least 1024×768), and jgl's +0x58 and
+0x5C answer 1 (the game quits at 0x56C3BA, exit code 0) when the display
lists no such mode: any display shorter than 768 lines. Each now answers
with jgl's own +0x60 keep branch (screen fields, the window owner told the
mode). [V] on a 1280×729 output (the game at 1280×768, the viewer at
1280×729) and on 1920×1080. The movie slots +0x6C/+0x70 stay jgl's, but
jgl's one `ChangeDisplaySettingsA` import is answered: the shutdown
(+0x08), +0x64 and the movie's switch and back (+0x6C, +0x70) all call
through it, none changes the display and each is told it did. Without it
the shutdown restored the mode +0x64 saves at +0x04, which nothing filled
once +0x58..+0x60 were taken over, and Wine answered -2 ("Changing (null)
display settings returned -2", seen at the end of a game on TS Neo). A
binary test checks that these four are all the calls through that import.

### Groups 2 and 3 (second round)

Done:

- Group 2 (fills, lines, frames, shade: image +0x44..+0x64) and group 3
  (copies: image +0x38, +0x40, +0x84, +0x8C) are taken over
  (`jgl-ffi::fill`, `jgl-ffi::copy`), each after the RVA check. Every call
  records a `jgl::FillCall` or `jgl::CopyCall` carrying the images it
  touches as `jgl::Surface` (owner, clip and bounds at the call). The
  game thread's `Jgl` and the hooks factory are shared by all groups
  (`jgl-ffi::state`), so the consumer sees the calls in the game's order.
- `civ3-ui::draw` (`FillCapture`, `CopyCapture`) turns the calls the
  interface mirror takes into its rectangles and copies:
  `ui_trace::on_fill`, `ui_trace::on_copy`. `mirrored_fill` and
  `mirrored_copy` name the calls taken, and `PixelPolicy` skips jgl's
  pixels only for those, exactly where the old detours did (the
  `Skipper`'s keep set, `ui_trace::skips`). `rgb555`, `shade_alpha` and the
  line rasteriser moved there from `ui.rs`.
- Removed: the exe detours on `PCX_Image::fill`, `fill_area`, `line`,
  `hline`, `vline` (0x5FFF30..0x600010), `draw_onto`, `copy_stretch`,
  `copy_same_rect`, `copy_rect`, `copy_rects` (0x5FC9F0..0x5FCCB0), the
  `jgl_shade` slot hook, and their definitions in `civ3-sys::ui_hooks`.
- What the slots are [V, jgl.dll and the exe disassembled; the binary
  tests check the bodies and the exe wrappers' calls]: +0x44 `fill(rect,
  color)` (null rect: the whole image); +0x48 `shade_rect`; +0x58
  `replace_color(rect, from, to)`, which swaps one pixel value for another
  within the rect and the clip (RVA 0x2740; reads the pixels); +0x64
  `line(x1, y1, x2, y2, color, flag)`, the exe's line, hline and vline
  passing flag 1; +0x5C draws on 8-bit images only; +0x40 `copy(target,
  src, dst)` is a `StretchBlt` from `this` onto the target; +0x84 the
  whole image keyed onto the target; +0x38 copies an 8-bit `this` through
  its colour table onto an 8- or 16-bit target. What +0x38's last
  argument selects, and what +0x4C, +0x50, +0x54, +0x60 and +0x8C do, is
  not known [G]: they are recorded with their raw arguments and left to
  jgl. +0x3C and +0x74 (copy slots of the catalogue) were not seen called
  and stay jgl's.

Not taken by the mirror, as before (jgl draws them): +0x38 (4050 calls,
all from the exe's image tiling at 0x5FCE9F), +0x58 and the slots not
seen called, palette-colour shades, and images without an owner.

Mirroring [V]: everything groups 2 and 3 feed is the interface mirror
(`ui.rs`'s display list), which copies the game's drawing operations
call by call; nothing here is re-rendered from game state. The map's
grid and range outline (`pcx_line` onto the map canvases) stay out of it
as before (`ui_trace::rerendered`: the renderer draws them from the game's
state). The new path invents no mirror: it takes exactly the calls the
old detours took.

Equivalence [V]: with both paths running on a loaded game
(`CIV3R_JGL_COMPARE=1`, commits 07a9082 and 5d9224c; save `t.SAV`: map,
advisors F1 to F4, F6, F7), every one of 49132 calls through the old
functions (draw_onto 39524, copy 5560, copy_same 159, copy_rect 6,
copy_rects 11, fill 772, fill_area 2617, hline 162, vline 142, line 108,
shade 71) gave the same events (65139 in all: sizes, fills and copies
with their clips) and asked the same skip question (canvas and source)
on the old and new path; none of the calls whose image had no pixels
reached the takeover. The takeover's calls outside the old functions
(4050 of +0x38, 49 fill-group calls such as +0x58) were all ones the
mirror does not take. Traces before (main) and after differ only in the
advisor's quote and unit animation frames; screenshots only in those, the
pointer and the hovered menu entry: `/tmp/c3r-jgl-g23-shots/{before,
compare,after,diff}`.

Counting mode after the takeover [V, same screens]: image +0x84 39990,
+0x40 5744, +0x38 4050, +0x44 3423, +0x64 411, +0x48 71, +0x58 52;
+0x4C, +0x50, +0x54, +0x5C, +0x60, +0x8C never. All from the game's
thread. (The first round counted +0x84 as never called: the old detour
skipped `draw_onto` above jgl, so no call reached the slot.)

Cost: not measured this round [G: a fill or copy record is 16 to 28
words, read from the image fields directly; no GDI query, no guarded
read].

Next groups, in order: sprite blits and colour tables (group 4, the
largest; then `ui.rs`, `ui/display_list.rs`, `ui_screen.rs` and
`interface_parts.rs` move to `civ3-ui`, and the map-canvas parts to
`civ3-world`); present and invalidate (group 5); then the factories and
surfaces (group 6, roadmap step 5) and the window (group 7, step 6).
