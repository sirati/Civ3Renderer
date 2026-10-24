//! The surface group's factories and deletes (`jgl::surface`,
//! `docs/jgl-replacement.md` §6, group 6): graphics +0x74..+0x80 and
//! +0x88..+0x94.
//!
//! Our method tables: one per class, a copy of jgl's table made when the
//! group is installed, after every other group's takeover (so it holds the
//! same entry points, ours and jgl's). A factory runs jgl's, then gives
//! the new object our table; the graphics object gets ours at the first
//! factory call. From then on the game reaches every object through our
//! tables, and a slot changes for all of them by writing our table only
//! (the counting mode wraps both, [`tables`]).
//!
//! The objects themselves (their memory, constructors, device contexts,
//! DIB sections) are still jgl's.

#![cfg_attr(not(all(windows, target_arch = "x86")), allow(dead_code, unused_imports))]

use std::sync::OnceLock;
use std::sync::atomic::AtomicU32;

use jgl::slots::Class;

use crate::takeover::{Takeover, TakeoverError};

/// Our method tables, by class (`Class::ALL` order), once installed.
static TABLES: OnceLock<[u32; 5]> = OnceLock::new();

/// Our method table of each class (`Class::ALL` order), if the group is
/// installed.
pub fn tables() -> Option<[u32; 5]> {
    TABLES.get().copied()
}

#[cfg(all(windows, target_arch = "x86"))]
mod entries {
    use super::*;
    use crate::state::{guarded, read, record};
    use crate::takeover::{Entry, original};
    use jgl::SurfaceCall;

    pub static NEW_FONT: AtomicU32 = AtomicU32::new(0);
    pub static NEW_TABLE: AtomicU32 = AtomicU32::new(0);
    pub static NEW_IMAGE: AtomicU32 = AtomicU32::new(0);
    pub static NEW_SPRITE: AtomicU32 = AtomicU32::new(0);
    pub static DELETE_FONT: AtomicU32 = AtomicU32::new(0);
    pub static DELETE_TABLE: AtomicU32 = AtomicU32::new(0);
    pub static DELETE_IMAGE: AtomicU32 = AtomicU32::new(0);
    pub static DELETE_SPRITE: AtomicU32 = AtomicU32::new(0);
    /// jgl's own tables (base + RVA), by class.
    pub static JGL_TABLES: OnceLock<[u32; 5]> = OnceLock::new();

    pub type M1 = extern "thiscall" fn(u32, u32) -> u32;
    pub type M2 = extern "thiscall" fn(u32, u32, u32) -> u32;

    /// Gives `object` our table of `class` if it has jgl's; whether it has
    /// ours now.
    fn adopt(class: Class, object: u32) -> bool {
        let (Some(ours), Some(jgls)) = (TABLES.get(), JGL_TABLES.get()) else { return false };
        let i = class.index() as usize;
        // SAFETY: `object` is an object of `class` jgl just made (or the
        // graphics object the factory was called on): its first word is
        // its method table.
        unsafe {
            let table = read::u32_at(object);
            if table == jgls[i] {
                read::set_u32(object, ours[i]);
            }
            read::u32_at(object) == ours[i]
        }
    }

    fn made(this: u32, class: Class, object: u32, owner: u32, arg: u32) -> u32 {
        guarded("factory", || {
            adopt(Class::Graphics, this);
            if object != 0 {
                let ours = adopt(class, object);
                record(|p| p.surface(&SurfaceCall::Created { class, object, owner, arg, ours }));
            }
        }, || ());
        object
    }

    fn deleted(class: Class, object: u32) {
        if object != 0 {
            guarded("delete", || record(|p| p.surface(&SurfaceCall::Deleted { class, object })), || None);
        }
    }

    pub extern "thiscall" fn new_font(this: u32, metrics: u32) -> u32 {
        let o = original::<M1>(&NEW_FONT)(this, metrics);
        made(this, Class::Font, o, metrics, 0)
    }

    pub extern "thiscall" fn new_table(this: u32, owner: u32) -> u32 {
        let o = original::<M1>(&NEW_TABLE)(this, owner);
        made(this, Class::ColorTable, o, owner, 0)
    }

    pub extern "thiscall" fn new_image(this: u32, owner: u32, a2: u32) -> u32 {
        let o = original::<M2>(&NEW_IMAGE)(this, owner, a2);
        made(this, Class::Image, o, owner, a2)
    }

    pub extern "thiscall" fn new_sprite(this: u32, owner: u32, a2: u32) -> u32 {
        let o = original::<M2>(&NEW_SPRITE)(this, owner, a2);
        made(this, Class::Sprite, o, owner, a2)
    }

    pub extern "thiscall" fn delete_font(this: u32, object: u32) -> u32 {
        deleted(Class::Font, object);
        original::<M1>(&DELETE_FONT)(this, object)
    }

    pub extern "thiscall" fn delete_table(this: u32, object: u32) -> u32 {
        deleted(Class::ColorTable, object);
        original::<M1>(&DELETE_TABLE)(this, object)
    }

    pub extern "thiscall" fn delete_image(this: u32, object: u32) -> u32 {
        deleted(Class::Image, object);
        original::<M1>(&DELETE_IMAGE)(this, object)
    }

    pub extern "thiscall" fn delete_sprite(this: u32, object: u32) -> u32 {
        deleted(Class::Sprite, object);
        original::<M1>(&DELETE_SPRITE)(this, object)
    }

    /// The group's graphics slots.
    pub fn graphics_slots() -> Vec<Entry> {
        vec![
            Entry { offset: 0x74, function: new_font as M1 as usize as u32, original: &NEW_FONT },
            Entry { offset: 0x78, function: new_table as M1 as usize as u32, original: &NEW_TABLE },
            Entry { offset: 0x7C, function: new_image as M2 as usize as u32, original: &NEW_IMAGE },
            Entry { offset: 0x80, function: new_sprite as M2 as usize as u32, original: &NEW_SPRITE },
            Entry { offset: 0x88, function: delete_font as M1 as usize as u32, original: &DELETE_FONT },
            Entry { offset: 0x8C, function: delete_table as M1 as usize as u32, original: &DELETE_TABLE },
            Entry { offset: 0x90, function: delete_image as M1 as usize as u32, original: &DELETE_IMAGE },
            Entry { offset: 0x94, function: delete_sprite as M1 as usize as u32, original: &DELETE_SPRITE },
        ]
    }
}

/// The taken-over surface group (for the life of the process, in
/// practice).
pub struct SurfaceGroup {
    _graphics: Takeover,
}

/// Takes over the surface group's factories and deletes of jgl.dll at
/// `base`, and makes our method tables. Install it after every other group
/// (our tables copy what jgl's hold then) and before the counting mode.
/// Its calls go to the hooks the text group was installed with
/// ([`crate::text::install`] first).
pub fn install(base: u32) -> Result<SurfaceGroup, TakeoverError> {
    install_impl(base)
}

#[cfg(all(windows, target_arch = "x86"))]
fn install_impl(base: u32) -> Result<SurfaceGroup, TakeoverError> {
    use crate::takeover::take_over;
    use entries::*;
    use inproc::Ptr32;
    if !crate::state::has_hooks() {
        return Err(TakeoverError::NoHooks);
    }
    if TABLES.get().is_some() {
        return Err(inproc::hook::HookError::Unsupported.into());
    }
    let takeover = take_over(base, Class::Graphics, &entries::graphics_slots())?;
    // Our tables: jgl's as they are now (every group's entry points in).
    let mut ours = [0u32; 5];
    let mut jgls = [0u32; 5];
    for class in Class::ALL {
        let i = class.index() as usize;
        jgls[i] = base + class.vtable_rva();
        let slots = Ptr32::<u32>::new(jgls[i]).read_array(class.slots().len()).map_err(inproc::hook::HookError::from)?;
        // For the life of the process: objects point at it.
        let table: &'static mut [u32] = Box::leak(slots.into_boxed_slice());
        ours[i] = table.as_ptr() as usize as u32;
    }
    let _ = JGL_TABLES.set(jgls);
    let _ = TABLES.set(ours);
    Ok(SurfaceGroup { _graphics: takeover })
}

#[cfg(not(all(windows, target_arch = "x86")))]
fn install_impl(_base: u32) -> Result<SurfaceGroup, TakeoverError> {
    Err(inproc::hook::HookError::Unsupported.into())
}

#[cfg(test)]
mod tests {
    use jgl::slots::Class;

    /// The factories and deletes take one or two arguments, as the entry
    /// points declare (`ret 4`, `ret 8`).
    #[test]
    fn factories_and_deletes_pop_what_the_entry_points_take() {
        let g = |o: u16| Class::Graphics.slot(o).map(|s| s.stack);
        assert_eq!([0x74, 0x78, 0x7C, 0x80].map(g), [Some(4), Some(4), Some(8), Some(8)]);
        assert_eq!([0x88, 0x8C, 0x90, 0x94].map(g), [Some(4); 4]);
    }
}
