# jgl object semantics

Disassembly-derived behaviour for the image, sprite, colour-table and font
classes, recovered on 25 October 2026. These facts guide the remaining object
ownership and window replacement work in [jgl-replacement.md](jgl-replacement.md).
[V] marks a claim checked in the named binary or C3X source; [G] marks a guess.
These labels preserve the researcher's evidence scope, not independent runtime
acceptance. The Palace selection finding supersedes the roadmap's earlier
navigator attribution. Runtime verification and implementation status are tracked
separately in the replacement plan.

The following research is retained in full, including its unresolved questions.

I've written a slot-by-slot spec for jgl's image, sprite, colour table and font classes below. It's tagged [V] where it comes from the disassembly (jgl.dll, the PCG exe, C3X source) and [G] where it's a guess. Nothing in the repo was changed; the only files written are helper scripts in `/tmp/c3r-g67-re2/`.

## Binary comparison corrections from the later continuation

[V, original jgl tests in `/mnt/datasets/c3r-build/jgl-group6-data-20261025`]
The image constructor's RGB555 key is `0x80007C1F`. Its arithmetic at
RVA `0x16D7..0x16F8` does not produce the earlier `0x7FFF7C1F` value.
Image create compares geometry/depth before checking any DC: after clear,
a same-size create succeeds as a no-op and leaves the DC null. A subsequent
`+0x40` returns 7. `+0x38` overlap walks forwards and observes preceding
writes, rather than snapshotting the source. PCX_Image::get_pixel at
`0x6002C0` returns zero for absent images or out-of-bounds coordinates.

[V, PCGames disassembly retained at that root] Both frame-reader sites
select four caches: `+0x11C`, `+0x120`, `+0x124` and `+0x128`.
`608E7E` handles the bottom and both sides. Their writers in `607170`
include background fills/tiles, frame lines, caption text and virtual
per-form drawing. They cannot be described as sprite-only file masks.

## Problems found first

1. **The Palace screen's click test needs real sprite pixels [V].** The reads at 0x57C706/0x57C7C4/0x57C7FA are not navigator code; docs §4 #5 calls them that, which is wrong. They belong to the Palace View (F9, form vtable 0x66E0F8, art `art\PalaceView\`).
   - 0x57C8A0 rebuilds an 8-bit map at form+0x3154 (screen size): it fills it with 0xFF, then draws each upgradeable palace piece i with `Sprite` 0x5F84A0, which calls sprite +0x74. +0x74 writes the solid index `a4 & 0xFF` wherever the sprite pixel is below 0xFE (body at 0x37770).
   - Mouse move (0x57C7E0) reads `get_pixel(x, y)` to get the piece under the pointer (0xFF means none). Mouse up (0x57C6F0) checks the index again and **changes game state**: the palace record at +0x574 (+0xAC counter, +0x20+4i, the bit mask at +0xA8, +0xA4).
   - With placeholder sprites and blits recorded only, this map stays 0xFF everywhere, so no piece can be picked [G, not run]. Nothing in the repo answers it: no hit for the palace form or these addresses.
2. **`blit.rs` has the colour table argument order wrong [V].** `table_get_colors`/`table_set_colors` (and `TableCall::SetColors`) take `(first, count, colors)`. jgl and the exe use `(colors, first, count)`; the exe call at 0x5FC00C pushes the buffer first.
3. **Wrong entries in `slots.rs` [V]:**
   - Image +0x14/+0x1C (0x2630) are `jmp [vt+0x0C]`, i.e. `pixel_at`, popping 8 bytes in effect. The catalogue calls them unlock with 0 bytes.
   - Image +0x18/+0x20 (0x2620) are `jmp [vt+0x10]`, i.e. `lock`, popping 0.
   - Image +0x24 is not `read_pixel`; it is `unlock_bits(n)`.
   - Image +0x74 loads a PNG file; image +0x78 saves the 8-bit image as `.pcx`.
   - Graphics +0xB8/+0xBC pop 0 bytes, not 8 (they jump to bodies ending in a plain `ret`).
   - Sprite +0x20 returns the pixel pointer (+0x14), not an unknown field.
   - Colour table +0x810/+0x814 are a change stamp, not fade state.

## Shared parts

**Factories [V].** Each one allocates with `operator new` (memory not zeroed [G]), runs the constructor, and returns 0 if the allocation fails.
- `new_font(a1)`: `ret 4`.
- `new_color_table(owner)`: `ret 4`.
- `new_image(owner, a2)`: `ret 8`; a2 is ignored (the exe passes `PCX_Image+8`).
- `new_sprite(owner, a2)`: `ret 8`; a2 is ignored.
- The exe passes:
  - font: `&exe_font+4`;
  - colour table: `PCX_Color_Table*`;
  - image: `PCX_Image*`;
  - sprite: `&Sprite+4`.
- Deletes +0x88..+0x94 (0x3C0C0): if the object is non-null, tail-call `obj->vt[0](1)`; otherwise `ret 4` with eax unchanged.

**Base class and registry [V].**
- The base constructor 0x1310 writes `[this] = 0x1006821C` and calls `registry.add(this, 0)` (0x14B0).
- The registry is a global intrusive list at 0x10070D34:

| Offset | Address | Meaning |
|---|---|---|
| +0 | 0x10070D34 | vtable 0x10068218 |
| +4 | 0x10070D38 | object under the cursor |
| +8 | 0x10070D3C | flag byte |
| +0xC | 0x10070D40 | count |
| +0x10 | 0x10070D44 | head node |
| +0x14 | 0x10070D48 | cursor node |

- Nodes are 0x14 bytes: vtable 0x10068220, +4 next, +8 prev, +0xC object, +0x10 byte. The list is circular. A new node goes right after the head; the cursor and current object become the new one; count++.
- The base destructor 0x1350 restores the base vtable, finds the node whose object is `this`, moves head and cursor to `node.next` (current object = its object), unlinks and frees the node, count--. The last node clears head, cursor and count.
- Registered: image, sprite and colour table register `this`; the font registers its embedded subobject at +8 (its vtable 0x100688C0, whose [0] = 0x3C710 does `ecx -= 8` and then the font's scalar_delete). The temporary stack sprites inside image +0x84/+0x8C also register and unregister.
- At exit, 0x1290 frees the nodes only, not the objects.

**Graphics +0x0C release (0x3B2C0) [V].**
- 0x1410: while count != 0 and the current object is non-null, call `cur->vt[0](1)`. Each destructor unregisters and moves the cursor on, so the order is circular from the cursor.
- Every destructor zeroes the exe owner's pointer: image `[+0x80]+4 = 0`; colour table `[+0x80C]+4 = 0`; sprite `*[+0x0C] = 0`; font `*[+0x04] = 0`.
- Then `[0x10070F30]->vt[0](1)` deletes the graphics object itself (0x10070F30 = 0x10070D30 = what the export returned).
- It does not destroy the window, the memory DC or the palette.

**Error codes [V].** 0 ok, 1 GDI failure or empty clip, 2 graphics +0xB8 DC failure, 3 bad argument, 4 out of memory, 6 file open failed, 7 no DC/bits/object, 0xD font GDI failure, 0x10 null pointer argument, 0x17 unsupported depth, 0x18 not implemented, 0x1C `AddFontResourceA` failed.

**Globals [V].**
- 0x10070F48 = the exe's default `PCX_Color_Table*` (0xCAADD0); jgl reads its +4 (the jgl table) whenever an 8-bit image has no +0x7C.
- 0x10070F4C = 0xCAAD50; it is written and never read.
- Both are set by graphics +0xC0 `create_palette(a1, a2)` (0x3D220), which:
  - builds a 256-entry LOGPALETTE: entries 0..9 and 246..255 are the system colours with flags 0, entries 10..245 have flags PC_RESERVED|PC_NOCOLLAPSE and uninitialised RGB, and entries 8, 9, 246, 247 are then overwritten as PC_EXPLICIT 8, 9, 0xF5, 0xF6;
  - calls `CreatePalette` and stores the HPALETTE in 0x10070F44; on failure it returns 1;
  - calls `a1->vt[0]()` and `a2->vt[0]()` (the exe's `PCX_Color_Table::create` 0x5FBEE0) and returns 0.
- 0x10070F34 = the font memory DC, `CreateCompatibleDC(NULL)` from graphics +0xB8 (0x3C660). The exe calls it once at 0x627C4D. It returns 0, or 2 on failure. +0xBC deletes the DC.

## Image (vtable 0x68238, 0x4D4 bytes)

**Constructor 0x15F0(owner) [V].**
- Base constructor, then zeroes: +0x08, +0x28, +0x38, +0x3C, +0x40, byte +0x74, +0x7C, +0x4AC..+0x4C8, +0x88, +0x8C, +0x94..+0xA0, +0xA8, and +0x54..+0x63 (image RECT).
- Sets +0x80 = owner; +0x84 = 0x28; +0x90 = 1 (u16); +0x92 = 8 (u16); +0xA4 = 0x100.
- `InitializeCriticalSection(+0x0C)`.
- +0x4D0 (colour key) = `graphics->vt+0xB4() == 1 ? 0x8000F81F : 0x7FFF7C1F`; in practice 0x7FFF7C1F.
- Not initialised: +0x04, +0x24, +0x2C..+0x36, +0x44 clip, +0x64..+0x78 (including the **+0x78 stamp**), +0xAC colours, +0x4CC.

**+0x00 scalar_delete(flags) (0x15D0 → 0x1710) [V].** Owner+4 = 0; `DeleteCriticalSection`; `clear()` called directly; vtable 0x10068328; base destructor; free if `flags & 1`; returns this.

**+0x04 create(w, h, bits, a4), `ret 0x10` [V].**
1. If w < 0 or h < 0, return 3.
2. If w == +0x38, h == +0x3C and (bits == +0x24 or bits == 0), return 0 at once: no clear, contents kept. Because `clear` does not reset +0x38/+0x3C, create after clear with the same size also returns 0 without making a DIB.
3. `vt+0x08 clear()`.
4. +0x24 = bits, or if bits is 0, `graphics->vt+0xB0()` (8), falling back to 8 if that returns 0.
5. +0x4BC = `CreateCompatibleDC(NULL)`; on failure return 1.
6. BITMAPINFOHEADER at +0x84: biWidth = w, biHeight = −h (top-down), biBitCount = +0x24, biClrUsed = 0; everything else as the constructor left it (BI_RGB). The colour table at +0xAC is **never written by jgl**.
7. +0x4B4 = `CreateDIBSection(dc, +0x84, DIB_RGB_COLORS, &+0x4C0, NULL, 0)`. On failure: FormatMessage, `MessageBox(0, "Unable to allocate draw-buffer; terminating prog…", "FATAL ERROR", 0x30)`, exit(4). There is no explicit zero fill; the OS zeroes the DIB [G].
8. +0x4B0 = `SelectObject(dc, bitmap)`.
9. +0x40 (row width in pixels): 8 bpp → `(w+3) & ~3`; 16 → `(w+1) & ~1`; 24 → `w + (w & 1)`; 32 → w; any other depth → return 0x17 (the DIB stays).
10. +0x38 = w, +0x3C = h, +0x28 = 0 (pixel format 555); u16 +0x2C = 10, +0x2E = 5, +0x30 = 0, +0x32 = 0x7C00, +0x34 = 0x3E0, +0x36 = 0x1F.
11. `SetBkMode(dc, TRANSPARENT)`, `SetTextAlign(dc, 0x18 = TA_BASELINE)`.
12. +0x54 = {0, 0, w, h}; `vt+0x34(&+0x54)` (its result is ignored); return 0.

The exe's 0x5FC5F0 passes `(w, h, bits, PCX_Image+8)`, where +8 defaults to 1.

**+0x08 clear (0x1780) [V].**
- If DC: lock counts +0x4C4/+0x4C8 = 0; if +0x4B0, `SelectObject(dc, +0x4B0)` and +0x4B0 = 0; `DeleteDC`; +0x4B8 = +0x4BC = 0.
- If bitmap: `DeleteObject`, +0x4B4 = 0. If region: `DeleteObject`, +0x4AC = 0.
- **Not reset:** +0x4C0 bits (left dangling), +0x4CC, +0x24, +0x38/+0x3C/+0x40, +0x54, +0x7C/+0x78. The return value is undefined.

**Pixel access [V]:**
- +0x0C `pixel_at(x, y)`: returns 0 if `x >= width()` or `y >= height()` (signed; **negative x/y are not checked**). Otherwise `p = vt+0x10 lock()`; 0 if p is null; returns `p + bpp/8 * (row * y + x)` for 8/16/24/32 bpp, else 0. Because it locks, the caller is expected to call +0x24(1).
- +0x10 `lock()`: +0x4CC = +0x4C0; if non-null, +0x4C8++; returns +0x4C0.
- +0x14/+0x1C: `pixel_at` thunks. +0x18/+0x20: `lock` thunks.
- +0x24 `unlock(n)`: +0x4C8 −= n; if the result ≤ 0, +0x4CC = +0x4C8 = 0. Returns the decremented value.
- In jgl, the counts and +0x4CC are written and never read.

**DC slots [V]:**
- +0x28 `acquire_dc()`: +0x4C4++, +0x4B8 = +0x4BC, returns +0x4BC (no GdiFlush; jgl never imports it).
- +0x2C `release_dc(n)`: same arithmetic on +0x4C4/+0x4B8; returns the count.
- +0x4B8 is read only by +0xEC.
- +0x30: `ret 4`, does nothing.

**+0x34 set_clip(RECT*) [V].**
1. Null → return 3.
2. +0x44 = `IntersectRect(+0x44, +0x54, *arg)`; empty → return 1.
3. `dc = acquire_dc()`. If dc: delete the old +0x4AC region; if `EqualRect(clip, image)`, `SelectClipRgn(dc, NULL)`; otherwise +0x4AC = `CreateRectRgnIndirect(clip)` (failure → return 1, with the DC lock leaked) and `SelectClipRgn(dc, rgn)`.
4. `release_dc(1)`; return 0.

**Clip, key and getters [V]:**
- +0xC4 `reset_clip` = `set_clip(&+0x54)`.
- +0xC8(out) copies the clip and returns out; +0xCC returns &+0x44.
- +0xD0(out) copies the image RECT; +0xD4 returns &+0x54.
- +0xD8 width = +0x5C − +0x54; +0xDC height = +0x60 − +0x58.
- +0xE0 returns +0x40; +0xE4 returns **&+0x24** (the format struct: bits, +0x28 pixel format, masks); +0xE8 returns +0x7C.
- +0xBC `set_color_key(v)`: +0x4D0 = v, returns 0. +0xC0 returns +0x4D0.

**+0xEC set_color_table(jgl_ct) [V].**
- If ct is null, return (eax undefined).
- +0x7C = ct.
- If +0x78 != ct+0x810 (the stamp): +0x78 = stamp; convert the entries to RGBQUAD (B, G, R, 0) through 0x3D3A0; `SetDIBColorTable(+0x4B8, 0, 256, buf)` between acquire and release.
- The bitmap info at +0xAC is not updated.
- Edge case: after a recreate with the same table and stamp, the new DIB keeps its uninitialised colour table.

**Copies and other drawing (meaning only) [V]:**
- **+0x38 (dst, sx, sy, dx, dy, w, h, key)**: paints `this` under `dst` wherever dst shows the key.
  - `this` must be 8-bit; dst must be 8 or 16 bpp, otherwise 0x17. A null dst crashes (vt+0xE4 is called first).
  - Clipping: negative sx/sy shrink w/h; sx > width → 0; w/h are clipped to `this`; the dst rect is intersected with dst's clip; empty → 0. Null pixel → 3.
  - 8→8: `if d == key, d = s`.
  - 8→16: tbl = (this+0x7C or [0x10070F48]+4) → null gives 0x10; dst+0x28 picks 0 → +0x40C (555) or 1 → +0x60C (565), else return 1; `if d == tbl[key], d = tbl[s]`.
  - Unlocks both images; returns 0.
- **+0x3C (dst, sx, sy, dx, dy, w, h)**: opaque rectangle copy (8→8 and 16→16 by `rep movs`, with direction handling when dst == this; 8→16 through the 16-bit table). this+0x4C0 null → 7, dst null → 0x10, depth → 0x17. Never called in the sessions.
- **+0x40 copy(dst, src_rect, dst_rect)**: `this` is the **source**. `StretchBlt(dst.dc, dst_rect, this.dc, src_rect, SRCCOPY)`. Null argument → 0x10; no DC → 7; returns 0.
- **+0x58 replace_color(rect, from, to)**: rect ∩ clip; every 8-bit byte (or 16-bit word) equal to `from` becomes `to`. Null rect → 0x10; depth → 0x17; null pixel_at → 7; returns 0.
- **+0x74(path, …)**: loads a PNG (libpng 1.0.12, mode "rb"); fopen failure → 6. Never called.
- **+0x78(path)**: 8-bit only (else 0x17); writes the image to `path` with its extension replaced by `.pcx`. Never called.
- **+0x84(dst, x, y) / +0x8C(dst, x, y, a4)**: builds a temporary sprite on the stack (constructor 0x7E80 with owner 0) with:
  - +0x20 = bits, +0x14 = `lock()`, +0x18 = 0, +0x2C = +0x40, +0x30/+0x34 = width/height, +0x10 = +0x7C, +0x28 = +0x4D0;
  - then calls sprite 0x8180 (+0x44) or 0x8860 (+0x6C) with `(dst, x, y[, a4], +0x7C)`;
  - `unlock(1)`, sprite +0x14 = 0, destructor; returns the blit's result.

## Sprite (vtable 0x68440, 0x50 bytes)

**Constructor 0x7E80(owner) [V].** Base constructor; +0x0C = owner; +0x08 and +0x10..+0x24 = 0; +0x28 = 0xFF; +0x2C..+0x34 = 0; `InitializeCriticalSection(+0x38)`.

**+0x00 delete [V].** `*[+0x0C] = 0`; `DeleteCriticalSection`; `clear()`; vtable 0x100684E8; base destructor; free if `flags & 1`.

**+0x08 clear (0x7F40) [V].**
- If +0x14: update the stat globals (0x10070D98 −= w·h; 0x10070D9C if compiled), free it, +0x14 = 0.
- If +0x1C: free, 0.
- +0x2C, +0x30, +0x34, +0x18, +0x24 = 0.
- **Kept:** +0x10 colour table, +0x20 bits, +0x28 key.

**+0x04 init(pixels, w, h, bits, a5, ct), `ret 0x18` [V].**
- bits 8 → 0xAA60, bits 16 → 0x9F50, both called with (pixels, w, h, bits, a5). a5 is not read.
- Other bits → eax = a5: if a5 is non-zero return a5, otherwise `vt+0x10(ct)` and return 0.
- Body:
  1. `vt+8 clear()`; +0x30 = w, +0x34 = h, +0x20 = bits, +0x24 = w·h·bits/8; +0x14 = malloc(+0x24) (failure → 4); 0x10070D98 += w·h.
  2. If pixels is null: +0x2C = w, return 0, then `set_color_table(ct)` (pixels left uninitialised).
  3. If the compile switch [0x10070D94] is set and 4 < w < 256 and w·h < 0x10000: build a span table +0x1C = malloc(4h), one entry per row {u8 first non-key column, u8 run length, u16 offset}, key = +0x28. Pack the runs, +0x18 |= 1, +0x24 = packed size, `realloc`. If it would not save space, free +0x1C and fall back to the plain path.
  4. Plain path: +0x1C = 0, +0x18 &= ~1, memcpy.
  5. +0x2C = w; return 0; then `vt+0x10(ct)`.
- `blit.rs`'s placeholder init matches these fields (flags 0 equals jgl's state after clear; it skips the stats).

**+0x0C duplicate(src) (0x9EB0) [V].**
- `clear()`; copy +0x10, +0x18 (including the compiled bit), +0x34, +0x30, +0x2C, +0x28, +0x20, +0x24.
- +0x14 = malloc(+0x24) (no null check) and memcpy.
- If src+0x1C: +0x1C = malloc(4·h) and copy.
- Returns this.
- The exe calls it from `Sprite::copy` 0x5F8A8E. The placeholder version deliberately drops the compiled bit and spans.

**Getters and setters [V]:**
- +0x10 `set_ct(v)`: +0x10 = v, returns v. +0x14 returns +0x10.
- +0x18 `set_key(v)`: +0x28 = v, returns v. +0x1C returns +0x28.
- +0x20 returns +0x14 (pixels). +0x24: `ret 4`, does nothing.
- +0x28 returns +0x20 (bits); +0x2C returns +0x2C (pitch in pixels = w); +0x30 returns width; +0x34 returns height.

## Colour table (vtable 0x68938, 0x820 bytes)

**Constructor 0x3CEE0(owner) [V].** Base constructor; +0x80C = owner; +0x08, +0x810, +0x814 = 0. The entries at +0x0C are 256 PALETTEENTRY {R, G, B, flags}.

**Destructor [V].** Owner+4 = 0; +0x810 = +0x814 = 0; vtable 0x1006895C; base destructor.

**Slots [V]:**
- **+0x04 init**: `vt+8`; then:
  - entries 0..9 = the RGBQUAD table at 0x100688E8 (system colours), flags 4;
  - entries 246..255 = the table at 0x10068910, flags 4;
  - entries 10..245 = (i, i, i), flags 5;
  - entries 8, 9 = (8, 0, 0) and (9, 0, 0), flags 2 (PC_EXPLICIT); entries 246, 247 = (0xF5, 0, 0) and (0xF6, 0, 0), flags 2;
  - then `restamp()`; returns 0.
- **+0x08**: +0x810 = +0x814 = 0.
- **restamp 0x3CFB0** (not a slot):
  - `QueryPerformanceCounter(&+0x818)` (8 bytes) seeds an LCG (×0x41C64E6D + 0x3039);
  - +0x810 = random value in 1..0xFFFF (redrawn while it is 0);
  - if +0x814 != +0x810: +0x814 = +0x810 and, for each i, +0x40C[i] = `(R>>3)<<10 | (G>>3)<<5 | B>>3` (555) and +0x60C[i] = `(R>>3)<<11 | (G>>2)<<5 | B>>3` (565).
- **+0x0C animate**: if [0x10070F44] is non-null, `AnimatePalette(hpal, 10, 236, &entries[10])` and return its result; else return 0.
- **+0x10 get_colors(rgb_out, first, count)**: writes count triplets R, G, B from the entries; returns 0.
- **+0x14 set_colors(rgb_in, first, count)**: sets R, G, B (flags kept), then `restamp()`; returns 0.
- **+0x18** returns &+0x40C; **+0x1C** returns &+0x60C.
- **+0x20 color(i)**: copies R, G, B of entry i into the static 0x10070F40 and returns 0x10070F40.

## Font (vtable 0x688AC, 0x24 bytes)

**Constructor 0x3C190(p) [V].** p = `&exe_font+4`. Vtable; base constructor on +8 (its vtable becomes 0x100688C0); +0x04 = p; +0x10..+0x20 = 0. +0x0C is never written.

**+0x00 delete [V].** `*p = 0`; `release()`; base destructor on +8; free if `flags & 1`. Never called.

**+0x04 load_file(path, face, height, flags), `ret 0x10` [V].**
1. `vt+0xC release()`.
2. path or face null → 0x10.
3. +0x20 = strdup(path) with the last 4 characters replaced by `.fot` (malloc failure → 4).
4. `CreateScalableFontResourceA(0, +0x20, cwd + "\" + path, NULL)`; on failure with an error other than 0 or 0x50 (file exists), free +0x20 and return 0xD.
5. `AddFontResourceA(+0x20)`; failure → free and return 0x1C.
6. `PostMessageA(HWND_BROADCAST, WM_FONTCHANGE, 0, 0)`; +0x14 |= 1; return `vt+8 create(face, height, flags)`.

**+0x08 create(face, height, flags), `ret 0xC` [V].**
1. Null face → 3.
2. If `+0x14 & 1` is clear: `release()`. If it is set: p[2], p[3], p[5], p[6] = 0, then delete the HFONT.
3. LOGFONTA: lfHeight = −height; width, escapement and orientation 0; lfWeight = `flags & 1 ? 700 : 0`; lfItalic = `flags>>1 & 1`; lfUnderline = `flags>>2 & 1`; lfStrikeOut 0; lfCharSet 0; lfOutPrecision 7 (OUT_TT_ONLY); clip, quality and pitch 0; lfFaceName = strcpy(face) with no length check.
4. +0x18 = `CreateFontIndirectA`; failure → 0xD.
5. `SelectObject(memDC 0x10070F34, hfont)`; `GetTextMetricsA(memDC, &tm)`.
6. Writes (word index into p):
   - p[2] = height + external leading;
   - p[3] = ascent − internal leading;
   - p[4] = internal leading;
   - p[5] = ascent;
   - p[6] = descent.

   In the exe these land at exe_font+0x0C..+0x1C. `layout.rs` describes them slightly wrong.
7. `SelectObject(memDC, GetStockObject(SYSTEM_FONT))`; return 0.

**+0x0C release [V].** If HFONT: `DeleteObject`, +0x18 = 0. If +0x20: `RemoveFontResourceA`, free, +0x20 = 0, `PostMessage(WM_FONTCHANGE)`. The +0x14 flag is kept.

**+0x10 measure(text, len) [V].** Null text → 0. Otherwise select +0x18 into memDC, `GetTextExtentPoint32A(memDC, text, len, &sz)`, restore SYSTEM_FONT, and return `sz.cx` (cy is dropped).

## Exe and C3X users

**Who acquires DCs [V; the scan is a lower bound]:**
- Inside jgl: set_clip, the text slots, +0x40, +0xEC.
- Exe: only `OpenGLRenderer::init` 0x6274B0 (singleton 0xA525F4, 13 callers, mostly form canvas +0x274). It calls acquire_dc and **never releases**, then: ChoosePixelFormat with PFD flags 0x28 (DRAW_TO_BITMAP | SUPPORT_OPENGL) and 16 bits, SetPixelFormat, `wglCreateContext(dc)`, `wglMakeCurrent`. Software GL draws the lines into the DIB.
- C3X:
  - GDI+ lines (`injected_code.c:34479`): `CreateFromHDC(JGL_Image->DC)`, reading +0x4BC directly. The graphics object is kept, so +0x4BC must stay a live DC with the image's own DIB selected [G].
  - Mill icons (`:34703`): acquire_dc on the canvas and on the sheet, `TransparentBlt` with key RGB 0xFF00FF, then release(1) on both.

**Pixel writers [V]:**
- The exe's PCX decoders 0x62BD90/0x62C0B0 use +0x18 lock (and +0x1C/+0x20 in 0x62C0B0), +0xE0, +0xE4 and +0x24(1), writing straight into the DIB with stride = row width.
- C3X city dot (patch at PCG 0x57921B): `pixel_at` via +0x1C, writes 0 at (x+1, y+1) on the minimap image, no unlock.
- C3X combat-odds HUD save/restore: +0x1C per pixel, read and write, no unlock.
- Lock counts drift with these calls; that is harmless, since nothing reads them.

**Pixel readers [V]:**
- `get_pixel` 0x600220: rejects negative or out-of-range coordinates, calls 0x600A10 (pixel_at), then +0x24(1), and returns the byte or word.
- The 0x57C7xx reads are the Palace click test (problem 1 above).

**+0x38 callers [V].**
- Only at 0x5FCE9C, inside `PCX_Image::tile_under` 0x5FCD40 (`this` = canvas; args tile, phase x/y, rect, key; it saves and sets the clip, tiles, restores).
- `tile_under` is called only by the frame painter 0x600520 (14 callers). For each of up to 8 frame pieces it draws the sprite with `Sprite::draw` 0x5F8210, then, if the frame has a tile image (+0x20), tiles it under the piece where the canvas pixel equals the key (+0x24).
- Effect: it fills a frame's key-coloured holes with texture. The result is display, plus the keyed hit test at 0x608FE3 (key pixels let clicks through), which the model already answers. It decides no other game behaviour [G].

**+0x58 callers [V]:**
- `replace_color_all` 0x5FFDB0 on freshly loaded 8-bit art: 0x4C6ABA (0→0xFF, 1→0xFF), 0x54A3A5 and 0x54ADBF (0xFD→0xFF).
- `replace_color` 0x5FFD50: about 20 calls in the text renderer 0x6230xx..0x6236xx, recolouring glyph pixels on a canvas. Display only [G].

**Direct field access [V]:**
- C3X reads only image +0x4BC and &+0x54.
- No exe access to jgl image +0x4xx fields was found (the +0x4BC/+0x4AC hits are an unrelated object at 0xCADBB0).
- Sprite +0x10/+0x18 and colour table +0x0C reads by the exe come from the docs; I did not check them again.

