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
| `ui.rs`, `ui/display_list.rs`, `ui_screen.rs`, `interface_parts.rs`, `ui_opacity.rs`, `looks.rs`, `lit_orders.rs` | `civ3-ui` | with the blit/copy/fill groups, when their `Event`s come from records. Done with group 4: `ui`, its display list, the frame parts of `ui_screen` and `interface_parts`, the loading cursor (§9); the rest needs `civ3-sys` types out of the way |
| `ui_trace.rs` exe detours | deleted group by group; the remaining install code shrinks to wiring | per group |
| `scene_map.rs`, `units.rs`, `unit_views.rs`, `effects.rs`, `districts*`, `city_labels.rs`, `targeting.rs`, `minimap.rs`, `revealed.rs`, `world*.rs` | `civ3-world` | with the map-blit group (the map canvas's jgl calls become "ignore"). The crate exists with `revealed`; `world.rs`'s model is made of `civ3-sys` types, which the rest builds on (§9) |
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
| `JGL_Color_Table` | 0x68938 | 0x820 | 9 | +0x0C entries, +0x40C/+0x60C 16-bit tables, +0x80C ctor argument, +0x810/+0x814 change stamp |
| font | 0x688AC | 0x24 | 5 | +0x04 metrics pointer, +0x08 embedded base object, +0x14 flags (bit 0: loaded from a file), +0x18 HFONT, +0x20 resource path; exact metric writes in [object semantics](jgl-object-semantics.md) |

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

Since then `FillHooks`, `CopyHooks` (groups 2 and 3), `BlitHooks` (blits,
the sprites' state, the draw scale) and `PaletteHooks` (the colour tables)
exist, and `PixelPolicy` answers per group (`original_fill`,
`original_copy`, `original_blit`, `original_sprite_pixels`). Groups 5 and 6
added `PresentHooks` (+0x30 invalidate, +0xA4 present; `PixelPolicy::
original_present`) and `SurfaceHooks` (objects made and deleted). To come,
in the same style: the rest of `SurfaceHooks` (image created/resized,
clip set/reset, colour key, palette, pixel access), `WindowHooks` (the
graphics window, pump and pointer slots).

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
| `Fill`..`CopyOther` | the fill and copy groups' calls (`jgl::FillCall`, `CopyCall`) with their `Surface`s | 64 to 128 |
| `Blit` | slot, call site, sprite (owner, table, bits, compiled), target and other image, mask sprite, (x, y), colour table and owner, draw scale and who set it, the raw arguments | 212 |
| `SpriteInit`, `SpriteDuplicate`, `SpriteSetTable`, `SpriteSetKey`, `SpriteClear` | the sprite state slots (`jgl::SpriteCall`) | 24 to 52 |
| `SetScale` | graphics +0x9C's three numbers and its call site | 20 |
| `Table` | a colour table slot that changes it (`jgl::TableCall`), slot first | 16 to 28 |
| `Invalidate`, `Present` | graphics +0x30 and +0xA4 (`jgl::PresentCall`): the rectangle (or none); for a present the back buffer as a `Surface` | 24, 68 |
| `Created`, `Deleted` | a factory (+0x74..+0x80) or delete (+0x88..+0x94) (`jgl::SurfaceCall`): class, object, the factory's arguments, whether it has our method table | 24, 12 |

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
| 4 | sprite blits and colour tables (done, §9) | sprite +0x00..+0x18, +0x38..+0xA0; colour table +0x04..+0x20; graphics +0x9C `set_scale` | the exe wrapper detours (roadmap §2) and the map blit detours; jgl makes no sprite pixels (placeholder) |
| 5 | present and invalidate (done, §9) | graphics +0x30, +0xA4 | `jgl_present` slot patch |
| 6 | surfaces (first part done, §9: factories, deletes, our method tables) | factories +0x74..+0x94, image +0x04/+0x08, pixel access | roadmap step 5 |
| 7 | window, pump, pointer, video modes | the remaining graphics slots | roadmap step 6 |

## 7. Counting mode

`CIV3R_JGL_COUNT=1` replaces every slot of the five vtables (201 slots)
with a counting stub generated at install time: `inc dword [counter]`
followed by `jmp [original]`, 11 bytes, no knowledge of the argument count
needed. It wraps whatever is in the slot at that moment, so it is
installed after the group takeovers and counts our entry points too. Every
60 s, and at exit, the bridge logs the slots called since start, with
names from `jgl::slots`, and the number of calls from threads other than
the first caller (the single-producer assumption). Once the surface group
has made our method tables (§9, group 6) they get stubs too, counted
apart: what still goes through jgl's tables are objects that kept
jgl's. This closes the gap the
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

### Group 4 (third round): sprites, records only

Done:

- Group 4 is taken over (`jgl-ffi::blit`) after the RVA check: every
  sprite blit slot +0x38..+0xA0, the sprite state slots +0x00 delete,
  +0x04 `init`, +0x08 `clear`, +0x0C `duplicate`, +0x10/+0x14 colour
  table, +0x18 key; the colour tables' +0x04..+0x20; graphics +0x9C
  `set_scale`. Each entry point is reached through a naked trampoline
  that keeps the call's return address. A blit records a `jgl::BlitCall`:
  the sprite (its owner, own colour table, bits, whether jgl compiled
  it), the images and mask its slot's shape names (`jgl::blit::Shape`,
  [V] for the ten slots the exe's wrappers call), (x, y), the colour
  table and its owner, the draw scale and the call site of the
  `set_scale` that set it, the raw arguments and the call site.
- What the owners are [V, the binary tests]: a `JGLSprite`'s +0x0C is
  its `Sprite`'s +4 (the exe's five `new_sprite` calls pass it; jgl's
  destructor clears it), a `JGL_Color_Table`'s +0x80C its
  `PCX_Color_Table`. The draw scale is jgl's three globals at RVA
  0x6C0FC; span compilation is the switch at 0x70D94 and the rule in
  `jgl::blit::compiles` (0xAA60 and 0x9F50).
- `civ3-ui::blit` names the exe function a blit came from by its call
  site (`CALL_SITES`, checked in both exes by `civ3-ui/tests/exe.rs`) and
  the scaled ones by the `set_scale` call before it (`SCALED`: they set
  the scale, put the scaled crop into the sprite and call an unscaled
  one). `ui_trace::on_blit` gives the interface mirror and the map
  tracer what the old detour of that function gave, from the wrapper's
  own (x, y) recovered exactly (`wrapper_xy`: the crop offset the
  wrapper added, scaled as its `imul`/`idiv` did).
- **Placeholder sprites.** With a viewer and the game not drawing itself,
  jgl makes no sprite pixels: `init` and `duplicate` write jgl's fields
  (size, bits, bytes, pitch, colour table) and point the pixels at one
  shared placeholder (`jgl_ffi::blit::placeholder`: runs of 8 near-magenta
  0x7C1E and 8 black 16-bit words), no pixel copied, nothing compiled
  (what jgl would have compiled is jgl's state: `SpriteRef::compiled`).
  `clear` and delete let go of the placeholder before jgl frees. No blit
  runs jgl's original: the blits are records, the interface model draws
  them from the art files. The game's own window shows the stripes where
  sprites were; with `CIV3R_GAME_RENDER=on`, the compatibility renderer
  or `CIV3R_JGL_COMPARE` (while it existed) jgl made and drew sprites as
  before. Consequence [G, not tested]: when the viewer is lost and the
  game's own drawing is restored, sprites made since start are stripes.
- **The keyed hit test** (0x608C70, its `get_pixel` call at 0x608FE3,
  roadmap §4 #1) is answered from the interface model
  (`UiModel::shown_at`): the topmost draw of the canvas's display list
  with a pixel there, the files' pixels from `ui_opacity::pixel`, the
  same decoded art and key indices (254, 255) the model's opacity uses.
  The viewer's own pointer routing uses the forms' rectangles
  (`tile-viewer` `Layout::hit`), not pixels; both come from the same
  model. Text gives no pixel there [G: jgl's canvas had the glyphs]; a
  file not decoded yet counts as drawn; a canvas the model has no list of
  keeps jgl's pixel.
- Removed: the exe detours on `Sprite::draw` 0x5F8210 and its twelve
  sibling wrappers (0x5F81A0, 0x5F8310, 0x5F8380, 0x5F83E0, 0x5F84A0,
  0x5F8620, 0x5F86A0, 0x5F8760, 0x5F8870, 0x5F89A0) and the map tracer's
  `draw_on_map` 0x5F8060, 0x5F8560 and `draw_with_color_table` 0x5F87E0,
  with their definitions in `civ3-sys`, and the nesting guards they
  needed (`SCALED_NATIVE`, `BLEND_WRAPPER_NATIVE`). The slice, copy and
  reset detours (0x5F7EC0, 0x5F8A20, 0x5F7DB0) stay: the slice's crop and
  source image are the exe's (jgl's `init` gets the trimmed pixels only).

Equivalence [V]: with both paths running (`CIV3R_JGL_COMPARE=1`, commit
eedb3b7, sprites jgl's), on a loaded game (map, advisors F1 to F4, F6,
F7, the city screen, the Civilopedia, diplomacy, unit animation, a turn
change), every one of 50538 calls of the old functions that reached
jgl gave the same events and map-trace records and the same pixel
decision on both paths: draw 20690, with_color_table 14532,
scaled_a4 8749, draw_on_map 3661, scaled_a8 1964, alpha 640, blend 119,
on_map_blend 79, a4b 72, masked 33. One call did not reach jgl (its
sprite read from a file at the draw). The takeover saw no blit outside
the old functions. +0x5C, +0x74 alone, +0x88 and +0x94 were not called
in these screens.
Read-back with placeholder sprites [V, the placeholder run logs every
canvas whose blits the old path drew with jgl and that are now left out,
and every read of one]:

- keyed form canvases (`native_hit_mask`: the HUD's, the city screen's,
  the advisors' and popups' forms): answered by the hit test above;
- three 8-bit images at 0xB3F3E0 (0x2B8 apart): at startup the game
  (0x549D60..) cuts sprites from `Art\oldinterface.pcx`, blits them into
  these images and reads them with `get_pixel` (0x549FAD, roadmap §4 #5).
  No record answered that in this round: sprites cut from that file kept
  jgl's pixels (`READ_BACK_ART`, gone with group 6, below);
- no other read of a canvas whose sprites were left out was logged.
  `CIV3R_TRACE_PIXELS=1` adds `pixel_at`/`lock` reads to the check [not
  run this round].

The hit test counts a copy of a surface the game draws itself (the
minimap on the HUD) as drawn [G for its own see-through pixels]: without
that, a click on the minimap went through to the map.

Placeholder check [V, `/tmp/c3r-g4-shots/{beforeB,placeholder,...}`]:
the viewer shows no stripes in any shot (near-magenta pixel counts within
0 to 11 of `before` everywhere); shots differ from `before` in unit
animation frames, the advisor's quote and portrait, hover and camera
timing only. Clicks: a HUD order button (a rename popup), the city
screen's production box and exit, an advisor's face work as before.
A click on the minimap and one on the map just above the HUD bar act as
before (after the generated-surface rule above). The game's own window is black with a viewer (the game
draws nothing there: `game rendering off`), so the stripes do not show
in it either; they would only show where jgl still draws a sprite.

Counting mode [V, same screens]: sprite +0x44 24350, +0x8C 14532,
+0x74 8749, +0x84 2036, +0x50 640, +0x7C 198, +0x58 33; init 19553,
clear 19553, set_key 19584, set_color_table 33904, delete 14327,
duplicate never; colour table set_colors 30692, get_colors 29999,
+0x20 11673, init 1390, animate 1; set_scale 317591. All from the
game's thread.

Mirroring: everything group 4 feeds is the interface mirror, as before
(`ui.rs`'s display list draws the recorded file regions with our art);
nothing new is mirrored. The mirror's consumers: the advisors, city
screen, diplomacy, Civilopedia, popups, HUD and unit pictures in panels
(roadmap §3). The world's sprites are not mirrored (`rerendered`), the
map tracer records them for `civ3-trace.txt` only.

Crates: `civ3-ui` now has the interface model (`ui`, its display list),
the frame part of `ui_screen` and `interface_parts`, and the loading
cursor; `civ3-world` exists with `revealed`. Left in the bridge: the
rest of `ui_screen` (fonts, render descriptions, the world model,
`civ3_sys::controls`), of `interface_parts` (the game's per-build tables,
prefs, popups), and the world files: `world.rs`'s model is made of
`civ3-sys` types, and `scene_map`, `units`, `effects`, `districts`,
`city_labels`, `targeting`, `minimap` build on it.

### Live check of main (fourth round)

Main at 8991bd5 and a6381cb (the placeholder's buffers as atomics) in a
real game [V, `/tmp/c3r-live-main/` with `index.txt`]: main menu, load,
map; a click on the minimap, on a map tile just above the HUD bar and on
a HUD order button; the city screen, its production list, a queue entry
dragged onto the current build (swapped) and one dragged down the queue
(reordered); advisors F1 to F4, F6, F7, F8; the Civilopedia; the game
menu and Escape; the split view with the floating lens, glass rims, glass
dividers and the bouncing arrow over the selected unit. Every one worked;
no near-magenta pixel in any shot. Nothing needed fixing.

For scripted input: with `input.alt_in_menus` on (the default) the city
screen turns on button snapping, which locks the pointer, and the test
harness's absolute VNC moves no longer steer it (the game's pointer stayed
on the city screen's close button). The sessions of this round run with it
off; a person's mouse is not affected.

### Group 5: present and invalidate

Done:

- Graphics +0x30 `invalidate` and +0xA4 `present` are taken over
  (`jgl-ffi::present`) after the RVA check. Both record a
  `jgl::PresentCall`: the rectangle (or none) and, for a present, the back
  buffer it copies from (`[[graphics + 0x148] + 4]`, as jgl's body reads
  it [V]). `invalidate` always runs jgl's original (`InvalidateRect` on
  jgl's window, whose paint `inproc::paint` turns into the game's paint
  while the window is jgl's). `present` runs it only where
  `PixelPolicy::original_present` says; the bridge answers as the slot hook
  did (`skipping()`, audited as `Present`) and returns jgl's 0 otherwise.
- Removed: the `jgl_present` slot hook and its definitions in `civ3-sys`.

Equivalence [V, `CIV3R_JGL_COMPARE=1`, commit 31f4d74]: each of the exe's
presents (0x606680, which passes its rectangle to +0xA4) brought exactly
one `Present` record with that rectangle, the exe's back buffer
(0xCACFF0) and the old hook's decision; each `Invalidate` record one call
of jgl's `InvalidateRect` import with the same rectangle. Two sessions:
3092 presents and 1905 invalidates (map, advisors, city screen and its
queue, diplomacy, HUD clicks, game menu, a turn change), and 35500
presents and 1494 invalidates (war declared on Babylon, two turns with
combat and a city lost, the Civilopedia): all the same, none missing or
extra. One `InvalidateRect` call of jgl's came without a record (a path
other than +0x30, at start [G]). Screenshots before (a6381cb) and after
(46f6201) of the same walk differ only in unit and leader animation:
`/tmp/c3r-g56-shots/{before,after5,diff-after5}`.

### Counting mode, the longer session (before group 6)

[V, `CIV3R_JGL_COUNT=1`, the two sessions above:
`/tmp/c3r-g56-shots/{compare-counts.txt,count-long.log-counts.txt}`]
102 slots were called. No call came from a thread other than the game's
(drawing threads 1, re-entered 0). Most called in the long session:
image +0xE0 `row_width` 10.4 M, graphics +0xB4 5.7 M, +0x128 4.0 M,
image +0x28/+0x2C `acquire_dc`/`release_dc` 1.5 M each, +0x34 `set_clip`
1.5 M, graphics +0x9C 737 k, image +0xC4 731 k, +0x84 694 k, font +0x10
437 k, image +0xD8/+0xDC 209 k, +0x10 `lock` 205 k, +0x0C `pixel_at`
194 k, +0x1C 169 k, +0xC0 103 k, +0x24 `read_pixel` 66 k. Factories: new
image 1123, new sprite 5223, new colour table 729, new font 297; deletes
906 colour tables, 266 images, 138 sprites (with their `scalar_delete`).
Slots seen called that the first round's list did not have: graphics
+0x12C (110, the long session only), sprite +0x74 (5698, the first
session only). Never called in either: graphics +0x00, +0x08..+0x1C
except +0x10, +0x34, +0x3C, +0x48..+0x58, +0x60..+0x70, +0x84, +0x88,
+0x98, +0x120, +0x134, +0x13C..+0x144, +0x154; image +0x30, +0x3C,
+0x4C..+0x54, +0x5C, +0x60, +0x74, +0x78, +0x8C, +0xBC; colour table
+0x1C; font +0x00.

### Group 6, first part: factories, deletes, our method tables

Done:

- The factories graphics +0x74 `new_font`, +0x78 `new_color_table`,
  +0x7C `new_image`, +0x80 `new_sprite` and the deletes +0x88..+0x94 are
  taken over (`jgl-ffi::surface`). A factory runs jgl's (allocation and
  constructor [V]) and gives the new object **our method table** of its
  class: a copy of jgl's table made when the group is installed, after
  every other group (the display modes' too), so it holds the same entry
  points. A group taken over later writes both tables
  (`takeover::take_over`). The graphics object gets ours at the first
  factory call. Factories and deletes record a `jgl::SurfaceCall`
  (`Created`, `Deleted`); the consumer counts live objects per class.
- The counting mode counts calls through our tables apart.

Result [V, counting mode, the walk above, commit d24b8bc]: through
jgl's tables only graphics +0x04 (`create_window`, before any factory),
the first +0x7C (whose call gives the graphics object our table), +0xB4
once, and 172 calls of sprite +0x20/+0x24: jgl's own temporary sprites on
the stack inside its keyed copies +0x84/+0x8C (it calls the sprite
constructor 0x7E80 directly at RVA 0x2432 and 0x2512). Everything else
the game calls goes through our tables. Screens unchanged
(`/tmp/c3r-g56-shots/{before,after6b,diff-after6b}`: unit and leader
animation only).

Not done (the rest of group 6): the objects themselves are jgl's: their
memory, constructors, DIB sections and DCs; image +0x04 `create`, +0x08
`clear` (recorded by the text group), the pixel access slots +0x0C..+0x24,
the DC slots +0x28/+0x2C (1.5 M calls a session), clip, colour key and
getters. Making them plain memory needs every pixel path that still runs
jgl's original to have an answer first: text, fills and copies onto
read-back canvases (the `Skipper`'s keep set), copies the mirror does not
take (+0x38, the exe's tiling at 0x5FCE9F, 4136 calls), +0x58, and the
DC users (C3X's mill icons and GDI+ lines, jgl's text measuring and
`text_out`).

### The art presence read (0x549FAD): resolved

What it is [V, the exe disassembled]: at start (0x549D60..) the game
loads `Art\oldinterface.pcx`, and for three 32×32 regions (x = 8, 55, 102;
y = 8) cuts a sprite (`Sprite::slice`), makes an 8-bit image (the three
`PCX_Image`s from 0xB3F3E0, 0x2B8 apart), draws the sprite at (0, 0)
(`Sprite::draw`), gives the image the sprite's colour table and reads its
pixel (0, 0) with `get_pixel`: 0xFF (the file has nothing there) deletes
the image, else it is kept in a table at 0xB40430.

Now answered from the model (`ui_trace::art_presence_pixel`, a hook on
that one call): the last sprite drawn into the image (its blit record:
the sprite and where its stored pixels went), the file region it was cut
from (the interface model's `sprite_source`) and that file's pixel,
decoded by `civ3_assets` (the same decoding the exe does into jgl's
image). Outside the sprite's stored pixels (its crop trimmed the key away)
it answers 0xFF [G: no such read seen]. `READ_BACK_ART` is gone: no sprite
gets jgl's pixels with a viewer.

Equivalence [V, commit 4075030]: the three reads gave jgl 0x40
and the model 0x40 (64). After the switch the game keeps the three images
as before; screens unchanged.

`CIV3R_TRACE_PIXELS=1` [V]: the three images are also read by jgl's +0x38
inside the exe's image tiling (0x5FCE9F), which tiles them onto form
canvases; that call runs jgl's original and now tiles the placeholder's
(empty) content. Nothing shows or reads those pixels: the viewer draws
the forms from the model (+0x38 is not mirrored, as before), and the
keyed hit test answers from the model too. The same run lists the keyed
form canvases read by 0x5FCE9F (sources of +0x38 too); those were already
left without sprite pixels in group 4.

The preceding sprite check did not exercise Palace selection, which is
covered below. Canvases whose fills, copies and text jgl still draws remain
the `Skipper`'s keep set: palette-index images of region maps, keyed form
canvases and the navigator.

### Remaining groups and recovered object semantics

The [full object semantics](jgl-object-semantics.md) preserves the image,
sprite, colour-table, font and registry research. Its checked ABI corrections
include palette methods taking `(colors, first, count)`, image +0x14/+0x1C
being pixel-at thunks, +0x18/+0x20 lock thunks and +0x24 unlocking bits.
Constructor, destructor, field, error-code, GDI and file-operation details
retain their original [V]/[G] evidence scope.

The latest reviewed unmerged checkpoint is `907d777` on
`t3/finish-jgl-groups-switch-3`. It was withheld from integration after
data-identity, allocation, copy-aliasing and frame-mask defects were found.
The earlier scoped navigator results below apply to `83caad2`, not this
newer checkpoint. See the [current handover](jgl-continuation.md) and
[review findings](evidence/jgl-group6-review/review.json).
Main's implementation baseline remains
`dd992e0`. The experimental implementation requires `CIV3R_JGL=own`.
Ordinary viewer launches still load original jgl; no-viewer/no-trace launches
keep the plain-game pass-through. The default no-jgl switch is not accepted.

### Palace selection and retained sources

The readers at `0x57C706/0x57C7C4/0x57C7FA` belong to Palace View and change
Palace game state. Their piece-index map is answered from the same file
source, crop, position, clip and eligibility as the viewer. Transparent holes
reveal earlier pieces. No second selection bitmap is rasterised.

A successful exe file-load hook retains the already decoded indices, stride
and palette in one guarded block copy. That copy scales with file size and
runs at loading, not at a blit or first click. Sprites and immutable selection
revisions retain source ownership after image clear, sprite reset and cache
eviction. Publisher-side anchor scans replay the control walk against fields
captured by the same Present, without reading live game memory there.

Parent review confirmed the earlier seven complete Palace maps, 14,515,200
cells and 15 actual reads with zero mismatches. Ten Palace crop comparisons
matched the original across the earlier diagnostic and production runs.
The latest source-capture comparison covers 203 loads and 23,713,540 indices
with zero index or palette differences, plus four matching final Palace
crops. Both upgrades, style changes, reopening without credits and confirmed
Quit passed in the retained local runs. Labels remain under Rename and Quit
and disappear under F1. DLL and ini restoration receipts match the baseline.
See [the Palace review](evidence/jgl-palace-review/review.json) and
[the latest source review](evidence/jgl-source-review/review.json).

### Registry and font lifetimes

Safe registry state now follows the original circular head/cursor order.
Font registration and deletion use the embedded base at +8, and the FOT path
remains owned until resource release. Original-jgl/GDI test logs cover ten
cursor observations, embedded-base deletion and eight font-resource cycles.
The source, binary and test evidence is bound in the latest review. The
previous synchronous query-path decoding and game-thread anchor scans are
removed. The load-time block-copy cost remains explicit.

### Open acceptance work

The PCGames HUD/advisor navigator buffers now preserve their game-written
data through safe, equal-size row copies and 16-bit clears. Original/owned
contract checks covered 40 calls and 1,369,360 bytes. Parent review also
confirmed eight live buffer pairs, 546,720 bytes, and exact final minimap,
F3 and F5 crops. Ordinary canvas drawing remains records; these bytes are not
sent to the viewer. This covers the identified static buffers. Dynamic and
replay navigator instances remain open. The earlier diagnostic F5 text
difference and empty trace limits remain recorded in
[the data review](evidence/jgl-data-review/review.json).

Group 6 still needs the other indexed-reader families, both frame hit masks,
dynamic/generated-data contracts, image +0x38/+0x58 and DC/C3X behavior. The
[contract matrix](jgl-contract-audit.md) records required behavior,
implementation, scoped evidence and each remaining gap. The broader object
replacement is still unaccepted for integration.

Group 7's standalone window/pump implementation is accepted at `5ca2cf6`.
Parent review verified 26 source/artifact/log/cleanup bindings and replayed
2,840 deterministic and 438 real Win32 comparisons with zero differences.
They include nested pump re-entry and 24 sequential hidden-window lifetimes.
See the [window contract matrix](jgl-window-contracts.md) and
[parent review](evidence/jgl-window-review/review.json).

The integrated game still needs launch lock, windowed mode, Alt/capture,
stale input, screen/turn/combat checks, confirmed Quit and the 30-minute
soak. The standalone fixtures intentionally load original jgl as their
oracle; they establish no no-jgl game-session claim. Existing owned game
sessions had no loader entry or mapping, but do not qualify all these
contracts or Neo behavior.

The active GPT-6.1 Sol continuation owns Group 6 and the shared game
reservation. After accepted common-source integration, spawn the unfinished
integrated Group 7 work using [its brief](jgl-group7-integration.md) and
reassign that reservation. The [continuation state](jgl-continuation-state.json)
has the current task and backup receipt. Completed work has no agent entry.
