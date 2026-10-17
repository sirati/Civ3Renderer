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

/// An image of palette indices, one byte per pixel (see
/// `tile_render_shared::SpriteDesc::palette`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexImage {
    pub width: u32,
    pub height: u32,
    /// Row-major, `width * height` indices.
    pub pixels: Vec<u8>,
}

impl IndexImage {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, pixels: vec![0; (width * height) as usize] }
    }

    pub fn from_fn(width: u32, height: u32, mut f: impl FnMut(u32, u32) -> u8) -> Self {
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.push(f(x, y));
            }
        }
        Self { width, height, pixels }
    }

    pub fn get(&self, x: u32, y: u32) -> u8 {
        self.pixels[(y * self.width + x) as usize]
    }

    /// Copies `w × h` pixels of `src` starting at (`sx`, `sy`) to (`dx`, `dy`).
    pub fn blit(&mut self, src: &IndexImage, sx: u32, sy: u32, w: u32, h: u32, dx: u32, dy: u32) {
        for y in 0..h {
            let s = ((sy + y) * src.width + sx) as usize;
            let d = ((dy + y) * self.width + dx) as usize;
            self.pixels[d..d + w as usize].copy_from_slice(&src.pixels[s..s + w as usize]);
        }
    }

    /// The `w × h` region at (`x`, `y`) as a new image.
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> IndexImage {
        let mut out = IndexImage::new(w, h);
        out.blit(self, x, y, w, h, 0, 0);
        out
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

/// Where sprites go on atlas pages, without their pixels: left to right in
/// rows ("shelves") as tall as the tallest sprite on them, a new page when
/// one is full. Tile art comes in few distinct heights, which is the case
/// shelves handle well. Places only ever move forward, so what was placed
/// stays where it is when more is added (an atlas grows by appending).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shelves {
    pub page_size: u32,
    /// Pages begun.
    pub pages: u32,
    x: u32,
    y: u32,
    h: u32,
}

impl Shelves {
    pub fn new(page_size: u32) -> Self {
        Self { page_size, pages: 0, x: 0, y: 0, h: 0 }
    }

    /// A place for a `w × h` sprite: page, x, y.
    pub fn place(&mut self, w: u32, h: u32) -> Result<(u32, u32, u32), AtlasError> {
        if w > self.page_size || h > self.page_size {
            return Err(AtlasError::TooLarge(w, h, self.page_size));
        }
        if self.pages == 0 {
            self.new_page();
        }
        if self.x + w > self.page_size {
            self.y += self.h;
            self.x = 0;
            self.h = 0;
        }
        if self.y + h > self.page_size {
            self.new_page();
        }
        let at = (self.pages - 1, self.x, self.y);
        self.x += w;
        self.h = self.h.max(h);
        Ok(at)
    }

    fn new_page(&mut self) {
        self.pages += 1;
        (self.x, self.y, self.h) = (0, 0, 0);
    }
}

/// Builds an atlas with its pixels: colour sprites on RGBA pages, indexed
/// ones on index pages.
pub struct AtlasBuilder {
    page_size: u32,
    rgba: Shelves,
    index: Shelves,
    pages: Vec<RgbaImage>,
    index_pages: Vec<IndexImage>,
    sprites: Vec<SpriteDesc>,
}

impl AtlasBuilder {
    pub fn new(page_size: u32) -> Self {
        Self { page_size, rgba: Shelves::new(page_size), index: Shelves::new(page_size), pages: Vec::new(), index_pages: Vec::new(), sprites: Vec::new() }
    }

    fn push(&mut self, (page, x, y): (u32, u32, u32), (w, h): (u32, u32), anchor: (i32, i32)) -> SpriteId {
        let id = SpriteId(self.sprites.len() as u32);
        self.sprites.push(SpriteDesc {
            atlas_x: x,
            atlas_y: y,
            width: w,
            height: h,
            anchor_x: anchor.0,
            anchor_y: anchor.1,
            page,
            // Shrunk from the whole sprite (see `sprite_mut`).
            box_w: w,
            box_h: h,
            ..Default::default()
        });
        id
    }

    /// Adds `image`, to be drawn with its top-left corner at `anchor` relative
    /// to the cell origin.
    pub fn add(&mut self, image: &RgbaImage, anchor: (i32, i32)) -> Result<SpriteId, AtlasError> {
        let at = self.rgba.place(image.width, image.height)?;
        while self.pages.len() < self.rgba.pages as usize {
            self.pages.push(RgbaImage::new(self.page_size, self.page_size));
        }
        self.pages[at.0 as usize].blit(image, 0, 0, image.width, image.height, at.1, at.2);
        Ok(self.push(at, (image.width, image.height), anchor))
    }

    /// Adds an indexed sprite: `image`'s indices drawn through palette row
    /// `own`, those in `remap` (`SpriteDesc::remap`) through the palette a
    /// draw picks.
    pub fn add_indexed(&mut self, image: &IndexImage, anchor: (i32, i32), own: u32, remap: u32) -> Result<SpriteId, AtlasError> {
        let at = self.index.place(image.width, image.height)?;
        while self.index_pages.len() < self.index.pages as usize {
            self.index_pages.push(IndexImage::new(self.page_size, self.page_size));
        }
        self.index_pages[at.0 as usize].blit(image, 0, 0, image.width, image.height, at.1, at.2);
        let id = self.push(at, (image.width, image.height), anchor);
        (self.sprites[id.0 as usize].palette, self.sprites[id.0 as usize].remap) = (own, remap);
        Ok(id)
    }

    /// Another sprite of the pixels of sprite `of` (another colouring of
    /// the same indexed art, say), drawn with its top-left at `anchor`.
    pub fn alias(&mut self, of: SpriteId, anchor: (i32, i32)) -> SpriteId {
        let s = self.sprites[of.0 as usize];
        self.push((s.page, s.atlas_x, s.atlas_y), (s.width, s.height), anchor);
        let id = SpriteId(self.sprites.len() as u32 - 1);
        (self.sprites[id.0 as usize].palette, self.sprites[id.0 as usize].remap) = (s.palette, s.remap);
        id
    }

    /// Sprite `id`, to say how a view showing the world smaller draws it
    /// (its `scaling`, sample box and `downscaled` twin).
    pub fn sprite_mut(&mut self, id: SpriteId) -> &mut SpriteDesc {
        &mut self.sprites[id.0 as usize]
    }

    pub fn sprite(&self, id: SpriteId) -> &SpriteDesc {
        &self.sprites[id.0 as usize]
    }

    pub fn finish(self) -> Atlas {
        let mut pages = self.pages;
        if pages.is_empty() {
            pages.push(RgbaImage::new(1, 1));
        }
        Atlas { page_size: self.page_size, pages, index_pages: self.index_pages, sprites: self.sprites, palettes: no_palettes(), named_palettes: 1 }
    }
}

/// Packed sprites, ready to upload.
#[derive(Clone)]
pub struct Atlas {
    pub page_size: u32,
    /// The colour sprites' pages.
    pub pages: Vec<RgbaImage>,
    /// The indexed sprites' pages (see `SpriteDesc::palette`).
    pub index_pages: Vec<IndexImage>,
    pub sprites: Vec<SpriteDesc>,
    /// The palette rows indexed sprites are drawn through (see
    /// `tile_render_shared::PALETTE_SIZE`; row 0 all clear): RGBA8 colours,
    /// red in the low byte.
    pub palettes: Vec<u32>,
    /// The row of named palette 1 (`FrameParams::named_palettes`).
    pub named_palettes: u32,
}

/// The palette table with row 0 only (no palettes).
pub fn no_palettes() -> Vec<u32> {
    vec![0; tile_render_shared::PALETTE_SIZE as usize]
}

/// An RGBA8 colour as a palette entry.
pub fn palette_entry([r, g, b, a]: [u8; 4]) -> u32 {
    u32::from_le_bytes([r, g, b, a])
}

impl Atlas {
    /// Replaces the pixels of `sprite` (see `TileRenderer::patch_sprite`);
    /// `image` must have its size. Returns whether it was written.
    pub fn patch(&mut self, sprite: crate::SpriteId, image: &RgbaImage) -> bool {
        let Some(s) = self.sprites.get(sprite.0 as usize).copied() else { return false };
        let Some(page) = self.pages.get_mut(s.page as usize).filter(|_| s.palette == 0) else { return false };
        if (s.width, s.height) != (image.width, image.height) {
            return false;
        }
        page.blit(image, 0, 0, image.width, image.height, s.atlas_x, s.atlas_y);
        true
    }

    /// Bytes its pages take: colour pages, index pages.
    pub fn page_bytes(&self) -> (u64, u64) {
        let bytes = |w: u32, h: u32| w as u64 * h as u64;
        (self.pages.iter().map(|p| 4 * bytes(p.width, p.height)).sum(), self.index_pages.iter().map(|p| bytes(p.width, p.height)).sum())
    }
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
