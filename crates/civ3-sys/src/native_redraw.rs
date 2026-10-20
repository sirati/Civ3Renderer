//! Rebuild retained native form canvases after renderer drawing resumes.
//! This is a one-shot game-thread operation, never a publisher-thread call.

use std::collections::HashSet;
use crate::{Build, controls::{self, Mem}};

const STATUS2: u32 = 0xA0;
const CHILDREN: u32 = 0x224;
const CHILD_COUNT: u32 = 0x22C;
const REDRAW_SLOT: u32 = 0x124;
const REDRAW: u32 = 0x60_26B0;
const FRAME_SLOT: u32 = 0x138;
const FRAME: u32 = 0x60_2A20;
const FRAME_PROLOGUE: &[u8] = &[0x83, 0xEC, 0x10, 0x53, 0x55, 0x56, 0x8B, 0xF1, 0x57];
const DIALOG_FRAME: u32 = 0x61_3FE0;
const DIALOG_FRAME_PROLOGUE: &[u8] = &[
    0x83, 0xEC, 0x10, 0x56, 0x57, 0x8B, 0x7C, 0x24, 0x1C, 0x8B, 0xF1,
    0x57, 0xE8, 0x2F, 0xEA, 0xFE, 0xFF,
];
const PROLOGUE: &[u8] = &[0x56, 0x8B, 0xF1, 0xF6, 0x86, 0x20, 0x02, 0x00, 0x00, 0x01];
const MAX_CHILDREN: u32 = 256;
const MAX_FORMS: usize = 4096;

#[derive(Debug)]
struct Form {
    address: u32,
    vtable: u32,
    path: Vec<u32>,
}

fn field(mem: &dyn Mem, address: u32, offset: u32) -> Option<u32> {
    mem.u32(address.checked_add(offset)?)
}
fn visible(mem: &dyn Mem, address: u32) -> bool {
    field(mem, address, STATUS2).is_some_and(|s| s & 1 != 0)
}
fn children(mem: &dyn Mem, address: u32) -> Vec<u32> {
    let Some(array) = field(mem, address, CHILDREN).filter(|&p| p != 0) else { return vec![] };
    let count = field(mem, address, CHILD_COUNT).unwrap_or(0).min(MAX_CHILDREN);
    (0..count).filter_map(|i| field(mem, array, i * 4)).filter(|&p| p != 0).collect()
}
fn roots(mem: &dyn Mem, sites: &controls::Sites) -> Vec<u32> {
    let count = mem.u32(sites.root_count).unwrap_or(0).min(128);
    (0..count).filter_map(|i| field(mem, sites.roots, i * 4)).filter(|&p| p != 0).collect()
}

fn forms(mem: &dyn Mem, sites: &controls::Sites) -> Vec<Form> {
    fn visit(mem: &dyn Mem, address: u32, path: &mut Vec<u32>, seen: &mut HashSet<u32>, out: &mut Vec<Form>) {
        if path.len() >= 32 || out.len() >= MAX_FORMS || !visible(mem, address) || !seen.insert(address) { return; }
        let Some(vtable) = mem.u32(address).filter(|&p| p != 0) else { return };
        path.push(address);
        out.push(Form { address, vtable, path: path.clone() });
        // Parents must draw first: translucent children copy their pixels.
        // Root/child index zero is topmost, so rebuild from the bottom up.
        for child in children(mem, address).into_iter().rev() {
            visit(mem, child, path, seen, out);
        }
        path.pop();
    }
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for root in roots(mem, sites).into_iter().rev() {
        visit(mem, root, &mut Vec::new(), &mut seen, &mut out);
    }
    out
}

fn still_live(mem: &dyn Mem, sites: &controls::Sites, form: &Form) -> bool {
    let Some(&root) = form.path.first() else { return false };
    roots(mem, sites).contains(&root)
        && form.path.iter().all(|&p| visible(mem, p))
        && form.path.windows(2).all(|p| children(mem, p[0]).contains(&p[1]))
        && mem.u32(form.address) == Some(form.vtable)
        && field(mem, form.vtable, REDRAW_SLOT) == Some(REDRAW)
}

inproc::thiscall_fn! {
    /// Base_Form::m73 draws its retained canvas through m22, then invalidates
    /// its complete screen rectangle. It guards visibility and busy draws.
    pub fn redraw_form(form;) -> ();
}

inproc::thiscall_fn! {
    /// Base_Form::m78 rebuilds its retained frame images. The signed side
    /// argument -1 selects all four sides; the native callee pops one word.
    pub fn rebuild_frame(form; side) -> ();
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Refreshed {
    pub canvases: usize,
    pub frame_caches: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Draw { Frame(u32), Canvas }

fn frame_site(mem: &dyn Mem, form: &Form) -> Option<u32> {
    if field(mem, form.address, STATUS2)? & 2 == 0 { return None; }
    field(mem, form.vtable, FRAME_SLOT).filter(|site| matches!(*site, FRAME | DIALOG_FRAME))
}

fn refresh_with(mem: &dyn Mem, sites: &controls::Sites, mut call: impl FnMut(Draw, u32)) -> Refreshed {
    let mut count = Refreshed::default();
    for form in forms(mem, sites) {
        if !still_live(mem, sites, &form) { continue; }
        // Frame images are authored separately from m22's client canvas.
        // Rebuild them first, then revalidate lifetime after that callback.
        if let Some(site) = frame_site(mem, &form) {
            call(Draw::Frame(site), form.address);
            count.frame_caches += 1;
        }
        if still_live(mem, sites, &form) {
            call(Draw::Canvas, form.address);
            count.canvases += 1;
        }
    }
    count
}

/// Rebuild all visible roots and descendants, including roots beneath a
/// modal. WM_PAINT alone only composites their previously skipped pixels.
/// The caller must run on the native UI thread, after camera restoration.
/// Membership and vtables are rechecked after every native drawing callback;
/// addresses never survive this synchronous call or cross an input event.
pub fn refresh(build: Build) -> Result<Refreshed, inproc::hook::HookError> {
    let Some(sites) = controls::sites(build) else { return Err(inproc::hook::HookError::Unsupported) };
    let draw = redraw_form::Function::at(REDRAW, PROLOGUE)?;
    let frame = rebuild_frame::Function::at(FRAME, FRAME_PROLOGUE)?;
    // The dialog override first calls Base_Form::m78, then paints its title
    // overlay into the top frame cache. Preserve that checked native dispatch.
    let dialog_frame = rebuild_frame::Function::at(DIALOG_FRAME, DIALOG_FRAME_PROLOGUE)?;
    Ok(refresh_with(&controls::Live, &sites, |kind, address| match kind {
        Draw::Frame(DIALOG_FRAME) => dialog_frame.call(address, u32::MAX),
        Draw::Frame(_) => frame.call(address, u32::MAX),
        Draw::Canvas => draw.call(address),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    #[derive(Default)]
    struct Memory(HashMap<u32, u32>);
    impl Mem for Memory {
        fn u32(&self, address: u32) -> Option<u32> { self.0.get(&address).copied() }
        fn u8(&self, address: u32) -> Option<u8> { self.u32(address).map(|v| v as u8) }
    }
    impl Memory {
        fn form(&mut self, address: u32, shown: bool, children: &[u32]) {
            self.0.insert(address, 0x800000);
            self.0.insert(0x800000 + REDRAW_SLOT, REDRAW);
            self.0.insert(address + STATUS2, u32::from(shown));
            self.0.insert(address + CHILDREN, address + 0x1000);
            self.0.insert(address + CHILD_COUNT, children.len() as u32);
            for (i, child) in children.iter().enumerate() { self.0.insert(address + 0x1000 + i as u32 * 4, *child); }
        }
        fn root_list(&mut self, list: &[u32]) -> controls::Sites {
            let sites = controls::Sites { roots: 0x100, root_count: 0x80, modal: 0x84, companion: 0x88, main_gui: 0, advisor: 0 };
            self.0.insert(sites.root_count, list.len() as u32);
            for (i, root) in list.iter().enumerate() { self.0.insert(sites.roots + i as u32 * 4, *root); }
            sites
        }
    }
    #[test]
    fn rebuilds_background_and_modal_parents_before_their_children() {
        let mut mem = Memory::default();
        let sites = mem.root_list(&[0x2000, 0x4000]);
        mem.0.insert(sites.modal, 0x2000);
        mem.form(0x2000, true, &[0x6000]);
        mem.form(0x4000, true, &[0x8000, 0xA000]);
        for at in [0x6000,0x8000,0xA000] { mem.form(at, true, &[]); }
        let plan = forms(&mem, &sites);
        assert_eq!(plan.iter().map(|f| f.address).collect::<Vec<_>>(), [0x4000,0xA000,0x8000,0x2000,0x6000]);
        assert!(plan.iter().all(|f| still_live(&mem, &sites, f)));
    }
    #[test]
    fn hidden_subtrees_and_cycles_never_enter_the_redraw_plan() {
        let mut mem = Memory::default();
        let sites = mem.root_list(&[0x2000]);
        mem.form(0x2000, true, &[0x4000,0x6000]);
        mem.form(0x4000, false, &[0x8000]);
        mem.form(0x6000, true, &[0x2000]);
        mem.form(0x8000, true, &[]);
        assert_eq!(forms(&mem, &sites).iter().map(|f| f.address).collect::<Vec<_>>(), [0x2000,0x6000]);
    }
    #[test]
    fn redraw_callbacks_cannot_leave_stale_forms_in_the_remaining_plan() {
        let mut mem = Memory::default();
        let sites = mem.root_list(&[0x2000]);
        mem.form(0x2000, true, &[0x4000]);
        mem.form(0x4000, true, &[]);
        let plan = forms(&mem, &sites);
        assert!(still_live(&mem, &sites, &plan[1]));
        mem.0.insert(0x2000 + CHILD_COUNT, 0);
        assert!(!still_live(&mem, &sites, &plan[1]));
        mem.0.insert(0x2000 + CHILD_COUNT, 1);
        mem.0.insert(0x4000, 0x900000);
        assert!(!still_live(&mem, &sites, &plan[1]));
        mem.0.insert(0x4000, 0x800000);
        mem.0.insert(0x2000 + STATUS2, 0);
        assert!(!still_live(&mem, &sites, &plan[1]));
    }
    #[test]
    fn native_frame_images_rebuild_before_client_canvas_and_children() {
        let mut mem = Memory::default();
        let sites = mem.root_list(&[0x2000]);
        mem.form(0x2000, true, &[0x4000]);
        mem.form(0x4000, true, &[]);
        mem.0.insert(0x2000 + STATUS2, 3);
        mem.0.insert(0x800000 + FRAME_SLOT, FRAME);
        let mut calls = Vec::new();
        assert_eq!(refresh_with(&mem, &sites, |kind, address| calls.push((kind, address))),
            Refreshed { canvases: 2, frame_caches: 1 });
        assert_eq!(calls, [(Draw::Frame(FRAME), 0x2000), (Draw::Canvas, 0x2000), (Draw::Canvas, 0x4000)]);
    }
    #[test]
    fn frame_rebuild_requires_framed_status_and_checked_native_vtable_entry() {
        let mut mem = Memory::default();
        let sites = mem.root_list(&[0x2000]);
        mem.form(0x2000, true, &[]);
        mem.0.insert(0x800000 + FRAME_SLOT, FRAME);
        let form = &forms(&mem, &sites)[0];
        assert_eq!(frame_site(&mem, form), None);
        mem.0.insert(0x2000 + STATUS2, 3);
        assert_eq!(frame_site(&mem, form), Some(FRAME));
        mem.0.insert(0x800000 + FRAME_SLOT, DIALOG_FRAME);
        assert_eq!(frame_site(&mem, form), Some(DIALOG_FRAME));
        mem.0.insert(0x800000 + FRAME_SLOT, FRAME + 16);
        assert_eq!(frame_site(&mem, form), None);
        let mut calls = Vec::new();
        assert_eq!(refresh_with(&mem, &sites, |kind, _| calls.push(kind)),
            Refreshed { canvases: 1, frame_caches: 0 });
        assert_eq!(calls, [Draw::Canvas]);
    }
    #[test]
    fn dialog_frame_dispatch_keeps_the_checked_title_overlay_override() {
        let mut mem = Memory::default();
        let sites = mem.root_list(&[0x2000]);
        mem.form(0x2000, true, &[]);
        mem.0.insert(0x2000 + STATUS2, 3);
        mem.0.insert(0x800000 + FRAME_SLOT, DIALOG_FRAME);
        let mut calls = Vec::new();
        refresh_with(&mem, &sites, |kind, _| calls.push(kind));
        assert_eq!(calls, [Draw::Frame(DIALOG_FRAME), Draw::Canvas]);
    }
    #[test]
    fn frame_callback_cannot_redraw_a_removed_parent_or_its_children() {
        use std::cell::Cell;
        struct Lifetime<'a> { memory: &'a Memory, alive: Cell<bool> }
        impl Mem for Lifetime<'_> {
            fn u32(&self, address: u32) -> Option<u32> {
                if address == 0x2000 + STATUS2 && !self.alive.get() { Some(0) }
                else { self.memory.u32(address) }
            }
            fn u8(&self, address: u32) -> Option<u8> { self.u32(address).map(|v| v as u8) }
        }
        let mut mem = Memory::default();
        let sites = mem.root_list(&[0x2000]);
        mem.form(0x2000, true, &[0x4000]);
        mem.form(0x4000, true, &[]);
        mem.0.insert(0x2000 + STATUS2, 3);
        mem.0.insert(0x800000 + FRAME_SLOT, FRAME);
        let lifetime = Lifetime { memory: &mem, alive: Cell::new(true) };
        let mut calls = Vec::new();
        assert_eq!(refresh_with(&lifetime, &sites, |kind, address| {
            calls.push((kind, address));
            lifetime.alive.set(false);
        }), Refreshed { canvases: 0, frame_caches: 1 });
        assert_eq!(calls, [(Draw::Frame(FRAME), 0x2000)]);
    }

}
