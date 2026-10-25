//! TEMPORARY compare mode (`CIV3R_JGL_COMPARE=1`, removed once our jgl
//! is in place): with jgl.dll loaded and its objects the game's, every
//! call of the slots below runs jgl's (the game gets jgl's answer) and
//! then ours ([`crate::own`]) on a copy of the object made before the
//! call, and the two are compared: the answer and the fields jgl sets.
//! The copy's method table is ours alone (no group entry points), so our
//! code never touches jgl's handles. Mismatches are counted per slot and
//! the first ones logged ([`report`]).
//!
//! Not compared: what has no answer to compare (drawing, which is records
//! and no pixels in ours; the window and pumps, which cannot run twice),
//! pointers to jgl's own memory (DCs, bitmaps, regions, which ours do
//! not have) and the colour tables' random change stamps.

#![cfg_attr(not(all(windows, target_arch = "x86")), allow(dead_code, unused_imports))]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;

use jgl::slots::Class;

#[derive(Default)]
struct Tally {
    calls: u64,
    differ: u64,
    samples: Vec<String>,
}

thread_local! {
    static TALLY: RefCell<BTreeMap<(u8, u16), Tally>> = const { RefCell::new(BTreeMap::new()) };
}

/// The report lines so far, published by the game's thread for the
/// reporter (latest wins).
static REPORT: OnceLock<handoff::Latest<String>> = OnceLock::new();

fn note(class: Class, slot: u16, mismatch: Option<String>) {
    TALLY.with_borrow_mut(|t| {
        let e = t.entry((class.index() as u8, slot)).or_default();
        e.calls += 1;
        if let Some(m) = mismatch {
            e.differ += 1;
            if e.samples.len() < 8 {
                log::warn!("jgl compare: {}+{slot:#x}: {m}", class.name());
                e.samples.push(m);
            }
        }
        if e.calls.is_power_of_two() || e.calls % 50_000 == 0 {
            publish(t);
        }
    });
}

fn publish(t: &BTreeMap<(u8, u16), Tally>) {
    let mut s = String::new();
    for (&(c, slot), e) in t {
        let class = Class::from_index(c as u32).unwrap_or(Class::Graphics);
        let name = class.slot(slot).map_or("?", |s| s.name);
        s.push_str(&format!("{}+{slot:#05x} {name}: {} calls, {} differ\n", class.name(), e.calls, e.differ));
    }
    REPORT.get_or_init(handoff::Latest::new).publish(s);
}

/// The counts so far: per slot, calls and mismatches.
pub fn report() -> Option<String> {
    REPORT.get().and_then(|r| r.get()).map(|s| String::clone(&s))
}

#[cfg(all(windows, target_arch = "x86"))]
mod imp {
    use super::*;
    use crate::own::call;
    use std::mem::size_of;

    /// Each compared slot's previous function (a group's entry point or
    /// jgl's), by (class, offset).
    pub static PREV: [[AtomicU32; 64]; 5] = [const { [const { AtomicU32::new(0) }; 64] }; 5];

    pub fn prev(class: Class, slot: u16) -> u32 {
        PREV[class.index() as usize][slot as usize / 4].load(Ordering::Acquire)
    }

    /// A copy of `object` (`size` bytes) with our method table alone.
    pub struct Scratch {
        pub at: u32,
        pub bytes: Vec<u32>,
    }

    impl Scratch {
        pub fn of(object: u32, class: Class) -> Scratch {
            let words = class.object_size() as usize / 4;
            // SAFETY: `object` is an object of `class` (the `this` of its
            // method), `size` bytes.
            let mut bytes: Vec<u32> = (0..words).map(|i| unsafe { call::get(object + 4 * i as u32) }).collect();
            bytes[0] = crate::own::pure_table(class);
            let at = bytes.as_mut_ptr() as usize as u32;
            Scratch { at, bytes }
        }

        pub fn word(&self, offset: usize) -> u32 {
            self.bytes[offset / 4]
        }
    }

    pub fn word(object: u32, offset: usize) -> u32 {
        // SAFETY: a field of the object the compared method was called on.
        unsafe { call::get(object + offset as u32) }
    }

    /// The words of `object` and `scratch` at `offsets` that differ.
    pub fn fields(object: u32, scratch: &Scratch, offsets: impl IntoIterator<Item = usize>) -> Option<String> {
        let d: Vec<String> = offsets
            .into_iter()
            .filter(|&o| word(object, o) != scratch.word(o))
            .map(|o| format!("+{o:#x} jgl {:#x} ours {:#x}", word(object, o), scratch.word(o)))
            .collect();
        (!d.is_empty()).then(|| d.join(", "))
    }

    pub fn both(a: Option<String>, b: Option<String>) -> Option<String> {
        match (a, b) {
            (None, None) => None,
            (a, b) => Some([a, b].into_iter().flatten().collect::<Vec<_>>().join("; ")),
        }
    }

    pub fn answers(jgl: u32, ours: u32) -> Option<String> {
        (jgl != ours).then(|| format!("answer jgl {jgl:#x} ours {ours:#x}"))
    }

    /// Image fields jgl's methods set (no handles, no pixel pointers).
    pub fn image_fields() -> Vec<usize> {
        let mut v: Vec<usize> = vec![0x24, 0x28, 0x2C, 0x30, 0x34, 0x38, 0x3C, 0x40];
        v.extend((0x44..0x64).step_by(4));
        v.extend([0x78, 0x7C, 0x80]);
        v.extend((0x84..0xAC).step_by(4));
        v.extend([0x4C4, 0x4C8, 0x4D0]);
        v
    }

    fn rows8(image: u32) -> Option<(u32, usize, usize)> {
        (word(image, 0x24) == 8 && word(image, 0x4C0) != 0).then(|| (word(image, 0x4C0), word(image, 0x40) as usize, word(image, 0x3C) as usize))
    }

    fn bytes_at(at: u32, len: usize) -> Vec<u8> {
        // SAFETY: jgl's DIB section of the image, `len` bytes.
        unsafe { std::slice::from_raw_parts(at as usize as *const u8, len) }.to_vec()
    }

    macro_rules! compared {
        ($name:ident, $class:expr, $slot:expr, ($($a:ident),*), |$this:ident, $r:ident| $check:expr) => {
            pub extern "thiscall" fn $name($this: u32 $(, $a: u32)*) -> u32 {
                let class: Class = $class;
                let prev = prev(class, $slot);
                let scratch = Scratch::of($this, class);
                let args = [$($a),*];
                // SAFETY: the slot's previous function, with its arguments.
                let $r = unsafe { call::function(prev, $this, &args) };
                let ours_fn = crate::own::implementation(class, $slot);
                // SAFETY: ours, on the copy.
                let ours = unsafe { call::function(ours_fn, scratch.at, &args) };
                let check: Option<String> = { let s = &scratch; let o = ours; $check(s, o) };
                note(class, $slot, check);
                $r
            }
        };
    }

    // Image getters and state: answer and fields.
    macro_rules! image_simple {
        ($name:ident, $slot:expr, ($($a:ident),*)) => {
            compared!($name, Class::Image, $slot, ($($a),*), |this, r| |s: &Scratch, o: u32| both(answers(r, o), fields(this, s, image_fields())));
        };
    }

    image_simple!(i_lock, 0x10, ());
    image_simple!(i_unlock, 0x24, (n));
    image_simple!(i_set_clip, 0x34, (rect));
    image_simple!(i_set_key, 0xBC, (key));
    image_simple!(i_key, 0xC0, ());
    image_simple!(i_reset_clip, 0xC4, ());
    image_simple!(i_width, 0xD8, ());
    image_simple!(i_height, 0xDC, ());
    image_simple!(i_row, 0xE0, ());
    image_simple!(i_table, 0xE8, ());
    image_simple!(i_set_table, 0xEC, (t));

    // pixel_at and its thunks: the same pointer (the copy's bits are
    // jgl's), the same lock counts.
    compared!(i_pixel_at, Class::Image, 0x0C, (x, y), |this, r| |s: &Scratch, o: u32| both(answers(r, o), fields(this, s, image_fields())));
    compared!(i_pixel_at_1c, Class::Image, 0x1C, (x, y), |this, r| |s: &Scratch, o: u32| both(answers(r, o), fields(this, s, image_fields())));
    compared!(i_lock_18, Class::Image, 0x18, (), |this, r| |s: &Scratch, o: u32| both(answers(r, o), fields(this, s, image_fields())));
    // The DC slots: ours has no DC (by design); the counts.
    compared!(i_acquire_dc, Class::Image, 0x28, (), |this, _r| |s: &Scratch, _o: u32| fields(this, s, [0x4C4]));
    compared!(i_release_dc, Class::Image, 0x2C, (n), |this, r| |s: &Scratch, o: u32| both(answers(r, o), fields(this, s, [0x4C4])));
    // Pointers into the object: the same offset from it.
    compared!(i_clip_rect, Class::Image, 0xCC, (), |this, r| |s: &Scratch, o: u32| answers(r.wrapping_sub(this), o.wrapping_sub(s.at)));
    compared!(i_image_rect, Class::Image, 0xD4, (), |this, r| |s: &Scratch, o: u32| answers(r.wrapping_sub(this), o.wrapping_sub(s.at)));
    compared!(i_format, Class::Image, 0xE4, (), |this, r| |s: &Scratch, o: u32| answers(r.wrapping_sub(this), o.wrapping_sub(s.at)));

    /// create: the answer and every field but handles and pixels.
    pub extern "thiscall" fn i_create(this: u32, w: u32, h: u32, bits: u32, a4: u32) -> u32 {
        let scratch = Scratch::of(this, Class::Image);
        // SAFETY: the slot's previous function.
        let r = unsafe { call::function(prev(Class::Image, 0x04), this, &[w, h, bits, a4]) };
        // SAFETY: ours, on the copy.
        let o = unsafe { call::function(crate::own::implementation(Class::Image, 0x04), scratch.at, &[w, h, bits, a4]) };
        crate::own::image::forget(scratch.at);
        note(Class::Image, 0x04, both(answers(r, o), fields(this, &scratch, image_fields())));
        r
    }

    /// The fills and `replace_color` on index images: the bytes jgl
    /// leaves against ours on a copy of the bytes before.
    fn indexed(slot: u16, this: u32, args: &[u32]) -> u32 {
        let before = rows8(this).map(|(at, row, h)| bytes_at(at, row * h));
        let clip = jgl::Rect::new(word(this, 0x44) as i32, word(this, 0x48) as i32, word(this, 0x4C) as i32, word(this, 0x50) as i32);
        // SAFETY: the slot's previous function.
        let r = unsafe { call::function(prev(Class::Image, slot), this, args) };
        let (Some(mut ours), Some((at, row, h))) = (before, rows8(this)) else { return r };
        let rect = (args[0] != 0).then(|| {
            // SAFETY: the game's RECT.
            let w: [i32; 4] = std::array::from_fn(|i| unsafe { call::get(args[0] + 4 * i as u32) } as i32);
            jgl::Rect::new(w[0], w[1], w[2], w[3])
        });
        let mut img = jgl::object::Indexed { bytes: &mut ours, row };
        if slot == 0x44 {
            if let Some(a) = jgl::object::clipped(rect, clip) {
                img.fill(a, args[1] as u8);
            }
        } else if let Some(a) = rect.and_then(|r| jgl::object::intersect(r, clip)) {
            img.replace(a, args[1] as u8, args[2] as u8);
        }
        let now = bytes_at(at, row * h);
        let differ = now.iter().zip(&ours).filter(|(a, b)| a != b).count();
        note(Class::Image, slot, (differ > 0).then(|| format!("{differ} bytes differ (rect {rect:?}, clip {clip:?}, args {args:x?})")));
        r
    }

    pub extern "thiscall" fn i_fill(this: u32, r: u32, color: u32) -> u32 {
        indexed(0x44, this, &[r, color])
    }

    pub extern "thiscall" fn i_replace(this: u32, r: u32, from: u32, to: u32) -> u32 {
        indexed(0x58, this, &[r, from, to])
    }

    // Sprites: the state slots.
    macro_rules! sprite_simple {
        ($name:ident, $slot:expr, ($($a:ident),*)) => {
            compared!($name, Class::Sprite, $slot, ($($a),*), |this, r| |s: &Scratch, o: u32| both(answers(r, o), fields(this, s, [0x10, 0x18, 0x20, 0x24, 0x28, 0x2C, 0x30, 0x34])));
        };
    }
    sprite_simple!(s_get_table, 0x14, ());
    sprite_simple!(s_key, 0x1C, ());
    sprite_simple!(s_bits, 0x28, ());
    sprite_simple!(s_pitch, 0x2C, ());
    sprite_simple!(s_width, 0x30, ());
    sprite_simple!(s_height, 0x34, ());

    // Colour tables: entries and 16-bit tables (not the stamps).
    fn table_fields() -> Vec<usize> {
        (0x0C..0x80C).step_by(4).collect()
    }
    compared!(t_init, Class::ColorTable, 0x04, (), |this, r| |s: &Scratch, o: u32| both(answers(r, o), fields(this, s, table_fields())));
    compared!(t_set_colors, Class::ColorTable, 0x14, (rgb, first, count), |this, r| |s: &Scratch, o: u32| both(answers(r, o), fields(this, s, table_fields())));
    compared!(t_555, Class::ColorTable, 0x18, (), |this, r| |s: &Scratch, o: u32| answers(r.wrapping_sub(this), o.wrapping_sub(s.at)));
    compared!(t_565, Class::ColorTable, 0x1C, (), |this, r| |s: &Scratch, o: u32| answers(r.wrapping_sub(this), o.wrapping_sub(s.at)));

    /// get_colors: the bytes written, against ours into a buffer of ours.
    pub extern "thiscall" fn t_get_colors(this: u32, rgb: u32, first: u32, count: u32) -> u32 {
        let scratch = Scratch::of(this, Class::ColorTable);
        // SAFETY: the slot's previous function.
        let r = unsafe { call::function(prev(Class::ColorTable, 0x10), this, &[rgb, first, count]) };
        let n = 3 * (count as i32).clamp(0, 256) as usize;
        let mut mine = vec![0u8; n.max(1)];
        let at = mine.as_mut_ptr() as usize as u32;
        // SAFETY: ours, on the copy, into our buffer.
        let o = unsafe { call::function(crate::own::implementation(Class::ColorTable, 0x10), scratch.at, &[at, first, count]) };
        let jgls = bytes_at(rgb, n);
        let differ = jgls.iter().zip(&mine).filter(|(a, b)| a != b).count();
        note(Class::ColorTable, 0x10, both(answers(r, o), (differ > 0).then(|| format!("{differ} bytes differ"))));
        r
    }

    // Fonts.
    /// create: jgl's metrics against ours written into a copy of the
    /// game's font's words, and the two GDI fonts' LOGFONTs.
    pub extern "thiscall" fn f_create(this: u32, face: u32, height: u32, flags: u32) -> u32 {
        let metrics = word(this, 4);
        let mut words: Vec<u32> = (0..8).map(|i| word(metrics, 4 * i)).collect();
        let mut scratch = Scratch::of(this, Class::Font);
        scratch.bytes[1] = words.as_mut_ptr() as usize as u32;
        // Ours must not delete jgl's HFONT.
        scratch.bytes[0x18 / 4] = 0;
        scratch.bytes[0x14 / 4] &= !1;
        let at = scratch.bytes.as_mut_ptr() as usize as u32;
        scratch.at = at;
        // SAFETY: the slot's previous function.
        let r = unsafe { call::function(prev(Class::Font, 0x08), this, &[face, height, flags]) };
        // SAFETY: ours, on the copy.
        let o = unsafe { call::function(crate::own::implementation(Class::Font, 0x08), scratch.at, &[face, height, flags]) };
        let jgls: Vec<u32> = (2..7).map(|i| word(metrics, 4 * i)).collect();
        let d = (jgls != words[2..7]).then(|| format!("metrics jgl {jgls:?} ours {:?}", &words[2..7]));
        let lf = (inproc::gdi::font(word(this, 0x18)) != inproc::gdi::font(scratch.word(0x18))).then(|| format!("fonts jgl {:?} ours {:?}", inproc::gdi::font(word(this, 0x18)), inproc::gdi::font(scratch.word(0x18))));
        crate::own::font::forget(scratch.at);
        note(Class::Font, 0x08, both(answers(r, o), both(d, lf)));
        r
    }

    /// measure: ours on jgl's own font (its HFONT, our memory DC).
    pub extern "thiscall" fn f_measure(this: u32, text: u32, len: u32) -> u32 {
        // SAFETY: the slot's previous function.
        let r = unsafe { call::function(prev(Class::Font, 0x10), this, &[text, len]) };
        // SAFETY: ours, on jgl's font (it reads only its HFONT).
        let o = unsafe { call::function(crate::own::implementation(Class::Font, 0x10), this, &[text, len]) };
        note(Class::Font, 0x10, answers(r, o));
        r
    }

    // Graphics: the screen getters and the scale.
    macro_rules! graphics_simple {
        ($name:ident, $slot:expr) => {
            compared!($name, Class::Graphics, $slot, (), |this, r| |_s: &Scratch, o: u32| answers(r, o));
        };
    }
    graphics_simple!(g_width, 0xA8);
    graphics_simple!(g_height, 0xAC);
    graphics_simple!(g_depth, 0xB0);
    graphics_simple!(g_hwnd, 0x20);
    graphics_simple!(g_hinstance, 0x24);
    graphics_simple!(g_dc, 0x3C);

    /// set_scale: ours keeps the same three numbers jgl keeps.
    pub extern "thiscall" fn g_set_scale(this: u32, a: u32, b: u32, c: u32) -> u32 {
        // SAFETY: the slot's previous function.
        let r = unsafe { call::function(prev(Class::Graphics, 0x9C), this, &[a, b, c]) };
        crate::own::graphics::set_scale(this, a, b, c);
        note(Class::Graphics, 0x9C, None);
        r
    }

    /// get_scale: jgl's three numbers against ours.
    pub extern "thiscall" fn g_get_scale(this: u32, a: u32, b: u32, c: u32) -> u32 {
        // SAFETY: the slot's previous function.
        let r = unsafe { call::function(prev(Class::Graphics, 0xA0), this, &[a, b, c]) };
        let jgls = [word(a, 0), word(b, 0), word(c, 0)];
        let mut mine = [0u32; 3];
        let base = mine.as_mut_ptr() as usize as u32;
        let (pa, pb, pc) = (base, base + 4, base + 8);
        // SAFETY: ours, into our buffers.
        unsafe { call::function(crate::own::implementation(Class::Graphics, 0xA0), this, &[pa, pb, pc]) };
        note(Class::Graphics, 0xA0, (jgls != mine).then(|| format!("scale jgl {jgls:?} ours {mine:?}")));
        r
    }

    type F = usize;
    pub fn entries() -> Vec<(Class, u16, F)> {
        type M0 = extern "thiscall" fn(u32) -> u32;
        type M1 = extern "thiscall" fn(u32, u32) -> u32;
        type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;
        type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
        type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;
        let _ = size_of::<u32>();
        vec![
            (Class::Image, 0x04, i_create as M4 as F),
            (Class::Image, 0x0C, i_pixel_at as M2 as F),
            (Class::Image, 0x10, i_lock as M0 as F),
            (Class::Image, 0x18, i_lock_18 as M0 as F),
            (Class::Image, 0x1C, i_pixel_at_1c as M2 as F),
            (Class::Image, 0x24, i_unlock as M1 as F),
            (Class::Image, 0x28, i_acquire_dc as M0 as F),
            (Class::Image, 0x2C, i_release_dc as M1 as F),
            (Class::Image, 0x34, i_set_clip as M1 as F),
            (Class::Image, 0x44, i_fill as M2 as F),
            (Class::Image, 0x58, i_replace as M3 as F),
            (Class::Image, 0xBC, i_set_key as M1 as F),
            (Class::Image, 0xC0, i_key as M0 as F),
            (Class::Image, 0xC4, i_reset_clip as M0 as F),
            (Class::Image, 0xCC, i_clip_rect as M0 as F),
            (Class::Image, 0xD4, i_image_rect as M0 as F),
            (Class::Image, 0xD8, i_width as M0 as F),
            (Class::Image, 0xDC, i_height as M0 as F),
            (Class::Image, 0xE0, i_row as M0 as F),
            (Class::Image, 0xE4, i_format as M0 as F),
            (Class::Image, 0xE8, i_table as M0 as F),
            (Class::Image, 0xEC, i_set_table as M1 as F),
            (Class::Sprite, 0x14, s_get_table as M0 as F),
            (Class::Sprite, 0x1C, s_key as M0 as F),
            (Class::Sprite, 0x28, s_bits as M0 as F),
            (Class::Sprite, 0x2C, s_pitch as M0 as F),
            (Class::Sprite, 0x30, s_width as M0 as F),
            (Class::Sprite, 0x34, s_height as M0 as F),
            (Class::ColorTable, 0x04, t_init as M0 as F),
            (Class::ColorTable, 0x10, t_get_colors as M3 as F),
            (Class::ColorTable, 0x14, t_set_colors as M3 as F),
            (Class::ColorTable, 0x18, t_555 as M0 as F),
            (Class::ColorTable, 0x1C, t_565 as M0 as F),
            (Class::Font, 0x08, f_create as M3 as F),
            (Class::Font, 0x10, f_measure as M2 as F),
            (Class::Graphics, 0x20, g_hwnd as M0 as F),
            (Class::Graphics, 0x24, g_hinstance as M0 as F),
            (Class::Graphics, 0x3C, g_dc as M0 as F),
            (Class::Graphics, 0x9C, g_set_scale as M3 as F),
            (Class::Graphics, 0xA0, g_get_scale as M3 as F),
            (Class::Graphics, 0xA8, g_width as M0 as F),
            (Class::Graphics, 0xAC, g_height as M0 as F),
            (Class::Graphics, 0xB0, g_depth as M0 as F),
        ]
    }
}

/// Installs the compare layer in jgl.dll's tables (at `base`) and ours
/// (`crate::surface`): after every other group, before the counting mode.
pub fn install(base: u32) -> Result<(), inproc::hook::HookError> {
    #[cfg(all(windows, target_arch = "x86"))]
    {
        use inproc::Ptr32;
        use inproc::hook::SlotPatch;
        crate::own::prepare_compare();
        let ours = crate::surface::tables();
        let mut patches = Vec::new();
        for (class, slot, f) in imp::entries() {
            let jgl_cell = Ptr32::<u32>::new(base + class.vtable_rva() + slot as u32);
            imp::PREV[class.index() as usize][slot as usize / 4].store(jgl_cell.read()?, Ordering::Release);
            patches.push(SlotPatch::replace(jgl_cell, f as u32)?);
            if let Some(t) = ours {
                patches.push(SlotPatch::replace(Ptr32::<u32>::new(t[class.index() as usize] + slot as u32), f as u32)?);
            }
        }
        // For the life of the process.
        std::mem::forget(patches);
        Ok(())
    }
    #[cfg(not(all(windows, target_arch = "x86")))]
    {
        let _ = base;
        Err(inproc::hook::HookError::Unsupported)
    }
}

/// TEMPORARY, with the compare mode: the controller's methods the window
/// procedure calls (the exe's vtable, 14 slots: `jgl::window::Controller`)
/// counted, and the input among them logged in order, so that a session
/// with jgl's window and one with ours can be put side by side.
pub mod controller {
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    static PREV: [AtomicU32; 14] = [const { AtomicU32::new(0) }; 14];
    static COUNTS: [AtomicU64; 14] = [const { AtomicU64::new(0) }; 14];

    const NAMES: [&str; 14] = ["size", "paint", "hit_test", "key", "char", "move", "wheel", "left_down", "left_up", "right_down", "right_up", "activate", "timer", "message_402"];

    /// The counts so far, one line.
    pub fn counts() -> String {
        NAMES.iter().zip(&COUNTS).map(|(n, c)| format!("{n} {}", c.load(Ordering::Relaxed))).collect::<Vec<_>>().join(", ")
    }

    #[cfg(all(windows, target_arch = "x86"))]
    mod imp {
        use super::*;

        fn logged(i: usize, args: &[u32]) {
            COUNTS[i].fetch_add(1, Ordering::Relaxed);
            // Input and window state, in order (not moves, hit tests, paints, timers).
            if matches!(i, 0 | 3 | 4 | 6 | 7 | 8 | 9 | 10 | 11 | 13) {
                log::info!("controller: {} {:?}", NAMES[i], args);
            }
        }

        macro_rules! wrapped {
            ($name:ident, $i:expr, ($($a:ident),*)) => {
                pub extern "thiscall" fn $name(this: u32 $(, $a: u32)*) -> u32 {
                    logged($i, &[$($a),*]);
                    let prev = PREV[$i].load(Ordering::Acquire);
                    // SAFETY: the method that was in the slot, its arguments.
                    unsafe { crate::own::call::function(prev, this, &[$($a),*]) }
                }
            };
        }

        wrapped!(m00, 0, (a, b));
        wrapped!(m04, 1, (a));
        wrapped!(m08, 2, (a, b));
        wrapped!(m0c, 3, (a, b));
        wrapped!(m10, 4, (a));
        wrapped!(m14, 5, (a, b));
        wrapped!(m18, 6, (a, b, c));
        wrapped!(m1c, 7, (a, b, c));
        wrapped!(m20, 8, (a, b));
        wrapped!(m24, 9, (a, b, c));
        wrapped!(m28, 10, (a, b));
        wrapped!(m2c, 11, (a, b));
        wrapped!(m30, 12, (a));
        wrapped!(m34, 13, ());

        pub fn functions() -> [u32; 14] {
            type M0 = extern "thiscall" fn(u32) -> u32;
            type M1 = extern "thiscall" fn(u32, u32) -> u32;
            type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;
            type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
            [
                m00 as M2 as usize, m04 as M1 as usize, m08 as M2 as usize, m0c as M2 as usize, m10 as M1 as usize, m14 as M2 as usize,
                m18 as M3 as usize, m1c as M3 as usize, m20 as M2 as usize, m24 as M3 as usize, m28 as M2 as usize, m2c as M2 as usize,
                m30 as M1 as usize, m34 as M0 as usize,
            ]
            .map(|f| f as u32)
        }
    }

    /// Wraps the controller's 14 methods in its vtable at `vtable`.
    pub fn install(vtable: u32) -> Result<(), inproc::hook::HookError> {
        #[cfg(all(windows, target_arch = "x86"))]
        {
            let mut patches = Vec::new();
            for (i, f) in imp::functions().into_iter().enumerate() {
                let cell = inproc::Ptr32::<u32>::new(vtable + 4 * i as u32);
                PREV[i].store(cell.read()?, Ordering::Release);
                patches.push(inproc::hook::SlotPatch::replace(cell, f)?);
            }
            std::mem::forget(patches);
            Ok(())
        }
        #[cfg(not(all(windows, target_arch = "x86")))]
        {
            let _ = vtable;
            Err(inproc::hook::HookError::Unsupported)
        }
    }
}
