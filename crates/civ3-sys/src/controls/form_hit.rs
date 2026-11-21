//! The form the mouse acts on, decided by us: the game's hit test
//! (`0x609030`, its recursive form test `0x608C70`) with every pixel read
//! taken out (docs §16.11). The game's own test compares canvas pixels with
//! the colour key twice: the frame pieces of a framed form (`0x608E0A`,
//! `0x608E7E`) and the canvas of a colour-keyed form (`0x608FE3`). Neither
//! is read here. A frame band takes every point on it, and a keyed form
//! takes a point where the caller says something is drawn (its recorded
//! draws, what the renderer shows), so no canvas pixel is ever needed.
//!
//! Everything else is the game's own rule, kept exactly: a mouse capture
//! takes everything; a modal form tests its tree and its companion's, and
//! a miss still goes to it; otherwise the roots from the topmost, first hit
//! wins; children before their parent; the hit form passes the event to
//! its receiver (`+0xB0`) unless the point is on a frame. The handlers the
//! game then calls (buttons, lists, screens) are the game's.

use super::{Mem, CHILDREN, CHILD_COUNT, CLIENT, FRAME_RELATIVE, KEYED, LEFT, MAX_CHILDREN, MAX_DEPTH, NO_MOUSE, PARENT, RELATIVE, STATUS1, STATUS2};
use crate::Build;

/// [`Mem`] that can also write (the coordinates and the frame flag).
pub trait HitMem: Mem {
    fn write_u32(&self, addr: u32, value: u32) -> Option<()>;
}

/// Status1 with [`KEYED`]: a rectangle-only test after all.
const RECT_ONLY: u32 = 0x100;
/// Status2: framed (an outer rectangle `+0x1BC` around the form's own).
const FRAMED: u32 = 0x2;
/// The form a form's events go to (itself, mostly).
const RECEIVER: u32 = 0xB0;
/// A form's canvas (`PCX_Image`, embedded).
pub const CANVAS: u32 = 0x274;

/// Where the hit test keeps its state (PCGames).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HitSites {
    /// Mouse captures, tested in this order, with whether the point is
    /// given in the form's own pixels (false: its outer rectangle's).
    pub captures: [(u32, bool); 4],
    pub modal: u32,
    pub companion: u32,
    pub roots: u32,
    pub root_count: u32,
    /// 1 when the point is on a form's frame.
    pub on_frame: u32,
    /// The screen point being tested (x, then y).
    pub screen: u32,
}

pub fn hit_sites(build: Build) -> Option<HitSites> {
    match build {
        Build::PcGames => Some(HitSites {
            captures: [(0xCA_D370, true), (0xCA_D374, false), (0xCA_D360, true), (0xCA_D364, false)],
            modal: 0xCA_D384,
            companion: 0xCA_D380,
            roots: 0xCA_CB10,
            root_count: 0xCA_D3C4,
            on_frame: 0xCA_D348,
            screen: 0xCA_C2F0,
        }),
        Build::Gog | Build::Steam => None,
    }
}

/// The game's hit test entry (`Base_Form* cdecl (int *x, int *y)`) and its
/// first instructions (`push ebx; push ebp; push esi; mov esi,[esp+0x10]`).
pub fn site(build: Build) -> Option<(u32, &'static [u8])> {
    match build {
        Build::PcGames => Some((0x60_9030, &[0x53, 0x55, 0x56, 0x8B, 0x74, 0x24, 0x10])),
        Build::Gog | Build::Steam => None,
    }
}

/// What the hit test found: the form (0: none), and the point in its
/// pixels (or its outer rectangle's, on a frame); `point` is unchanged on
/// a miss, as in the game.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub form: u32,
    pub point: (i32, i32),
    pub on_frame: bool,
}

struct Walk<'a> {
    mem: &'a dyn HitMem,
    screen: (i32, i32),
    /// The frame flag as last written (`None`: not written).
    on_frame: Option<bool>,
    /// Whether a keyed form takes (x, y) of its own pixels.
    keyed: &'a dyn Fn(u32, i32, i32) -> bool,
}

/// The form under screen point `point` that the mouse acts on, and the
/// point in its pixels, as `0x609030` decides it but without reading a
/// pixel (module docs). Writes the frame flag and the tested screen point
/// where the game keeps them. `keyed(form, x, y)`: whether something is
/// drawn on colour-keyed `form` at (x, y) of its pixels.
pub fn form_under(mem: &dyn HitMem, sites: &HitSites, point: (i32, i32), keyed: &dyn Fn(u32, i32, i32) -> bool) -> Hit {
    let _ = mem.write_u32(sites.screen, point.0 as u32);
    let _ = mem.write_u32(sites.screen + 4, point.1 as u32);
    let mut walk = Walk { mem, screen: point, on_frame: None, keyed };
    let result = walk.target(sites, point);
    if let Some(on) = walk.on_frame {
        let _ = mem.write_u32(sites.on_frame, on as u32);
    }
    result
}

impl Walk<'_> {
    fn i32(&self, at: u32) -> i32 {
        self.mem.i32(at).unwrap_or(0)
    }

    fn u32(&self, at: u32) -> u32 {
        self.mem.u32(at).unwrap_or(0)
    }

    fn hit(&mut self, form: u32, point: (i32, i32), on_frame: bool) -> Hit {
        self.on_frame = Some(on_frame);
        Hit { form, point, on_frame }
    }

    fn target(&mut self, sites: &HitSites, point: (i32, i32)) -> Hit {
        // A capture of a form no longer shown is dropped (0x609069).
        for &(at, own) in &sites.captures {
            let form = self.u32(at);
            if form == 0 {
                continue;
            }
            if self.u32(form + STATUS2) & 1 == 0 {
                let _ = self.mem.write_u32(at, 0);
                continue;
            }
            let p = if own { self.to_local(form, point, 0) } else { self.to_outer(form, point) };
            return self.hit(form, p, !own);
        }
        let modal = self.u32(sites.modal);
        let found = if modal != 0 {
            let in_modal = self.to_outer(modal, point);
            match self.form(modal, in_modal, 0) {
                Some(hit) => hit,
                None => {
                    let companion = self.u32(sites.companion);
                    let hit = (companion != 0).then(|| {
                        let p = self.to_outer(companion, point);
                        self.form(companion, p, 0)
                    }).flatten();
                    match hit {
                        Some(hit) => hit,
                        // A miss still goes to the modal form.
                        None => {
                            let p = self.to_local(modal, point, 0);
                            self.hit(modal, p, false)
                        }
                    }
                }
            }
        } else {
            let count = self.i32(sites.root_count).clamp(0, MAX_CHILDREN as i32) as u32;
            let hit = (0..count).find_map(|i| {
                let root = self.u32(sites.roots + 4 * i);
                (root != 0).then(|| self.form(root, point, 0)).flatten()
            });
            match hit {
                Some(hit) => hit,
                None => return Hit { form: 0, point, on_frame: self.on_frame.unwrap_or(false) },
            }
        };
        if found.on_frame { found } else { Hit { form: self.u32(found.form + RECEIVER), ..found } }
    }

    /// Screen to the form's own pixels (`0x604A90`).
    fn to_local(&self, form: u32, (mut x, mut y): (i32, i32), depth: u32) -> (i32, i32) {
        x -= self.i32(form + CLIENT) + self.i32(form + LEFT);
        y -= self.i32(form + CLIENT + 4) + self.i32(form + LEFT + 4);
        let status = self.u32(form + STATUS1);
        let parent = self.u32(form + PARENT);
        if status & RELATIVE != 0 && parent != 0 && depth < MAX_DEPTH {
            (x, y) = self.to_local(parent, (x, y), depth + 1);
            if status & FRAME_RELATIVE != 0 {
                x += self.i32(parent + LEFT);
                y += self.i32(parent + LEFT + 4);
            }
        }
        (x, y)
    }

    /// Screen to the form's outer rectangle (`0x604B60`).
    fn to_outer(&self, form: u32, point: (i32, i32)) -> (i32, i32) {
        let (x, y) = self.to_local(form, point, 0);
        (x + self.i32(form + LEFT), y + self.i32(form + LEFT + 4))
    }

    /// The form and every parent shown (`0x609AC0`).
    fn shown(&self, form: u32) -> bool {
        let mut f = form;
        for _ in 0..=MAX_DEPTH {
            if self.u32(f + STATUS2) & 1 == 0 {
                return false;
            }
            f = self.u32(f + PARENT);
            if f == 0 {
                return true;
            }
        }
        false
    }

    /// Half-open `left <= x < right`, `top <= y < bottom` (`0x60ECE0`).
    fn inside(&self, (x, y): (i32, i32), at: u32) -> bool {
        x >= self.i32(at) && x < self.i32(at + 8) && y >= self.i32(at + 4) && y < self.i32(at + 12)
    }

    fn children(&self, form: u32) -> Vec<u32> {
        let n = self.i32(form + CHILD_COUNT).clamp(0, MAX_CHILDREN as i32) as u32;
        let array = self.u32(form + CHILDREN);
        if array == 0 {
            return Vec::new();
        }
        (0..n).map(|i| self.u32(array + 4 * i)).filter(|&c| c != 0 && c != form).collect()
    }

    /// The recursive test (`0x608C70`) of `form` at `point` (in its
    /// parent's pixels; the screen's for a form placed absolutely).
    fn form(&mut self, form: u32, point: (i32, i32), depth: u32) -> Option<Hit> {
        if depth > MAX_DEPTH || !self.shown(form) {
            return None;
        }
        let status1 = self.u32(form + STATUS1);
        let status2 = self.u32(form + STATUS2);
        let mut p = if status1 & RELATIVE == 0 { self.screen } else { point };
        let children = self.children(form);
        // Children placed absolutely first, in order.
        for &child in &children {
            if self.u32(child + STATUS1) & RELATIVE == 0 {
                if let Some(hit) = self.form(child, p, depth + 1) {
                    return Some(hit);
                }
            }
        }
        let receives = self.u32(form + RECEIVER) == form;
        if status2 & FRAMED != 0 {
            if !self.inside(p, form + CLIENT) {
                return None;
            }
            p = (p.0 - self.i32(form + CLIENT), p.1 - self.i32(form + CLIENT + 4));
            for &child in &children {
                let s = self.u32(child + STATUS1);
                if receives && s & FRAME_RELATIVE != 0 && s & RELATIVE != 0 {
                    if let Some(hit) = self.form(child, p, depth + 1) {
                        return Some(hit);
                    }
                }
            }
            if !self.inside(p, form + LEFT) {
                // The frame band: the game tests its pieces' pixels; every
                // point on it is the frame's here.
                return Some(self.hit(form, p, true));
            }
        } else if !self.inside(p, form + LEFT) {
            return None;
        }
        p = (p.0 - self.i32(form + LEFT), p.1 - self.i32(form + LEFT + 4));
        self.on_frame = Some(false);
        for &child in &children {
            let s = self.u32(child + STATUS1);
            if receives && s & FRAME_RELATIVE == 0 && s & RELATIVE != 0 {
                if let Some(hit) = self.form(child, p, depth + 1) {
                    return Some(hit);
                }
            }
        }
        if status1 & NO_MOUSE != 0 {
            return None;
        }
        if status1 & KEYED != 0 && status1 & RECT_ONLY == 0 && !(self.keyed)(form, p.0, p.1) {
            return None;
        }
        Some(self.hit(form, p, false))
    }
}

#[cfg(test)]
#[path = "form_hit_tests.rs"]
mod tests;
