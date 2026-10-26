# Rendering everything ourselves: jgl.dll inventory and roadmap

Goal (from the user): nothing copied from the game, everything rendered by
us, and `jgl.dll`, the game's drawing library, never loaded. The main track
is a **replacement jgl**: our code provides what the game asks of jgl.dll,
so the game calls us directly. World drawing calls become no-ops, interface
drawing calls become our render commands, and surfaces, palettes and the
window come from plain memory and sensible values.

Tags: **[V]** checked for this document (method given), **[D]** stated by
the repo's code or docs (cited), **[G]** a guess or inference, still to be
checked. Addresses are PCGames v1.22 (the base of the C3X exe), as in
`docs/civ3-map-rendering.md`. Binaries used:
`/mnt/datasets/winvm/civ3-src-c3x28/Civilization III Complete/Conquests/`
(`jgl.dll` of 2004-03-17, `Civ3Conquests-Unmodded.exe`, `Civ3Conquests.exe`
= C3X R28), read with pefile and capstone.

## 1. What the game gets from jgl.dll

### 1.1 One export, five object classes [V]

- The exe does **not** import jgl.dll. Its PE import table lists opengl32,
  binkw32, ifc23, winmm, advapi32, user32, ole32, comdlg32, kernel32, gdi32
  and wsock32, and no jgl.
- jgl.dll exports exactly one function, `get_graphsy_object_ptr` (RVA
  0x3B290). It takes no arguments and returns a new 0x14C-byte "graphics"
  object (vtable RVA 0x685F8).
- Load path: `0x56C297` calls `0x620240(“jgl.dll”, title, …)`, which calls
  `0x6306E0`. That function runs `LoadLibraryA("jgl.dll")` (bare name; string
  at 0x72CCD8), stores the HMODULE at 0xCCCF78, runs
  `GetProcAddress(h, "get_graphsy_object_ptr")`, calls the result as
  `call eax` with no arguments, and stores the object at **0xCAB28C**. On
  failure it calls `0x6306A0`, which runs `FreeLibrary`. Then `0x620288`
  calls graphics vt+0x04 (create window) with
  `(controller 0xCACD30, flags, back buffer 0xCACFF0, a4)`.
- jgl.dll imports only KERNEL32, USER32 and GDI32. It uses **no DirectDraw,
  no sound and no winmm**. The brief said DirectDraw, but that is wrong for
  this build: surfaces are GDI DIB sections (`CreateDIBSection`), and the
  window is updated with `BitBlt`.

Every other entry point is a virtual method of an object that jgl creates.
The classes, found from the vtables their constructors store:

| Class | vtable RVA | Size | Slots | Made by |
|---|---|---|---|---|
| graphics (`JGL_Graphics`) | 0x685F8 | 0x14C | 86 | the export |
| `JGL_Image` (surface) | 0x68238 | 0x4D4 | 60 | graphics +0x7C |
| `JGLSprite` | 0x68440 | 0x50 | 41 | graphics +0x80 |
| `JGL_Color_Table` | 0x68938 | 0x820 | 9 | graphics +0x78 |
| font | 0x688AC | 0x24 | 5 | graphics +0x74 |

jgl itself stubs out 24 graphics slots and 12 image slots: they only run
`mov eax,0x18; ret` ("not implemented", e.g. graphics +0xC8..+0x11C, image
+0x68..+0xA4 except +0x74/+0x78/+0x84/+0x8C).

### 1.2 Slots the exe calls

Method: a linear disassembly of `.text`, tracking registers loaded from
`[0xCAB28C]` (graphics) and `[x+4]` (the jgl object behind a
`PCX_Image`/`Sprite`/`PCX_Color_Table`/font, all at +4), then
`call [vt+N]` [V]. This is a **lower bound**: the scan misses calls whose
pointer was held across many instructions or spilled. For example the pump
slots +0x124/+0x12C, which `docs/modal-input.md` documents, do not appear.
The C3X exe gives the same counts in `.text`. C3X's own code is listed
separately in 1.3.

**Graphics object.** 900 call sites over 31 slots were found, 799 of them
+0xB4. The meanings come from the jgl implementations [V]:

| Slot | jgl RVA | What it does | Exe use | Group |
|---|---|---|---|---|
| +0x04 | 0x3B400 | register the class, `CreateWindowExA`, `GetDC`; fields: +0x12C/+0x130 screen w/h (`GetSystemMetrics`), +0x134 = 8, +0x138 DC, +0x140 HINSTANCE, +0x144 HWND | 0x620288 | window |
| +0x08 | 0x3B390 | `ChangeDisplaySettingsA` reset, `DestroyWindow` | shutdown | window/video |
| +0x0C | 0x3B2C0 | release | 0x6306AC | lifetime |
| +0x10..+0x1C | 0x3BEA0.. | `ShowWindow` variants | 1 | window |
| +0x20 / +0x24 | 0x3BE80/90 | get HWND / HINSTANCE | 19 / 3 | window (dialogs, Bink, focus) |
| +0x28 | 0x3BC10 | `SetWindowPos` | 1 | window |
| +0x2C | 0x3BBD0 | `GetCursorPos` + `ScreenToClient` | 2 | input |
| +0x30 | 0x3BBB0 | `InvalidateRect(hwnd, r, FALSE)` | 5 (Base_Form m73) | paint |
| +0x38 | 0x3BB70 | `GetWindowRect` | 2 | window |
| +0x58, +0x5C, +0x60 | 0x3B870, 0x3B900, 0x3B810 | set the mode: look it up in +0x68's list and switch with +0x64 (no such mode: 1, the game quits); +0x60 with its flag set takes the size without switching. Taken over: `jgl::display` | 1 each (0x56C376, 0x56C3A0, 0x56C3B7) | video mode |
| +0x64..+0x70 | 0x3B9E0.. | set mode by index (`ChangeDisplaySettingsA`), the mode list (`EnumDisplaySettingsA`), a movie's 640×480 switch and back | 2+2 | video mode |
| +0x74 / +0x78 / +0x7C / +0x80 | | new font / colour table / image / sprite | 2 / 1 / 1 / 5 | factories |
| +0x88..+0x94 | 0x3C0C0 | delete an object | 1 each | factories |
| +0x9C / +0xA0 | 0x3C0E0/0x3C100 | set / get the global draw scale (num_x, num_y, den) | 30 / 6 | drawing state |
| +0xA4 | 0x3BAA0 | present: `BitBlt` of the back buffer to the window | 1 (0x6068AE) | present |
| +0xA8 / +0xAC / +0xB0 | | get screen w / h / depth (+0x12C/+0x130/+0x134) | 4 / 2 / 0 | screen |
| +0xB4 | 0x3BF10 | `xor eax,eax; ret`, the pixel format query | **799** | trivial |
| +0xB8 / +0xBC | | `CreateCompatibleDC` / `DeleteDC` | 1 / 1 | GDI |
| +0xC0 / +0xC4 | | `CreatePalette` / `DeleteObject` | 1 / 0 | palette |
| +0x120..+0x150 | 0x3D850.. | message pumps: `PeekMessageA`/`WaitMessage`/`TranslateMessage`/`DispatchMessageA` | +0x124, +0x12C [D modal-input.md], +0x148/+0x14C [V] | input/timing |

**Objects behind `+4`.** 409 sites over 38 slot offsets were found. The
scan cannot always tell an image from a sprite, so slots are named from the
jgl implementation and the C3X header (`c3x-src/Civ3Conquests.h:3712`):

- `JGL_Image`
  - pixel access: +0x0C `pixel_at` (pointer), +0x10 lock (bits);
    +0x14/+0x1C and +0x18/+0x20 are thunks to these two; +0x24 reads a
    pixel value.
  - GDI DC: +0x28 `acquire_dc`, +0x2C `release_dc`.
  - clip: +0x34 set (`SelectClipRgn`), +0xC4 reset.
  - copy: +0x40 opaque copy (`StretchBlt`), +0x84/+0x8C keyed draw-onto.
  - fill: +0x44 fill, +0x48 `shade_rect`.
  - lines and frames: +0x4C..+0x64, with +0x64 the line and +0x5C the rect
    frame.
  - text: +0xA8/+0xAC select font, +0xB0/+0xB4 `SetTextColor`, +0xB8
    `TextOutA`.
  - getters: +0xCC clip RECT*, +0xD4 image RECT*, +0xD8/+0xDC width/height
    (113 sites), +0xE4 format (bit count).
  - palette: +0xEC (`SetDIBColorTable`).
  - lifetime: +0x04 create (`CreateDIBSection`, zeroed), +0x08 clear.
- `JGLSprite`
  - +0x04 init from an image rect, +0x0C duplicate.
  - +0x10/+0x14 set/get colour table: the paired +0x10/+0x14 sites all over
    the exe (city screen, citizens, units) are palette swaps around blits,
    not pixel reads [G, from the pairing and the slot bodies].
  - +0x18 colour key.
  - blits +0x38..+0xA0. The exe wrappers use +0x44, +0x50, +0x58, +0x5C,
    +0x74, +0x7C, +0x84, +0x88, +0x8C, +0x94 (`docs/civ3-map-rendering.md`
    §16.4).
- `JGL_Color_Table`
  - +0x0C `AnimatePalette`, +0x10/+0x14 get/set colours; +0x04 and +0x14
    use `QueryPerformanceCounter` (palette fades).
  - Entries start at +0x0C. The bridge reads them directly
    (`crates/civ3-sys/src/ui_hooks.rs:421-466`).
- font
  - +0x04 `AddFontResourceA` from a file, +0x08 `CreateFontIndirectA` +
    `GetTextMetricsA`, +0x10 `GetTextExtentPoint32A` (measuring), HFONT at
    +0x18 (read directly by `crates/civ3-sys/src/map_messages.rs:65-77`).

The exe and C3X also read object **fields** directly, so a replacement must
keep these offsets:

- `JGL_Image`: +0x24 bit count, +0x44 clip, +0x54 image rect, +0x80 the
  owning `PCX_Image`, +0x4BC DC, +0x4C0 bits.
- `JGLSprite`: +0x10 colour table, +0x18 flags.
- colour table: +0x0C entries.

Sources: `ui_hooks.rs:497-560`, C3X `injected_code.c:34478`.

### 1.3 C3X's direct use [V, grep of `c3x-src/injected_code.c`]

| What | Where | jgl dependency |
|---|---|---|
| widths/heights of loaded art | 25 calls `m54/m55` | image +0xD8/+0xDC |
| minimap city dot | `patch_get_pixel_to_draw_city_dot` :18367 | `pixel_at`, writes a navigator pixel |
| combat-odds HUD background | `save/restore_combat_odds_hud_background` :29260-29330 | reads and writes canvas pixels one by one |
| mill resource icons on the city screen | :34703 | `acquire_dc` on canvas and sheet, then GDI `TransparentBlt` |
| clip | 2× `m13_Set_Clip_Region` | image +0x34 |
| lines by GDI+ on Wine | `patch_OpenGLRenderer_initialize` :34465 | reads `JGL_Image->DC` directly |

The vanilla exe's `OpenGLRenderer` (0x6274B0..0x627900) draws lines with
WGL on an image's DC (`acquire_dc` at 0x6274CA). C3X swaps in GDI+ under
Wine. The bridge hooks all five entry points (`ui_hooks.rs:1072`,
`ui_trace.rs:1474-1510`).

## 2. What the bridge does with each group today

Policy: with a viewer, the original pixel work is **skipped by default**
(`CIV3R_GAME_RENDER=off`; `crates/civ3-bridge/src/game_render.rs:1-31`).
Each hook records the operation into the UI mirror and then skips the
original call (`pixel_callback`, `game_render.rs:66`), except for canvases
the game reads back (`ui_trace.rs:343-372`; see §4).

| jgl group | How we reach it today | Mirrored? | Original runs? |
|---|---|---|---|
| sprite blits | **taken over** in jgl's method tables (sprite +0x38..+0xA0, the state slots, colour tables, graphics +0x9C; `jgl-ffi::blit`); the exe wrapper is named by the call site (`civ3-ui::blit`), replacing the detours on 0x5F8210, 0x5F81A0, 0x5F84A0, 0x5F8760, 0x5F83E0/86A0/8870, 0x5F89A0, 0x5F8620, 0x5F8310, 0x5F8380 | yes | **never**: with a viewer the sprites' pixels are a placeholder (`docs/jgl-replacement.md` §9) |
| map blits | from the sprite group too (`draw_on_map` 0x5F8060, 0x5F8560, `draw_with_color_table` 0x5F87E0 by their call sites; the detours are gone) | recorded per sprite by the map tracer; world drawn natively | no |
| copies | **taken over** in jgl's method tables (image +0x38, +0x40, +0x84, +0x8C; `jgl-ffi::copy`, `civ3-ui::draw`); +0x40 and +0x84 mirrored, as the exe detours 0x5FC9F0..0x5FCCB0 did | yes (+0x38 not) | only onto read-back canvases |
| fills, h/v lines | **taken over** (image +0x44..+0x64; `jgl-ffi::fill`, `civ3-ui::draw`), replacing the detours 0x5FFF30, 0x5FFF50, 0x5FFFE0, 0x600010 | yes, as fill rects | same |
| **lines** | image +0x64, taken over (was the 0x5FFFB0 detour): **one fill event per run of pixels** (`civ3_ui::draw::line_rects`) | yes | same |
| OpenGL/GDI+ lines | 5 C3X-jumped sites, emitted as coverage fills (`ui_trace.rs:1500-1505`) | yes | same |
| shade | image +0x48, taken over with the fill group (was a slot patch) | yes | same |
| text | **taken over** in jgl's method tables (image +0xA8..+0xB8, font +0x08/+0x0C; `jgl-ffi::text`, `docs/jgl-replacement.md`) | yes, as text runs from records | `text_out` skipped on mirrored canvases; measuring runs |
| present, invalidate | **taken over** (graphics +0xA4, +0x30; `jgl-ffi::present`, `docs/jgl-replacement.md` §9): records; the exe's present (0x606680) still gives the composition events | composition events | `BitBlt` skipped; `InvalidateRect` runs (the window is jgl's) |
| pixel reads | `get_pixel` 0x600220 (`ui_trace.rs:1156`); image slots +0x0C/+0x10 traced only with `CIV3R_TRACE_PIXELS` (`ui_trace.rs:1394`) | n/a | yes |
| window | jgl's `CreateWindowExA`/`ShowWindow`/`SetWindowPos` (`inproc/src/hidden.rs:84`), `InvalidateRect`/`BeginPaint` (`inproc/src/paint.rs:102`) | n/a | yes, kept hidden |
| screen size | jgl's `GetSystemMetrics` (`game_window.rs:137-139`) | n/a | yes, scaled |
| pointer | jgl's `GetCursorPos` (`game_window.rs:145`, `live.rs:2675-2745`) | n/a | yes, redirected |
| pump | jgl's `PeekMessageA`/`WaitMessage` as the heartbeat (`heartbeat.rs:99-110`) | n/a | yes |
| factories, deletes | **taken over** (graphics +0x74..+0x94; `jgl-ffi::surface`): records; every object made gets our method tables, the objects are still jgl's | no | yes (jgl builds the objects) |
| video mode | taken over (`jgl::display`) | no | never switches |

So today the bridge does not copy jgl's output pixels. It **re-records the
game's drawing calls** (the "mirror") and suppresses most of jgl's
rasterising. The cost is in recording. Each hooked call runs a detour into
32-bit Rust (emulated by FEX on the Mac), reads game memory and updates
per-canvas display lists. `pcx_line` is the worst case: thousands of 1×1
fills per line.

## 3. Inventory per surface: native or mirrored

| Surface / screen | Status | Code |
|---|---|---|
| World map (terrain, overlays, cities, units, effects, fog, districts) | **native** from game state; game map blits skipped | `scene_map.rs`, `units.rs`, `effects.rs`, `districts/`, `trace.rs` (recorder) |
| Map canvas extras: grid, trade net, labels, lines drawn onto the map canvas (`ui_trace.rs:326-331`: MSF canvas, unit overlay, 0x9C7560) | **mirrored**; mirroring removal **in progress on branch `native-tradenet`** | `ui_trace.rs` |
| City labels | native port of `draw_city_hud`; mirror copies also recorded | `city_labels.rs` |
| Targeting outlines, go-to path, city-site preview | native | `targeting.rs`, `city_site.rs` |
| Map messages | native | `unit_notices.rs` |
| Minimap picture | native, pixel-exact; frame and viewport rectangle mirrored | `minimap.rs` |
| Summary replay map | native | `replay.rs`, `replay_thread.rs` |
| Scenario preview | native | `preview.rs` |
| Cursor | native (game sprite cursor excluded from the mirror) | `cursor.rs` |
| Button hover/pressed looks, lit orders | native from control state | `looks.rs`, `lit_orders.rs` |
| City screen's map view | native, overlay mirrored and shifted | `city_screen.rs` |
| Preferences: Resolution/Layout tabs | native; CivIII tab mirrored | `prefs.rs` |
| HUD (`Main_GUI`), unit box, order buttons | **mirrored** and arranged | `ui.rs`, `ui_screen.rs`, `interface_parts.rs` |
| Advisors F1-F7, F8, F11, end screens, Hall of Fame, ranking, victory | **mirrored** | same |
| City screen, palace, diplomacy, espionage, Civilopedia, popups, context menus, title menu, setup, load/save | **mirrored** | same |
| Unit pictures in panels, leader heads (FLC frames) | **mirrored** as animation frames | `ui_trace.rs`, `ui.rs` |
| Text | **mirrored** text runs, drawn with our glyphs | `text.rs` |
| Tooltips (drawn onto the back buffer in `present`) | **mirrored** (overlay layer) | `ui.rs` |
| Right-click tile info | kept by the viewer; contents mirrored | `detached.rs` |

Headline: the whole world is native. Every form of the interface is
mirror-only. With a replacement jgl, the interface stays command-driven: the
game's UI code still decides what to draw. Its calls become our commands
instead of being intercepted and copied.

## 4. Pixel read-back: what the game actually reads

Pixel read-back includes keyed forms and the Palace selection map. Found by scanning every direct
call of `PCX_Image::get_pixel` (0x600220, 24 sites) [V] and from the
bridge's audit classes (`ui_trace.rs:62-75`, `game_render.rs:12-17`) [D].
The [object semantics](jgl-object-semantics.md) corrects the earlier
attribution of `0x57C7xx` to navigator code.

| # | Reader | Image | Data origin | Needs our renderer? | Preferred answer (hook) |
|---|---|---|---|---|---|
| 1 | hit test 0x608C70, call at **0x608FE3** | the form's own 16-bit canvas (+0x274) of keyed forms (`Status1 & 0x1000000`, not `& 0x102`; `ui_hooks.rs:529-538`); e.g. the HUD over the map | **drawn by the UI** | **yes** | **done** (group 4): the call at 0x608FE3 is answered by `UiModel::shown_at`, the canvas's display list with the files' pixels (`ui_opacity::pixel`), whenever sprites are the placeholder. Text gives no pixel [G]. |
| 2 | frame hits at 0x608E0A and 0x608E7E | form+0x11C and form+0x128 | Allocated images, source/writer provenance still open [V for allocation and readers] | shared frame/model answer if drawn | Both compare against the key. The earlier file-source guess is insufficient. See [contract audit](jgl-contract-audit.md). |
| 3 | region maps: city screen 0x414FC9/0x4154C9/0x415719 (form+0xC54); 0x4CCBF6 (+0x662C); espionage 0x52B409/0x52B55E/0x52B629 (+0x3DC8); 0x5835BA/0x583626/0x583A23 (+0x2B60); 0x5A45DC/0x5A488F/0x5A5013 (+0x574); 0x5DEA2A/0x5DEA94 (+0x3958) | 8-bit index images | PCX files, decoded by the **exe** (0x5FC700 → 0x600100 → lock + exe decoders 0x62B5F0/0x62C0B0/0x62BD90) [V] | no | none needed: the exe writes the file pixels into our plain memory |
| 4 | cursor hotspots 0x54AFC5/0x54AFE9 (0x54AF90 loads `Art\CURSOR.PCX`) | stack temporary | file | no | none |
| 5a | 0x549FAD | Three 8-bit images at 0xB3F3E0, 0x2B8 apart | Sprite cuts from `Art\oldinterface.pcx`, pixel (0, 0) [V] | no | **answered** from the file since group 6's first part, `docs/jgl-replacement.md` §9 |
| 5b | Palace View reads 0x57C706/0x57C7C4/0x57C7FA | form+0x3154, screen-size 8-bit piece-index map | 0x57C8A0 clears to 0xFF and draws upgradeable pieces with sprite +0x74, solid piece index wherever source pixel is below 0xFE [V] | file geometry and Palace state | **open**: answer hover/click from the same geometry/state as the viewer; click changes Palace game state. Placeholder sprites alone leave this unanswered. See [object semantics](jgl-object-semantics.md). |
| 5c | Navigator image copies at 0x5784EC/0x578625/0x578733/0x57886C | Navigator images | Game-written pixels | no | Plain memory plus real `copy` (+0x40) for game-written data; separate from the Palace reads |
| 6 | sprite crop trimming (worker 0x62B170 via slots +0x18/+0x1C), FLC decoding, exe dotted-frame loops 0x62B720/0x62B9D0/0x62B860/0x62BBA0 | 8-bit sources, temporaries | game-written | no | plain memory. The frame should be captured as a command at 0x600980. |
| 7 | C3X combat-odds HUD save/restore, city dot | canvas / navigator | game-written | no | plain memory (it saves and restores pixels nobody shows) |

Plain memory preserves pixels the exe decodes or writes itself. Keyed forms
and Palace selection also need model/file answers. Palace interaction must
be verified before claiming all sprite read-back is covered. A CPU-surface
fallback remains optional; it does not replace the required shared source
for drawing and hit testing.

## 5. The replacement jgl

### 5.1 Getting it loaded instead of the original [V for the mechanism]

- The bridge already redirects the exe's `LoadLibraryA` import from
  `DllMain`, before jgl loads (`game_window.rs:101-106`,
  `crates/inproc/src/loader.rs:38-48`). Today it calls the original and then
  hooks jgl (`load_then_notify`, `loader.rs:117-128`). For `jgl.dll` it can
  instead return our module without loading the original.
- The simplest module is **the bridge itself** (loaded as `IFC23.dll`, which
  the exe imports statically). It exports `get_graphsy_object_ptr`
  (`extern "C"`, no arguments, returns the object). Then
  `GetProcAddress(h, "get_graphsy_object_ptr")` resolves with no further
  hook. The hook must add a reference to the module (`LoadLibraryW` on our
  own path) because the exe calls `FreeLibrary` on the handle at exit
  (0x6306B9). A separate small `jgl` cdylib would also work. It is not
  worth a second module and the state shared across modules.
- Wine `WINEDLLOVERRIDES` cannot do this. The bare name resolves to the exe
  folder's `jgl.dll` first, and an override only chooses between native and
  builtin. Writing a `jgl.dll` into the game folder would change the user's
  install. Both are rejected.
- The loader tests (`loader.rs:137-240`) already cover the name forms
  (`jgl`, `JGL.DLL`, full paths).

### 5.2 What each slot needs

| Class | Trivial (constant or no-op) | Plain-memory implementation | Command into the ring | Real work |
|---|---|---|---|---|
| graphics | +0xB4 (0), +0x98, jgl's 24 stub slots (0x18), show/position/rect slots, display-mode slots (keep the mode), +0xB8/+0xBC/+0xC0/+0xC4 (GDI objects: dummies) | +0x20/+0x24 handles, +0xA8/+0xAC/+0xB0 size from `game_size` (replaces the `GetSystemMetrics` hook), +0x9C/+0xA0 scale state | +0x30 invalidate (dirty rect), +0xA4 present | +0x04 window: create a hidden window whose window procedure maps WM_* to controller slots (`docs/modal-input.md` table: key +0x0C, char +0x10, move +0x14, wheel +0x18, buttons +0x1C/+0x24, timers +0x30), paint = controller vt[1]. +0x120..+0x150 pumps (Peek/Wait/Dispatch, stamping the heartbeat directly). +0x2C pointer position (the viewer's pointer directly). |
| image | +0x30, stubs (0x18), getters (+0xCC..+0xE8 return fields) | +0x04 allocate (zeroed, lazily for 16-bit canvases), +0x08 free, +0x0C/+0x10 and thunks (pointers into our buffer), +0x24 read, +0x34/+0xC4 clip fields, +0xEC palette field; +0x40 copy as a real `memcpy` only for images in §4 #5 | +0x40/+0x84/+0x8C copies, +0x44 fill, +0x48 shade, +0x4C..+0x64 lines and frames, +0xB8 text (string, font, colour) | +0x28 `acquire_dc`: C3X's mill icons and GDI+ need an HDC. Return NULL (C3X checks and skips) and draw the icons natively, or give a memory DC on our buffer as a command source. Fonts selected (+0xA8) feed the text command. |
| sprite | getters, colour key, set/get colour table (fields) | +0x04 init (copy 8-bit pixels from the source image, once per slice), +0x0C duplicate | all blit slots +0x38..+0xA0 (sprite id, dest image id, x, y, mode args, colour table id) | none |
| colour table | +0x08 | entries at +0x0C, get/set, the 16-bit tables at +0x40C/+0x60C | +0x0C `AnimatePalette`: a palette command if the UI uses it [G] | none |
| font | | +0x18 HFONT | | +0x04/+0x08/+0x10: keep GDI for **measuring** (`CreateFontIndirectA`, `GetTextMetricsA`, `GetTextExtentPoint32A`; gdi32 is not jgl), or answer from `gdi-text` metrics |

Blockers (each needs design work, none needs jgl):

1. The window and pump. Input injection (`Posted<…>` + `PostMessage` in
   `docs/concurrency.md`) needs a real window to post to, and timers arrive
   as `WM_USER+1`.
2. Paint synthesis. A hidden window gets no `WM_PAINT`. Today
   `inproc::paint` fakes it; the replacement pump should call the
   controller's paint when idle with the accumulated dirty rect.
3. The keyed hit test answer (§4 #1).
4. C3X's GDI-on-DC features (§1.3).
5. Bink movies. `_BinkBufferOpen` takes the HWND and draws on it directly.
   They are invisible today too, since the window is hidden [G].

### 5.3 Performance design

- **Today's hooks** each run an inline detour into 32-bit Rust. Under FEX
  that is emulated. Each hook reads game structures (`obj::placement`,
  `obj::image_size`) and updates display lists (`ui/display_list.rs`). It
  costs microseconds per call. `pcx_line` costs about 3 ms per line, because
  every pixel is one event (`ui_trace.rs:1185`).
- **Replacement slot bodies** should stay minimal on the 32-bit side. Read
  the arguments and append a fixed record of 16 to 32 bytes to a
  single-producer ring: opcode, object ids (the 32-bit pointers),
  coordinates, colour, and for text the bytes inline. The ring lives in
  memory the PE side allocates. The native unixlib can read 32-bit memory of
  the same process (`crates/wine-unixlib/src/lib.rs:9`), so the native side
  reads it in place.
- There is no per-call thunk into native code and no per-pixel loop.
  Publication is one release store of the head index. The Mac launcher sets
  `HADRON_HARDWARE_TSO=1` (`docs/macos-tsneo.md:116`), so plain stores
  already have x86 ordering.
- The native consumer runs on its own thread. It wakes once per present,
  either polling or woken by one futex/`WakeByAddressSingle`.
- **Estimate [G]:** 30-60 guest instructions per slot, about 2-4 host
  instructions each under FEX, so **about 50-200 ns per call**. A busy
  interface redraw of about 2,000 operations would then cost 0.1-0.4 ms on
  the game thread. That is about 10⁴× better than today's line path and
  10-100× better than today's ordinary hook. One unix call per frame, if
  one is used, costs microseconds [G] and does not matter.
- **Lifetimes without locks** (`docs/concurrency.md`): object destruction
  is a record. Our allocator frees sprite and image memory only after the
  native side has published `consumed_seq` past that record, so the native
  side may read sprite pixels in place.
- **Ring full:** never block the game thread. Set a "resync" flag; the
  native side then asks for a full invalidate, and the game redraws
  everything.
- **Single producer:** check the thread id in debug builds. Fall back to an
  atomic reservation if a second drawing thread appears.
- **Where the mirror logic goes:** `ui.rs`, `ui/display_list.rs`,
  `ui_screen.rs` and `interface_parts.rs` move to a crate built for the
  native side, which consumes the ring. Only the slot bodies and the
  semantic answers (§4 #1) stay 32-bit.

## 6. Roadmap

Each step ships on its own. They are ordered so that the biggest
performance wins come first. "Verify" lists the evidence a step must show.

| # | Step | Size | Risk | Verify |
|---|---|---|---|---|
| 0 | **In progress (branch `native-tradenet`, another agent):** stop mirroring the world/map canvas (grid, trade net, labels drawn onto MSF canvas). Do not duplicate it. | — | — | — |
| 1 | **Lines as one command.** `pcx_line` emits one `Line` event instead of per-pixel fills (`ui_trace.rs:1185`). The OpenGL/GDI+ line emits one anti-aliased line with width and alpha, not coverage fills (`ui_trace.rs:1500`). The display list rasterises natively. Remaining callers: city screen 0x41A402/0x4211E1, Main_GUI 0x5535F3.., 0x5A80D7, 0x5BA89D, plus whatever step 0 leaves. | S | low | `civ3-pixel-audit.request` counts before/after; the same Mac stall repro; screenshot diff of the city screen and HUD lines |
| 2 | **Command ring plus slot capture, jgl still loaded.** Patch the used slots of the five jgl vtables in place (the technique of `ui_hooks.rs:819-870`). Each slot appends a ring record and, in this step, still calls the original where §4 needs pixels. Move the exe-side detours (`ui_hooks.rs:299-370`) to slot level one family at a time; keep `read_file`/`slice` hooks for file names (cheap, once per load). Also catches C3X and other unaudited writers that bypass the exe wrappers (README "outside that audit"). | M | medium (record format, colour-table and crop semantics per blit slot) | a record-for-record equivalence test against today's `Event` stream on tracer dumps (`civ3-tool`); no jgl slot used with an unhandled opcode (counter); screenshots of every screen family |
| 3 | **Native consumer.** Move the UI model to the native side, reading the ring in place; the 32-bit publisher shrinks to world snapshots. The largest gain under FEX. | L | medium-high (big move; keep the in-process Windows viewer working) | Mac frame-time and game-thread hook-time logs (`inproc::hook_time`); an unchanged acceptance matrix (`docs/handover-2026-10-22/evidence/handover/creative-qa/acceptance-matrix.json`) |
| 4 | **Answer the keyed hit test.** **Partly done (group 4, `docs/jgl-replacement.md` §9): the hit test is answered from the interface model, and every blit is a record only (placeholder sprites). Fills, copies and text still keep jgl's pixels on read-back canvases.** Hook the call at 0x608FE3. Answer from the per-keyed-canvas opacity model (§4 #1). Then make every 16-bit drawing slot unconditionally a no-op plus record; the `Skipper` and read-back keep-sets (`game_render.rs:101-160`, `ui_trace.rs:343-372`) go away. | M | medium (clicks through the HUD to the map) | scripted clicks on transparent and opaque HUD pixels with the answer compared to the original pixel (run with `CIV3R_GAME_RENDER=compare`); `CIV3R_TRACE_PIXELS=1` shows no reads of skipped canvases |
| 5 | **Partly done (jgl group 6, first part, `docs/jgl-replacement.md` §9): the factories and deletes are taken over and every object gets our method tables; the objects, their memory and DCs are still jgl's.** Own pixel objects: factories +0x74/+0x78/+0x7C/+0x80 return our objects with ABI-compatible layouts (§1.2 field list), plain memory, 16-bit canvases allocated lazily on first lock. Real `copy` for the navigator images. Fonts measure through GDI. jgl remains only for the window. | M-L | medium (layout mismatches crash; C3X reads fields) | the game reaches the map, opens every screen, saves and loads; no access to a jgl image from code (assert on our vtables); the minimap stays pixel-exact against `minimap.rs` |
| 6 | **Own graphics object and window.** Our `get_graphsy_object_ptr`: hidden window, window procedure mapped to the controller, pump slots with heartbeat, idle paint synthesis, screen size from `game_size`, display modes as no-ops, pointer position from the viewer. Remove the import hooks on jgl (`inproc::hidden`, `inproc::paint`, `inproc::screen`, `inproc::pointer::hook_cursor_pos`, `heartbeat`). Not loading jgl yet: the factory just stops being jgl's. | M-L | high (input, modal loops, timers, focus) | `docs/modal-input.md` scenarios; wheel, keys, edge scroll, Alt mode; the stale-input tests; a 30-minute autoplay soak; heartbeat in the viewer |
| 7 | **Do not load jgl.dll.** The `LoadLibraryA` hook returns the bridge's module for `jgl.dll`, adding a reference; the bridge exports the factory. Delete the jgl-base code (`ui_hooks.rs:819-870`, `live.rs:2675-2745`, `game_window.rs:117-146`). | S | low after step 6 | `inproc::pe::module_base("jgl.dll")` is `None` all game long; the Wine `+loaddll` log shows no jgl; a Mac run |
| 8 | **Optional: CPU-surface fallback.** The viewer renders a requested canvas offscreen into a CPU buffer for any reader found later (§4 #1 if the model proves insufficient). On demand or at a low rate, only for read canvases. | M | low | the readback audit rows match |
| 9 | **Optional compatibility.** Native mill icons on the city screen (instead of C3X's `TransparentBlt`); GDI+ lines; Bink movies shown in the viewer (`BinkCopyToBuffer` into our buffer); [G] skip the game's own map pass (`Map_Renderer` draw) when the world is ours, if nothing needs its side effects (unit animation ticks must stay: `unit_views.rs`). | M | medium | per item |

Top 5 by value: steps 1, 2, 3, 4, then 6+7 together (no jgl).

The isolated native mill-icon correction retains C3X's actual row queries
and file-art commands while bypassing its legacy mill drawing block in
owned mode. Twelve supported-binary pairs match query/order/return
observations and avoid 41 DC/blit/release calls, with zero owned raster
calls [V]. Original-jgl/viewer drawing and plain-game pass-through remain.
Enabled-mill live game and Neo qualification are still open [G]. See the
[mill contract](jgl-mill-icons.md) for source, art and fixture bindings.

## 7. Open points

- The slot scan is a lower bound. Before step 5, log every slot call at
  run time in step 2 (count per slot, once per session) so that no used
  slot is left unimplemented.
- The exact pixel semantics of JGLSprite blit modes +0x50/+0x58/+0x5C/+0x74/
  +0x84/+0x88/+0x8C/+0x94 are still unknown (`docs/civ3-map-rendering.md`
  §16.7). The mirror already models them at the wrapper level, and the
  replacement keeps that model. Since group 4 jgl draws none of
  them with a viewer: the blits are records and the sprites a placeholder.
- Whether the exe reads graphics-object fields directly, not only through
  slots [G: allocate the full 0x14C bytes and fill +0x12C..+0x148 as jgl
  does].
- Palace selection's `0x57C7xx` readers are identified (§4 #5b). Their
  model/file answer and live verification remain open.
- Whether the frame images at form+0x11C are file data (§4 #2).

### Isolated Group 6 correction candidate, 25 October 2026

[V] The continuation combines checkpoints `339bac3` and standalone Group 7
`5ca2cf6`, with no default switch. It corrects original-binary `+0x38`
forward overlap, image key/error state and scopes C3X GDI+ drawing statuses.
It removes the evictable frame reconstruction and guessed missing-source
answer. All four frame caches now query the viewer's retained model, which
refuses incomplete pixel semantics. Step 5 remains unaccepted until the
remaining frame, transformed-art, reader-family and dynamic data gates in
[the contract matrix](jgl-contract-audit.md) have exact live evidence.

[V] The final-code session reaches a loaded save but fails on a keyed HUD
read at canvas `0x9fdd84`. That observed missing semantic answer blocks
step 5 and further integrated Group 7 qualification. It is preserved in
`runtime/final-own` at the correction evidence root. The all-target workspace
run completed with only the known subpixel coverage failure; affected
packages, generic-crate guard and lock guard passed separately.

### HUD source correction, 26 October 2026

[V] The rejected scratch-DIB font calibration and duplicate glyph generator
are removed. Owned semantic text reads now use the renderer's retained atlas
pixels or packed outline words and its shared integer coverage routine,
including measured placement, underline and keyed blending. Owned C3X line
hooks no longer invoke the legacy GDI+ bind, state setters or draw after
emitting renderer model fills; pass-through keeps those calls. Focused tests
and the Windows-target check pass; fresh combined live qualification remains
open.
