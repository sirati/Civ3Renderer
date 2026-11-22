//! The setup screens' option masks, answered from their fills.
//!
//! The world setup and player setup screens draw their choices (world size,
//! climate, civilization, difficulty, rules) into their canvas, and fill one
//! rectangle per choice into a separate 8-bit image with the choice's index
//! (`civ3_sys::controls::setup::mask_form`). Their hover and click handlers
//! read that image's pixel under the mouse (`PCX_Image::get_pixel`) to know
//! the choice, and our control walk reads the same image for the choices'
//! rectangles. Without jgl.dll no fill writes pixels, so every lookup read
//! the image's empty memory: index 0 (the smallest world, no civilization).
//!
//! The game creates each mask at screen size and fills it whole with 255
//! (0x5DD4B2, 0x5819D6), then fills the offered choices (0x5FFEF0). A
//! pixel's value is the last fill over it, so the fills are kept as
//! rectangles and answer both readers.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;

/// At most this many rectangles a mask keeps (a screen fills a few dozen).
const MAX_FILLS: usize = 1024;

/// What a pixel no fill covered reads as: not a choice.
const NONE: u8 = 0xFF;

/// The rectangles filled into one mask, oldest first.
#[derive(Clone, Debug, Default)]
pub struct Mask {
    /// (left, top, right, bottom) and the index.
    fills: Vec<([i32; 4], u8)>,
    size: (i32, i32),
    /// Each index's largest rectangle (x, y, width, height), until the next fill.
    regions: OnceCell<Vec<(u8, [i32; 4])>>,
}

impl Mask {
    /// A fill of `rect` (left, top, right, bottom; within the image) with
    /// `index` into an image of `size`. One covering the whole image
    /// starts the mask anew.
    pub fn fill(&mut self, rect: [i32; 4], index: u8, size: (i32, i32)) {
        let rect = [rect[0].max(0), rect[1].max(0), rect[2].min(size.0), rect[3].min(size.1)];
        if rect[2] <= rect[0] || rect[3] <= rect[1] {
            return;
        }
        if size != self.size || rect == [0, 0, size.0, size.1] {
            self.fills.clear();
        }
        self.size = size;
        self.regions.take();
        self.fills.push((rect, index));
        if self.fills.len() > MAX_FILLS {
            self.fills.drain(..self.fills.len() - MAX_FILLS);
        }
    }

    /// The pixel at (x, y): the last fill's index there (`None` where none
    /// was, or off the image).
    pub fn index(&self, x: i32, y: i32) -> Option<u8> {
        if x < 0 || y < 0 || x >= self.size.0 || y >= self.size.1 {
            return None;
        }
        self.fills.iter().rev().find(|(r, _)| x >= r[0] && y >= r[1] && x < r[2] && y < r[3]).map(|&(_, i)| i)
    }

    /// Each index's largest rectangle of its own pixels, as the control
    /// walk takes it from the pixels (`setup::regions`), 255 excluded.
    pub fn regions(&self) -> Vec<(u8, [i32; 4])> {
        self.regions.get_or_init(|| {
            let (w, h) = (self.size.0.max(0) as usize, self.size.1.max(0) as usize);
            let mut pixels = vec![NONE; w * h];
            for &([l, t, r, b], index) in &self.fills {
                for y in t as usize..b as usize {
                    pixels[y * w + l as usize..y * w + r as usize].fill(index);
                }
            }
            civ3_sys::controls::setup::regions(&pixels, w, h, w, NONE - 1).into_iter().collect()
        }).clone()
    }
}

thread_local! {
    /// The masks by image (`PCX_Image*`), on the game's thread.
    static MASKS: RefCell<HashMap<u32, Mask>> = RefCell::new(HashMap::new());
}

/// Whether `image` is a setup screen's option mask.
#[cfg(windows)]
fn is_mask(image: u32) -> bool {
    civ3_sys::controls::setup::mask_form(&civ3_sys::controls::Live, image).is_some()
}

/// A fill the game made (jgl's +0x44): kept where it is into a mask.
#[cfg(windows)]
pub(crate) fn fill(call: &jgl::FillCall) {
    let jgl::FillCall::Fill { target, rect, color } = *call else { return };
    if !is_mask(target.owner) {
        return;
    }
    let size = target.size();
    let c = target.clip;
    MASKS.with_borrow_mut(|masks| {
        let mask = masks.entry(target.owner).or_default();
        for r in target.fill_regions(rect) {
            let clipped = [r.left.max(c.left), r.top.max(c.top), r.right.min(c.right), r.bottom.min(c.bottom)];
            mask.fill(clipped, color as u8, size);
        }
    });
}

/// The image lost its pixels (or is another's now).
#[cfg(windows)]
pub(crate) fn cleared(image: u32) {
    MASKS.with_borrow_mut(|masks| {
        masks.remove(&image);
    });
}

/// The game's `get_pixel` of `image` at (x, y), where it is a mask and a
/// fill covers the point.
#[cfg(windows)]
pub(crate) fn pixel(image: u32, x: i32, y: i32) -> Option<u32> {
    if !is_mask(image) {
        return None;
    }
    MASKS.with_borrow(|masks| masks.get(&image)?.index(x, y)).map(u32::from)
}

/// The choices' rectangles of the mask `image` (x, y, width, height), if
/// the game filled it.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn regions(image: u32) -> Option<Vec<(u8, [i32; 4])>> {
    MASKS.with_borrow(|masks| masks.get(&image).map(Mask::regions))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The world setup's way: all 255, then each choice's rectangle.
    fn world() -> Mask {
        let mut m = Mask::default();
        let size = (400, 300);
        m.fill([0, 0, 400, 300], 255, size);
        m.fill([112, 40, 278, 62], 0, size);
        m.fill([112, 62, 278, 84], 1, size);
        m.fill([120, 70, 140, 80], 7, size);
        m
    }

    #[test]
    fn a_choice_is_the_last_fill_under_the_point_and_a_whole_fill_starts_anew() {
        let mut m = world();
        let size = (400, 300);
        assert_eq!(m.index(150, 50), Some(0));
        assert_eq!(m.index(150, 70), Some(1));
        assert_eq!(m.index(130, 75), Some(7), "the later fill over the earlier");
        assert_eq!(m.index(10, 10), Some(255), "no choice");
        assert_eq!(m.index(400, 10), None, "off the image");
        // Rebuilt (a choice no longer offered): the old fills are gone.
        m.fill([0, 0, 400, 300], 255, size);
        m.fill([112, 40, 278, 62], 0, size);
        assert_eq!(m.index(150, 70), Some(255));
        // An empty rectangle changes nothing.
        m.fill([50, 50, 50, 60], 9, size);
        assert_eq!(m.index(50, 55), Some(255));
    }

    #[test]
    fn the_regions_are_those_the_control_walk_finds_in_the_same_pixels() {
        let mut m = world();
        let regions: HashMap<u8, [i32; 4]> = m.regions().into_iter().collect();
        assert_eq!(regions.get(&0), Some(&[112, 40, 166, 22]));
        assert_eq!(regions.get(&7), Some(&[120, 70, 20, 10]));
        // Index 1 is cut by 7: its largest own rectangle, not its box.
        let one = regions[&1];
        assert!(one[2] * one[3] < 166 * 22 && one[2] * one[3] > 0, "{one:?}");
        assert!(!regions.contains_key(&255), "255 is no choice");
        // A later fill is seen.
        m.fill([0, 200, 50, 220], 3, (400, 300));
        assert!(m.regions().contains(&(3, [0, 200, 50, 20])));
    }
}
