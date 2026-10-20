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
        if !growth.palettes.is_empty() {
            let mut palettes = self.palettes.clone();
            palettes.extend_from_slice(&growth.palettes);
            palette_bounds(&mut self.sprites, &self.palettes, &palettes, self.named_palettes);
            self.palettes = palettes;
        }
        self.sprites.extend_from_slice(&growth.sprites);
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
        patch_bounds(&mut self.sprites, &s, image);
        true
    }

    /// Finds every sprite's blank margins (`SpriteDesc::blank`) from its
    /// pixels (none without them), once the palettes are set: texels of
    /// alpha 0, of an indexed sprite those whose entry of its own row has
    /// alpha 0 (not those of the range a draw may remap); and whether its
    /// texels are all opaque or clear (remapped ones as every named
    /// palette has them).
    pub fn trim_blank(&mut self) {
        let named_alpha = named_alpha(&self.palettes, self.named_palettes);
        let mut found: std::collections::HashMap<[u32; 7], (u32, u32)> = std::collections::HashMap::new();
        for i in 0..self.sprites.len() {
            let s = self.sprites[i];
            let key = [s.page, s.atlas_x, s.atlas_y, s.width, s.height, s.palette, s.remap];
            let (blank, cuts) = match found.get(&key) {
                Some(&b) => b,
                None => {
                    let (rgba, index) = (self.pages.get(s.page as usize), self.index_pages.get(s.page as usize));
                    let b = margins(&s, &named_alpha, &self.palettes, rgba.map(|p| (p, s.atlas_x, s.atlas_y)), index.map(|p| (p, s.atlas_x, s.atlas_y)));
                    found.insert(key, b);
                    b
                }
            };
            (self.sprites[i].blank, self.sprites[i].cuts) = (blank, cuts);
        }
    }

    /// Bytes its pages take: colour pages, index pages.
    pub fn page_bytes(&self) -> (u64, u64) {
        let bytes = |w: u32, h: u32| w as u64 * h as u64;
        (self.pages.iter().map(|p| 4 * bytes(p.width, p.height)).sum(), self.index_pages.iter().map(|p| bytes(p.width, p.height)).sum())
    }
}

/// Updates every colour sprite whose texels a patch touches, including
/// aliases and subregions. Pixels wholly inside the patch are known;
/// partial overlaps lose their bounds until the atlas is trimmed again.
/// Returns the descriptors whose bounds changed, for a GPU table update.
pub(crate) fn patch_bounds(sprites: &mut [SpriteDesc], patched: &SpriteDesc, image: &RgbaImage) -> Vec<usize> {
    let whole = margins(patched, &[(true, true); 256], &[], Some((image, 0, 0)), None);
    let alpha_flags = tile_render_shared::BLANK_BINARY | tile_render_shared::BLANK_OPAQUE | tile_render_shared::BLANK_EMPTY;
    let mut changed = Vec::new();
    for (i, s) in sprites.iter_mut().enumerate() {
        if s.palette != 0 || s.page != patched.page
            || s.atlas_x >= patched.atlas_x + patched.width || patched.atlas_x >= s.atlas_x + s.width
            || s.atlas_y >= patched.atlas_y + patched.height || patched.atlas_y >= s.atlas_y + s.height
        {
            continue;
        }
        let bounds = if s.atlas_x >= patched.atlas_x && s.atlas_y >= patched.atlas_y
            && s.atlas_x + s.width <= patched.atlas_x + patched.width && s.atlas_y + s.height <= patched.atlas_y + patched.height
        {
            if s.atlas_x == patched.atlas_x && s.atlas_y == patched.atlas_y && s.width == patched.width && s.height == patched.height {
                whole
            } else {
                margins(s, &[(true, true); 256], &[], Some((image, s.atlas_x - patched.atlas_x, s.atlas_y - patched.atlas_y)), None)
            }
        } else {
            // Both the old pixels and every replacement texel must prove
            // an alpha property before a partially overwritten alias keeps it.
            (s.blank & whole.0 & alpha_flags, 0)
        };
        if (s.blank, s.cuts) != bounds {
            (s.blank, s.cuts) = bounds;
            changed.push(i);
        }
    }
    changed
}

/// Alpha changes can expose indexed texels outside their old bounds, or
/// make a formerly binary sprite translucent. RGB-only changes keep all
/// bounds. Without the index pages, binary alpha can still be proved from
/// the affected palette rows; unchanged sprites keep their exact metadata.
pub(crate) fn palette_bounds(sprites: &mut [SpriteDesc], old: &[u32], new: &[u32], named_palettes: u32) -> Vec<usize> {
    let size = tile_render_shared::PALETTE_SIZE as usize;
    let alpha = |p: &[u32], i: usize| p.get(i).copied().unwrap_or(0) >> 24;
    let rows = old.len().max(new.len()).div_ceil(size);
    let changed_rows: Vec<bool> = (0..rows).map(|r| (r * size..(r + 1) * size).any(|i| alpha(old, i) != alpha(new, i))).collect();
    let (before, after) = (named_alpha(old, named_palettes), named_alpha(new, named_palettes));
    let mut changed = Vec::new();
    for (i, s) in sprites.iter_mut().enumerate().filter(|(_, s)| s.palette != 0) {
        let own_changed = changed_rows.get(s.palette as usize).copied().unwrap_or(false);
        let (lo, hi) = ((s.remap & 0xFFFF).min(256) as usize, (s.remap >> 16).min(256) as usize);
        let named_changed = (lo..hi).any(|k| before[k] != after[k]);
        if !own_changed && !named_changed {
            continue;
        }
        let (mut binary, mut opaque) = (true, true);
        for k in 0..size {
            let a = alpha(new, s.palette as usize * size + k);
            binary &= a == 0 || a == 255;
            opaque &= a == 255;
            if (lo..hi).contains(&k) {
                binary &= after[k].0;
                opaque &= after[k].1;
            }
        }
        let flags = if binary { tile_render_shared::BLANK_BINARY } else { 0 } | if opaque { tile_render_shared::BLANK_OPAQUE } else { 0 };
        let margins = if own_changed { 0 } else { s.blank & !(tile_render_shared::BLANK_BINARY | tile_render_shared::BLANK_OPAQUE) };
        let bounds = (margins | flags, if own_changed { 0 } else { s.cuts });
        if (s.blank, s.cuts) != bounds {
            (s.blank, s.cuts) = bounds;
            changed.push(i);
        }
    }
    changed
}

/// The blank margins of `s` and its alpha flags, and its corner cuts
/// (`SpriteDesc::cuts`), from its pixels in `rgba` or `index` (an image and
/// where the sprite's top-left texel is in it; 0 where they are not there)
/// and the palette rows `palettes` (`named`: see [`named_alpha`]).
fn margins(s: &SpriteDesc, named: &[(bool, bool); 256], palettes: &[u32], rgba: Option<(&RgbaImage, u32, u32)>, index: Option<(&IndexImage, u32, u32)>) -> (u32, u32) {
    let (w, h) = (s.width, s.height);
    if w == 0 || h == 0 {
        return (0, 0);
    }
    // Per texel: alpha 0 (blank), alpha 0 or 255, alpha 255.
    let alpha: Box<dyn Fn(u32, u32) -> (bool, bool, bool) + '_> = if s.palette == 0 {
        let Some((page, x0, y0)) = rgba.filter(|(p, x0, y0)| x0 + w <= p.width && y0 + h <= p.height) else { return (0, 0) };
        Box::new(move |x, y| {
            let a = page.pixels[((y0 + y) * page.width + x0 + x) as usize][3];
            (a == 0, a == 0 || a == 255, a == 255)
        })
    } else {
        let Some((page, x0, y0)) = index.filter(|(p, x0, y0)| x0 + w <= p.width && y0 + h <= p.height) else { return (0, 0) };
        let row = s.palette as usize * tile_render_shared::PALETTE_SIZE as usize;
        let Some(entries) = palettes.get(row..row + tile_render_shared::PALETTE_SIZE as usize) else { return (0, 0) };
        let (lo, hi) = (s.remap & 0xFFFF, s.remap >> 16);
        Box::new(move |x, y| {
            let i = page.pixels[((y0 + y) * page.width + x0 + x) as usize] as u32;
            let a = entries[i as usize] >> 24;
            // (A remapped entry is drawn from the own row or any named
            // one.)
            let (binary, opaque) = if (lo..hi).contains(&i) { (named[i as usize].0 && (a == 0 || a == 255), named[i as usize].1 && a == 255) } else { (a == 0 || a == 255, a == 255) };
            (!(lo..hi).contains(&i) && a == 0, binary, opaque)
        })
    };
    let (mut binary, mut opaque) = (true, true);
    // The corner cuts: per corner the least of its diagonal measure
    // over the texels that are not blank, less half a pixel (texel
    // centres are at +0.5), so that they all lie strictly inside.
    let (mut c, (wi, hi)) = ([u32::MAX; 4], (w as i64, h as i64));
    for y in 0..h {
        for x in 0..w {
            let (clear, b, o) = alpha(x, y);
            binary &= b;
            opaque &= o;
            if !clear {
                let (x, y) = (x as i64, y as i64);
                for (k, d) in [x + 2 * y + 1, wi - x + 2 * y, wi - x + 2 * hi - 2 * y - 2, x + 2 * hi - 2 * y - 1].into_iter().enumerate() {
                    c[k] = c[k].min(d.max(0) as u32);
                }
            }
        }
    }
    let cuts = if c[0] == u32::MAX { 0 } else { tile_render_shared::corner_cuts(c, w, h) };
    let flags = if binary { tile_render_shared::BLANK_BINARY } else { 0 } | if opaque { tile_render_shared::BLANK_OPAQUE } else { 0 };
    let flags = flags | if c[0] == u32::MAX { tile_render_shared::BLANK_EMPTY } else { 0 };
    let blank = |x: u32, y: u32| alpha(x, y).0;
    let row_blank = |y: u32| (0..w).all(|x| blank(x, y));
    let top = (0..h).take_while(|&y| row_blank(y)).count() as u32;
    if top == h {
        return (tile_render_shared::blank_margins(w, h, w, h) | flags, 0);
    }
    let bottom = (top..h).rev().take_while(|&y| row_blank(y)).count() as u32;
    let column_blank = |x: u32| (top..h - bottom).all(|y| blank(x, y));
    let left = (0..w).take_while(|&x| column_blank(x)).count() as u32;
    let right = (left..w).rev().take_while(|&x| column_blank(x)).count() as u32;
    (tile_render_shared::blank_margins(left, top, right, bottom) | flags, cuts)
}


/// Per palette entry, whether every named row (from row `first_named` on)
/// has it at alpha 0 or 255, and at 255.
fn named_alpha(palettes: &[u32], first_named: u32) -> [(bool, bool); 256] {
    let size = tile_render_shared::PALETTE_SIZE as usize;
    let named = palettes.get(first_named as usize * size..).unwrap_or(&[]);
    let mut out = [(true, true); 256];
    for row in named.chunks(size) {
        for (i, &c) in row.iter().enumerate() {
            let a = c >> 24;
            out[i].0 &= a == 0 || a == 255;
            out[i].1 &= a == 255;
        }
    }
    out
}

impl AtlasGrowth {
    /// Finds the blank margins and corners of the sprites it adds (see
    /// [`Atlas::trim_blank`]) where their pixels are among its own blits,
    /// with the palette rows `palettes` of the atlas it grows (its named
    /// rows from `named_palettes` on) and its own.
    pub fn trim_blank(&mut self, palettes: &[u32], named_palettes: u32) {
        let mut all = palettes.to_vec();
        all.extend_from_slice(&self.palettes);
        let named = named_alpha(&all, named_palettes);
        for s in &mut self.sprites {
            // The blit holding all of its texels, if one does.
            let inside = |b: &&Blit, w: u32, h: u32| b.page == s.page && b.x <= s.atlas_x && b.y <= s.atlas_y && s.atlas_x + s.width <= b.x + w && s.atlas_y + s.height <= b.y + h;
            let rgba = self.blits.iter().find_map(|b| match &b.pixels {
                BlitPixels::Rgba(i) if s.palette == 0 && inside(&b, i.width, i.height) => Some((i, s.atlas_x - b.x, s.atlas_y - b.y)),
                _ => None,
            });
            let index = self.blits.iter().find_map(|b| match &b.pixels {
                BlitPixels::Index(i) if s.palette != 0 && inside(&b, i.width, i.height) => Some((i, s.atlas_x - b.x, s.atlas_y - b.y)),
                _ => None,
            });
            (s.blank, s.cuts) = margins(s, &named, &all, rgba, index);
        }
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
        let opaque = b.add(&RgbaImage::from_fn(5, 5, |_, _| [1, 2, 3, 255]), (0, 0)).unwrap();
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
        let binary = tile_render_shared::BLANK_BINARY;
        assert_eq!(atlas.sprites[empty.0 as usize].blank, tile_render_shared::blank_margins(4, 3, 4, 3) | binary | tile_render_shared::BLANK_EMPTY);
        assert_eq!(atlas.sprites[opaque.0 as usize].blank, binary | tile_render_shared::BLANK_OPAQUE);
        // (Its remapped index 200 is clear in every named row.)
        assert_eq!(atlas.sprites[i.0 as usize].blank, tile_render_shared::blank_margins(1, 0, 1, 0) | binary);
        let s = atlas.sprites[a.0 as usize];
        assert_eq!(tile_render_shared::drawn_part(&s, &tile_render_shared::placed(&s, 1)), (3, 2, 7, 9));
        // Every other texel from texel 1 on: pixels 1..3 show texels 3, 5.
        let half = tile_render_shared::Placed { texel_x: 1, texel_y: 0, step: 2, width: 5, height: 5, ..Default::default() };
        assert_eq!(tile_render_shared::drawn_part(&s, &half), (1, 1, 3, 5));
        assert!(atlas.patch(a, &RgbaImage::new(12, 10)));
        assert_eq!(atlas.sprites[a.0 as usize].blank, tile_render_shared::blank_margins(12, 10, 12, 10) | tile_render_shared::BLANK_BINARY | tile_render_shared::BLANK_EMPTY);
    }

    #[test]
    fn palette_rgb_keeps_bounds_and_binary_alpha_can_be_proved_again() {
        let binary = tile_render_shared::BLANK_BINARY;
        let mut sprites = [SpriteDesc { palette: 1, blank: tile_render_shared::blank_margins(2, 1, 3, 0) | binary, cuts: 1, ..Default::default() }];
        let mut old = vec![0; 512];
        old[257] = 0xFF00_0000;
        let mut new = old.clone();
        new[257] |= 0x0012_3456;
        assert!(palette_bounds(&mut sprites, &old, &new, 2).is_empty());
        assert_ne!(sprites[0].blank & !binary, 0);
        old = new.clone();
        new[257] = 0x8012_3456;
        assert_eq!(palette_bounds(&mut sprites, &old, &new, 2), vec![0]);
        assert_eq!((sprites[0].blank, sprites[0].cuts), (0, 0));
        old = new.clone();
        new[257] = 0xFF12_3456;
        assert_eq!(palette_bounds(&mut sprites, &old, &new, 2), vec![0]);
        assert_eq!(sprites[0].blank, binary);
    }

    #[test]
    fn a_growth_invalidates_existing_alpha_flags_and_keeps_new_sprites_bounds() {
        let mut b = AtlasBuilder::new(16);
        b.add_indexed(&IndexImage::from_fn(4, 4, |_, _| 1), (0, 0), 1, tile_render_shared::remap_range(1, 2)).unwrap();
        let mut atlas = b.finish();
        atlas.palettes = vec![0; 256];
        atlas.palettes.extend(vec![0xFF00_0000; 256]);
        atlas.named_palettes = 2;
        atlas.trim_blank();
        assert_ne!(atlas.sprites[0].blank & tile_render_shared::BLANK_BINARY, 0);
        let fresh = SpriteDesc { blank: tile_render_shared::blank_margins(1, 1, 1, 1), palette: 2, ..atlas.sprites[0] };
        atlas.grow(&AtlasGrowth { first: 1, pages: 0, index_pages: 1, palettes: vec![0x8000_0000; 256], sprites: vec![fresh], ..Default::default() });
        assert_eq!(atlas.sprites[0].blank & tile_render_shared::BLANK_BINARY, 0);
        assert_eq!(atlas.sprites[1], fresh);
    }

    #[test]
    fn a_growth_finds_the_blank_margins_of_the_sprites_it_brings_pixels_for() {
        let img = RgbaImage::from_fn(10, 6, |x, y| if (2..7).contains(&x) && y >= 1 { [1, 1, 1, 255] } else { [0; 4] });
        let mut whole = AtlasBuilder::new(64);
        let id = whole.add(&img, (0, 0)).unwrap();
        let mut whole = whole.finish();
        whole.trim_blank();
        let s = whole.sprites[id.0 as usize];
        // The same sprite as a growth's: its pixels in a blit, and one whose
        // pixels are elsewhere (not found: no margins).
        let blit = Blit { page: 0, x: 4, y: 8, pixels: BlitPixels::Rgba(img.clone()) };
        let fresh = SpriteDesc { page: 0, atlas_x: 4, atlas_y: 8, blank: 0, cuts: 0, ..s };
        let elsewhere = SpriteDesc { atlas_x: 30, ..fresh };
        let mut g = AtlasGrowth { first: 0, pages: 1, index_pages: 0, sprites: vec![fresh, elsewhere], blits: vec![blit], palettes: Vec::new() };
        g.trim_blank(&no_palettes(), 1);
        assert_eq!((g.sprites[0].blank, g.sprites[0].cuts), (s.blank, s.cuts));
        assert_ne!(s.blank, 0);
        assert_eq!((g.sprites[1].blank, g.sprites[1].cuts), (0, 0));
    }

    #[test]
    fn corner_cuts_keep_every_texel_that_is_not_blank_inside() {
        // A 2:1 diamond, as isometric tiles are, and a full rectangle.
        let (w, h) = (64u32, 32u32);
        let inside = |x: u32, y: u32| {
            let (dx, dy) = ((2.0 * x as f32 + 1.0 - w as f32).abs() / w as f32, (2.0 * y as f32 + 1.0 - h as f32).abs() / h as f32);
            dx + dy <= 1.0
        };
        let mut b = AtlasBuilder::new(128);
        let d = b.add(&RgbaImage::from_fn(w, h, |x, y| if inside(x, y) { [5, 5, 5, 255] } else { [0; 4] }), (0, 0)).unwrap();
        let full = b.add(&RgbaImage::from_fn(w, h, |_, _| [5, 5, 5, 255]), (0, 0)).unwrap();
        let mut atlas = b.finish();
        atlas.trim_blank();
        // (A cut of 1 leaves every texel centre in.)
        assert_eq!(atlas.sprites[full.0 as usize].cuts, 0x0101_0101);
        let cuts = atlas.sprites[d.0 as usize].cuts;
        let c = |k: u32| ((cuts >> (8 * k)) & 0xFF) as f32;
        assert!((0..4).all(|k| c(k) >= 24.0), "the corners are cut: {cuts:#x}");
        let (wf, hf) = (w as f32, h as f32);
        for y in 0..h {
            for x in 0..w {
                let (u, v) = (x as f32 + 0.5, y as f32 + 0.5);
                let kept = u + 2.0 * v > c(0) && wf - u + 2.0 * v > c(1) && wf - u + 2.0 * (hf - v) > c(2) && u + 2.0 * (hf - v) > c(3);
                assert!(kept || !inside(x, y), "texel ({x}, {y}) is cut off");
            }
        }
    }
}

#[cfg(test)]
mod empty_proof_tests {
    use super::*;
    #[test]
    fn large_empty_sprites_keep_their_sample_box_and_refresh_after_patches() {
        let mut builder = AtlasBuilder::new(512);
        let id = builder.add(&RgbaImage::new(400, 300), (17, -9)).unwrap();
        let mut atlas = builder.finish(); atlas.trim_blank();
        let original = atlas.sprites[id.0 as usize];
        assert_ne!(original.blank & tile_render_shared::BLANK_EMPTY, 0, "saturated margins cannot prove this alone");
        assert!(atlas.patch(id, &RgbaImage::from_fn(400, 300, |x, y| if (x, y) == (200, 150) { [9, 8, 7, 1] } else { [0; 4] })));
        let changed = atlas.sprites[id.0 as usize];
        assert_eq!((changed.box_w, changed.box_h, changed.anchor_x, changed.anchor_y), (original.box_w, original.box_h, original.anchor_x, original.anchor_y));
        assert_eq!(changed.blank & tile_render_shared::BLANK_EMPTY, 0);
        assert!(atlas.patch(id, &RgbaImage::new(400, 300)));
        assert_ne!(atlas.sprites[id.0 as usize].blank & tile_render_shared::BLANK_EMPTY, 0);
    }
    #[test]
    fn indexed_empty_proof_survives_rgb_and_is_invalidated_by_alpha_changes() {
        let mut builder = AtlasBuilder::new(512);
        let id = builder.add_indexed(&IndexImage::from_fn(400, 300, |_, _| 7), (0, 0), 1, 0).unwrap();
        let mut atlas = builder.finish(); atlas.palettes = vec![0; 512]; atlas.trim_blank();
        assert_ne!(atlas.sprites[id.0 as usize].blank & tile_render_shared::BLANK_EMPTY, 0);
        let old = atlas.palettes.clone(); let mut rgb = old.clone(); rgb[256 + 7] = 0x00123456;
        assert!(palette_bounds(&mut atlas.sprites, &old, &rgb, 0).is_empty());
        assert_ne!(atlas.sprites[id.0 as usize].blank & tile_render_shared::BLANK_EMPTY, 0);
        let mut visible = rgb.clone(); visible[256 + 7] |= 0x01000000;
        assert_eq!(palette_bounds(&mut atlas.sprites, &rgb, &visible, 0), vec![id.0 as usize]);
        assert_eq!(atlas.sprites[id.0 as usize].blank & tile_render_shared::BLANK_EMPTY, 0);
    }
}
