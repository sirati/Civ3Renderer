//! Native setup choices painted into indexed hit maps, rather than child
//! buttons. Read the same pixels that the game's hover and click handlers
//! read, so scenario restrictions and variable civilization counts agree.

use super::{Control, ControlKind, Form, Mem, part_id};
use std::collections::BTreeMap;

const TAG_HITMAP: u32 = 0x5000_0000;
const WORLD_FORM: u32 = 0x67_0390;
const PLAYER_FORM: u32 = 0x66_E2F0;

/// The setup screen form whose option mask `image` (a `PCX_Image`) is:
/// the world setup's at +0x3958, the player setup's at +0x2B60. The game
/// fills one rectangle per option into it with the option's index and
/// looks its clicks and hovers up there (`PCX_Image::get_pixel`).
pub fn mask_form(mem: &dyn Mem, image: u32) -> Option<u32> {
    [(WORLD_FORM, 0x3958), (PLAYER_FORM, 0x2B60)].into_iter().find_map(|(vtable, offset)| {
        let form = image.checked_sub(offset)?;
        (mem.u32(form)? == vtable).then_some(form)
    })
}

pub(super) fn controls(mem: &dyn Mem, form: &Form, out: &mut Vec<Control>) {
    let (pcx, last) = match form.vtable {
        WORLD_FORM => (form.addr.checked_add(0x3958), 33),
        PLAYER_FORM => {
            let (Some(civs), Some(difficulties)) = (mem.u32(0x9C_3D74), mem.u32(0x9C_3D54)) else { return };
            if !(1..=64).contains(&civs) || !(1..=32).contains(&difficulties) { return; }
            (form.addr.checked_add(0x2B60), civs + difficulties + 26)
        }
        _ => return,
    };
    let Some(pcx) = pcx else { return };
    // Without jgl the mask has no pixels; its recorded fills answer.
    let found: BTreeMap<u8, [i32; 4]> = match mem.index_regions(pcx) {
        Some(cells) => cells.into_iter().filter(|&(index, _)| index <= last as u8).collect(),
        None => {
            let Some((width, height, stride, pixels)) = hitmap(mem, pcx) else { return };
            // A rectangular run remains a valid native target even if another
            // control cuts into this index's bounding box. Pick the largest run
            // rectangle, rather than putting the pointer into such a hole.
            regions(&pixels, width, height, stride, last as u8)
        }
    };
    for (index, rect) in found {
        let rect = form.on_screen(rect);
        if rect[2] > 0 && rect[3] > 0 {
            out.push(Control { rect, kind: ControlKind::Button, id: part_id(form.addr, TAG_HITMAP, index as u32) });
        }
    }
}

fn hitmap(mem: &dyn Mem, pcx: u32) -> Option<(usize, usize, usize, Vec<u8>)> {
    let image = mem.u32(pcx.checked_add(4)?)?;
    if image == 0 || mem.u32(image.checked_add(0x24)?)? != 8 { return None; }
    let width = mem.i32(image.checked_add(0x5C)?)?.checked_sub(mem.i32(image.checked_add(0x54)?)?)?;
    let height = mem.i32(image.checked_add(0x60)?)?.checked_sub(mem.i32(image.checked_add(0x58)?)?)?;
    let stride = mem.i32(image.checked_add(0x40)?)?;
    if !(1..=4096).contains(&width) || !(1..=2160).contains(&height) || stride < width || stride > 16384 { return None; }
    let (width, height, stride) = (width as usize, height as usize, stride as usize);
    let len = stride.checked_mul(height - 1)?.checked_add(width)?;
    let pixels = mem.u32(image.checked_add(0x4C0)?)?;
    if pixels == 0 { return None; }
    pixels.checked_add(u32::try_from(len).ok()?)?;
    Some((width, height, stride, mem.read_bytes(pixels, len)?))
}

/// Rectangles made of one palette index. The maps use 255 for areas that
/// cannot be clicked. A missing or disabled choice has no pixels at all.
pub fn regions(pixels: &[u8], width: usize, height: usize, stride: usize, last: u8) -> BTreeMap<u8, [i32; 4]> {
    let mut best = BTreeMap::new();
    let mut active: BTreeMap<(u8, usize, usize), [i32; 4]> = BTreeMap::new();
    let keep = |best: &mut BTreeMap<u8, [i32; 4]>, index, rect: [i32; 4]| {
        let area = |r: &[i32; 4]| r[2] as i64 * r[3] as i64;
        if best.get(&index).is_none_or(|old| area(&rect) > area(old)) { best.insert(index, rect); }
    };
    for y in 0..height {
        let row = &pixels[y * stride..y * stride + width];
        let mut next = BTreeMap::new();
        let mut x = 0;
        while x < width {
            let (start, index) = (x, row[x]);
            x += 1;
            while x < width && row[x] == index { x += 1; }
            if index > last { continue; }
            let key = (index, start, x);
            let mut rect = active.remove(&key).unwrap_or([start as i32, y as i32, (x - start) as i32, 0]);
            rect[3] += 1;
            next.insert(key, rect);
        }
        for ((index, _, _), rect) in active { keep(&mut best, index, rect); }
        active = next;
    }
    for ((index, _, _), rect) in active { keep(&mut best, index, rect); }
    best
}

/// Where the native start screen tests its eleven text rows. Labels come
/// from the mirrored frame so their translated text and font determine
/// the width, as they do in 0x54ECA0 and 0x54ED90.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TitleLayout {
    pub origin: (i32, i32),
    pub row_height: i32,
    pub first: u32,
}

pub fn title_layout(mem: &dyn Mem, build: crate::Build) -> Option<TitleLayout> {
    if build != crate::Build::PcGames { return None; }
    // A nested New Game/Preferences/etc. dialog blocks the title behind it.
    if mem.u8(0xB4_25CA)? == 0 || mem.u32(0xCA_D384)? != 0 { return None; }
    let (width, height) = (mem.i32(0x9C_72F8)?, mem.i32(0x9C_72FC)?);
    let row_height = mem.i32(0xB3_E274)?;
    if !(1..=16384).contains(&width) || !(1..=16384).contains(&height) || !(1..=256).contains(&row_height) { return None; }
    Some(TitleLayout {
        origin: ((width - 1024) / 2 + mem.i32(0x72_C2A4)?, (height - 1024) / 2 + mem.i32(0x72_C2A8)?),
        row_height,
        first: u32::from(mem.u32(0xA5_2CE8)? == 0),
    })
}
