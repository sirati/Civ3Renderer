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

    /// A sprite of pixels placed already (part of another sprite's, say):
    /// `desc` says where.
    pub fn add_desc(&mut self, desc: SpriteDesc) -> SpriteId {
        self.sprites.push(desc);
        SpriteId(self.sprites.len() as u32 - 1)
    }

    /// Sprite `id`, to say how a view showing the world smaller draws it
    /// (its `scaling`, sample box and `downscaled` twin).
    pub fn sprite_mut(&mut self, id: SpriteId) -> &mut SpriteDesc {
        &mut self.sprites[id.0 as usize]
    }

    pub fn sprite(&self, id: SpriteId) -> &SpriteDesc {
        &self.sprites[id.0 as usize]
    }

    /// Where the next colour and indexed sprites would go: for growing the
    /// finished atlas ([`AtlasGrowth`]).
    pub fn shelves(&self) -> (&Shelves, &Shelves) {
        (&self.rgba, &self.index)
    }

    pub fn finish(self) -> Atlas {
        let mut pages = self.pages;
        if pages.is_empty() {
            pages.push(RgbaImage::new(1, 1));
        }
        Atlas { page_size: self.page_size, pages, index_pages: self.index_pages, sprites: self.sprites, palettes: no_palettes(), named_palettes: 1 }
    }
}

/// A sprite of `size` at `(page, x, y)`, drawn with its top-left at
/// `anchor`, shrunk from the whole sprite by views showing the world
/// smaller (see `SpriteDesc::box_x`).
pub fn sprite_at((page, x, y): (u32, u32, u32), (w, h): (u32, u32), anchor: (i32, i32)) -> SpriteDesc {
    SpriteDesc { atlas_x: x, atlas_y: y, width: w, height: h, anchor_x: anchor.0, anchor_y: anchor.1, page, box_w: w, box_h: h, ..Default::default() }
}

/// Pixels written into an atlas page: a new sprite's (see [`AtlasGrowth`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blit {
    pub page: u32,
    pub x: u32,
    pub y: u32,
    pub pixels: BlitPixels,
}

/// What a [`Blit`] writes: colours on a colour page, palette indices on an
/// index page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlitPixels {
    Rgba(RgbaImage),
    Index(IndexImage),
}

impl Blit {
    /// Bytes it writes.
    pub fn bytes(&self) -> u64 {
        match &self.pixels {
            BlitPixels::Rgba(i) => i.pixels.len() as u64 * 4,
            BlitPixels::Index(i) => i.pixels.len() as u64,
        }
    }
}

/// Sprites appended to an atlas: the sprites before keep their ids and
/// places, the new ones take the next ids and go where nothing was (on the
/// pages after, or the free part of the last page, see [`Shelves`]). What
/// changes on a GPU is only what is new ([`Atlas::grow`],
/// `TileRenderer::grow_atlas`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AtlasGrowth {
    /// How many sprites the atlas has before (the id of the first new one).
    pub first: u32,
    /// The colour and index pages it uses after.
    pub pages: u32,
    pub index_pages: u32,
    pub sprites: Vec<SpriteDesc>,
    /// The new sprites' pixels (several sprites may share pixels, or show a
    /// part of an older sprite's: fewer blits than sprites).
    pub blits: Vec<Blit>,
    /// Palette rows appended to the table (whole rows).
    pub palettes: Vec<u32>,
}

impl AtlasGrowth {
    /// Bytes of pixels it writes.
    pub fn bytes(&self) -> u64 {
        self.blits.iter().map(Blit::bytes).sum()
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
    /// Appends `growth` (made for this atlas: its `first` is how many sprites
    /// this one has); with pixels (pages), its pixels too.
    pub fn grow(&mut self, growth: &AtlasGrowth) {
        debug_assert_eq!(growth.first as usize, self.sprites.len(), "a growth for another atlas");
        self.sprites.extend_from_slice(&growth.sprites);
        self.palettes.extend_from_slice(&growth.palettes);
        if self.pages.is_empty() {
            return;
        }
        // (A 1×1 page stands for none.)
        if self.pages.len() == 1 && self.pages[0].width != self.page_size && growth.pages > 0 {
            self.pages.clear();
        }
        while (self.pages.len() as u32) < growth.pages {
            self.pages.push(RgbaImage::new(self.page_size, self.page_size));
        }
        while (self.index_pages.len() as u32) < growth.index_pages {
            self.index_pages.push(IndexImage::new(self.page_size, self.page_size));
        }
        for b in &growth.blits {
            match &b.pixels {
                BlitPixels::Rgba(image) => {
                    if let Some(page) = self.pages.get_mut(b.page as usize) {
                        page.blit(image, 0, 0, image.width, image.height, b.x, b.y);
                    }
                }
                BlitPixels::Index(image) => {
                    if let Some(page) = self.index_pages.get_mut(b.page as usize) {
                        page.blit(image, 0, 0, image.width, image.height, b.x, b.y);
                    }
                }
            }
        }
    }

    /// Replaces the pixels of `sprite` (see `TileRenderer::patch_sprite`);
    /// `image` must have its size. Returns whether it was written.
    pub fn patch(&mut self, sprite: crate::SpriteId, image: &RgbaImage) -> bool {
        let Some(s) = self.sprites.get(sprite.0 as usize).copied() else { return false };
        let Some(page) = self.pages.get_mut(s.page as usize).filter(|_| s.palette == 0) else { return false };
        if (s.width, s.height) != (image.width, image.height) {
            return false;
        }
        page.blit(image, 0, 0, image.width, image.height, s.atlas_x, s.atlas_y);
        // (Its blank margins are not known any more.)
        self.sprites[sprite.0 as usize].blank = 0;
        true
    }

    /// Finds every sprite's blank margins (`SpriteDesc::blank`) from its
    /// pixels (none without them), once the palettes are set: texels of
    /// alpha 0, of an indexed sprite those whose entry of its own row has
    /// alpha 0 (not those of the range a draw may remap).
    pub fn trim_blank(&mut self) {
        let mut found: std::collections::HashMap<[u32; 7], u32> = std::collections::HashMap::new();
        for i in 0..self.sprites.len() {
            let s = self.sprites[i];
            let key = [s.page, s.atlas_x, s.atlas_y, s.width, s.height, s.palette, s.remap];
            let blank = match found.get(&key) {
                Some(&b) => b,
                None => {
                    let b = self.margins(&s);
                    found.insert(key, b);
                    b
                }
            };
            self.sprites[i].blank = blank;
        }
    }

    /// The blank margins of `s` (0 where its pixels are not here).
    fn margins(&self, s: &SpriteDesc) -> u32 {
        let (w, h) = (s.width, s.height);
        if w == 0 || h == 0 {
            return 0;
        }
        let blank: Box<dyn Fn(u32, u32) -> bool + '_> = if s.palette == 0 {
            let Some(page) = self.pages.get(s.page as usize).filter(|p| s.atlas_x + w <= p.width && s.atlas_y + h <= p.height) else { return 0 };
            Box::new(move |x, y| page.pixels[((s.atlas_y + y) * page.width + s.atlas_x + x) as usize][3] == 0)
        } else {
            let Some(page) = self.index_pages.get(s.page as usize).filter(|p| s.atlas_x + w <= p.width && s.atlas_y + h <= p.height) else { return 0 };
            let row = s.palette as usize * tile_render_shared::PALETTE_SIZE as usize;
            let Some(entries) = self.palettes.get(row..row + tile_render_shared::PALETTE_SIZE as usize) else { return 0 };
            let (lo, hi) = (s.remap & 0xFFFF, s.remap >> 16);
            Box::new(move |x, y| {
                let i = page.pixels[((s.atlas_y + y) * page.width + s.atlas_x + x) as usize] as u32;
                !(lo..hi).contains(&i) && entries[i as usize] >> 24 == 0
            })
        };
        let row_blank = |y: u32| (0..w).all(|x| blank(x, y));
        let top = (0..h).take_while(|&y| row_blank(y)).count() as u32;
        if top == h {
            return tile_render_shared::blank_margins(w, h, w, h);
        }
        let bottom = (top..h).rev().take_while(|&y| row_blank(y)).count() as u32;
        let column_blank = |x: u32| (top..h - bottom).all(|y| blank(x, y));
        let left = (0..w).take_while(|&x| column_blank(x)).count() as u32;
        let right = (left..w).rev().take_while(|&x| column_blank(x)).count() as u32;
        tile_render_shared::blank_margins(left, top, right, bottom)
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

    #[test]
    fn blank_margins_are_the_alpha_0_edges_and_go_with_new_pixels() {
        let mut b = AtlasBuilder::new(64);
        // Opaque in x 3..7, y 2..9 of 12 × 10 (one translucent texel counts).
        let img = RgbaImage::from_fn(12, 10, |x, y| if (3..7).contains(&x) && (2..9).contains(&y) { [9, 9, 9, if x == 3 { 1 } else { 255 }] } else { [7, 7, 7, 0] });
        let a = b.add(&img, (0, 0)).unwrap();
        let empty = b.add(&RgbaImage::new(4, 3), (0, 0)).unwrap();
        // Indexed: index 0 clear in its own row, 5 opaque, 200 remapped.
        let idx = IndexImage::from_fn(8, 4, |x, _| match x {
            1 => 5,
            6 => 200,
            _ => 0,
        });
        let i = b.add_indexed(&idx, (0, 0), 1, tile_render_shared::remap_range(100, 255)).unwrap();
        let mut atlas = b.finish();
        let mut palettes = no_palettes();
        palettes.extend((0..256u32).map(|k| if k == 0 || k >= 100 { 0 } else { 0xFF00_0000 }));
        atlas.palettes = palettes;
        atlas.trim_blank();
        assert_eq!(atlas.sprites[a.0 as usize].blank, tile_render_shared::blank_margins(3, 2, 5, 1));
        assert_eq!(atlas.sprites[empty.0 as usize].blank, tile_render_shared::blank_margins(4, 3, 4, 3));
        assert_eq!(atlas.sprites[i.0 as usize].blank, tile_render_shared::blank_margins(1, 0, 1, 0));
        let s = atlas.sprites[a.0 as usize];
        assert_eq!(tile_render_shared::drawn_part(&s, &tile_render_shared::placed(&s, 1)), (3, 2, 7, 9));
        // Every other texel from texel 1 on: pixels 1..3 show texels 3, 5.
        let half = tile_render_shared::Placed { texel_x: 1, texel_y: 0, step: 2, width: 5, height: 5, ..Default::default() };
        assert_eq!(tile_render_shared::drawn_part(&s, &half), (1, 1, 3, 5));
        assert!(atlas.patch(a, &RgbaImage::new(12, 10)));
        assert_eq!(atlas.sprites[a.0 as usize].blank, 0);
    }
}
