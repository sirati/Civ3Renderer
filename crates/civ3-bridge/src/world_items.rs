//! World annotations with separate layouts for full-size and reduced worlds.
//!
//! Native reduced layouts keep their text and plates at the same pixel size.
//! Doubling only their world positions restores the native reduced view, but
//! breaks the annotation in a full-detail magnifier. Publish both layouts so
//! each camera selects the one matching its own world downscale.

use std::ops::Deref;
use crate::{text::Fonts, ui_screen::{self, Item}};
use render_desc::Compiled;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldItems {
    items: Vec<Item>,
    reduced_start: Option<usize>,
}

impl WorldItems {
    pub fn new(reduced: bool, mut layout: impl FnMut(bool) -> Vec<Item>) -> Self {
        let mut items = layout(false);
        let reduced_start = reduced.then_some(items.len());
        if reduced { items.extend(layout(true)); }
        Self { items, reduced_start }
    }

    /// An ordinary full-resolution layout, for previews built separately.
    pub fn full(items: Vec<Item>) -> Self { Self { items, reduced_start: None } }

    pub fn sprites(&self, fonts: &Fonts, compiled: &Compiled, pass: u32, outlines: bool) -> Vec<scene::FreeSprite> {
        let split = self.reduced_start.unwrap_or(self.items.len());
        let mut out = ui_screen::sprites(&self.items[..split], fonts, compiled, pass, outlines);
        for sprite in &mut out {
            sprite.screen = false;
            sprite.clip = None;
            if self.reduced_start.is_some() { sprite.category |= tile_render::CATEGORY_FULL_RESOLUTION; }
        }
        if self.reduced_start.is_some() {
            let mut reduced = ui_screen::sprites(&self.items[split..], fonts, compiled, pass, outlines);
            for sprite in &mut reduced {
                sprite.screen = false;
                sprite.clip = None;
                sprite.category |= tile_render::CATEGORY_REDUCED_RESOLUTION;
            }
            crate::scene_map::from_zoomed_out(&mut reduced);
            out.extend(reduced);
        }
        out
    }
}

impl Deref for WorldItems {
    type Target = [Item];
    fn deref(&self) -> &[Item] { &self.items }
}

impl<'a> IntoIterator for &'a WorldItems {
    type Item = &'a Item;
    type IntoIter = std::slice::Iter<'a, Item>;
    fn into_iter(self) -> Self::IntoIter { self.items.iter() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use menu::screen::{Element, ScreenArt};

    fn sample(reduced: bool) -> Vec<Item> {
        let (x, y) = if reduced { (35, 75) } else { (95, 115) };
        vec![Item::Element(Element::Fill { rect: [x, y, 40, 10], rgb: [80, 20, 10], alpha: 255, clip: [0, 0, 1000, 1000] }, true)]
    }

    fn compile(items: &WorldItems) -> Compiled {
        let mut art = ScreenArt::default();
        for item in items { if let Item::Element(element, _) = item { art.add(std::slice::from_ref(element)); } }
        let desc = format!("[grid]\nlayout = \"orthogonal\"\nstep = [1, 1]\n{}", ui_screen::description(&art));
        let files = civ3_assets::GameFiles::with_roots(Vec::new());
        crate::assets::compile_as(&desc, files, crate::assets::Pixels::Made(None), 128).unwrap()
    }

    #[test]
    fn each_camera_receives_an_intact_plate_layout_with_its_clean_map_bit() {
        let items = WorldItems::new(true, sample);
        let compiled = compile(&items);
        let mut sprites = items.sprites(&Fonts::default(), &compiled, 0, false);
        for sprite in &mut sprites { sprite.category |= viewer_settings::clean_map::CITIES; }
        assert_eq!(sprites.len(), 2);
        assert_eq!((sprites[0].x, sprites[0].y), (95, 115));
        assert_eq!((sprites[1].x, sprites[1].y), (70, 150));
        for downscale in [1, 2, 1, 2] {
            let hidden = tile_render::Camera { downscale, ..tile_render::Camera::new((100, 100)) }.effective_hidden_categories();
            let selected: Vec<_> = sprites.iter().filter(|s| s.category & hidden == 0).collect();
            assert_eq!(selected.len(), 1);
            let selected = selected[0];
            let plate = selected.solid.expect("world plates are GPU rectangle commands");
            assert_eq!((plate.width, plate.height), (40, 10), "KEEP_SIZE preserves native plate dimensions in reduced views");
            assert!(!selected.screen && selected.clip.is_none());
            assert_ne!(selected.category & viewer_settings::clean_map::CITIES, 0);
            let hidden = tile_render::Camera { downscale, hidden_categories: viewer_settings::clean_map::CITIES, ..tile_render::Camera::new((100, 100)) }.effective_hidden_categories();
            assert!(sprites.iter().all(|s| s.category & hidden != 0));
        }
    }

    #[test]
    fn ordinary_world_layouts_remain_unrestricted_and_do_not_request_reduced_art() {
        let items = WorldItems::new(false, |reduced| { assert!(!reduced); sample(false) });
        let compiled = compile(&items);
        let sprites = items.sprites(&Fonts::default(), &compiled, 0, false);
        assert_eq!(sprites.len(), 1);
        assert_eq!(sprites[0].category, 0);
        assert_eq!(viewer_settings::clean_map::ALL & (tile_render::CATEGORY_FULL_RESOLUTION | tile_render::CATEGORY_REDUCED_RESOLUTION), 0);
    }
}
