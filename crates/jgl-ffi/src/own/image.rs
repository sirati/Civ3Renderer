//! Our `JGL_Image` (0x4D4 bytes, jgl's layout): plain memory for its
//! pixels, no DIB section, no device context. jgl's behaviour [V, jgl.dll
//! disassembled, RVAs in each body's doc] on our object:
//!
//! - its fields as jgl's constructor and `create` set them (the game, C3X
//!   and the bridge read some directly);
//! - the pixels in memory of ours, rows as jgl's (top-down, 4-byte
//!   aligned), zeroed as a new DIB section is; `pixel_at` and `lock` point
//!   into it, and the exe writes and reads it with its own code (the file
//!   decoders, the minimap, C3X);
//! - no pixel drawing: fills, copies, text and blits are records
//!   (`docs/jgl-replacement.md` §9). On index (8-bit) images, which are
//!   the game's data and never shown, a fill and `replace_color` change
//!   the bytes as jgl's would (the exe reads them back with its own code:
//!   crop trimming, the cursor's hot spots); a sprite drawn onto one has no
//!   pixels, and the game's reads of such images are answered from the
//!   interface model (the bridge);
//! - `acquire_dc` answers no DC (0): nothing the game reads back comes
//!   from drawing on an image's DC (jgl's text and copies, the exe's GL
//!   lines, C3X's GDI+ lines and mill icons are all display, and they test
//!   for a null DC).

use std::cell::RefCell;
use std::collections::HashMap;
use std::mem::offset_of;

use jgl::object::{self as o, Indexed};
use jgl::slots::Class;
use jgl::Rect;

use super::call::{self, get, put};
use super::constant;
use super::stubs;
use crate::layout::{JglImage, Rect32};

const BITS: usize = offset_of!(JglImage, bit_count);
const FORMAT: usize = offset_of!(JglImage, field_28);
const CHANNELS: usize = offset_of!(JglImage, channels);
const WIDTH: usize = offset_of!(JglImage, field_38);
const HEIGHT: usize = offset_of!(JglImage, field_3c);
const ROW: usize = offset_of!(JglImage, row_width);
const CLIP: usize = offset_of!(JglImage, clip);
const IMAGE: usize = offset_of!(JglImage, image_rect);
const STAMP: usize = 0x78;
const TABLE: usize = offset_of!(JglImage, field_7c);
const OWNER: usize = offset_of!(JglImage, owner);
const INFO: usize = offset_of!(JglImage, bitmap_info);
const CURRENT_DC: usize = offset_of!(JglImage, current_dc);
const DC: usize = offset_of!(JglImage, dc);
const PIXELS: usize = offset_of!(JglImage, bits);
const DC_LOCKS: usize = offset_of!(JglImage, dc_locks);
const LOCKS: usize = offset_of!(JglImage, bits_locks);
const CURRENT: usize = offset_of!(JglImage, current_bits);
const KEY: usize = offset_of!(JglImage, color_key);

fn field(this: u32, at: usize) -> u32 {
    // SAFETY: `this` is an image of ours (0x4D4 bytes).
    unsafe { get(this + at as u32) }
}

fn set(this: u32, at: usize, v: u32) {
    // SAFETY: as `field`.
    unsafe { put(this + at as u32, v) }
}

fn set16(this: u32, at: usize, v: u16) {
    // SAFETY: as `field`.
    unsafe { std::ptr::write_unaligned((this + at as u32) as usize as *mut u16, v) }
}

fn rect(this: u32, at: usize) -> Rect {
    // SAFETY: as `field`; a RECT inside the image.
    let r: Rect32 = unsafe { std::ptr::read_unaligned((this + at as u32) as usize as *const Rect32) };
    r.into()
}

fn set_rect(this: u32, at: usize, r: Rect) {
    let r = Rect32 { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    // SAFETY: as `field`.
    unsafe { std::ptr::write_unaligned((this + at as u32) as usize as *mut Rect32, r) }
}

thread_local! {
    /// The pixels of each image of ours (address, bytes), kept until the
    /// image is made again at another size or deleted: jgl's `clear`
    /// leaves +0x4C0 pointing at the pixels (and a `create` of the same
    /// size after it keeps them), so ours stay valid where jgl's would
    /// be unmapped.
    static PIXEL_MEMORY: RefCell<HashMap<u32, (u32, usize)>> = RefCell::new(HashMap::new());
}

fn free_pixels(this: u32) {
    if let Some((at, len)) = PIXEL_MEMORY.with_borrow_mut(|m| m.remove(&this)) {
        if len > 0 {
            // SAFETY: allocated by `alloc_pixels` with this length.
            unsafe { std::alloc::dealloc(at as usize as *mut u8, std::alloc::Layout::from_size_align_unchecked(len, 4)) };
        }
    }
}

/// Zeroed pixels of `len` bytes for `this` (the old ones freed); 0 if
/// none could be had.
fn alloc_pixels(this: u32, len: usize) -> u32 {
    free_pixels(this);
    let len = len.max(4);
    let Ok(layout) = std::alloc::Layout::from_size_align(len, 4) else { return 0 };
    // SAFETY: a non-zero size.
    let p = unsafe { std::alloc::alloc_zeroed(layout) };
    if p.is_null() {
        return 0;
    }
    PIXEL_MEMORY.with_borrow_mut(|m| m.insert(this, (p as usize as u32, len)));
    p as usize as u32
}

/// Lets go of the pixels of ours of `this` (the compare mode's copies).
pub(crate) fn forget(this: u32) {
    free_pixels(this);
}

/// The index image `this`'s bytes, if it is one with pixels.
fn indexed<R>(this: u32, f: impl FnOnce(Indexed<'_>) -> R) -> Option<R> {
    if field(this, BITS) != 8 {
        return None;
    }
    let (at, len) = PIXEL_MEMORY.with_borrow(|m| m.get(&this).copied())?;
    let row = field(this, ROW) as usize;
    // SAFETY: the pixels of ours of this image, `len` bytes, alive while
    // the image is (only the game's thread touches them).
    let bytes = unsafe { std::slice::from_raw_parts_mut(at as usize as *mut u8, len) };
    let used = (row * field(this, HEIGHT) as usize).min(len);
    Some(f(Indexed { bytes: &mut bytes[..used], row }))
}

/// graphics +0x7C `new_image(owner, a2)`'s original (0x3BFF0, constructor
/// 0x15F0): a new image of ours.
pub extern "thiscall" fn new(_graphics: u32, owner: u32, _a2: u32) -> u32 {
    let this = call::alloc::<JglImage>(super::table_of(Class::Image));
    if this == 0 {
        return 0;
    }
    set(this, OWNER, owner);
    // BITMAPINFOHEADER: size, planes 1, 8 bits, 256 colours used.
    set(this, INFO, 0x28);
    set16(this, INFO + 0x0C, 1);
    set16(this, INFO + 0x0E, 8);
    set(this, INFO + 0x20, 0x100);
    set(this, KEY, o::COLOR_KEY);
    super::registry::made(this);
    this
}

/// +0x00 `scalar_delete(flags)` (0x15D0 → 0x1710): the owner lets go of
/// it, it is cleared, and freed with flag 1.
extern "thiscall" fn scalar_delete(this: u32, flags: u32) -> u32 {
    let owner = field(this, OWNER);
    if owner != 0 {
        // SAFETY: the game's `PCX_Image` this image belongs to: its +4
        // points at the image (jgl's destructor clears it).
        unsafe { put(owner + 4, 0) };
    }
    clear(this);
    free_pixels(this);
    super::registry::gone(this);
    if flags & 1 != 0 {
        // SAFETY: an image of ours, from `alloc::<JglImage>`.
        unsafe { call::free::<JglImage>(this) };
    }
    this
}

/// +0x04 `create(width, height, bits, a4)` (0x1800).
extern "thiscall" fn create(this: u32, width: u32, height: u32, bits: u32, _a4: u32) -> u32 {
    let (w, h, b) = (width as i32, height as i32, bits as i32);
    if w < 0 || h < 0 {
        return o::error::ARGUMENT;
    }
    if w == field(this, WIDTH) as i32 && h == field(this, HEIGHT) as i32 && (b == field(this, BITS) as i32 || b == 0) {
        return 0;
    }
    // Through the table, as jgl (the text group records it).
    // SAFETY: our image's +0x08, no arguments.
    unsafe { call::method(this, 0x08, &[]) };
    let depth = match b {
        0 => match super::graphics::object() {
            0 => 8,
            // SAFETY: our graphics object's +0xB0 (its depth), no arguments.
            g => match unsafe { call::method(g, 0xB0, &[]) } {
                0 => 8,
                d => d as i32,
            },
        },
        b => b,
    };
    set(this, BITS, depth as u32);
    set(this, INFO + 4, w as u32);
    set(this, INFO + 8, h.wrapping_neg() as u32);
    set16(this, INFO + 0x0E, depth as u16);
    // +0xA4 is biClrUsed: create clears the constructor's 0x100.
    set(this, INFO + 0x20, 0);
    let Some(row) = o::row_width(w, depth) else {
        return o::error::DEPTH;
    };
    let pixels = alloc_pixels(this, o::image_bytes(row, h, depth));
    if pixels == 0 {
        log::error!("our jgl: no memory for a {w}×{h} image at {depth} bits");
        return o::error::GDI;
    }
    set(this, PIXELS, pixels);
    set(this, ROW, row as u32);
    set(this, WIDTH, w as u32);
    set(this, HEIGHT, h as u32);
    set(this, FORMAT, 0);
    for (i, v) in o::FORMAT_555.iter().enumerate() {
        set16(this, CHANNELS + 2 * i, *v);
    }
    let whole = Rect::new(0, 0, w, h);
    set_rect(this, IMAGE, whole);
    // jgl sets the clip to the whole image through its table.
    let at = this + IMAGE as u32;
    // SAFETY: our image's +0x34 with a pointer to its own rectangle.
    unsafe { call::method(this, 0x34, &[at]) };
    0
}

/// +0x08 `clear` (0x1780): the DC and bitmap go (ours have none); the
/// pixels stay (see `PIXEL_MEMORY`), as do the sizes.
pub extern "thiscall" fn clear(this: u32) -> u32 {
    for at in [DC_LOCKS, LOCKS, CURRENT_DC, DC, 0x4B0, 0x4B4, 0x4AC] {
        set(this, at, 0);
    }
    0
}

/// +0x0C `pixel_at(x, y)` (0x1BC0): a pointer to the pixel (the image is
/// locked), 0 past the right or bottom edge.
extern "thiscall" fn pixel_at(this: u32, x: u32, y: u32) -> u32 {
    let (w, h) = (width(this) as i32, height(this) as i32);
    let Some(offset) = o::pixel_offset(x as i32, y as i32, w, h, field(this, ROW) as i32, field(this, BITS) as i32) else {
        return 0;
    };
    // Through the table, as jgl.
    // SAFETY: our image's +0x10, no arguments.
    let p = unsafe { call::method(this, 0x10, &[]) };
    if p == 0 { 0 } else { p.wrapping_add(offset as u32) }
}

/// +0x10 `lock` (0x1B70): the pixels, counted.
extern "thiscall" fn lock(this: u32) -> u32 {
    let p = field(this, PIXELS);
    set(this, CURRENT, p);
    if p != 0 {
        set(this, LOCKS, field(this, LOCKS).wrapping_add(1));
    }
    p
}

/// +0x14, +0x1C (0x2630): `pixel_at` through the table.
extern "thiscall" fn pixel_at_thunk(this: u32, x: u32, y: u32) -> u32 {
    // SAFETY: our image's +0x0C, two words.
    unsafe { call::method(this, 0x0C, &[x, y]) }
}

/// +0x18, +0x20 (0x2620): `lock` through the table.
extern "thiscall" fn lock_thunk(this: u32) -> u32 {
    // SAFETY: our image's +0x10, no arguments.
    unsafe { call::method(this, 0x10, &[]) }
}

/// +0x24 `unlock(n)` (0x1B90): the count less `n`; at 0 or below, none.
extern "thiscall" fn unlock(this: u32, n: u32) -> u32 {
    let left = (field(this, LOCKS) as i32).wrapping_sub(n as i32);
    set(this, LOCKS, left as u32);
    if left <= 0 {
        set(this, CURRENT, 0);
        set(this, LOCKS, 0);
    }
    left as u32
}

/// +0x28 `acquire_dc` (0x1B20): counted; our images have no DC (0).
extern "thiscall" fn acquire_dc(this: u32) -> u32 {
    set(this, DC_LOCKS, field(this, DC_LOCKS).wrapping_add(1));
    set(this, CURRENT_DC, field(this, DC));
    field(this, DC)
}

/// +0x2C `release_dc(n)` (0x1B40).
extern "thiscall" fn release_dc(this: u32, n: u32) -> u32 {
    let left = (field(this, DC_LOCKS) as i32).wrapping_sub(n as i32);
    set(this, DC_LOCKS, left as u32);
    if left <= 0 {
        set(this, CURRENT_DC, 0);
        set(this, DC_LOCKS, 0);
    }
    left as u32
}

constant!(nothing_1, 0, a);

/// +0x34 `set_clip(rect)` (0x1A40): the clip is the image's part of
/// `rect`; 3 for no rectangle, 1 for an empty clip.
extern "thiscall" fn set_clip(this: u32, r: u32) -> u32 {
    if r == 0 {
        return o::error::ARGUMENT;
    }
    // SAFETY: the game's RECT (jgl reads it unchecked), or the image's own.
    let asked: Rect = unsafe { std::ptr::read_unaligned(r as usize as *const Rect32) }.into();
    match o::intersect(rect(this, IMAGE), asked) {
        Some(c) => {
            set_rect(this, CLIP, c);
            0
        }
        None => {
            set_rect(this, CLIP, Rect::default());
            o::error::GDI
        }
    }
}

/// +0xC4 `reset_clip` (0x2640): `set_clip` to the whole image.
extern "thiscall" fn reset_clip(this: u32) -> u32 {
    set_clip(this, this + IMAGE as u32)
}

/// The copy group's originals (+0x38, +0x40, +0x84, +0x8C): no pixels;
/// jgl's answer for a copy made.
pub extern "thiscall" fn copy_38(_this: u32, _dst: u32, _sx: u32, _sy: u32, _x: u32, _y: u32, _w: u32, _h: u32, _key: u32) -> u32 {
    0
}
pub extern "thiscall" fn copy_40(_this: u32, _dst: u32, _src: u32, _dst_rect: u32) -> u32 {
    0
}
pub extern "thiscall" fn copy_84(_this: u32, _dst: u32, _x: u32, _y: u32) -> u32 {
    0
}
pub extern "thiscall" fn copy_8c(_this: u32, _dst: u32, _x: u32, _y: u32, _a4: u32) -> u32 {
    0
}

/// +0x3C, an opaque copy (0x1FA0): never called (the counting sessions);
/// no pixels, noted.
extern "thiscall" fn copy_3c(this: u32, dst: u32, _sx: u32, _sy: u32, _x: u32, _y: u32, _w: u32, _h: u32) -> u32 {
    log::warn!("our jgl: image +0x3C (an opaque copy) of {this:#x} onto {dst:#x}, never seen before: no pixels");
    0
}

/// The fill group's originals: no pixels, but on an index image a fill
/// (+0x44) and `replace_color` (+0x58) change its bytes as jgl's do.
pub extern "thiscall" fn fill(this: u32, r: u32, color: u32) -> u32 {
    let asked = (r != 0).then(|| {
        // SAFETY: the game's RECT, read unchecked by jgl.
        Rect::from(unsafe { std::ptr::read_unaligned(r as usize as *const Rect32) })
    });
    if let Some(area) = o::clipped(asked, rect(this, CLIP)) {
        indexed(this, |mut img| img.fill(area, color as u8));
    }
    0
}

pub extern "thiscall" fn replace_color(this: u32, r: u32, from: u32, to: u32) -> u32 {
    if r == 0 {
        return o::error::NULL;
    }
    // SAFETY: the game's RECT, read unchecked by jgl.
    let asked = Rect::from(unsafe { std::ptr::read_unaligned(r as usize as *const Rect32) });
    if let Some(area) = o::intersect(asked, rect(this, CLIP)) {
        indexed(this, |mut img| img.replace(area, from as u8, to as u8));
    }
    0
}

constant!(drawn_2, 0, a, b);
constant!(drawn_3, 0, a, b, c);
constant!(drawn_4, 0, a, b, c, d);
constant!(drawn_6, 0, a, b, c, d, e, f);
constant!(drawn_9, 0, a, b, c, d, e, f, g, h, i);

/// +0x74 (load a PNG) and +0x78 (save as PCX): never called; noted.
extern "thiscall" fn load_png(this: u32, _a: u32, _b: u32, _c: u32, _d: u32) -> u32 {
    log::error!("our jgl: image +0x74 (load a PNG) on {this:#x}, never seen before: not done");
    o::error::NOT_IMPLEMENTED
}
extern "thiscall" fn save_pcx(this: u32, _path: u32) -> u32 {
    log::error!("our jgl: image +0x78 (save as PCX) on {this:#x}, never seen before: not done");
    o::error::NOT_IMPLEMENTED
}

/// The text group's originals: no DC, nothing to select or draw; jgl's
/// answer 0.
pub extern "thiscall" fn text_state_1(_this: u32, _a: u32) -> u32 {
    0
}
pub extern "thiscall" fn text_state_0(_this: u32) -> u32 {
    0
}
pub extern "thiscall" fn text_state_3(_this: u32, _a: u32, _b: u32, _c: u32) -> u32 {
    0
}
pub extern "thiscall" fn text_out(_this: u32, _x: u32, _y: u32, _text: u32, _len: u32) -> u32 {
    0
}

/// +0xBC, +0xC0: the colour key.
extern "thiscall" fn set_color_key(this: u32, key: u32) -> u32 {
    set(this, KEY, key);
    0
}
extern "thiscall" fn color_key(this: u32) -> u32 {
    field(this, KEY)
}

/// +0xC8, +0xD0: the clip, the image's rectangle, copied out.
extern "thiscall" fn get_clip(this: u32, out: u32) -> u32 {
    let r = rect(this, CLIP);
    // SAFETY: the game's out RECT.
    unsafe { std::ptr::write_unaligned(out as usize as *mut Rect32, Rect32 { left: r.left, top: r.top, right: r.right, bottom: r.bottom }) };
    out
}
extern "thiscall" fn get_image_rect(this: u32, out: u32) -> u32 {
    let r = rect(this, IMAGE);
    // SAFETY: the game's out RECT.
    unsafe { std::ptr::write_unaligned(out as usize as *mut Rect32, Rect32 { left: r.left, top: r.top, right: r.right, bottom: r.bottom }) };
    out
}

/// +0xCC, +0xD4: where the clip and the image's rectangle are.
extern "thiscall" fn clip_rect(this: u32) -> u32 {
    this + CLIP as u32
}
extern "thiscall" fn image_rect(this: u32) -> u32 {
    this + IMAGE as u32
}

/// +0xD8, +0xDC: the image's width, height (of its rectangle).
extern "thiscall" fn width(this: u32) -> u32 {
    let r = rect(this, IMAGE);
    r.right.wrapping_sub(r.left) as u32
}
extern "thiscall" fn height(this: u32) -> u32 {
    let r = rect(this, IMAGE);
    r.bottom.wrapping_sub(r.top) as u32
}

/// +0xE0 the row width, +0xE4 where the format is (+0x24), +0xE8 the
/// colour table.
extern "thiscall" fn row_width(this: u32) -> u32 {
    field(this, ROW)
}
extern "thiscall" fn format(this: u32) -> u32 {
    this + BITS as u32
}
extern "thiscall" fn color_table(this: u32) -> u32 {
    field(this, TABLE)
}

/// +0xEC `set_color_table(table)` (0x1CA0): kept, with its change stamp
/// (jgl then gives its DIB the table's colours: ours has none).
extern "thiscall" fn set_color_table(this: u32, table: u32) -> u32 {
    if table == 0 {
        return 0;
    }
    set(this, TABLE, table);
    // SAFETY: a colour table (ours): its stamp at +0x810.
    let stamp = unsafe { get(table + 0x810) };
    if field(this, STAMP) != stamp {
        set(this, STAMP, stamp);
    }
    0
}

type M0 = extern "thiscall" fn(u32) -> u32;
type M1 = extern "thiscall" fn(u32, u32) -> u32;
type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;
type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;
type M6 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32) -> u32;
type M7 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32, u32) -> u32;
type M8 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32, u32, u32) -> u32;
type M9 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32, u32, u32, u32) -> u32;

/// Our implementation of image slot `offset` (0: none).
pub fn slot(offset: u16) -> u32 {
    let f: usize = match offset {
        0x00 => scalar_delete as M1 as usize,
        0x04 => create as M4 as usize,
        0x08 => clear as M0 as usize,
        0x0C => pixel_at as M2 as usize,
        0x10 => lock as M0 as usize,
        0x14 | 0x1C => pixel_at_thunk as M2 as usize,
        0x18 | 0x20 => lock_thunk as M0 as usize,
        0x24 => unlock as M1 as usize,
        0x28 => acquire_dc as M0 as usize,
        0x2C => release_dc as M1 as usize,
        0x30 => nothing_1 as M1 as usize,
        0x34 => set_clip as M1 as usize,
        0x38 => copy_38 as M8 as usize,
        0x3C => copy_3c as M7 as usize,
        0x40 => copy_40 as M3 as usize,
        0x44 => fill as M2 as usize,
        0x48 => drawn_3 as M3 as usize,
        0x4C | 0x5C => drawn_2 as M2 as usize,
        0x50 => drawn_3 as M3 as usize,
        0x54 => drawn_4 as M4 as usize,
        0x58 => replace_color as M3 as usize,
        0x60 => drawn_9 as M9 as usize,
        0x64 => drawn_6 as M6 as usize,
        0x74 => load_png as M4 as usize,
        0x78 => save_pcx as M1 as usize,
        0x84 => copy_84 as M3 as usize,
        0x8C => copy_8c as M4 as usize,
        0xA8 | 0xB0 => text_state_1 as M1 as usize,
        0xAC => text_state_0 as M0 as usize,
        0xB4 => text_state_3 as M3 as usize,
        0xB8 => text_out as M4 as usize,
        0xBC => set_color_key as M1 as usize,
        0xC0 => color_key as M0 as usize,
        0xC4 => reset_clip as M0 as usize,
        0xC8 => get_clip as M1 as usize,
        0xCC => clip_rect as M0 as usize,
        0xD0 => get_image_rect as M1 as usize,
        0xD4 => image_rect as M0 as usize,
        0xD8 => width as M0 as usize,
        0xDC => height as M0 as usize,
        0xE0 => row_width as M0 as usize,
        0xE4 => format as M0 as usize,
        0xE8 => color_table as M0 as usize,
        0xEC => set_color_table as M1 as usize,
        _ => {
            return match Class::Image.slot(offset) {
                Some(s) if s.group == jgl::slots::Group::Stub => stubs::by_stack(s.stack),
                _ => 0,
            };
        }
    };
    f as u32
}
