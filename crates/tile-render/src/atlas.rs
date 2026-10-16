//! Packing sprite images into atlas pages.

use tile_render_shared::SpriteDesc;

/// A straight-alpha RGBA8 image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// Row-major, `width * height` pixels.
    pub pixels: Vec<[u8; 4]>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, pixels: vec![[0; 4]; (width * height) as usize] }
    }

    pub fn from_fn(width: u32, height: u32, mut f: impl FnMut(u32, u32) -> [u8; 4]) -> Self {
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.push(f(x, y));
            }
        }
        Self { width, height, pixels }
    }

    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        self.pixels[(y * self.width + x) as usize]
    }

    pub fn put(&mut self, x: u32, y: u32, px: [u8; 4]) {
        self.pixels[(y * self.width + x) as usize] = px;
    }

    /// Copies `w × h` pixels of `src` starting at (`sx`, `sy`) to (`dx`, `dy`).
    pub fn blit(&mut self, src: &RgbaImage, sx: u32, sy: u32, w: u32, h: u32, dx: u32, dy: u32) {
        for y in 0..h {
            let s = ((sy + y) * src.width + sx) as usize;
            let d = ((dy + y) * self.width + dx) as usize;
            self.pixels[d..d + w as usize].copy_from_slice(&src.pixels[s..s + w as usize]);
        }
    }

    /// The `w × h` region at (`x`, `y`) as a new image.
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
        let mut out = RgbaImage::new(w, h);
        out.blit(self, x, y, w, h, 0, 0);
        out
    }

    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.pixels)
    }
}

/// Index of a sprite in the atlas.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpriteId(pub u32);

#[derive(Debug, thiserror::Error)]
pub enum AtlasError {
    #[error("sprite of {0}×{1} does not fit an atlas page of {2}×{2}")]
    TooLarge(u32, u32, u32),
}

/// Shelf packer: sprites are placed left to right in rows ("shelves") as
/// tall as the tallest sprite on them; a new page starts when one is full.
/// Tile art comes in few distinct heights, which is the case shelves handle
/// well.
pub struct AtlasBuilder {
    page_size: u32,
    pages: Vec<RgbaImage>,
    sprites: Vec<SpriteDesc>,
    shelf_x: u32,
    shelf_y: u32,
    shelf_h: u32,
}

impl AtlasBuilder {
    pub fn new(page_size: u32) -> Self {
        Self { page_size, pages: Vec::new(), sprites: Vec::new(), shelf_x: 0, shelf_y: 0, shelf_h: 0 }
    }

    /// Adds `image`, to be drawn with its top-left corner at `anchor` relative
    /// to the cell origin.
    pub fn add(&mut self, image: &RgbaImage, anchor: (i32, i32)) -> Result<SpriteId, AtlasError> {
        let (w, h) = (image.width, image.height);
        if w > self.page_size || h > self.page_size {
            return Err(AtlasError::TooLarge(w, h, self.page_size));
        }
        if self.pages.is_empty() {
            self.new_page();
        }
        if self.shelf_x + w > self.page_size {
            self.shelf_y += self.shelf_h;
            self.shelf_x = 0;
            self.shelf_h = 0;
        }
        if self.shelf_y + h > self.page_size {
            self.new_page();
        }
        let page = self.pages.len() - 1;
        let (x, y) = (self.shelf_x, self.shelf_y);
        self.pages[page].blit(image, 0, 0, w, h, x, y);
        self.shelf_x += w;
        self.shelf_h = self.shelf_h.max(h);

        let id = SpriteId(self.sprites.len() as u32);
        self.sprites.push(SpriteDesc {
            atlas_x: x,
            atlas_y: y,
            width: w,
            height: h,
            anchor_x: anchor.0,
            anchor_y: anchor.1,
            page: page as u32,
            _pad: 0,
        });
        Ok(id)
    }

    fn new_page(&mut self) {
        self.pages.push(RgbaImage::new(self.page_size, self.page_size));
        self.shelf_x = 0;
        self.shelf_y = 0;
        self.shelf_h = 0;
    }

    pub fn sprite(&self, id: SpriteId) -> &SpriteDesc {
        &self.sprites[id.0 as usize]
    }

    pub fn finish(self) -> Atlas {
        let mut pages = self.pages;
        if pages.is_empty() {
            pages.push(RgbaImage::new(1, 1));
        }
        Atlas { page_size: self.page_size, pages, sprites: self.sprites }
    }
}

/// Packed sprites, ready to upload.
pub struct Atlas {
    pub page_size: u32,
    pub pages: Vec<RgbaImage>,
    pub sprites: Vec<SpriteDesc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_shelves_and_pages_without_overlap() {
        let mut b = AtlasBuilder::new(64);
        let img = |w, h, v| RgbaImage::from_fn(w, h, |_, _| [v, v, v, 255]);
        let ids: Vec<_> = (0..10).map(|i| b.add(&img(30, 20, i as u8), (0, 0)).unwrap()).collect();
        let atlas = b.finish();
        // 2 per shelf, 3 shelves per page => 6 per page.
        assert_eq!(atlas.pages.len(), 2);
        for (i, id) in ids.iter().enumerate() {
            let s = atlas.sprites[id.0 as usize];
            let page = &atlas.pages[s.page as usize];
            assert_eq!(page.get(s.atlas_x, s.atlas_y)[0], i as u8);
            assert_eq!(page.get(s.atlas_x + 29, s.atlas_y + 19)[0], i as u8);
        }
    }
}
