//! The producer (what slot bodies call: append a record) and the consumer
//! (decode records, keep jgl's state, call the hooks), and [`Jgl`], both
//! joined with the buffer between them drained after every call.
//!
//! The split is the one the ring of `docs/jgl-replacement.md` §5 needs:
//! the producer keeps no more state than it must to know what to ask the
//! shim (which images and fonts it has seen), the consumer has the rest.

use std::collections::HashSet;

use crate::hooks::{Hooks, PixelPolicy, TextDraw, TextHooks};
use crate::record::{Record, Records, Writer};
use crate::text::{FontDesc, TextColor, TextState};
use crate::{FontId, ImageId, Rect};

/// The slot bodies' side.
pub struct Producer {
    writer: Writer,
    images: HashSet<ImageId>,
    fonts: HashSet<FontId>,
    policy: Box<dyn PixelPolicy>,
}

impl Producer {
    fn new(policy: Box<dyn PixelPolicy>) -> Producer {
        Producer { writer: Writer::new(), images: HashSet::new(), fonts: HashSet::from([FontId::STOCK]), policy }
    }

    /// Whether `font` must be defined (from GDI, [`Producer::font_defined`])
    /// before it is selected: created before the takeover.
    pub fn needs_font(&self, font: FontId) -> bool {
        !self.fonts.contains(&font)
    }

    /// Whether the DC state of `image` must be read (and given to
    /// [`Producer::dc_state`]) before its next text call: its DC predates
    /// the takeover.
    pub fn needs_dc(&self, image: ImageId) -> bool {
        !self.images.contains(&image)
    }

    pub fn font_created(&mut self, font: FontId, height: i32, flags: u32, face: &[u8]) {
        self.fonts.insert(font);
        self.writer.push(&Record::FontCreated { font, height, flags, face });
    }

    pub fn font_defined(&mut self, font: FontId, desc: &FontDesc) {
        self.fonts.insert(font);
        let face = desc.face.chars().map(|c| c as u32 as u8).collect::<Vec<u8>>();
        self.writer.push(&Record::FontDefined { font, height: desc.height, weight: desc.weight, style: desc.style_bits(), face: &face });
    }

    pub fn font_released(&mut self, font: FontId) {
        self.fonts.remove(&font);
        self.writer.push(&Record::FontReleased { font });
    }

    pub fn image_reset(&mut self, image: ImageId) {
        self.images.insert(image);
        self.writer.push(&Record::ImageReset { image });
    }

    pub fn dc_state(&mut self, image: ImageId, font: &FontDesc, colorref: u32) {
        self.images.insert(image);
        let face = font.face.chars().map(|c| c as u32 as u8).collect::<Vec<u8>>();
        self.writer.push(&Record::DcState { image, colorref, height: font.height, weight: font.weight, style: font.style_bits(), face: &face });
    }

    pub fn select_font(&mut self, image: ImageId, font: FontId) {
        self.writer.push(&Record::SelectFont { image, font });
    }

    pub fn text_color(&mut self, image: ImageId, colorref: u32) {
        self.writer.push(&Record::TextColor { image, colorref });
    }

    /// Returns whether jgl's original `text_out` is to run.
    pub fn text(&mut self, image: ImageId, owner: u32, x: i32, y: i32, clip: Rect, bytes: &[u8]) -> bool {
        self.writer.push(&Record::Text { image, owner, x, y, clip, bytes });
        self.policy.original_pixels(image, owner)
    }

    /// The records not yet consumed.
    pub fn pending(&self) -> &[u8] {
        self.writer.bytes()
    }
}

/// The reader's side.
pub struct Consumer {
    text: TextState,
    hooks: Box<dyn TextHooks>,
    /// Malformed buffers seen (logged once).
    pub errors: u32,
    /// Text records whose image had no state (should not happen: the
    /// producer asks for it).
    pub stateless: u32,
}

impl Consumer {
    pub fn new(hooks: Box<dyn TextHooks>, stock: FontDesc) -> Consumer {
        Consumer { text: TextState::new(stock), hooks, errors: 0, stateless: 0 }
    }

    pub fn text_state(&self) -> &TextState {
        &self.text
    }

    /// Applies the records of `bytes` in order.
    pub fn consume(&mut self, bytes: &[u8]) {
        for r in Records::new(bytes) {
            match r {
                Ok(r) => self.apply(r),
                Err(e) => {
                    if self.errors == 0 {
                        log::error!("jgl: malformed record at byte {}", e.at);
                    }
                    self.errors += 1;
                }
            }
        }
    }

    fn define(&mut self, font: FontId, desc: FontDesc) {
        self.hooks.font(&desc);
        self.text.define_font(font, desc);
    }

    fn apply(&mut self, r: Record<'_>) {
        match r {
            Record::FontCreated { font, height, flags, face } => self.define(font, FontDesc::from_create(face, height, flags)),
            Record::FontDefined { font, height, weight, style, face } => self.define(font, desc(face, height, weight, style)),
            Record::FontReleased { font } => self.text.release_font(font),
            Record::ImageReset { image } => self.text.reset(image),
            Record::SelectFont { image, font } => {
                if !self.text.select(image, font) {
                    log::warn!("jgl: image {:#x} selects unknown font {:#x}", image.0, font.0);
                }
            }
            Record::TextColor { image, colorref } => self.text.set_color(image, TextColor::from_colorref(colorref)),
            Record::DcState { image, colorref, height, weight, style, face } => {
                let font = desc(face, height, weight, style);
                self.hooks.font(&font);
                self.text.set_dc(image, font, TextColor::from_colorref(colorref));
            }
            Record::Text { image, owner, x, y, clip, bytes } => {
                let Some(dc) = self.text.dc(image) else {
                    self.stateless += 1;
                    return;
                };
                let (font, color) = (dc.font.clone(), dc.color);
                self.hooks.text(&TextDraw { image, owner, x, y, bytes, font: &font, color, clip });
            }
        }
    }
}

fn desc(face: &[u8], height: i32, weight: i32, style: u32) -> FontDesc {
    FontDesc { face: crate::text::latin1(face), height, weight, italic: style & 1 != 0, underline: style & 2 != 0 }
}

/// Producer and consumer of one thread, joined: every call is consumed
/// before it returns (the migration's ordering, `docs/jgl-replacement.md`
/// §5). A tap sees every batch of records first (dumps for offline replay).
pub struct Jgl {
    pub producer: Producer,
    pub consumer: Consumer,
    tap: Option<Box<dyn FnMut(&[u8])>>,
}

impl Jgl {
    /// `stock`: GDI's `SYSTEM_FONT`, as the shim reads it.
    pub fn new(hooks: Hooks, stock: FontDesc) -> Jgl {
        let mut producer = Producer::new(hooks.policy);
        producer.font_defined(FontId::STOCK, &stock);
        let mut jgl = Jgl { producer, consumer: Consumer::new(hooks.text, stock), tap: None };
        jgl.flush();
        jgl
    }

    pub fn set_tap(&mut self, tap: Option<Box<dyn FnMut(&[u8])>>) {
        self.tap = tap;
    }

    /// Hands the pending records to the consumer.
    pub fn flush(&mut self) {
        if self.producer.writer.is_empty() {
            return;
        }
        if let Some(tap) = self.tap.as_mut() {
            tap(self.producer.writer.bytes());
        }
        self.consumer.consume(self.producer.writer.bytes());
        self.producer.writer.clear();
    }

    /// Runs `f` on the producer, then consumes what it appended.
    pub fn with<R>(&mut self, f: impl FnOnce(&mut Producer) -> R) -> R {
        let r = f(&mut self.producer);
        self.flush();
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Seen(Rc<RefCell<Vec<(u32, i32, i32, String, FontDesc, TextColor)>>>);

    impl TextHooks for Seen {
        fn text(&mut self, d: &TextDraw<'_>) {
            self.0.borrow_mut().push((d.owner, d.x, d.y, crate::text::latin1(d.bytes), d.font.clone(), d.color));
        }
    }

    struct Skip;
    impl PixelPolicy for Skip {
        fn original_pixels(&self, _: ImageId, owner: u32) -> bool {
            owner == 0
        }
    }

    fn system() -> FontDesc {
        FontDesc { face: "System".into(), height: 16, weight: 700, italic: false, underline: false }
    }

    #[test]
    fn text_arrives_with_the_selected_font_and_colour() {
        let seen = Seen::default();
        let out = seen.0.clone();
        let mut j = Jgl::new(Hooks { text: Box::new(seen), policy: Box::new(Skip) }, system());
        let (img, font) = (ImageId(0x5000), FontId(0x6000));
        j.with(|p| p.image_reset(img));
        assert!(!j.with(|p| p.text(img, 0x7000, 1, 2, Rect::default(), b"a")));
        j.with(|p| p.font_created(font, 14, 1, b"Times New Roman"));
        j.with(|p| p.select_font(img, font));
        j.with(|p| p.text_color(img, 0x0000_FF00));
        assert!(j.with(|p| p.text(img, 0, 3, 4, Rect::default(), b"b")));
        j.with(|p| p.select_font(img, FontId::STOCK));
        j.with(|p| p.text_color(img, 0x10FF_1234));
        j.with(|p| p.text(img, 0x7000, 5, 6, Rect::default(), b"c"));
        let out = out.borrow();
        assert_eq!(out[0], (0x7000, 1, 2, "a".into(), system(), TextColor::BLACK));
        assert_eq!(out[1].4, FontDesc::from_create(b"Times New Roman", 14, 1));
        assert_eq!(out[1].5, TextColor::Rgb([0, 0xFF, 0]));
        assert_eq!((out[2].4.clone(), out[2].5), (system(), TextColor::Index(0x1234)));
    }

    #[test]
    fn the_producer_asks_for_state_it_has_not_seen() {
        let mut j = Jgl::new(Hooks::default(), system());
        let (img, font) = (ImageId(1), FontId(2));
        assert!(j.producer.needs_dc(img) && j.producer.needs_font(font));
        assert!(!j.producer.needs_font(FontId::STOCK));
        j.with(|p| p.dc_state(img, &system(), 0xFF));
        j.with(|p| p.font_defined(font, &system()));
        assert!(!j.producer.needs_dc(img) && !j.producer.needs_font(font));
        j.with(|p| p.font_released(font));
        assert!(j.producer.needs_font(font));
        assert_eq!(j.consumer.text_state().dc(img).unwrap().color, TextColor::Rgb([0xFF, 0, 0]));
    }

    #[test]
    fn the_tap_sees_the_stream() {
        let bytes = Rc::new(RefCell::new(Vec::new()));
        let b = bytes.clone();
        let mut j = Jgl::new(Hooks::default(), system());
        j.set_tap(Some(Box::new(move |r| b.borrow_mut().extend_from_slice(r))));
        j.with(|p| p.image_reset(ImageId(9)));
        j.with(|p| p.text(ImageId(9), 0, 0, 0, Rect::default(), b"x"));
        let n = Records::new(&bytes.borrow()).count();
        assert_eq!(n, 2);
    }
}
