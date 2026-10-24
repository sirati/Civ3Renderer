//! The surface group's object lifetimes (`docs/jgl-replacement.md` §6,
//! group 6): jgl's factories, graphics slots +0x74 `new_font(metrics)`,
//! +0x78 `new_color_table(owner)`, +0x7C `new_image(owner, a2)`, +0x80
//! `new_sprite(owner, a2)`, and its deletes +0x88..+0x94 (one function:
//! the object's own slot 0, `scalar_delete`, with 1).
//!
//! What the factories do [V, jgl.dll disassembled]: allocate the class's
//! size (`slots::Class::object_size`) and run its constructor with the
//! first argument (the owner the object points back at: a sprite's +0x0C,
//! an image's +0x80, a colour table's +0x80C; a font's metrics). The
//! objects are jgl's; the shim gives each the method table that is ours
//! ([`SurfaceCall::Created`] `ours`), so every later call of the object
//! goes through our table.

use crate::slots::Class;

/// A call of the surface group.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SurfaceCall {
    /// A factory made `object` of `class` from its arguments (`owner`: the
    /// first, `arg`: the second or 0); `ours`: it has our method table.
    Created { class: Class, object: u32, owner: u32, arg: u32, ours: bool },
    /// A delete slot: `object` of `class` goes.
    Deleted { class: Class, object: u32 },
}

impl Class {
    /// The class's index in [`Class::ALL`] (its number in records).
    pub fn index(self) -> u32 {
        Class::ALL.iter().position(|&c| c == self).unwrap_or(0) as u32
    }

    pub fn from_index(i: u32) -> Option<Class> {
        Class::ALL.get(i as usize).copied()
    }
}
