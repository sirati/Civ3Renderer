//! Our `JGLSprite` (0x50 bytes, jgl's layout). Its pixels are never
//! jgl's: `init` and `duplicate` point them at the placeholder
//! (`crate::blit::placeholder`, the sprite group's entry points do that),
//! and the blits are records only. jgl's behaviour [V, jgl.dll
//! disassembled] for the rest.

use std::mem::offset_of;

use jgl::slots::Class;

use super::call::{self, get, put};
use super::constant;
use super::stubs;
use crate::layout::JglSprite;

const OWNER: usize = offset_of!(JglSprite, owner);
const TABLE: usize = offset_of!(JglSprite, color_table);
const PIXELS: usize = offset_of!(JglSprite, field_14);
const FLAGS: usize = offset_of!(JglSprite, flags);
const SPANS: usize = offset_of!(JglSprite, field_1c);
const BITS: usize = offset_of!(JglSprite, bits);
const BYTES: usize = offset_of!(JglSprite, field_24);
const KEY: usize = offset_of!(JglSprite, key);
const PITCH: usize = offset_of!(JglSprite, field_2c);
const WIDTH: usize = offset_of!(JglSprite, field_30);
const HEIGHT: usize = offset_of!(JglSprite, field_34);

fn field(this: u32, at: usize) -> u32 {
    // SAFETY: `this` is a sprite of ours (0x50 bytes).
    unsafe { get(this + at as u32) }
}

fn set(this: u32, at: usize, v: u32) {
    // SAFETY: as `field`.
    unsafe { put(this + at as u32, v) }
}

/// graphics +0x80 `new_sprite(owner, a2)`'s original (0x3C060,
/// constructor 0x7E80).
pub extern "thiscall" fn new(_graphics: u32, owner: u32, _a2: u32) -> u32 {
    let this = call::alloc::<JglSprite>(super::table_of(Class::Sprite));
    if this == 0 {
        return 0;
    }
    set(this, OWNER, owner);
    set(this, KEY, 0xFF);
    super::registry::made(this);
    this
}

/// +0x00 `scalar_delete(flags)`'s original (0x7E60): the owner lets go
/// of it (`*owner = 0`), it is cleared, freed with flag 1.
pub extern "thiscall" fn scalar_delete(this: u32, flags: u32) -> u32 {
    let owner = field(this, OWNER);
    if owner != 0 {
        // SAFETY: the field of the game's `Sprite` this sprite is kept in.
        unsafe { put(owner, 0) };
    }
    clear(this);
    super::registry::gone(this);
    if flags & 1 != 0 {
        // SAFETY: a sprite of ours, from `alloc::<JglSprite>`.
        unsafe { call::free::<JglSprite>(this) };
    }
    this
}

/// +0x08 `clear`'s original (0x7F40): the pixels (the placeholder, not
/// ours to free) and spans let go; the colour table, depth and key kept.
pub extern "thiscall" fn clear(this: u32) -> u32 {
    for at in [PIXELS, SPANS, PITCH, WIDTH, HEIGHT, FLAGS, BYTES] {
        set(this, at, 0);
    }
    0
}

/// +0x04 `init(pixels, w, h, bits, a5, table)`'s original (0x7FC0), for
/// what the sprite group does not answer with the placeholder: a depth
/// other than 8 and 16 (jgl answers `a5` if it is not 0, else sets the
/// colour table and answers 0), and an empty size (no pixels).
pub extern "thiscall" fn init(this: u32, _pixels: u32, w: u32, h: u32, bits: u32, a5: u32, table: u32) -> u32 {
    if bits == 8 || bits == 16 {
        // SAFETY: our sprite's +0x08, no arguments (as jgl's init).
        unsafe { call::method(this, 0x08, &[]) };
        set(this, WIDTH, w);
        set(this, HEIGHT, h);
        set(this, BITS, bits);
        set(this, BYTES, w.wrapping_mul(h).wrapping_mul(bits / 8));
        set(this, PITCH, w);
    } else if a5 != 0 {
        return a5;
    }
    // SAFETY: our sprite's +0x10, one word.
    unsafe { call::method(this, 0x10, &[table]) };
    0
}

/// +0x0C `duplicate(source)`'s original (0x9EB0), for a null source: the
/// sprite cleared (jgl would read through the null pointer).
pub extern "thiscall" fn duplicate(this: u32, _source: u32) -> u32 {
    clear(this);
    this
}

/// +0x10, +0x14: the colour table.
pub extern "thiscall" fn set_color_table(this: u32, table: u32) -> u32 {
    set(this, TABLE, table);
    table
}
pub extern "thiscall" fn color_table(this: u32) -> u32 {
    field(this, TABLE)
}

/// +0x18, +0x1C: the key.
pub extern "thiscall" fn set_key(this: u32, key: u32) -> u32 {
    set(this, KEY, key);
    key
}
extern "thiscall" fn key(this: u32) -> u32 {
    field(this, KEY)
}

/// +0x20 the pixels, +0x28 the depth, +0x2C the pitch, +0x30, +0x34 the
/// size.
extern "thiscall" fn pixels(this: u32) -> u32 {
    field(this, PIXELS)
}
extern "thiscall" fn bits(this: u32) -> u32 {
    field(this, BITS)
}
extern "thiscall" fn pitch(this: u32) -> u32 {
    field(this, PITCH)
}
extern "thiscall" fn width(this: u32) -> u32 {
    field(this, WIDTH)
}
extern "thiscall" fn height(this: u32) -> u32 {
    field(this, HEIGHT)
}

constant!(nothing_1, 0, a);

/// The blits' originals: no pixels; jgl's answer for a blit made.
macro_rules! blits {
    ($($name:ident: $($a:ident),*;)*) => {
        $(constant!($name, 0, $($a),*);)*
    };
}

blits! {
    blit_2: a, b;
    blit_4: a, b, c, d;
    blit_5: a, b, c, d, e;
    blit_6: a, b, c, d, e, f;
    blit_7: a, b, c, d, e, f, g;
    blit_9: a, b, c, d, e, f, g, h, i;
}

type M0 = extern "thiscall" fn(u32) -> u32;
type M1 = extern "thiscall" fn(u32, u32) -> u32;
type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;
type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;
type M5 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32) -> u32;
type M6 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32) -> u32;
type M7 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32, u32) -> u32;
type M9 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32, u32, u32, u32) -> u32;

/// Our implementation of sprite slot `offset` (0: none).
pub fn slot(offset: u16) -> u32 {
    let f: usize = match offset {
        0x00 => scalar_delete as M1 as usize,
        0x04 => init as M6 as usize,
        0x08 => clear as M0 as usize,
        0x0C => duplicate as M1 as usize,
        0x10 => set_color_table as M1 as usize,
        0x14 => color_table as M0 as usize,
        0x18 => set_key as M1 as usize,
        0x1C => key as M0 as usize,
        0x20 => pixels as M0 as usize,
        0x24 => nothing_1 as M1 as usize,
        0x28 => bits as M0 as usize,
        0x2C => pitch as M0 as usize,
        0x30 => width as M0 as usize,
        0x34 => height as M0 as usize,
        o if o >= 0x38 => match Class::Sprite.slot(o).map(|s| s.stack / 4) {
            Some(2) => blit_2 as M2 as usize,
            Some(4) => blit_4 as M4 as usize,
            Some(5) => blit_5 as M5 as usize,
            Some(6) => blit_6 as M6 as usize,
            Some(7) => blit_7 as M7 as usize,
            Some(9) => blit_9 as M9 as usize,
            _ => 0,
        },
        _ => {
            return match Class::Sprite.slot(offset) {
                Some(s) if s.group == jgl::slots::Group::Stub => stubs::by_stack(s.stack),
                _ => 0,
            };
        }
    };
    f as u32
}
