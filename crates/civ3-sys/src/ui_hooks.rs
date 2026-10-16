//! The game's user interface drawing (docs §16): where images come from,
//! how sprites are cut from them, every blit onto a canvas, and how
//! canvases are put on the screen. All `__thiscall` (`this` in ECX, hooked
//! as fastcall with a dummy EDX); PCGames build only so far.

use inproc::Ptr32;

use crate::Build;

inproc::fastcall_hook! {
    /// `int PCX_Image::read_file(this, char *path, PCX_Color_Table *ct,
    /// int ct_start, int ct_count, unsigned flags)`: loads an image file.
    pub mod pcx_read_file(image, edx; path, ct, ct_start, ct_count, flags) -> i32;
}

inproc::fastcall_hook! {
    /// `void PCX_Image::clear(this)`: frees the pixels.
    pub mod pcx_clear(image, edx;) -> ();
}

inproc::fastcall_hook! {
    /// `int Sprite::slice_pcx(this, PCX_Image *img, x, y, w, h, type,
    /// bool crop)`: cuts a sprite from an 8-bit image.
    pub mod sprite_slice(sprite, edx; image, x, y, w, h, kind, crop) -> i32;
}

inproc::fastcall_hook! {
    /// `Sprite *Sprite::copy(this, Sprite *src)`.
    pub mod sprite_copy(sprite, edx; source) -> u32;
}

inproc::fastcall_hook! {
    /// `void Sprite::reset(this)`: releases the pixels.
    pub mod sprite_reset(sprite, edx;) -> ();
}

inproc::fastcall_hook! {
    /// `int Sprite::draw(this, PCX_Image *canvas, x, y, PCX_Color_Table *ct)`:
    /// a colour-keyed blit (index 255 transparent).
    pub mod sprite_draw(sprite, edx; canvas, x, y, ct) -> i32;
}

inproc::fastcall_hook! {
    /// A blit variant with the same arguments (`0x5F81A0`, city screen).
    pub mod sprite_draw_variant(sprite, edx; canvas, x, y, ct) -> i32;
}

inproc::fastcall_hook! {
    /// Blit variants with one more argument (`0x5F84A0`, `0x5F8760`).
    pub mod sprite_draw_a4(sprite, edx; canvas, x, y, a4, ct) -> i32;
}

inproc::fastcall_hook! {
    /// Blit variants with one more argument (the second of the pair).
    pub mod sprite_draw_a4b(sprite, edx; canvas, x, y, a4, ct) -> i32;
}

inproc::fastcall_hook! {
    /// `Sprite::draw_blend(this, canvas, x, y, void *lut, ct)`: translucent
    /// overlay through a blend table.
    pub mod sprite_draw_blend(sprite, edx; canvas, x, y, lut, ct) -> i32;
}

inproc::fastcall_hook! {
    /// `Sprite::draw_alpha(this = image, Sprite *mask, PCX_Image *background,
    /// PCX_Image *dest, x, y, ct)`: a button face blended over a background
    /// by an alpha mask (no crop offset).
    pub mod sprite_draw_alpha(sprite, edx; mask, background, dest, x, y, ct) -> i32;
}

inproc::fastcall_hook! {
    /// `Sprite::draw_masked(this, Sprite *mask, PCX_Image *dest, x, y, ct)`
    /// (`0x5F8380`, the main menu's button faces): `this` blended onto
    /// `dest` through the alpha sprite `mask` (no crop offset).
    pub mod sprite_draw_masked(sprite, edx; mask, dest, x, y, ct) -> i32;
}

inproc::fastcall_hook! {
    /// `int PCX_Image::draw_onto(this, PCX_Image *dst, x, y)`: the whole
    /// image, colour-keyed.
    pub mod pcx_draw_onto(image, edx; dst, x, y) -> i32;
}

inproc::fastcall_hook! {
    /// `int PCX_Image::copy_stretch(this, PCX_Image *dst, sx, sy, sw, sh,
    /// dx, dy, dw, dh)`: an opaque rectangle copy (every rectangle copy
    /// ends here or in `copy_rects`).
    pub mod pcx_copy(image, edx; dst, sx, sy, sw, sh, dx, dy, dw, dh) -> i32;
}

inproc::fastcall_hook! {
    /// `int PCX_Image::copy_same_rect(this, PCX_Image *dst, x, y, w, h)`.
    pub mod pcx_copy_same(image, edx; dst, x, y, w, h) -> i32;
}

inproc::fastcall_hook! {
    /// `int PCX_Image::copy_rect(this, PCX_Image *dst, RECT *r)` (same
    /// rectangle in both).
    pub mod pcx_copy_rect(image, edx; dst, rect) -> i32;
}

inproc::fastcall_hook! {
    /// `int PCX_Image::copy_rects(this, PCX_Image *dst, RECT *src, RECT *dst)`.
    pub mod pcx_copy_rects(image, edx; dst, src_rect, dst_rect) -> i32;
}

inproc::fastcall_hook! {
    /// `int PCX_Image::fill(this, color)`: the whole image.
    pub mod pcx_fill(image, edx; color) -> i32;
}

inproc::fastcall_hook! {
    /// `int PCX_Image::fill_area(this, RECT *r, color)`.
    pub mod pcx_fill_area(image, edx; rect, color) -> i32;
}

inproc::fastcall_hook! {
    /// `int PCX_Image::line(this, x1, y1, x2, y2, color)`.
    pub mod pcx_line(image, edx; x1, y1, x2, y2, color) -> i32;
}

inproc::fastcall_hook! {
    /// `int PCX_Image::hline(this, x1, x2, y, color)`.
    pub mod pcx_hline(image, edx; x1, x2, y, color) -> i32;
}

inproc::fastcall_hook! {
    /// `int PCX_Image::vline(this, x, y1, y2, color)`.
    pub mod pcx_vline(image, edx; x, y1, y2, color) -> i32;
}

inproc::fastcall_hook! {
    /// `void Base_Form::m79_composite(this, RECT *dirty)`: copies the form's
    /// canvas onto the back buffer.
    pub mod form_composite(form, edx; dirty) -> ();
}

inproc::fastcall_hook! {
    /// `void Controller::paint(this, RECT *r)`: composes every form and
    /// presents the result (one screen update).
    pub mod paint(controller, edx; rect) -> ();
}

inproc::fastcall_hook! {
    /// The text worker `0x5FE7B0 (this, char *s, x, y, len)` every text function
    /// (left, centred, in a rectangle, right-aligned...) ends in.
    pub mod text_worker(image, edx; text, x, y, len) -> i32;
}

inproc::fastcall_hook! {
    /// `PCX_Image::draw_text(this, char *s, x, y, len)`.
    pub mod text_draw(image, edx; text, x, y, len) -> i32;
}

inproc::fastcall_hook! {
    /// `PCX_Image::draw_centered_text(this, char *s, x, y, width, len)`.
    pub mod text_draw_centered(image, edx; text, x, y, width, len) -> i32;
}

inproc::fastcall_hook! {
    /// `PCX_Image::draw_text_in_rect(this, char *s, RECT *r, len)`.
    pub mod text_draw_rect(image, edx; text, rect, len) -> i32;
}

/// A function to hook: its address and its first instructions (checked
/// before patching; position independent).
pub type Site = (u32, &'static [u8]);

/// Where the UI drawing functions are, per build.
pub struct UiSites {
    pub pcx_read_file: Site,
    pub pcx_clear: Site,
    pub sprite_slice: Site,
    pub sprite_copy: Site,
    pub sprite_reset: Site,
    pub sprite_draw: Site,
    pub sprite_draw_variant: Site,
    pub sprite_draw_a4: Site,
    pub sprite_draw_a4b: Site,
    pub sprite_draw_blend: Site,
    pub sprite_draw_alpha: Site,
    pub sprite_draw_masked: Site,
    pub pcx_draw_onto: Site,
    pub pcx_copy: Site,
    pub pcx_copy_same: Site,
    pub pcx_copy_rect: Site,
    pub pcx_copy_rects: Site,
    pub pcx_fill: Site,
    pub pcx_fill_area: Site,
    pub pcx_line: Site,
    pub pcx_hline: Site,
    pub pcx_vline: Site,
    pub form_composite: Site,
    pub paint: Site,
    pub text_draw: Site,
    pub text_worker: Site,
    pub text_draw_centered: Site,
    pub text_draw_rect: Site,
}

pub fn ui_sites(build: Build) -> Option<UiSites> {
    match build {
        Build::PcGames => Some(UiSites {
            // push -1 / push imm32
            pcx_read_file: (0x5F_C700, &[0x6A, 0xFF, 0x68, 0x29, 0x2E, 0x66, 0x00]),
            // push ebx / push ebp / push esi / mov esi,ecx
            pcx_clear: (0x5F_C4D0, &[0x53, 0x55, 0x56, 0x8B, 0xF1]),
            // push esi / mov esi,[esp+8]
            sprite_slice: (0x5F_7EC0, &[0x56, 0x8B, 0x74, 0x24, 0x08]),
            // push esi / push edi / mov edi,[esp+0Ch]
            sprite_copy: (0x5F_8A20, &[0x56, 0x57, 0x8B, 0x7C, 0x24, 0x0C]),
            // push ebx / push esi / mov esi,ecx / mov ecx,[0CAB28Ch]
            sprite_reset: (0x5F_7DB0, &[0x53, 0x56, 0x8B, 0xF1, 0x8B, 0x0D, 0x8C, 0xB2, 0xCA, 0x00]),
            // sub esp,0Ch / push ebx / push esi
            sprite_draw: (0x5F_8210, &[0x83, 0xEC, 0x0C, 0x53, 0x56]),
            // mov eax,[ecx+8] / push esi / test eax,eax
            sprite_draw_variant: (0x5F_81A0, &[0x8B, 0x41, 0x08, 0x56, 0x85, 0xC0]),
            // mov eax,ecx / push esi / mov ecx,[eax+8]
            sprite_draw_a4: (0x5F_84A0, &[0x8B, 0xC1, 0x56, 0x8B, 0x48, 0x08]),
            // mov eax,[ecx+8] / push esi / test eax,eax
            sprite_draw_a4b: (0x5F_8760, &[0x8B, 0x41, 0x08, 0x56, 0x85, 0xC0]),
            sprite_draw_blend: (0x5F_8620, &[0x8B, 0x41, 0x08, 0x56, 0x85, 0xC0]),
            // mov eax,[esp+8] / push esi
            sprite_draw_alpha: (0x5F_8310, &[0x8B, 0x44, 0x24, 0x08, 0x56]),
            // mov eax,[esp+8] / push esi / test eax,eax
            sprite_draw_masked: (0x5F_8380, &[0x8B, 0x44, 0x24, 0x08, 0x56, 0x85, 0xC0]),
            // mov ecx,[ecx+4] / test ecx,ecx
            pcx_draw_onto: (0x5F_C9F0, &[0x8B, 0x49, 0x04, 0x85, 0xC9]),
            // mov eax,[esp+4] / sub esp,20h
            pcx_copy: (0x5F_CC10, &[0x8B, 0x44, 0x24, 0x04, 0x83, 0xEC, 0x20]),
            // mov eax,[esp+4] / sub esp,10h
            pcx_copy_same: (0x5F_CB60, &[0x8B, 0x44, 0x24, 0x04, 0x83, 0xEC, 0x10]),
            // mov eax,[esp+4] / test eax,eax
            pcx_copy_rect: (0x5F_CBD0, &[0x8B, 0x44, 0x24, 0x04, 0x85, 0xC0]),
            pcx_copy_rects: (0x5F_CCB0, &[0x8B, 0x44, 0x24, 0x04, 0x85, 0xC0]),
            // mov ecx,[ecx+4] / test ecx,ecx
            pcx_fill: (0x5F_FF30, &[0x8B, 0x49, 0x04, 0x85, 0xC9]),
            pcx_fill_area: (0x5F_FF50, &[0x8B, 0x49, 0x04, 0x85, 0xC9]),
            pcx_line: (0x5F_FFB0, &[0x8B, 0x49, 0x04, 0x85, 0xC9]),
            pcx_hline: (0x5F_FFE0, &[0x8B, 0x49, 0x04, 0x85, 0xC9]),
            pcx_vline: (0x60_0010, &[0x8B, 0x49, 0x04, 0x85, 0xC9]),
            // sub esp,18h / push ebp / push esi
            form_composite: (0x60_2710, &[0x83, 0xEC, 0x18, 0x55, 0x56]),
            // push esi / mov esi,[esp+8]
            paint: (0x62_C3F0, &[0x56, 0x8B, 0x74, 0x24, 0x08]),
            // sub esp,8 / mov eax,[esp+0Ch] / push ebp
            text_worker: (0x5F_E7B0, &[0x83, 0xEC, 0x08, 0x8B, 0x44, 0x24, 0x0C, 0x55]),
            // push ebx / push esi / mov esi,[esp+0Ch]
            text_draw: (0x5F_EE50, &[0x53, 0x56, 0x8B, 0x74, 0x24, 0x0C]),
            // push ebx / mov ebx,[esp+8]
            text_draw_centered: (0x5F_EFC0, &[0x53, 0x8B, 0x5C, 0x24, 0x08]),
            // mov edx,[esp+4] / sub esp,10h
            text_draw_rect: (0x5F_F0D0, &[0x8B, 0x54, 0x24, 0x04, 0x83, 0xEC, 0x10]),
        }),
        Build::Gog | Build::Steam => None,
    }
}

/// The back buffer (`PCX_Image` at 0xCACFF0): its pixels are the window's.
pub fn back_buffer(build: Build) -> Option<u32> {
    match build {
        Build::PcGames => Some(0xCA_CFF0),
        Build::Gog | Build::Steam => None,
    }
}

/// The civ colour tables (`Units_Image_Data.Color_Tables`, 32
/// `PCX_Color_Table*` loaded from `Art/Units/Palettes/ntpNN.pcx`).
fn color_tables(build: Build) -> Option<u32> {
    match build {
        Build::PcGames => Some(0x73_DA04),
        Build::Gog | Build::Steam => None,
    }
}

/// Which civ colour table (NN of `ntpNN.pcx`) `ct` is, if it is one: a
/// sprite drawn through it shows that file's palette (e.g. the foreign
/// advisor's ring around a leader).
pub fn civ_color_table(build: Build, ct: u32) -> Option<u32> {
    let base = color_tables(build).filter(|_| ct != 0)?;
    (0..32).find(|&i| Ptr32::<u32>::new(base + 4 * i).read().ok() == Some(ct))
}

/// Fields of the game's image, sprite and form objects (docs §16.2-16.5).
pub mod obj {
    use super::*;

    fn i32_at(base: u32, at: u32) -> Option<i32> {
        Ptr32::<i32>::new(base.wrapping_add(at)).read().ok()
    }

    /// A `PCX_Image`'s size (its `JGL_Image`'s `Image_Rect`), if it has
    /// pixels.
    pub fn image_size(image: u32) -> Option<(i32, i32)> {
        let jgl = Ptr32::<u32>::new(image.wrapping_add(4)).read().ok().filter(|&p| p != 0)?;
        let [l, t, r, b] = [0, 4, 8, 12].map(|o| i32_at(jgl, 0x54 + o));
        Some((r? - l?, b? - t?))
    }

    /// A sprite's stored pixels: crop offset and size (`+0x20/+0x24`,
    /// `+0x10/+0x14`).
    pub fn sprite_crop(sprite: u32) -> Option<(i32, i32, i32, i32)> {
        Some((i32_at(sprite, 0x20)?, i32_at(sprite, 0x24)?, i32_at(sprite, 0x10)?, i32_at(sprite, 0x14)?))
    }

    /// Whether the sprite has pixels.
    pub fn sprite_has_pixels(sprite: u32) -> bool {
        Ptr32::<u32>::new(sprite.wrapping_add(4)).read().is_ok_and(|p| p != 0)
    }

    /// A `RECT` (left, top, right, bottom).
    pub fn rect(at: u32) -> Option<[i32; 4]> {
        Some([i32_at(at, 0)?, i32_at(at, 4)?, i32_at(at, 8)?, i32_at(at, 12)?])
    }

    /// A form as it is put on the screen.
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct FormPlacement {
        /// Its vtable (tells the form's class).
        pub vtable: u32,
        /// Its canvas (`PCX_Image`).
        pub canvas: u32,
        /// Screen position of the canvas' (0, 0).
        pub origin: (i32, i32),
        /// Canvas size.
        pub size: (i32, i32),
        /// Screen clip rectangle.
        pub clip: [i32; 4],
        /// Composited colour-keyed (index 255 shows what is below).
        pub keyed: bool,
    }

    const STATUS1: u32 = 0x9C;
    const PARENT: u32 = 0x130;
    const LEFT: u32 = 0x1AC;
    const CLIENT: u32 = 0x1BC;
    const CLIP: u32 = 0x1DC;
    const CANVAS: u32 = 0x274;

    /// `Base_Form::local_to_screen(0, 0)` (0x604A00), computed.
    fn origin(form: u32, depth: u32) -> Option<(i32, i32)> {
        let mut x = i32_at(form, LEFT)? + i32_at(form, CLIENT)?;
        let mut y = i32_at(form, LEFT + 4)? + i32_at(form, CLIENT + 4)?;
        let status = i32_at(form, STATUS1)? as u32;
        let parent = i32_at(form, PARENT)? as u32;
        if status & 0x20 != 0 && parent != 0 && depth < 32 {
            let (px, py) = origin(parent, depth + 1)?;
            x += px;
            y += py;
            if status & 0x8000 != 0 {
                x -= i32_at(parent, LEFT)?;
                y -= i32_at(parent, LEFT + 4)?;
            }
        }
        Some((x, y))
    }

    /// Where `form`'s canvas goes on the screen (what `m79` does).
    pub fn placement(form: u32) -> Option<FormPlacement> {
        let [l, t, r, b] = rect(form + LEFT)?;
        Some(FormPlacement {
            vtable: Ptr32::<u32>::new(form).read().ok()?,
            canvas: form + CANVAS,
            origin: origin(form, 0)?,
            size: (r - l, b - t),
            clip: rect(form + CLIP)?,
            keyed: i32_at(form, STATUS1)? as u32 & 0x100_0000 != 0,
        })
    }
}

/// Surfaces the game fills pixel by pixel, which a mirror has to make
/// itself, by name: the minimap's shown image (`Navigator_Data` at
/// Main_GUI+0x10720, its `PCX_4` at +0x858; docs §17).
pub fn generated_surfaces(build: Build) -> &'static [(u32, &'static str)] {
    match build {
        Build::PcGames => generated_surfaces_pcg(),
        Build::Gog | Build::Steam => &[],
    }
}

/// [`generated_surfaces`] of the PCGames build, usable in constants.
pub const fn generated_surfaces_pcg() -> &'static [(u32, &'static str)] {
    // The navigators' images (`Navigator_Data` + 0x858): the interface's
    // minimap, and the one of the military and cultural advisors (300×145).
    &[(0xA0_E230 + 0x858, "minimap"), (0x9B_3F38 + 0x858, "minimap-advisor")]
}

inproc::fastcall_hook! {
    /// jgl.dll `JGL_Image::shade_rect(this, RECT *r, color, percent)`
    /// (vtable slot +0x48): mixes `color` into the rectangle (NULL: the whole
    /// image), keeping `percent` of what is there — the translucent city
    /// label plates (docs §16.6). `color`: bit 31 set: a direct 16-bit
    /// colour, else a palette index.
    pub mod jgl_shade(image, edx; rect, color, percent) -> i32;
}

/// The `JGL_Image` vtable slot of `shade_rect` in jgl.dll loaded at `base`,
/// and the function expected there (jgl.dll of 2004-03-17: vtable at RVA
/// 0x68238, the function at RVA 0x2320).
pub fn jgl_shade_slot(base: u32) -> (Ptr32<u32>, u32) {
    (Ptr32::new(base + 0x6_8238 + 0x48), base + 0x2320)
}

/// The `PCX_Image` owning a `JGL_Image` (its +0x80 back pointer).
pub fn jgl_owner(jgl_image: u32) -> Option<u32> {
    Ptr32::<u32>::new(jgl_image.wrapping_add(0x80)).read().ok().filter(|&p| p != 0)
}

/// The canvas the game draws unit decorations on (team discs, movement
/// LEDs, health bars, stack lines; a keyed overlay over the map, measured
/// at 0x9FD80C = main screen form + 0x514C on the PCGames build).
pub fn unit_overlay_canvas(build: Build) -> Option<u32> {
    match build {
        Build::PcGames => Some(0x9F_D80C),
        Build::Gog | Build::Steam => None,
    }
}

// The game's line drawer (`OpenGLRenderer`, docs §10.9/§16.6): bound to a
// canvas, then coloured lines. C3X redirects these (a `jmp` at each entry).

inproc::fastcall_hook! {
    /// `int OpenGLRenderer::initialize(this, PCX_Image *texture)`.
    pub mod line_bind(renderer, edx; canvas) -> i32;
}

inproc::fastcall_hook! {
    /// `void OpenGLRenderer::set_color(this, unsigned rgb555)`.
    pub mod line_color(renderer, edx; rgb555) -> ();
}

inproc::fastcall_hook! {
    /// `void OpenGLRenderer::set_opacity(this, unsigned alpha)` (0..255).
    pub mod line_opacity(renderer, edx; alpha) -> ();
}

inproc::fastcall_hook! {
    /// `void OpenGLRenderer::set_line_width(this, int width)`.
    pub mod line_width(renderer, edx; width) -> ();
}

inproc::fastcall_hook! {
    /// `void OpenGLRenderer::draw_line(this, x1, y1, x2, y2)`.
    pub mod line_draw(renderer, edx; x1, y1, x2, y2) -> ();
}

/// The line drawer's functions: initialize, set_color, set_opacity,
/// set_line_width, draw_line.
pub fn line_sites(build: Build) -> Option<[u32; 5]> {
    match build {
        Build::PcGames => Some([0x62_74B0, 0x62_77A0, 0x62_7880, 0x62_78C0, 0x62_7760]),
        Build::Gog | Build::Steam => None,
    }
}

/// Where the map renderer keeps its focus city (`City*`, set while the city
/// screen is open; docs §16.8).
pub fn focus_city_slot(build: Build) -> Option<u32> {
    match build {
        Build::PcGames => Some(0x9C_B3F4),
        Build::Gog | Build::Steam => None,
    }
}

inproc::fastcall_hook! {
    /// `int Flic_Anim_Info::load(this, char *path, a2, a3, a4)` (0x600FA0,
    /// `ret 0x10`; the other arguments not decoded): an FLC
    /// animation (unit art) loaded from `path`.
    pub mod flc_load(flic, edx; path, a2, a3, a4) -> i32;
}

/// `Flic_Anim_Info::load`: address and first instructions (push -1 / push
/// imm32).
pub fn flc_load_site(build: Build) -> Option<Site> {
    match build {
        Build::PcGames => Some((0x60_0FA0, &[0x6A, 0xFF, 0x68, 0xF3, 0x2E, 0x66, 0x00])),
        Build::Gog | Build::Steam => None,
    }
}

/// `FLC_Frame_Image`'s vtable: a `Sprite` preceded by it is a frame of an
/// FLC animation (docs §16.9).
const FLC_FRAME_VTABLE: u32 = 0x66_5450;

/// If `sprite` is an FLC animation frame: its animation (`Flic_Anim_Info*`)
/// and its chunk (block · (frames per block + 1) + frame, how unit art
/// numbers frames).
pub fn flc_frame(sprite: u32) -> Option<(u32, u32)> {
    let u32_at = |a: u32| Ptr32::<u32>::new(a).read().ok();
    if u32_at(sprite.wrapping_sub(8))? != FLC_FRAME_VTABLE {
        return None;
    }
    let flic = u32_at(sprite.wrapping_sub(4)).filter(|&f| f != 0)?;
    let frame = u32_at(sprite + 0x34)? as i32;
    let block = u32_at(sprite + 0x38)? as i32;
    let header = u32_at(flic + 0x58).filter(|&h| h != 0)?;
    let fpd = Ptr32::<u16>::new(header + 0x62).read().ok()? as i32;
    (frame >= 0 && block >= 0).then(|| (flic, (block * (fpd + 1) + frame) as u32))
}

/// A canvas's clip rectangle (its `JGL_Image`'s `Clip_Rect`, left, top,
/// right, bottom), when it is smaller than the canvas.
pub fn canvas_clip(image: u32) -> Option<[i32; 4]> {
    let jgl = Ptr32::<u32>::new(image.wrapping_add(4)).read().ok().filter(|&p| p != 0)?;
    let clip = obj::rect(jgl + 0x44)?;
    let full = obj::rect(jgl + 0x54)?;
    (clip != full && clip[2] > clip[0] && clip[3] > clip[1]).then_some(clip)
}
