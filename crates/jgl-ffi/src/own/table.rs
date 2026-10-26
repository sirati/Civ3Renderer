//! Our `JGL_Color_Table` (0x820 bytes, jgl's layout): 256 entries (R, G,
//! B, flags) at +0x0C, the 16-bit tables kept with them, a change stamp.
//! jgl's behaviour [V, jgl.dll disassembled, RVAs in each body's doc].

use std::mem::offset_of;
use std::sync::atomic::{AtomicU32, Ordering};

use jgl::slots::Class;

use super::call::{self, get, put};
use super::stubs;
use crate::layout::JglColorTable;

const ENTRIES: usize = offset_of!(JglColorTable, entries);
const TABLE_A: usize = offset_of!(JglColorTable, table16_a);
const TABLE_B: usize = offset_of!(JglColorTable, table16_b);

/// The table as plain data (ours: the game's thread alone uses it).
fn table(this: u32) -> &'static mut JglColorTable {
    // SAFETY: `this` is a colour table of ours (0x820 bytes, `JglColorTable`
    // plain data), only used on the game's thread, one call at a time.
    unsafe { &mut *(this as usize as *mut JglColorTable) }
}

fn entries(this: u32) -> [[u8; 4]; 256] {
    table(this).entries.map(|e| e.to_le_bytes())
}

/// graphics +0x78 `new_color_table(owner)`'s original (0x3BF80,
/// constructor 0x3CEE0).
pub extern "thiscall" fn new(_graphics: u32, owner: u32) -> u32 {
    let this = call::alloc::<JglColorTable>(super::table_of(Class::ColorTable));
    if this == 0 {
        return 0;
    }
    table(this).owner = owner;
    super::registry::made(this);
    this
}

/// +0x00 `scalar_delete(flags)` (0x3CF10): the owner lets go of it.
extern "thiscall" fn scalar_delete(this: u32, flags: u32) -> u32 {
    let owner = table(this).owner;
    if owner != 0 {
        // SAFETY: the game's `PCX_Color_Table`: its +4 points at this table.
        unsafe { put(owner + 4, 0) };
    }
    table(this).fade = [0, 0];
    super::registry::gone(this);
    if flags & 1 != 0 {
        // SAFETY: a table of ours, from `alloc::<JglColorTable>`.
        unsafe { call::free::<JglColorTable>(this) };
    }
    this
}

/// The seed of the stamps (jgl seeds from the performance counter each
/// time; the stamp only has to change).
static SEED: AtomicU32 = AtomicU32::new(0);

/// A new change stamp, and the 16-bit tables again if it differs (0x3CFB0,
/// 0x3D010).
fn restamp(this: u32) {
    let mut counter = 0i64;
    // SAFETY: a plain query into a local.
    unsafe { windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut counter) };
    let t = table(this);
    t.field_818 = [counter as u32, (counter >> 32) as u32];
    let (stamp, seed) = jgl::object::stamp((counter as u32) ^ SEED.load(Ordering::Relaxed));
    SEED.store(seed, Ordering::Relaxed);
    t.fade[0] = stamp;
    if t.fade[1] != t.fade[0] {
        t.fade[1] = t.fade[0];
        let (a, b) = jgl::object::table16(&entries(this));
        let t = table(this);
        t.table16_a = a;
        t.table16_b = b;
    }
}

/// +0x04 `init` (0x3D090): +0x08 through the table, the standard entries,
/// a new stamp; 0.
extern "thiscall" fn init(this: u32) -> u32 {
    // SAFETY: our table's +0x08, no arguments.
    unsafe { call::method(this, 0x08, &[]) };
    table(this).entries = jgl::object::table_init().map(u32::from_le_bytes);
    restamp(this);
    0
}

/// +0x08 (0x3CFA0): the stamps cleared.
extern "thiscall" fn reset_stamp(this: u32) -> u32 {
    table(this).fade = [0, 0];
    0
}

/// +0x0C `animate` (0x3D170): the entries 10..245 into jgl's palette.
extern "thiscall" fn animate(this: u32) -> u32 {
    let palette = super::graphics::palette();
    if palette == 0 {
        return 0;
    }
    // SAFETY: the palette of graphics +0xC0 and 236 entries of this table.
    unsafe { windows_sys::Win32::Graphics::Gdi::AnimatePalette(palette as usize as _, 10, 236, (this + ENTRIES as u32 + 40) as usize as *const _) as u32 }
}

/// +0x10 `get_colors(rgb, first, count)` (0x3D190): (R, G, B) bytes out.
extern "thiscall" fn get_colors(this: u32, rgb: u32, first: u32, count: u32) -> u32 {
    let e = entries(this);
    for k in 0..(count as i32).max(0) as u32 {
        let [r, g, b, _] = e[(first.wrapping_add(k) & 0xFF) as usize];
        for (i, v) in [r, g, b].into_iter().enumerate() {
            // SAFETY: the game's buffer of `count` triplets.
            unsafe { *((rgb + 3 * k + i as u32) as usize as *mut u8) = v };
        }
    }
    0
}

/// +0x14 `set_colors(rgb, first, count)` (0x3D1D0): (R, G, B) bytes in,
/// the flags kept; a new stamp.
extern "thiscall" fn set_colors(this: u32, rgb: u32, first: u32, count: u32) -> u32 {
    let t = table(this);
    for k in 0..(count as i32).max(0) as u32 {
        // SAFETY: the game's buffer of `count` triplets.
        let c: [u8; 3] = std::array::from_fn(|i| unsafe { *((rgb + 3 * k + i as u32) as usize as *const u8) });
        let i = (first.wrapping_add(k) & 0xFF) as usize;
        let [_, _, _, flags] = t.entries[i].to_le_bytes();
        t.entries[i] = u32::from_le_bytes([c[0], c[1], c[2], flags]);
    }
    restamp(this);
    0
}

/// +0x18, +0x1C: where the 16-bit tables are.
extern "thiscall" fn table_555(this: u32) -> u32 {
    this + TABLE_A as u32
}
extern "thiscall" fn table_565(this: u32) -> u32 {
    this + TABLE_B as u32
}

/// jgl's static colour of +0x20.
static COLOR: AtomicU32 = AtomicU32::new(0);

/// +0x20 `color(i)` (0x3D370): entry `i`'s (R, G, B) in a static, its
/// address.
extern "thiscall" fn color(this: u32, i: u32) -> u32 {
    // SAFETY: jgl reads entry `i` unchecked; `i` is a palette index.
    let [r, g, b, _] = unsafe { get(this + ENTRIES as u32 + 4 * i) }.to_le_bytes();
    let at = COLOR.as_ptr() as usize as u32;
    // SAFETY: our static's bytes.
    unsafe { put(at, u32::from_le_bytes([r, g, b, (get(at) >> 24) as u8])) };
    at
}

type M0 = extern "thiscall" fn(u32) -> u32;
type M1 = extern "thiscall" fn(u32, u32) -> u32;
type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;

/// Our implementation of colour table slot `offset` (0: none).
pub fn slot(offset: u16) -> u32 {
    let f: usize = match offset {
        0x00 => scalar_delete as M1 as usize,
        0x04 => init as M0 as usize,
        0x08 => reset_stamp as M0 as usize,
        0x0C => animate as M0 as usize,
        0x10 => get_colors as M3 as usize,
        0x14 => set_colors as M3 as usize,
        0x18 => table_555 as M0 as usize,
        0x1C => table_565 as M0 as usize,
        0x20 => color as M1 as usize,
        _ => {
            return match Class::ColorTable.slot(offset) {
                Some(s) if s.group == jgl::slots::Group::Stub => stubs::by_stack(s.stack),
                _ => 0,
            };
        }
    };
    f as u32
}
