//! The counting mode (`CIV3R_JGL_COUNT=1`, `docs/jgl-replacement.md` §7):
//! every slot of the five vtables goes through a generated stub that counts
//! the call and jumps on to whatever the slot held.
//!
//! A stub knows nothing of the method's arguments, so it works for every
//! slot, jgl's or ours:
//!
//! ```text
//! mov eax, fs:[0x24]        ; this thread's id (TEB ClientId.UniqueThread)
//! cmp eax, [first_thread]
//! je  +6
//! inc dword [other_threads]
//! inc dword [count_i]
//! jmp dword [original_i]
//! ```
//!
//! `eax` is free at a method's entry (not an argument in `thiscall`).
//! Install it after every other slot patch: it wraps what is there, and
//! patches that check for jgl's own function would no longer find it.
//!
//! Our method tables (`crate::surface`), when installed, get stubs too,
//! counted apart: calls through jgl's tables are then the objects that
//! still have jgl's.

use std::sync::OnceLock;

use inproc::Ptr32;
use inproc::hook::{HookError, SlotPatch};
use jgl::counts::Counts;
use jgl::slots::Class;

const STUB: usize = 32;

/// The counters' block: per slot a count, then per slot the original,
/// then the first thread and the other threads' count, then the stubs.
struct Block {
    at: u32,
    slots: usize,
    /// Whether a set counts our tables (the last).
    ours: bool,
    /// Whether a set counts jgl's tables (the first).
    jgls: bool,
}

impl Block {
    fn count(&self, i: usize) -> u32 {
        self.at + 4 * i as u32
    }
    fn original(&self, i: usize) -> u32 {
        self.at + 4 * (self.slots + i) as u32
    }
    fn first_thread(&self) -> u32 {
        self.at + 8 * self.slots as u32
    }
    fn other_threads(&self) -> u32 {
        self.first_thread() + 4
    }
    fn stub(&self, i: usize) -> u32 {
        ((self.other_threads() + 4 + 15) & !15) + (STUB * i) as u32
    }
    fn len(slots: usize) -> usize {
        8 * slots + 8 + 16 + STUB * slots
    }
}

static BLOCK: OnceLock<Block> = OnceLock::new();

fn stub_code(b: &Block, i: usize) -> [u8; STUB] {
    let mut c = [0u8; STUB];
    let mut at = 0;
    let mut put = |bytes: &[u8]| {
        c[at..at + bytes.len()].copy_from_slice(bytes);
        at += bytes.len();
    };
    put(&[0x64, 0xA1, 0x24, 0, 0, 0]);
    put(&[0x3B, 0x05]);
    put(&b.first_thread().to_le_bytes());
    put(&[0x74, 0x06]);
    put(&[0xFF, 0x05]);
    put(&b.other_threads().to_le_bytes());
    put(&[0xFF, 0x05]);
    put(&b.count(i).to_le_bytes());
    put(&[0xFF, 0x25]);
    put(&b.original(i).to_le_bytes());
    c
}

/// Puts counting stubs into every slot of jgl.dll at `base`. `thread`: the
/// game's thread id (calls from others are counted apart). Once per
/// process; later calls do nothing.
pub fn install(base: u32, thread: u32) -> Result<(), HookError> {
    if BLOCK.get().is_some() {
        return Ok(());
    }
    let per_set: usize = Class::ALL.iter().map(|c| c.slots().len()).sum();
    // Our jgl (jgl.dll not loaded): our tables are the only ones, counted
    // as "ours".
    let own = crate::own::tables();
    let ours = own.or(crate::surface::tables());
    let jgls = (own.is_none() && base != 0).then(|| Class::ALL.map(|c| base + c.vtable_rva()));
    let slots = per_set * (usize::from(ours.is_some()) + usize::from(jgls.is_some()));
    let at = inproc::hook::alloc_code(Block::len(slots))?;
    let block = Block { at, slots, ours: ours.is_some(), jgls: jgls.is_some() };
    let mut sets: Vec<[u32; 5]> = jgls.into_iter().collect();
    sets.extend(ours);
    Ptr32::<u32>::new(block.first_thread()).write(&thread)?;
    let mut i = 0;
    let mut patches = Vec::with_capacity(slots);
    for tables in sets {
    for (class, table) in Class::ALL.into_iter().zip(tables) {
        for slot in class.slots() {
            let cell = Ptr32::<u32>::new(table + slot.offset as u32);
            Ptr32::<u32>::new(block.original(i)).write(&cell.read()?)?;
            inproc::hook::write_code(block.stub(i), &stub_code(&block, i))?;
            patches.push(SlotPatch::replace(cell, block.stub(i))?);
            i += 1;
        }
    }
    }
    // For the life of the process.
    std::mem::forget(patches);
    let _ = BLOCK.set(block);
    Ok(())
}

/// The counts so far through jgl's tables and through ours (if the
/// surface group made them), and the calls from other threads than the
/// game's, if the counting mode is installed.
pub fn snapshot() -> Option<(Counts, Option<Counts>, u32)> {
    let b = BLOCK.get()?;
    let all = Ptr32::<u32>::new(b.at).read_array(b.slots).ok()?;
    let mut rest = &all[..];
    let mut set = || {
        let mut counts = Counts::zero();
        for (i, class) in Class::ALL.iter().enumerate() {
            let (mine, more) = rest.split_at(class.slots().len());
            counts.per_class[i] = mine.to_vec();
            rest = more;
        }
        counts
    };
    let jgls = if b.jgls { set() } else { Counts::zero() };
    let ours = b.ours.then(set);
    let other = Ptr32::<u32>::new(b.other_threads()).read().ok()?;
    Some((jgls, ours, other))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stubs_fit_and_point_at_their_cells() {
        let b = Block { at: 0x0100_0000, slots: 201, ours: false, jgls: true };
        let c = stub_code(&b, 3);
        assert_eq!(&c[0..6], &[0x64, 0xA1, 0x24, 0, 0, 0]);
        assert_eq!(&c[20..22], &[0xFF, 0x05]);
        assert_eq!(&c[22..26], &b.count(3).to_le_bytes());
        assert_eq!(&c[26..28], &[0xFF, 0x25]);
        assert_eq!(&c[28..32], &b.original(3).to_le_bytes());
        // je skips exactly the other threads' inc.
        assert_eq!(&c[12..16], &[0x74, 0x06, 0xFF, 0x05]);
        assert_eq!(b.stub(0) % 16, 0);
        assert!(b.stub(200) + STUB as u32 <= b.at + Block::len(201) as u32);
        assert!(b.stub(0) >= b.other_threads() + 4);
    }
}
