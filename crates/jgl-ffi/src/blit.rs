//! The sprite group (`docs/jgl-replacement.md` §6, group 4): jgl's sprite
//! slots +0x38..+0xA0 (the blits), the sprite state slots +0x04 `init`,
//! +0x0C `duplicate`, +0x10 `set_color_table`, +0x14 `color_table` and
//! +0x18 `set_key`, graphics +0x9C `set_scale`, and the colour tables'
//! slots +0x04..+0x20.
//!
//! A blit's entry point reads the sprite, the images and the colour table
//! its slot's [`jgl::blit::Shape`] names into a [`jgl::BlitCall`], records
//! it and runs jgl's original only where [`jgl::PixelPolicy`] wants jgl's
//! pixels. A null image or mask where the shape has one goes to the
//! original unrecorded (jgl answers it with an error code). The state slots
//! and the colour tables' record and always run the original: what they
//! set is jgl's until the surfaces are ours.
//!
//! Every entry point is reached through a trampoline that keeps the call's
//! return address ([`caller`]): it says which of the game's functions
//! called (several share a slot, and the scaled ones call an unscaled one
//! after `set_scale`).

#![cfg_attr(not(all(windows, target_arch = "x86")), allow(dead_code, unused_imports))]

use std::sync::atomic::{AtomicU32, Ordering};

use crate::takeover::{Takeover, TakeoverError};

/// The return address of the taken-over call being made: stored by its
/// trampoline, read first by its body (the game's thread alone draws).
static CALLER: AtomicU32 = AtomicU32::new(0);

/// jgl.dll's base, for its globals (the draw scale).
static BASE: AtomicU32 = AtomicU32::new(0);

/// The draw scale's three globals (`set_scale` stores num_x, num_y, den
/// there; the scaled blits read them), relative to jgl's base [V, the
/// binary tests].
pub const SCALE_RVA: u32 = 0x6_C0FC;

/// jgl's span compilation switch (the sprite inits compile only with it
/// set), relative to its base [V, 0xAA60 and 0x9F50 test it].
pub const COMPILE_RVA: u32 = 0x7_0D94;

/// The one picture every sprite's pixels are when jgl does not make them
/// (`jgl::PixelPolicy::original_sprite_pixels`): loud stripes, so that
/// anything still drawing a sprite shows. 16-bit words alternate in runs
/// of 8 between near-magenta 0x7C1E (0x7C1F is the 16-bit colour key) and
/// black; as 8-bit indices, 0x1E/0x7C and 0. One buffer, grown to the
/// largest sprite asked for; the smaller ones stay (sprites point at them).
pub mod placeholder {
    use std::sync::Mutex;

    /// (address, bytes) of each buffer made, the last the largest.
    static BUFFERS: Mutex<Vec<(u32, u32)>> = Mutex::new(Vec::new());

    /// The first buffer is this large (a 1024×1024 16-bit sprite).
    const FIRST: u32 = 2 << 20;

    fn make(bytes: u32) -> u32 {
        let words = bytes.div_ceil(2) as usize + 8;
        let stripes: Vec<u16> = (0..words).map(|i| if (i / 8) % 2 == 0 { 0x7C1E } else { 0 }).collect();
        // For the life of the process: sprites point into it.
        Box::leak(stripes.into_boxed_slice()).as_ptr() as usize as u32
    }

    /// A placeholder of at least `bytes`.
    pub fn at_least(bytes: u32) -> u32 {
        let Ok(mut b) = BUFFERS.lock() else { return 0 };
        match b.last() {
            Some(&(at, n)) if n >= bytes => at,
            _ => {
                let n = bytes.max(FIRST).max(b.last().map_or(0, |l| l.1.saturating_mul(2)));
                let at = make(n);
                b.push((at, n));
                at
            }
        }
    }

    /// Whether `pixels` is a placeholder (not jgl's to free).
    pub fn holds(pixels: u32) -> bool {
        pixels != 0 && BUFFERS.lock().is_ok_and(|b| b.iter().any(|&(at, _)| at == pixels))
    }
}

/// The return address of the call the current entry point serves.
fn caller() -> u32 {
    CALLER.load(Ordering::Relaxed)
}

#[cfg(all(windows, target_arch = "x86"))]
mod entries {
    use super::*;
    use crate::layout::{JglColorTable, JglSprite};
    use crate::state::{guarded, read, record};
    use crate::takeover::original;
    use jgl::blit::{MAX_BLIT_ARGS, Shape};
    use jgl::{BlitCall, Scale, SpriteCall, SpriteId, SpriteRef, TableCall, TableId, TableRef};
    use std::mem::offset_of;

    /// A trampoline `$tramp` for entry point `$body`: keeps the return
    /// address (eax is free at a `thiscall` method's entry) and jumps on,
    /// the stack and ecx as the caller left them.
    macro_rules! trampoline {
        ($tramp:ident => $body:ident) => {
            #[unsafe(naked)]
            pub extern "thiscall" fn $tramp() {
                core::arch::naked_asm!(
                    "mov eax, dword ptr [esp]",
                    "mov dword ptr [{cell}], eax",
                    "jmp {body}",
                    cell = sym CALLER,
                    body = sym $body,
                )
            }
        };
    }

    pub type M0 = extern "thiscall" fn(u32) -> u32;
    pub type M1 = extern "thiscall" fn(u32, u32) -> u32;
    pub type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;
    pub type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
    pub type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;
    pub type M5 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32) -> u32;
    pub type M6 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32) -> u32;
    pub type M7 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32, u32) -> u32;
    pub type M9 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32, u32, u32, u32) -> u32;

    /// A sprite's fields.
    pub fn sprite(at: u32) -> SpriteRef {
        // SAFETY: `this` of a sprite method, or a sprite argument jgl's
        // original calls methods of unchecked when it is not null; the
        // fields are inside its 0x50 bytes.
        let field = |o: usize| unsafe { read::u32_at(at + o as u32) };
        SpriteRef {
            sprite: SpriteId(at),
            owner: field(offset_of!(JglSprite, owner)),
            table: TableId(field(offset_of!(JglSprite, color_table))),
            bits: field(offset_of!(JglSprite, bits)),
            compiled: field(offset_of!(JglSprite, flags)) & 1 != 0,
            placeholder: super::placeholder::holds(field(offset_of!(JglSprite, field_14))),
        }
    }

    /// A colour table argument (0: none).
    pub fn table(at: u32) -> TableRef {
        if at == 0 {
            return TableRef::default();
        }
        // SAFETY: a colour table jgl's original reads unchecked when it is
        // not null; the owner is inside its 0x820 bytes.
        let owner = unsafe { read::u32_at(at + offset_of!(JglColorTable, owner) as u32) };
        TableRef { table: TableId(at), owner }
    }

    fn surface(image: u32) -> Option<jgl::Surface> {
        // SAFETY: an image argument jgl's original calls methods of
        // unchecked when it is not null.
        (image != 0).then(|| unsafe { read::surface(image) })
    }

    /// jgl's draw scale, read from its globals (it was set before the
    /// takeover).
    fn scale_now() -> Scale {
        let at = BASE.load(Ordering::Relaxed) + SCALE_RVA;
        // SAFETY: jgl.dll's .data, three words `set_scale` writes.
        let w = |i: u32| unsafe { read::u32_at(at + 4 * i) };
        Scale { num_x: w(0), num_y: w(1), den: w(2), set_at: 0 }
    }

    /// The call of blit slot `slot` on `this` with `given`, `None` when
    /// an image or mask its shape names is null.
    fn blit_call(slot: u16, this: u32, given: &[u32], caller: u32) -> Option<BlitCall> {
        let mut args = [0; MAX_BLIT_ARGS];
        args[..given.len()].copy_from_slice(given);
        let mut call = BlitCall {
            slot,
            caller,
            sprite: sprite(this),
            target: None,
            other: None,
            mask: None,
            at: (0, 0),
            table: TableRef::default(),
            scale: Scale::default(),
            args,
        };
        let Some(shape) = Shape::of(slot) else { return Some(call) };
        let image = |i: Option<usize>| match i {
            Some(i) => surface(args[i]).map(Some),
            None => Some(None),
        };
        call.target = image(shape.target)?;
        call.other = image(shape.other)?;
        call.mask = match shape.mask {
            Some(i) => Some((args[i] != 0).then(|| sprite(args[i]))?),
            None => None,
        };
        if let Some(i) = shape.at {
            call.at = (args[i] as i32, args[i + 1] as i32);
        }
        if let Some(i) = shape.table {
            call.table = table(args[i]);
        }
        Some(call)
    }

    /// Records blit `slot` and says whether jgl's original runs.
    pub fn blitted(slot: u16, this: u32, given: &[u32]) -> bool {
        let caller = caller();
        guarded("blit", || match blit_call(slot, this, given, caller) {
            Some(call) => record(|p| {
                if p.needs_scale() {
                    p.set_scale(scale_now());
                }
                p.blit(&call)
            })
            .unwrap_or(true),
            None => true,
        }, || true)
    }

    /// A blit slot's entry point and trampoline.
    macro_rules! blit {
        ($tramp:ident, $name:ident, $cell:ident, $slot:expr, $ty:ident, $($a:ident),*) => {
            pub static $cell: AtomicU32 = AtomicU32::new(0);

            pub extern "thiscall" fn $name(this: u32, $($a: u32),*) -> u32 {
                if blitted($slot, this, &[$($a),*]) { original::<$ty>(&$cell)(this, $($a),*) } else { 0 }
            }

            trampoline!($tramp => $name);
        };
    }

    blit!(t_38, slot_38, SLOT_38, 0x38, M2, a, b);
    blit!(t_3c, slot_3c, SLOT_3C, 0x3C, M5, a, b, c, d, e);
    blit!(t_40, slot_40, SLOT_40, 0x40, M5, a, b, c, d, e);
    blit!(t_44, slot_44, SLOT_44, 0x44, M4, a, b, c, d);
    blit!(t_48, slot_48, SLOT_48, 0x48, M6, a, b, c, d, e, f);
    blit!(t_4c, slot_4c, SLOT_4C, 0x4C, M6, a, b, c, d, e, f);
    blit!(t_50, slot_50, SLOT_50, 0x50, M6, a, b, c, d, e, f);
    blit!(t_54, slot_54, SLOT_54, 0x54, M5, a, b, c, d, e);
    blit!(t_58, slot_58, SLOT_58, 0x58, M5, a, b, c, d, e);
    blit!(t_5c, slot_5c, SLOT_5C, 0x5C, M4, a, b, c, d);
    blit!(t_60, slot_60, SLOT_60, 0x60, M6, a, b, c, d, e, f);
    blit!(t_64, slot_64, SLOT_64, 0x64, M6, a, b, c, d, e, f);
    blit!(t_68, slot_68, SLOT_68, 0x68, M6, a, b, c, d, e, f);
    blit!(t_6c, slot_6c, SLOT_6C, 0x6C, M5, a, b, c, d, e);
    blit!(t_70, slot_70, SLOT_70, 0x70, M5, a, b, c, d, e);
    blit!(t_74, slot_74, SLOT_74, 0x74, M5, a, b, c, d, e);
    blit!(t_78, slot_78, SLOT_78, 0x78, M5, a, b, c, d, e);
    blit!(t_7c, slot_7c, SLOT_7C, 0x7C, M5, a, b, c, d, e);
    blit!(t_80, slot_80, SLOT_80, 0x80, M5, a, b, c, d, e);
    blit!(t_84, slot_84, SLOT_84, 0x84, M5, a, b, c, d, e);
    blit!(t_88, slot_88, SLOT_88, 0x88, M9, a, b, c, d, e, f, g, h, i);
    blit!(t_8c, slot_8c, SLOT_8C, 0x8C, M6, a, b, c, d, e, f);
    blit!(t_90, slot_90, SLOT_90, 0x90, M5, a, b, c, d, e);
    blit!(t_94, slot_94, SLOT_94, 0x94, M6, a, b, c, d, e, f);
    blit!(t_98, slot_98, SLOT_98, 0x98, M5, a, b, c, d, e);
    blit!(t_9c, slot_9c, SLOT_9C, 0x9C, M7, a, b, c, d, e, f, g);
    blit!(t_a0, slot_a0, SLOT_A0, 0xA0, M6, a, b, c, d, e, f);

    /// Records a state call (`make`), then runs jgl's original always.
    fn stated(name: &str, make: impl FnOnce() -> Option<SpriteCall>) {
        guarded(name, || {
            if let Some(call) = make() {
                record(|p| p.sprite(&call));
            }
        }, || ());
    }

    pub static INIT: AtomicU32 = AtomicU32::new(0);
    pub static DUPLICATE: AtomicU32 = AtomicU32::new(0);
    pub static SET_TABLE: AtomicU32 = AtomicU32::new(0);
    pub static GET_TABLE: AtomicU32 = AtomicU32::new(0);
    pub static SET_KEY: AtomicU32 = AtomicU32::new(0);
    pub static CLEAR: AtomicU32 = AtomicU32::new(0);
    pub static DELETE: AtomicU32 = AtomicU32::new(0);

    /// The sprite fields `init` and `duplicate` write [V, 0xAA60, 0x9F50,
    /// 0x9EB0].
    const PIXELS: u32 = 0x14;
    const FLAGS: u32 = 0x18;
    const SPANS: u32 = 0x1C;
    const BITS: u32 = 0x20;
    const BYTES: u32 = 0x24;
    const KEY: u32 = 0x28;
    const PITCH: u32 = 0x2C;
    const WIDTH: u32 = 0x30;
    const HEIGHT: u32 = 0x34;

    fn field(sprite: u32, at: u32) -> u32 {
        // SAFETY: a field of the sprite whose method this is (or its
        // source's, which jgl's original reads unchecked), inside 0x50.
        unsafe { read::u32_at(sprite + at) }
    }

    fn set(sprite: u32, at: u32, v: u32) {
        // SAFETY: as `field`; the sprite is jgl's, written by its methods.
        unsafe { read::set_u32(sprite + at, v) }
    }

    /// Whether jgl's span compilation is on (its global the inits test).
    fn compile_on() -> bool {
        // SAFETY: jgl.dll's .data.
        unsafe { read::u32_at(BASE.load(Ordering::Relaxed) + super::COMPILE_RVA) != 0 }
    }

    /// Calls jgl's `clear` through the sprite's own table, as its `init` and
    /// `duplicate` do first (our entry point: the placeholder is let go).
    fn clear_via_table(this: u32) {
        let clear = field(field(this, 0), 0x08) as usize;
        // SAFETY: the sprite's vtable +0x08, `clear(this)`: ours or jgl's.
        let clear: M0 = unsafe { std::mem::transmute::<usize, M0>(clear) };
        clear(this);
    }

    /// Lets go of the placeholder before jgl frees the pixels.
    fn drop_placeholder(this: u32) {
        if super::placeholder::holds(field(this, PIXELS)) {
            set(this, PIXELS, 0);
        }
    }

    /// `init`'s fields, the pixels the placeholder (`docs/jgl-replacement.md`
    /// §9): no pixel is copied, nothing compiled.
    fn placeholder_init(this: u32, w: u32, h: u32, depth: u32, ct: u32) -> u32 {
        clear_via_table(this);
        let bytes = w.saturating_mul(h).saturating_mul(depth / 8);
        for (at, v) in [(WIDTH, w), (HEIGHT, h), (BITS, depth), (BYTES, bytes), (PITCH, w), (SPANS, 0), (FLAGS, 0)] {
            set(this, at, v);
        }
        set(this, PIXELS, super::placeholder::at_least(bytes));
        // As jgl's init: the colour table through the table (+0x10).
        let set_table = field(field(this, 0), 0x10) as usize;
        // SAFETY: the sprite's vtable +0x10, `set_color_table(this, table)`.
        let set_table: M1 = unsafe { std::mem::transmute::<usize, M1>(set_table) };
        set_table(this, ct);
        0
    }

    pub extern "thiscall" fn init(this: u32, pixels: u32, w: u32, h: u32, depth: u32, a5: u32, ct: u32) -> u32 {
        let call = SpriteCall::Init {
            sprite: sprite(this),
            pixels,
            size: (w as i32, h as i32),
            depth,
            a5,
            table: TableId(ct),
            compile: compile_on(),
        };
        let real = guarded("sprite init", || record(|p| {
            p.sprite(&call);
            p.sprite_pixels()
        }).unwrap_or(true), || true);
        let shaped = (depth == 8 || depth == 16) && (w as i32) > 0 && (h as i32) > 0;
        if real || !shaped { original::<M6>(&INIT)(this, pixels, w, h, depth, a5, ct) } else { placeholder_init(this, w, h, depth, ct) }
    }

    pub extern "thiscall" fn duplicate(this: u32, source: u32) -> u32 {
        let real = guarded("sprite duplicate", || {
            if source == 0 {
                return true;
            }
            let call = SpriteCall::Duplicate { sprite: sprite(this), source: sprite(source) };
            record(|p| {
                p.sprite(&call);
                p.sprite_pixels()
            }).unwrap_or(true)
        }, || true);
        if real {
            return original::<M1>(&DUPLICATE)(this, source);
        }
        // jgl's fields copied, the pixels the placeholder.
        clear_via_table(this);
        for at in [0x10, HEIGHT, WIDTH, PITCH, KEY, BITS, BYTES] {
            set(this, at, field(source, at));
        }
        set(this, FLAGS, field(source, FLAGS) & !1);
        set(this, SPANS, 0);
        set(this, PIXELS, super::placeholder::at_least(field(this, BYTES)));
        this
    }

    pub extern "thiscall" fn clear(this: u32) -> u32 {
        stated("sprite clear", || Some(SpriteCall::Clear { sprite: sprite(this) }));
        drop_placeholder(this);
        original::<M0>(&CLEAR)(this)
    }

    /// `scalar_delete(flags)`: its destructor calls jgl's `clear` directly.
    pub extern "thiscall" fn delete(this: u32, flags: u32) -> u32 {
        stated("sprite delete", || Some(SpriteCall::Clear { sprite: sprite(this) }));
        drop_placeholder(this);
        original::<M1>(&DELETE)(this, flags)
    }

    pub extern "thiscall" fn set_table(this: u32, ct: u32) -> u32 {
        stated("sprite set_color_table", || Some(SpriteCall::SetTable { sprite: sprite(this), table: TableId(ct) }));
        original::<M1>(&SET_TABLE)(this, ct)
    }

    /// A getter: ours so that the slot is the group's, jgl answers.
    pub extern "thiscall" fn get_table(this: u32) -> u32 {
        original::<M0>(&GET_TABLE)(this)
    }

    pub extern "thiscall" fn set_key(this: u32, key: u32) -> u32 {
        stated("sprite set_key", || Some(SpriteCall::SetKey { sprite: sprite(this), key }));
        original::<M1>(&SET_KEY)(this, key)
    }

    pub static SET_SCALE: AtomicU32 = AtomicU32::new(0);

    pub extern "thiscall" fn set_scale(this: u32, num_x: u32, num_y: u32, den: u32) -> u32 {
        let set_at = caller();
        guarded("set_scale", || {
            record(|p| p.set_scale(Scale { num_x, num_y, den, set_at }));
        }, || ());
        original::<M3>(&SET_SCALE)(this, num_x, num_y, den)
    }

    trampoline!(t_set_scale => set_scale);

    /// Records a colour table call (`make`), then runs jgl's original.
    fn tabled(name: &str, make: impl FnOnce() -> TableCall) {
        guarded(name, || {
            let call = make();
            record(|p| p.table(&call));
        }, || ());
    }

    pub static TABLE_INIT: AtomicU32 = AtomicU32::new(0);
    pub static TABLE_RESET_FADE: AtomicU32 = AtomicU32::new(0);
    pub static TABLE_ANIMATE: AtomicU32 = AtomicU32::new(0);
    pub static TABLE_GET_COLORS: AtomicU32 = AtomicU32::new(0);
    pub static TABLE_SET_COLORS: AtomicU32 = AtomicU32::new(0);
    pub static TABLE_16A: AtomicU32 = AtomicU32::new(0);
    pub static TABLE_16B: AtomicU32 = AtomicU32::new(0);
    pub static TABLE_20: AtomicU32 = AtomicU32::new(0);

    pub extern "thiscall" fn table_init(this: u32) -> u32 {
        tabled("color_table init", || TableCall::Init { table: table(this) });
        original::<M0>(&TABLE_INIT)(this)
    }

    pub extern "thiscall" fn table_reset_fade(this: u32) -> u32 {
        tabled("color_table reset_fade", || TableCall::ResetFade { table: table(this) });
        original::<M0>(&TABLE_RESET_FADE)(this)
    }

    pub extern "thiscall" fn table_animate(this: u32) -> u32 {
        tabled("color_table animate", || TableCall::Animate { table: table(this) });
        original::<M0>(&TABLE_ANIMATE)(this)
    }

    pub extern "thiscall" fn table_get_colors(this: u32, first: u32, count: u32, colors: u32) -> u32 {
        original::<M3>(&TABLE_GET_COLORS)(this, first, count, colors)
    }

    pub extern "thiscall" fn table_set_colors(this: u32, first: u32, count: u32, colors: u32) -> u32 {
        tabled("color_table set_colors", || TableCall::SetColors { table: table(this), first, count, colors });
        original::<M3>(&TABLE_SET_COLORS)(this, first, count, colors)
    }

    pub extern "thiscall" fn table_16a(this: u32) -> u32 {
        original::<M0>(&TABLE_16A)(this)
    }

    pub extern "thiscall" fn table_16b(this: u32) -> u32 {
        original::<M0>(&TABLE_16B)(this)
    }

    pub extern "thiscall" fn table_20(this: u32, arg: u32) -> u32 {
        tabled("color_table slot_20", || TableCall::Slot20 { table: table(this), arg });
        original::<M1>(&TABLE_20)(this, arg)
    }
}

/// The taken-over sprite group (for the life of the process, in practice).
pub struct BlitGroup {
    _sprite: Takeover,
    _graphics: Takeover,
    _table: Takeover,
}

/// The sprite slots this group takes over: the state slots (+0x00 delete
/// and +0x08 `clear` too: they must not free the placeholder) and every
/// blit slot. The getters +0x1C..+0x34 stay jgl's.
pub const SPRITE_SLOTS: [u16; 34] = [
    0x00, 0x04, 0x08, 0x0C, 0x10, 0x14, 0x18, 0x38, 0x3C, 0x40, 0x44, 0x48, 0x4C, 0x50, 0x54, 0x58, 0x5C, 0x60, 0x64, 0x68, 0x6C, 0x70,
    0x74, 0x78, 0x7C, 0x80, 0x84, 0x88, 0x8C, 0x90, 0x94, 0x98, 0x9C, 0xA0,
];

/// The colour table slots taken over: all but +0x00 (delete).
pub const TABLE_SLOTS: [u16; 8] = [0x04, 0x08, 0x0C, 0x10, 0x14, 0x18, 0x1C, 0x20];

/// Takes over the sprite group of jgl.dll at `base`. Its calls go to the
/// hooks the text group was installed with ([`crate::text::install`]
/// first).
pub fn install(base: u32) -> Result<BlitGroup, TakeoverError> {
    install_impl(base)
}

#[cfg(all(windows, target_arch = "x86"))]
fn install_impl(base: u32) -> Result<BlitGroup, TakeoverError> {
    use crate::takeover::{Entry, take_over};
    use entries::*;
    use jgl::slots::Class;
    if !crate::state::has_hooks() {
        return Err(TakeoverError::NoHooks);
    }
    BASE.store(base, Ordering::Relaxed);
    let f = |p: extern "thiscall" fn()| p as usize as u32;
    let blit = |offset: u16, tramp: extern "thiscall" fn(), original: &'static AtomicU32| Entry { offset, function: f(tramp), original };
    let sprite = [
        Entry { offset: 0x00, function: delete as M1 as usize as u32, original: &DELETE },
        Entry { offset: 0x04, function: init as M6 as usize as u32, original: &INIT },
        Entry { offset: 0x08, function: clear as M0 as usize as u32, original: &CLEAR },
        Entry { offset: 0x0C, function: duplicate as M1 as usize as u32, original: &DUPLICATE },
        Entry { offset: 0x10, function: set_table as M1 as usize as u32, original: &SET_TABLE },
        Entry { offset: 0x14, function: get_table as M0 as usize as u32, original: &GET_TABLE },
        Entry { offset: 0x18, function: set_key as M1 as usize as u32, original: &SET_KEY },
        blit(0x38, t_38, &SLOT_38),
        blit(0x3C, t_3c, &SLOT_3C),
        blit(0x40, t_40, &SLOT_40),
        blit(0x44, t_44, &SLOT_44),
        blit(0x48, t_48, &SLOT_48),
        blit(0x4C, t_4c, &SLOT_4C),
        blit(0x50, t_50, &SLOT_50),
        blit(0x54, t_54, &SLOT_54),
        blit(0x58, t_58, &SLOT_58),
        blit(0x5C, t_5c, &SLOT_5C),
        blit(0x60, t_60, &SLOT_60),
        blit(0x64, t_64, &SLOT_64),
        blit(0x68, t_68, &SLOT_68),
        blit(0x6C, t_6c, &SLOT_6C),
        blit(0x70, t_70, &SLOT_70),
        blit(0x74, t_74, &SLOT_74),
        blit(0x78, t_78, &SLOT_78),
        blit(0x7C, t_7c, &SLOT_7C),
        blit(0x80, t_80, &SLOT_80),
        blit(0x84, t_84, &SLOT_84),
        blit(0x88, t_88, &SLOT_88),
        blit(0x8C, t_8c, &SLOT_8C),
        blit(0x90, t_90, &SLOT_90),
        blit(0x94, t_94, &SLOT_94),
        blit(0x98, t_98, &SLOT_98),
        blit(0x9C, t_9c, &SLOT_9C),
        blit(0xA0, t_a0, &SLOT_A0),
    ];
    debug_assert!(sprite.iter().map(|e| e.offset).eq(SPRITE_SLOTS));
    let table = [
        Entry { offset: 0x04, function: table_init as M0 as usize as u32, original: &TABLE_INIT },
        Entry { offset: 0x08, function: table_reset_fade as M0 as usize as u32, original: &TABLE_RESET_FADE },
        Entry { offset: 0x0C, function: table_animate as M0 as usize as u32, original: &TABLE_ANIMATE },
        Entry { offset: 0x10, function: table_get_colors as M3 as usize as u32, original: &TABLE_GET_COLORS },
        Entry { offset: 0x14, function: table_set_colors as M3 as usize as u32, original: &TABLE_SET_COLORS },
        Entry { offset: 0x18, function: table_16a as M0 as usize as u32, original: &TABLE_16A },
        Entry { offset: 0x1C, function: table_16b as M0 as usize as u32, original: &TABLE_16B },
        Entry { offset: 0x20, function: table_20 as M1 as usize as u32, original: &TABLE_20 },
    ];
    let graphics = [Entry { offset: 0x9C, function: f(t_set_scale), original: &SET_SCALE }];
    // All three or none: the tables are checked first, together.
    crate::takeover::check(base, Class::Sprite, &sprite)?;
    crate::takeover::check(base, Class::ColorTable, &table)?;
    crate::takeover::check(base, Class::Graphics, &graphics)?;
    Ok(BlitGroup {
        _graphics: take_over(base, Class::Graphics, &graphics)?,
        _table: take_over(base, Class::ColorTable, &table)?,
        _sprite: take_over(base, Class::Sprite, &sprite)?,
    })
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn install_impl(_base: u32) -> Result<BlitGroup, TakeoverError> {
    Err(inproc::hook::HookError::Unsupported.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jgl::slots::{Class, Group};

    /// Every blit slot is taken over, and the entry points' argument
    /// counts are jgl's (`ret n`).
    #[test]
    fn the_group_is_every_blit_slot_and_the_state_slots() {
        let blits: Vec<u16> = Class::Sprite.slots().iter().filter(|s| s.offset >= 0x38).map(|s| s.offset).collect();
        assert!(blits.iter().all(|o| SPRITE_SLOTS.contains(o)));
        assert!(SPRITE_SLOTS.iter().all(|&o| Class::Sprite.slot(o).is_some_and(|s| s.group == Group::Blit)));
        let stack = |o: u16| Class::Sprite.slot(o).unwrap().stack;
        assert_eq!([0x00, 0x04, 0x08, 0x0C, 0x10, 0x14, 0x18].map(stack), [4, 24, 0, 4, 4, 0, 4]);
        let table = |o: u16| Class::ColorTable.slot(o).unwrap().stack;
        assert_eq!(TABLE_SLOTS.map(table), [0, 0, 0, 12, 12, 0, 0, 4]);
        assert_eq!(Class::Graphics.slot(0x9C).unwrap().stack, 12);
    }

    /// The `blit!` lines above, as (slot, stack bytes): each slot's count.
    #[test]
    fn blit_entry_arities_are_the_catalogue_s() {
        let entries: [(u16, u8); 27] = [
            (0x38, 8), (0x3C, 20), (0x40, 20), (0x44, 16), (0x48, 24), (0x4C, 24), (0x50, 24), (0x54, 20), (0x58, 20),
            (0x5C, 16), (0x60, 24), (0x64, 24), (0x68, 24), (0x6C, 20), (0x70, 20), (0x74, 20), (0x78, 20), (0x7C, 20),
            (0x80, 20), (0x84, 20), (0x88, 36), (0x8C, 24), (0x90, 20), (0x94, 24), (0x98, 20), (0x9C, 28), (0xA0, 24),
        ];
        for (o, n) in entries {
            assert_eq!(Class::Sprite.slot(o).unwrap().stack, n, "+{o:#x}");
        }
    }
}
