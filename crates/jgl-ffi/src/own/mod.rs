//! Our jgl: the five classes' objects in our memory, made by our
//! constructors, every slot of every class ours (`docs/jgl-replacement.md`
//! §6 groups 6 and 7, §8). Used when jgl.dll is not loaded.
//!
//! The groups taken over one by one (text, fills, copies, sprites,
//! present, display modes, factories) keep their entry points: they record
//! and then call "jgl's original" where jgl's behaviour is wanted. Here the
//! original is ours ([`graphics`], [`image`], [`sprite`], [`table`],
//! [`font`]): jgl's behaviour on our objects, without its pixels. Every
//! other slot is ours directly. [`install`] builds the five method tables
//! that way; [`new_graphics`] is `get_graphsy_object_ptr`.
//!
//! What each slot does is jgl's (the spec in each module, from jgl.dll
//! disassembled); what to record and whether pixels matter is `jgl`'s and
//! the hooks', as for the groups.

#![cfg_attr(not(all(windows, target_arch = "x86")), allow(dead_code, unused_imports))]

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use jgl::slots::Class;
use jgl::WindowHooks;

use crate::state::HooksFactory;
use crate::takeover::TakeoverError;

#[cfg(all(windows, target_arch = "x86"))]
pub mod font;
#[cfg(all(windows, target_arch = "x86"))]
pub mod graphics;
#[cfg(all(windows, target_arch = "x86"))]
pub mod image;
#[cfg(all(windows, target_arch = "x86"))]
pub mod sprite;
#[cfg(all(windows, target_arch = "x86"))]
pub mod table;
#[cfg(all(windows, target_arch = "x86"))]
pub(crate) mod call;
#[cfg(all(windows, target_arch = "x86"))]
mod registry;

/// Our method tables, by class (`Class::ALL` order), once installed.
static TABLES: OnceLock<[u32; 5]> = OnceLock::new();
/// Whether our jgl is in place (jgl.dll is not loaded).
static ON: AtomicBool = AtomicBool::new(false);
/// The host's window hooks.
static WINDOW: OnceLock<Box<dyn WindowHooks>> = OnceLock::new();

/// Whether our jgl is in place.
pub fn on() -> bool {
    ON.load(Ordering::Acquire)
}

/// Our method table of each class (`Class::ALL` order), if installed.
pub fn tables() -> Option<[u32; 5]> {
    TABLES.get().copied()
}

pub(crate) fn window_hooks() -> &'static dyn WindowHooks {
    static NONE: jgl::hooks::NoWindowHooks = jgl::hooks::NoWindowHooks;
    WINDOW.get().map_or(&NONE as &dyn WindowHooks, |w| w.as_ref())
}

/// Puts our jgl in place: the hooks every group reports to (`hooks`, as
/// [`crate::text::install`] takes them), the host's window hooks, our five
/// method tables. Once per process; before the game asks for the graphics
/// object ([`new_graphics`]).
pub fn install(hooks: HooksFactory, window: Box<dyn WindowHooks>) -> Result<(), TakeoverError> {
    install_impl(hooks, window)
}

#[cfg(all(windows, target_arch = "x86"))]
fn install_impl(hooks: HooksFactory, window: Box<dyn WindowHooks>) -> Result<(), TakeoverError> {
    use crate::takeover::Entry;
    if crate::state::FACTORY.set(hooks).is_err() || TABLES.get().is_some() {
        return Err(inproc::hook::HookError::AlreadyInstalled.into());
    }
    crate::text::set_stock_font();
    let _ = WINDOW.set(window);
    // Every group's entry points, by class.
    let (text_image, text_font) = crate::text::entries::split();
    let groups: Vec<(Class, Vec<Entry>)> = vec![
        (Class::Font, text_font),
        (Class::Image, text_image),
        (Class::Image, crate::fill::entries::image()),
        (Class::Image, crate::copy::entries::image()),
        (Class::Sprite, crate::blit::entries::sprite_slots()),
        (Class::ColorTable, crate::blit::entries::table_slots()),
        (Class::Graphics, crate::blit::entries::graphics_slots()),
        (Class::Graphics, crate::present::entries::graphics()),
        (Class::Graphics, crate::display::entries::graphics()),
        (Class::Graphics, crate::surface::entries::graphics_slots()),
    ];
    let mut ours = [0u32; 5];
    for class in Class::ALL {
        let slots = class.slots();
        let mut table: Vec<u32> = slots.iter().map(|s| implementation(class, s.offset)).collect();
        if let Some(missing) = slots.iter().zip(&table).find(|(_, f)| **f == 0) {
            log::error!("our jgl: {}+{:#x} has no implementation", class.name(), missing.0.offset);
            return Err(TakeoverError::NoSuchSlot { class, offset: missing.0.offset });
        }
        for (_, entries) in groups.iter().filter(|(c, _)| *c == class) {
            for e in entries {
                let i = e.offset as usize / 4;
                // The entry calls ours where it called jgl's.
                e.original.store(table[i], Ordering::Release);
                table[i] = e.function;
            }
        }
        // For the life of the process: our objects point at it.
        let table: &'static mut [u32] = Box::leak(table.into_boxed_slice());
        ours[class.index() as usize] = table.as_ptr() as usize as u32;
    }
    let _ = TABLES.set(ours);
    ON.store(true, Ordering::Release);
    Ok(())
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn install_impl(_hooks: HooksFactory, _window: Box<dyn WindowHooks>) -> Result<(), TakeoverError> {
    Err(inproc::hook::HookError::Unsupported.into())
}

/// Our implementation of `class`'s slot at `offset` (0: none).
#[cfg(all(windows, target_arch = "x86"))]
pub(crate) fn implementation(class: Class, offset: u16) -> u32 {
    match class {
        Class::Graphics => graphics::slot(offset),
        Class::Image => image::slot(offset),
        Class::Sprite => sprite::slot(offset),
        Class::ColorTable => table::slot(offset),
        Class::Font => font::slot(offset),
    }
}

/// A method table of `class` with our implementations alone (no group's
/// entry points): for the compare mode's copies of jgl's objects.
#[cfg(all(windows, target_arch = "x86"))]
pub(crate) fn pure_table(class: Class) -> u32 {
    static PURE: OnceLock<[u32; 5]> = OnceLock::new();
    PURE.get_or_init(|| {
        Class::ALL.map(|c| {
            let t: Vec<u32> = c.slots().iter().map(|s| implementation(c, s.offset)).collect();
            Box::leak(t.into_boxed_slice()).as_ptr() as usize as u32
        })
    })[class.index() as usize]
}

/// What the compare mode needs of ours with jgl.dll loaded: the memory DC
/// fonts are made and measured in.
#[cfg(all(windows, target_arch = "x86"))]
pub(crate) fn prepare_compare() {
    graphics::ensure_memory_dc();
}

/// `get_graphsy_object_ptr`: a new graphics object with our method table
/// (0 if our jgl is not installed).
pub fn new_graphics() -> u32 {
    #[cfg(all(windows, target_arch = "x86"))]
    {
        if on() { graphics::new() } else { 0 }
    }
    #[cfg(not(all(windows, target_arch = "x86")))]
    0
}

/// The method table of ours for `class` (0 before [`install`]).
pub(crate) fn table_of(class: Class) -> u32 {
    tables().map_or(0, |t| t[class.index() as usize])
}

/// A `ret n` body returning `value` with `n` argument words, for jgl's
/// stubs (`mov eax, 0x18; ret n`) and the slots that do nothing.
#[cfg(all(windows, target_arch = "x86"))]
macro_rules! constant {
    ($name:ident, $value:expr, $($a:ident),*) => {
        pub extern "thiscall" fn $name(_this: u32 $(, $a: u32)*) -> u32 {
            $(let _ = $a;)*
            $value
        }
    };
}
#[cfg(all(windows, target_arch = "x86"))]
pub(crate) use constant;

/// jgl's "not implemented" bodies, by argument words.
#[cfg(all(windows, target_arch = "x86"))]
pub(crate) mod stubs {
    constant!(stub0, 0x18,);
    constant!(stub1, 0x18, a);
    constant!(stub2, 0x18, a, b);
    constant!(stub3, 0x18, a, b, c);
    constant!(stub4, 0x18, a, b, c, d);
    constant!(stub6, 0x18, a, b, c, d, e, f);

    /// The stub popping `stack` bytes.
    pub fn by_stack(stack: u8) -> u32 {
        type M0 = extern "thiscall" fn(u32) -> u32;
        type M1 = extern "thiscall" fn(u32, u32) -> u32;
        type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;
        type M3 = extern "thiscall" fn(u32, u32, u32, u32) -> u32;
        type M4 = extern "thiscall" fn(u32, u32, u32, u32, u32) -> u32;
        type M6 = extern "thiscall" fn(u32, u32, u32, u32, u32, u32, u32) -> u32;
        (match stack {
            0 => stub0 as M0 as usize,
            4 => stub1 as M1 as usize,
            8 => stub2 as M2 as usize,
            12 => stub3 as M3 as usize,
            16 => stub4 as M4 as usize,
            24 => stub6 as M6 as usize,
            _ => 0,
        }) as u32
    }
}
