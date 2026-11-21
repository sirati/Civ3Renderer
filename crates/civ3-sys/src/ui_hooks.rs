//! The game's user interface drawing (docs §16): where images come from,
//! how sprites are cut from them, every blit onto a canvas, and how
//! canvases are put on the screen. All `__thiscall` (`this` in ECX, hooked
//! as fastcall with a dummy EDX); PCGames build only so far.

use inproc::Ptr32;

use crate::{Build, layout as L};

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
    /// `int PCX_Image::create_and_init_jgl_image(this, width, height,
    /// bit_depth, param_4, param_5, param_6)` (C3X's declaration).
    pub mod pcx_create(image, edx; width, height, bit_depth, param_4, param_5, param_6) -> i32;
}

inproc::fastcall_hook! {
    /// The same PCX creation function, called when a form sizes its canvas.
    pub mod pcx_create_sized(image, edx; width, height, bit_depth, param_4, param_5, param_6) -> i32;
}

inproc::fastcall_hook! {
    /// The same PCX creation function, called when a form recreates its canvas.
    pub mod pcx_create_resized(image, edx; width, height, bit_depth, param_4, param_5, param_6) -> i32;
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
    /// `int PCX_Image::get_pixel(this, x, y)` (0x600220): the pixel's value
    /// (palette index or 16-bit colour). How the game tests what is under
    /// the mouse: index images of regions (the value is the region, 0xFF
    /// none) and the canvases of colour-keyed forms (the key lets a click
    /// through, 0x608C70).
    pub mod pcx_get_pixel(image, edx; x, y) -> u32;
}

inproc::fastcall_hook! {
    /// The science advisor's `int advance_at(this, x, y)` (0x5A8EF0):
    /// replaced by `controls::advisors::science_advance_at`.
    pub mod science_advance_at(form, edx; x, y) -> u32;
}

inproc::cdecl_hook! {
    /// `Base_Form *cdecl form_under_pointer(int *x, int *y)` (0x609030):
    /// the form the mouse acts on at screen point (*x, *y), which it turns
    /// into the form's pixels. Every mouse message, the wheel and the
    /// cursor ask it. Replaced by `controls::form_hit::form_under`, which
    /// reads no pixel (`controls::form_hit::site`).
    pub mod form_under_pointer(x, y) -> u32;
}

inproc::fastcall_hook! {
    /// The same `get_pixel` at [`art_presence_site`].
    pub mod art_presence_pixel(image, edx; x, y) -> u32;
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

inproc::cdecl_hook! {
    /// `void compose_all(RECT *r, Base_Form *form)` (cdecl): composes the
    /// forms shown onto the back buffer, bottom to top: all of them, or,
    /// with `form` while the draw list starts at a modal form
    /// ([`modal_draw_root`] set), `form` and those above it.
    /// `Controller::paint` is this and a present; screens running a loop of
    /// their own (the victory screen at the end of a game) call it and
    /// present themselves (docs §16.5).
    pub mod compose_all(rect, form) -> ();
}

inproc::cdecl_hook! {
    /// `void present(RECT *r)` (cdecl): draws the tooltip and the mouse
    /// cursor onto the back buffer and copies its rectangle `r` (screen
    /// coordinates; all of it if null) to the window. The only way the
    /// back buffer reaches the screen (docs §16.5).
    pub mod present(rect) -> ();
}


inproc::fastcall_hook! {
    /// Creates navigator geometry and images, then optionally tints and reveals them.
    pub mod navigator_init(navigator, edx; width, height, tint) -> ();
}
inproc::fastcall_hook! {
    /// Tints one tile's navigator pixels with an RGB555 colour.
    pub mod navigator_tint(navigator, edx; x, y, color) -> ();
}
inproc::fastcall_hook! {
    pub mod navigator_restore(navigator, edx; x, y) -> ();
}
inproc::fastcall_hook! {
    pub mod navigator_add_city(navigator, edx; x, y) -> ();
}
inproc::fastcall_hook! {
    pub mod navigator_erase_city(navigator, edx; x, y, color) -> ();
}
inproc::cdecl_hook! {
    /// Writes the navigator's terrain pixels, without changing game state.
    pub mod navigator_terrain(image, width, height) -> ();
}

/// A function to hook: its address and its first instructions (checked
/// before patching; position independent).
pub type Site = (u32, &'static [u8]);

/// Where the UI drawing functions are, per build.
pub struct UiSites {
    pub pcx_read_file: Site,
    pub pcx_clear: Site,
    pub form_canvas_create: (u32, &'static [u8; 5]),
    pub form_canvas_size: (u32, &'static [u8; 5]),
    pub form_canvas_recreate: (u32, &'static [u8; 5]),
    pub sprite_slice: Site,
    pub sprite_copy: Site,
    pub sprite_reset: Site,
    pub pcx_get_pixel: Site,
    pub form_composite: Site,
    pub paint: Site,
    pub compose_all: Site,
    pub present: Site,
}

pub fn ui_sites(build: Build) -> Option<UiSites> {
    match build {
        Build::PcGames => Some(UiSites {
            // push -1 / push imm32
            pcx_read_file: (0x5F_C700, &[0x6A, 0xFF, 0x68, 0x29, 0x2E, 0x66, 0x00]),
            // push ebx / push ebp / push esi / mov esi,ecx
            pcx_clear: (0x5F_C4D0, &[0x53, 0x55, 0x56, 0x8B, 0xF1]),
            // Base_Form creates, sizes, and recreates its canvas through
            // these calls. Leave the callee's relative-call prologue at
            // its original address.
            form_canvas_create: (0x60_209D, &[0xE8, 0x4E, 0xA5, 0xFF, 0xFF]),
            form_canvas_size: (0x60_23FE, &[0xE8, 0xED, 0xA1, 0xFF, 0xFF]),
            form_canvas_recreate: (0x60_2952, &[0xE8, 0x99, 0x9C, 0xFF, 0xFF]),
            // push esi / mov esi,[esp+8]
            sprite_slice: (0x5F_7EC0, &[0x56, 0x8B, 0x74, 0x24, 0x08]),
            // push esi / push edi / mov edi,[esp+0Ch]
            sprite_copy: (0x5F_8A20, &[0x56, 0x57, 0x8B, 0x7C, 0x24, 0x0C]),
            // push ebx / push esi / mov esi,ecx / mov ecx,[0CAB28Ch]
            sprite_reset: (0x5F_7DB0, &[0x53, 0x56, 0x8B, 0xF1, 0x8B, 0x0D, 0x8C, 0xB2, 0xCA, 0x00]),
            // push ebx / push esi / mov esi,ecx / push edi
            pcx_get_pixel: (0x60_0220, &[0x53, 0x56, 0x8B, 0xF1, 0x57]),
            // sub esp,18h / push ebp / push esi
            form_composite: (0x60_2710, &[0x83, 0xEC, 0x18, 0x55, 0x56]),
            // push esi / mov esi,[esp+8]
            paint: (0x62_C3F0, &[0x56, 0x8B, 0x74, 0x24, 0x08]),
            // mov eax,[0CAD3C0h] / sub esp,10h
            compose_all: (0x60_9210, &[0xA1, 0xC0, 0xD3, 0xCA, 0x00, 0x83, 0xEC, 0x10]),
            // mov eax,[0CAD2ECh]
            present: (0x60_6680, &[0xA1, 0xEC, 0xD2, 0xCA, 0x00]),
        }),
        Build::Gog | Build::Steam => None,
    }
}

/// Whether WinMain has received the jgl graphics object. PCGames stores
/// the factory result in this global at 0x62025A, after its LoadLibraryA
/// callback and CRT/C3X initialization have completed. Used only for late
/// injection, outside DllMain; a merely mapped jgl.dll is insufficient.
pub fn graphics_initialized(build: Build) -> bool {
    match build {
        Build::PcGames => Ptr32::<u32>::new(0xCA_B28C).read().is_ok_and(|p| p != 0),
        Build::Gog | Build::Steam => false,
    }
}

/// At start the game cuts 32×32 sprites from `Art\oldinterface.pcx`
/// (0x549D60..), draws each into a new 8-bit image (the three `PCX_Image`s
/// from 0xB3F3E0, 0x2B8 apart) and reads its pixel (0, 0) here, `call
/// PCX_Image::get_pixel`: 0xFF (the file has nothing there) and it deletes
/// the image (`docs/rerender-roadmap.md` §4 #5).
pub fn art_presence_site(build: Build) -> Option<(u32, &'static [u8; 5])> {
    match build {
        Build::PcGames => Some((0x54_9FAD, &[0xE8, 0x6E, 0x62, 0x0B, 0x00])),
        Build::Gog | Build::Steam => None,
    }
}

/// Palace View's piece-index image at form+0x3154. Its class check keeps
/// an unrelated 8-bit canvas out of the semantic selection path.
pub fn palace_mask_form(image: u32) -> Option<u32> {
    let form = image.checked_sub(0x3154)?;
    (Ptr32::<u32>::new(form).read().ok()? == crate::controls::screens::PALACE).then_some(form)
}


/// The back buffer (`PCX_Image` at 0xCACFF0): its pixels are the window's.
pub fn back_buffer(build: Build) -> Option<u32> {
    match build {
        Build::PcGames => Some(0xCA_CFF0),
        Build::Gog | Build::Steam => None,
    }
}

/// The modal form the draw list starts at, if any (`Base_Form *`; docs
/// §16.5): while it is null, `compose_all` composes every form whatever
/// form it is given.
pub fn modal_draw_root(build: Build) -> Option<u32> {
    match build {
        Build::PcGames => Some(0xCA_D310),
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

/// Which civ colour table (NN of `ntpNN.pcx`) `ct` is, if it is one (the
/// `PCX_Color_Table` or its `JGL_Color_Table`, +4): a sprite drawn through
/// it shows that file's palette (e.g. the foreign advisor's ring around a
/// leader).
pub fn civ_color_table(build: Build, ct: u32) -> Option<u32> {
    let base = color_tables(build).filter(|_| ct != 0)?;
    let read = |a: u32| Ptr32::<u32>::new(a).read().ok().filter(|&p| p != 0);
    (0..32).find(|&i| read(base + 4 * i).is_some_and(|t| t == ct || read(t + 4) == Some(ct)))
}

/// Civ colour table `n` (`ntpNN.pcx`, as the game loaded it): its
/// `PCX_Color_Table`, for [`color_table_entries`].
pub fn civ_table(build: Build, n: u32) -> Option<u32> {
    let base = color_tables(build).filter(|_| n < 32)?;
    Ptr32::<u32>::new(base + 4 * n).read().ok().filter(|&p| p != 0)
}

/// The first 64 colours (RGB, `0xBBGGRR`) of the `JGL_Color_Table` `jgl`
/// (entry i a dword at +0x0C + 4i, a flag byte on top).
fn first_colors(jgl: u32) -> Option<Vec<u32>> {
    let entries = Ptr32::<u32>::new(jgl.checked_add(0x0C)?).read_array(64).ok()?;
    Some(entries.into_iter().map(|c| c & 0xFF_FFFF).collect())
}

/// The first 64 colours of each civ colour table (see [`civ_color_table`]),
/// by table number; `None` before they are loaded.
pub fn civ_table_colors(build: Build) -> Option<Vec<Vec<u32>>> {
    let tables = Ptr32::<u32>::new(color_tables(build)?).read_array(32).ok()?;
    tables.iter().map(|&t| first_colors(Ptr32::<u32>::new(t.checked_add(4)?).read().ok().filter(|&p| p != 0)?)).collect()
}

/// The first 64 colours of `sprite`'s own colour table, if it has one
/// (`Sprite+0x0C`, a `PCX_Color_Table*`): the game recolours such sprites by
/// copying a civ's colours into it before drawing them (e.g. the foreign
/// advisor's ring around a leader, 0x52E696).
pub fn sprite_own_colors(sprite: u32) -> Option<Vec<u32>> {
    let read = |a: u32| Ptr32::<u32>::new(a).read().ok().filter(|&p| p != 0);
    let table = read(sprite.checked_add(0x0C)?)?;
    first_colors(read(table.checked_add(4)?)?)
}

/// The first 64 colours of the palette `sprite`'s pixels were cut with (its
/// `JGLSprite`'s, at +0x10, a `JGL_Color_Table*`): sprites cut from an image
/// recoloured with a civ's colours keep them (the citizens' heads,
/// `popHeads.pcx`, and the units' icons, `units_32.pcx`, in the player's
/// colours, measured).
pub fn sprite_cut_colors(sprite: u32) -> Option<Vec<u32>> {
    let read = |a: u32| Ptr32::<u32>::new(a).read().ok().filter(|&p| p != 0);
    let jgl = read(sprite.checked_add(4)?)?;
    first_colors(read(jgl.checked_add(0x10)?)?)
}

/// Entries `first..first + count` (RGB, `0xBBGGRR`) of the `PCX_Color_Table`
/// `ct` (its `JGL_Color_Table` at +4).
pub fn color_table_entries(ct: u32, first: u32, count: u32) -> Option<Vec<u32>> {
    let jgl = Ptr32::<u32>::new(ct.checked_add(4)?).read().ok().filter(|&p| p != 0)?;
    let entries = Ptr32::<u32>::new(jgl.checked_add(0x0C + 4 * first)?).read_array(count as usize).ok()?;
    Some(entries.into_iter().map(|c| c & 0xFF_FFFF).collect())
}

/// The first civ colour table (NN of `ntpNN.pcx`) whose entries
/// `first..first + count` are `colors`: the table a colour table made by
/// copying those entries of a civ's comes from (the territory borders',
/// entries 64 and 65 of the owner's, 0x4C3B87..0x4C3BEC).
pub fn civ_table_with(build: Build, first: u32, colors: &[u32]) -> Option<u32> {
    let tables = Ptr32::<u32>::new(color_tables(build)?).read_array(32).ok()?;
    (0u32..).zip(tables).find_map(|(n, t)| (t != 0 && color_table_entries(t, first, colors.len() as u32).as_deref() == Some(colors)).then_some(n))
}

/// Where the pointers to the game's two sets of 16-bit blend tables are
/// (docs §6.7a, §10.14), on builds where that is known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlendTables {
    /// The shadow tables (0x403D40 builds them): four of 32768 RGB555
    /// entries, darkening to 8, 10, 12 and 14 sixteenths. Resource,
    /// airfield, outpost and radar shadows go through them.
    pub shadows: u32,
    /// The shade and smoke levels (0x6283A0 builds them; the "flags" of
    /// the colour-table blits): levels 0..14 darken to 1..15 sixteenths.
    pub levels: u32,
}

pub fn blend_tables(build: Build) -> Option<BlendTables> {
    match build {
        Build::PcGames => Some(BlendTables { shadows: 0x73_D9F0, levels: 0x9C_7304 }),
        Build::Gog | Build::Steam => None,
    }
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

    /// An image's bits per pixel (its `JGL_Image`'s +0x24): 8 for images
    /// of palette indices, 16 for canvases.
    pub fn image_bits(image: u32) -> Option<u32> {
        let jgl = Ptr32::<u32>::new(image.wrapping_add(4)).read().ok().filter(|&p| p != 0)?;
        Ptr32::<u32>::new(jgl + 0x24).read().ok()
    }

    /// The exe's decoded 8-bit source, copied as one guarded block while
    /// its file-load call still owns the image. No drawing is read back.
    pub fn image_indices(image: u32) -> Option<(u32, u32, u32, Vec<u8>)> {
        let jgl = Ptr32::<u32>::new(image.checked_add(4)?).read().ok().filter(|&p| p != 0)?;
        if i32_at(jgl, 0x24)? != 8 { return None; }
        let (w, h) = image_size(image)?;
        let row = i32_at(jgl, 0x40)?;
        if w <= 0 || h <= 0 || row < w { return None; }
        let len = usize::try_from(row).ok()?.checked_mul(usize::try_from(h).ok()?)?;
        let bits = Ptr32::<u32>::new(jgl.checked_add(0x4C0)?).read().ok().filter(|&p| p != 0)?;
        Some((w as u32, h as u32, row as u32, Ptr32::<u8>::new(bits).read_array(len).ok()?))
    }

    /// Palette attached to a decoded image. A null optional palette in
    /// PCX_Image::read_file does not mean the image has no palette.
    pub fn image_color_entries(image: u32) -> Option<Vec<u32>> {
        let jgl = Ptr32::<u32>::new(image.checked_add(4)?).read().ok().filter(|&p| p != 0)?;
        let table = Ptr32::<u32>::new(jgl.checked_add(0x7C)?).read().ok().filter(|&p| p != 0)?;
        Ptr32::<u32>::new(table.checked_add(0x0C)?).read_array(256).ok()
    }

    /// Sprite+8 selects the native lazy file-load branch. At8060 this
    /// branch loads a PCX and calls its hooked ordinary draw, without
    /// applying the wrapper's crop or scale.
    pub fn sprite_loads_file(sprite: u32) -> bool {
        Ptr32::<u32>::new(sprite.wrapping_add(8)).read().is_ok_and(|path| path != 0)
    }

    /// Span-compiled sprites skip the plain scaler's extra integer-ratio
    /// origin offset. This flag is part of JGLSprite's checked draw dispatch.
    pub fn sprite_compiled(sprite: u32) -> Option<bool> {
        let jgl = Ptr32::<u32>::new(sprite.wrapping_add(4)).read().ok().filter(|&p| p != 0)?;
        Ptr32::<u32>::new(jgl + 0x18).read().ok().map(|flags| flags & 1 != 0)
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

/// Which of the navigator's two source images the engine has selected.
/// 0x579E70 switches `Navigator_Data.PCX_0` at +4 between bright terrain
/// at +0x18 and the territory/city image at +0x2D0.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum NavigatorMode {
    #[default]
    Territory,
    Geography,
}

impl NavigatorMode {
    pub fn from_source(navigator: u32, source: u32) -> Self {
        if source == navigator.wrapping_add(L::NAVIGATOR_DATA_PCX_1) { Self::Geography } else { Self::Territory }
    }
}

/// The HUD and advisor have independent navigator objects and toggles.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct NavigatorModes {
    pub main: NavigatorMode,
    pub advisor: NavigatorMode,
}

impl NavigatorModes {
    pub fn for_surface(self, name: &str) -> NavigatorMode {
        if name == "minimap-advisor" { self.advisor } else { self.main }
    }
}

impl crate::Civ3 {
    /// Reads the selected source, rather than inferring it from a button's
    /// transient appearance. No image pixels are read.
    pub fn navigator_modes(&self) -> Result<NavigatorModes, crate::Civ3Error> {
        let mut modes = NavigatorModes::default();
        for &(surface, name) in generated_surfaces(self.build) {
            let navigator = surface - 0x858;
            let source = Ptr32::<u32>::new(navigator + 4).read()?;
            let mode = NavigatorMode::from_source(navigator, source);
            match name {
                "minimap" => modes.main = mode,
                "minimap-advisor" => modes.advisor = mode,
                _ => {}
            }
        }
        Ok(modes)
    }
}

#[cfg(test)]
mod navigator_mode_tests {
    use super::*;

    #[test]
    fn the_selected_source_follows_the_engine_toggle() {
        for &(surface, _) in generated_surfaces_pcg() {
            let navigator = surface - 0x858;
            assert_eq!(NavigatorMode::from_source(navigator, navigator + 0x2D0), NavigatorMode::Territory);
            assert_eq!(NavigatorMode::from_source(navigator, navigator + 0x18), NavigatorMode::Geography);
            assert_eq!(NavigatorMode::from_source(navigator, navigator + 0x2D0), NavigatorMode::Territory);
            assert_eq!(NavigatorMode::from_source(navigator, 0), NavigatorMode::Territory);
        }
        let modes = NavigatorModes { main: NavigatorMode::Geography, advisor: NavigatorMode::Territory };
        assert_eq!(modes.for_surface("minimap"), NavigatorMode::Geography);
        assert_eq!(modes.for_surface("minimap-advisor"), NavigatorMode::Territory);
    }
}

/// The navigator operations used by the summary replay. These express
/// terrain and history changes, independently of the game's image pixels.
pub struct ReplaySites {
    pub init: Site,
    pub tint: Site,
    pub restore: Site,
    pub add_city: Site,
    pub erase_city: Site,
    pub terrain: Site,
}

pub fn replay_sites(build: Build) -> Option<ReplaySites> {
    match build {
        Build::PcGames => Some(ReplaySites {
            init: (0x57_8050, &[0x64, 0xA1, 0, 0, 0, 0]),
            tint: (0x57_9290, &[0x83, 0xEC, 0x1C, 0x8B, 0x15, 0x94, 0x74, 0x9C, 0]),
            restore: (0x57_9510, &[0x83, 0xEC, 0x24, 0xA1, 0x94, 0x74, 0x9C, 0]),
            add_city: (0x57_97B0, &[0x51, 0x55, 0x56, 0x57, 0x8B, 0xF9]),
            erase_city: (0x57_99B0, &[0x83, 0xEC, 0x38, 0x53, 0x55]),
            terrain: (0x57_A760, &[0x83, 0xEC, 0x34, 0x53, 0x55]),
        }),
        Build::Gog | Build::Steam => None,
    }
}

/// Whether this navigator belongs to a summary replay form. The form's
/// layout is checked by the executable's callers, rather than pixel data.
pub fn replay_navigator(build: Build, navigator: u32) -> bool {
    build == Build::PcGames && Ptr32::<u32>::new(navigator.wrapping_sub(L::GAME_RESULTS_FORM_NAVIGATOR_DATA)).read().ok() == Some(0x66_E838)
}

/// Whether an image belongs to one of the summary replay navigator's four
/// caller-written buffers. The replay form owns a complete navigator just as
/// the main HUD does, but its images are created after the static surface
/// list is published.
pub fn replay_data_image(build: Build, image: u32) -> bool {
    build == Build::PcGames && [0x18, 0x2D0, 0x588, 0x858].into_iter().any(|offset| replay_navigator(build, image.wrapping_sub(offset)))
}

pub fn navigator_size(navigator: u32) -> Option<(u32, u32)> {
    let w = Ptr32::<u32>::new(navigator.wrapping_add(L::NAVIGATOR_DATA_MINI_MAP_WIDTH2)).read().ok()?;
    let h = Ptr32::<u32>::new(navigator.wrapping_add(L::NAVIGATOR_DATA_MINI_MAP_HEIGHT2)).read().ok()?;
    (w > 0 && h > 0 && w <= 4096 && h <= 4096).then_some((w, h))
}

/// An image the game fills pixel by pixel that belongs to a form: it is at
/// `offset` from every form of class `vtable`. The renderer makes its
/// picture from world and replay state.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct FormSurface {
    pub vtable: u32,
    pub offset: u32,
    pub name: &'static str,
}

/// The name of the Load Scenario screen's map of the highlighted scenario.
pub const SCENARIO_MAP: &str = "scenario-map";

/// The Load Scenario (and Conquests!) screen, class 0x66EAB0 (docs §19.1,
/// §19.3), keeps its map of the scenario in an image of its own at +0x59C:
/// 0x59EA10 makes it (230 × 110, or narrower or lower in the map's
/// proportions) and fills it with the navigator's terrain rasterizer
/// 0x57A760 when the scenario has a map; its draw 0x59EAC0 copies it
/// (0x5FCB30 at 0x59F0FB) centred into the box at (623, 531).
const SCENARIO_FORM: u32 = 0x66_EAB0;
const SCENARIO_FORM_MAP_IMAGE: u32 = 0x59C;

/// The [`FormSurface`]s of `build`: the summary replay's map (its
/// navigator's political image, 426×202 in the games measured, copied onto
/// the replay's canvas. The navigator is at form +0x51E4 and the source image
/// at navigator +0x2D0, rather than its explored-only image at +0x858), and
/// the Load Scenario screen's map of the scenario.
pub fn form_surfaces(build: Build) -> &'static [FormSurface] {
    match build {
        Build::PcGames => &[
            FormSurface { vtable: 0x66_E838, offset: L::GAME_RESULTS_FORM_NAVIGATOR_DATA + L::NAVIGATOR_DATA_PCX_2, name: "replay-map" },
            FormSurface { vtable: SCENARIO_FORM, offset: SCENARIO_FORM_MAP_IMAGE, name: SCENARIO_MAP },
        ],
        Build::Gog | Build::Steam => &[],
    }
}

/// The form a canvas belongs to (the canvas is at its +4 + the base form's
/// canvas offset).
fn form_of_canvas(canvas: u32) -> u32 {
    canvas.wrapping_sub(4 + crate::layout::BASE_FORM_DATA_CANVAS)
}

/// The [`FormSurface`] of `surfaces` that `image`, copied onto `canvas` of
/// the form of class `vtable`, is: one of that form's own.
fn find_form_surface(surfaces: &[FormSurface], image: u32, canvas: u32, vtable: u32) -> Option<FormSurface> {
    let form = form_of_canvas(canvas);
    surfaces.iter().find(|s| form.wrapping_add(s.offset) == image && s.vtable == vtable).copied()
}

/// The [`FormSurface`] that `image`, copied onto `canvas` (a form's
/// canvas), is: one of the form's own.
pub fn form_surface(build: Build, image: u32, canvas: u32) -> Option<FormSurface> {
    let vtable = Ptr32::<u32>::new(form_of_canvas(canvas)).read().ok()?;
    find_form_surface(form_surfaces(build), image, canvas, vtable)
}

/// Whether `image` is one of the images that a known form fills itself. This
/// check runs from the image owner at construction time, before a later copy
/// can register the image in the bridge's event stream.
pub fn form_surface_image(build: Build, image: u32) -> bool {
    form_surfaces(build).iter().any(|surface| {
        let form = image.wrapping_sub(surface.offset);
        form != 0 && Ptr32::<u32>::new(form).read().ok() == Some(surface.vtable)
    })
}

#[cfg(test)]
mod form_surface_tests {
    use super::*;

    #[test]
    fn the_scenario_screens_own_map_image_is_its_scenario_map() {
        let form = 0x0123_0000u32;
        let canvas = form + 4 + crate::layout::BASE_FORM_DATA_CANVAS;
        let surfaces = form_surfaces(Build::PcGames);
        let found = find_form_surface(surfaces, form + 0x59C, canvas, 0x66_EAB0);
        assert_eq!(found.map(|s| s.name), Some(SCENARIO_MAP));
        // Not another form's image at that place, nor another image of it.
        assert_eq!(find_form_surface(surfaces, form + 0x59C, canvas, 0x66_E838), None);
        assert_eq!(find_form_surface(surfaces, form + 0x5A0, canvas, 0x66_EAB0), None);
        // The replay's map stays its own.
        let replay = form + L::GAME_RESULTS_FORM_NAVIGATOR_DATA + L::NAVIGATOR_DATA_PCX_2;
        assert_eq!(find_form_surface(surfaces, replay, canvas, 0x66_E838).map(|s| s.name), Some("replay-map"));
    }
}

inproc::fastcall_hook! {
    /// jgl.dll `JGL_Image::pixel_at(this, x, y)` (vtable slot +0x0C; +0x14
    /// and +0x1C jump to it): a pointer to the pixel (x, y).
    pub mod jgl_pixel_at(image, edx; x, y) -> u32;
}

inproc::fastcall_hook! {
    /// jgl.dll `JGL_Image::lock(this)` (vtable slot +0x10; +0x18 and +0x20
    /// jump to it): the pixels (the DIB section's bits).
    pub mod jgl_lock(image, edx;) -> u32;
}

/// The `JGL_Image` vtable slots giving out its pixels (`pixel_at`, `lock`)
/// in jgl.dll loaded at `base`, and the functions expected there (RVAs
/// 0x1BC0, 0x1B70): every read or write of a canvas's pixels that is not
/// one of jgl's own drawing operations goes through them.
pub fn jgl_pixel_slots(base: u32) -> [(Ptr32<u32>, u32); 2] {
    let vt = base + 0x6_8238;
    [(Ptr32::new(vt + 0x0C), base + 0x1BC0), (Ptr32::new(vt + 0x10), base + 0x1B70)]
}

/// Images whose pixels the game itself reads back (so drawing onto them
/// must not be skipped, see `CIV3R_GAME_RENDER`): the four images of each
/// navigator (minimap) of [`generated_surfaces`] (`Navigator_Data` +0x18,
/// +0x2D0, +0x588, +0x858, docs §17.1): the minimap's builders copy them
/// into each other and read and write their pixels one by one.
pub fn read_back_images(build: Build) -> Vec<u32> {
    generated_surfaces(build)
        .iter()
        .flat_map(|&(shown, _)| {
            let nav = shown - 0x858;
            [0x18, 0x2D0, 0x588, 0x858].map(|o| nav + o)
        })
        .collect()
}

/// Whether an image is one of the established HUD/advisor navigator data
/// buffers. Copies among these preserve game-written data, not canvas draws.
pub fn navigator_data_image(build: Build, image: u32) -> bool {
    generated_surfaces(build).iter().any(|&(shown, _)| {
        [0x18, 0x2D0, 0x588, 0x858].into_iter().any(|offset| image == shown - 0x858 + offset)
    })
}

#[cfg(test)]
mod navigator_data_tests {
    use super::*;

    #[test]
    fn only_the_known_navigators_own_data_buffers() {
        for image in read_back_images(Build::PcGames) {
            assert!(navigator_data_image(Build::PcGames, image));
            assert!(!navigator_data_image(Build::PcGames, image + 4));
            assert!(!navigator_data_image(Build::Gog, image));
            assert!(!navigator_data_image(Build::Steam, image));
        }
        assert!(!navigator_data_image(Build::PcGames, 0));
        assert!(!navigator_data_image(Build::PcGames, 0x9FDD84), "the HUD canvas is not a data buffer");
    }
}

/// The `PCX_Image` owning a `JGL_Image` (its +0x80 back pointer).
pub fn jgl_owner(jgl_image: u32) -> Option<u32> {
    Ptr32::<u32>::new(jgl_image.wrapping_add(0x80)).read().ok().filter(|&p| p != 0)
}

/// The game's Preferences screen (docs §16.11): a form of its own class, of
/// which there is one instance, a static object.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct PreferencesForm {
    /// Its vtable (`Base_Form` derived: m22 0x587B50 draws it).
    pub vtable: u32,
    /// The instance (`this` of its constructor 0x5877C0 and of 0x588370,
    /// which opens it from the title menu and the game's menu).
    pub form: u32,
    /// Its background, drawn centred on its screen-sized canvas (the path
    /// it asks `BIC_get_asset_path` for).
    pub background: &'static str,
}

/// The Preferences screen's form (PCGames build).
pub fn preferences_form(build: Build) -> Option<PreferencesForm> {
    match build {
        Build::PcGames => Some(PreferencesForm { vtable: 0x66_E628, form: 0xC7_2888, background: "art\\interface\\x_pref background.pcx" }),
        Build::Gog | Build::Steam => None,
    }
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

/// The game's screens a renderer may place elsewhere, by the class
/// (vtable) of the form showing them, and the name the viewer's settings
/// give them (docs §16.12): the advisors F1-F8, the city screen, the palace
/// (F9), demographics (F11; after a game also its "Top 5 Cities"), the
/// espionage screen, the game menu and the Civilopedia. GOG's vtables are
/// PCGames'; Steam's only where C3X's tables give them.
pub fn screen_forms(build: Build) -> &'static [(u32, &'static str)] {
    match build {
        Build::PcGames | Build::Gog => &[
            (0x66_C270, "F1"),
            (0x66_F750, "F2"),
            (0x66_DF50, "F3"),
            (0x66_C930, "F4"),
            (0x66_BBC0, "F5"),
            (0x66_F420, "F6"),
            (0x66_FF98, "F7"),
            (0x66_D380, "F8"),
            (0x66_BF10, "F11"),
            (0x66_58B0, "City"),
            (0x66_E0F8, "Palace"),
            (0x66_C5E0, "Spy"),
            (0x66_DD80, "Menu"),
            (0x66_B0A8, "Help"),
        ],
        Build::Steam => &[
            (0x68_935C, "F1"),
            (0x68_C80C, "F2"),
            (0x68_B03C, "F3"),
            (0x68_9A20, "F4"),
            (0x68_8CB8, "F5"),
            (0x68_C4DC, "F6"),
            (0x68_2914, "City"),
            (0x68_96D0, "Spy"),
            (0x68_81A8, "Help"),
        ],
    }
}

/// The class (vtable) of the city screen's own buttons (`Button`'s city
/// screen subclass): its production box, the one big one at its lower right
/// (docs §16.12).
pub fn city_screen_button_vtable(build: Build) -> Option<u32> {
    match build {
        Build::PcGames | Build::Gog => Some(0x66_5F20),
        Build::Steam => None,
    }
}

/// Buttons of the game's main interface (`Main_GUI`, docs §16.12), by
/// their canvases (each button is a form with a canvas of its own).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HudButtons {
    /// The three buttons in the top-left corner: menu, Civilopedia,
    /// advisors.
    pub corner: [u32; 3],
    /// The active unit's orders (`Unit_Command_Buttons[42]`): the first
    /// one's canvas, the distance to the next, how many.
    pub unit_orders: (u32, u32, u32),
}

impl HudButtons {
    /// Whether `canvas` is one of the unit's order buttons.
    pub fn is_unit_order(&self, canvas: u32) -> bool {
        let (first, stride, count) = self.unit_orders;
        canvas >= first && canvas < first + stride * count && (canvas - first) % stride == 0
    }
}

/// `Main_GUI`, the map screen's interface (PCGames 0x9FDB10; GOG 0x40 and
/// Steam 0x22840 further, as C3X's globals around it).
fn main_gui(build: Build) -> u32 {
    match build {
        Build::PcGames => 0x9F_DB10,
        Build::Gog => 0x9F_DB50,
        Build::Steam => 0xA2_0350,
    }
}

/// The screens shown only once a game has ended (won, lost or retired), by
/// the class (vtable) of their forms: the victory screen, the summary
/// replay, the historical ranking and the hall of fame (the end-of-game
/// sequence at 0x4F1572, PCGames). `Main_GUI` is still composed under
/// them, but there is no game to show any more: the game leaves its map
/// black there. Between the replay and the ranking the sequence also opens
/// demographics (F11, 0x66BF10, as "Top 5 Cities") and the histograph (F8,
/// 0x66D380): the screens of a running game, which alone do not tell that
/// it has ended; what was shown before them does.
pub fn end_of_game_forms(build: Build) -> &'static [u32] {
    match build {
        Build::PcGames => &[0x66_B888, 0x66_E838, 0x66_B700, 0x66_D1F0],
        Build::Gog | Build::Steam => &[],
    }
}

/// The screens after a game that a renderer places as it places the
/// advisors ([`screen_forms`]), by the class (vtable) of their forms, with
/// a name: the summary replay, the historical ranking and the hall of
/// fame. Not the victory screen: its form covers the whole window, black
/// round its board.
pub fn end_of_game_screens(build: Build) -> &'static [(u32, &'static str)] {
    match build {
        Build::PcGames => &[(0x66_E838, "Replay"), (0x66_B700, "Ranking"), (0x66_D1F0, "Hall of Fame")],
        Build::Gog | Build::Steam => &[],
    }
}

/// The canvas of `Main_GUI` (a form, its canvas at +0x274): composed while
/// a game runs, under any screen open over the map. (The title menus are
/// drawn over the main screen form as well, but without it.)
pub fn main_gui_canvas(build: Build) -> u32 {
    main_gui(build) + 4 + crate::layout::BASE_FORM_DATA_CANVAS
}

pub fn hud_buttons(build: Build) -> HudButtons {
    // `Main_GUI` + the field + 0x274 (the canvas).
    let main_gui = main_gui(build);
    let canvas = |field: u32| main_gui + field + 0x274;
    HudButtons { corner: [canvas(0x8B30), canvas(0x98D8), canvas(0x845C)], unit_orders: (canvas(0x1_6CE4), 0x6DC, 42) }
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
    let clip = canvas_text_clip(image)?;
    let jgl = Ptr32::<u32>::new(image.wrapping_add(4)).read().ok().filter(|&p| p != 0)?;
    let full = obj::rect(jgl + 0x54)?;
    (clip != full && clip[2] > clip[0] && clip[3] > clip[1]).then_some(clip)
}

/// The native text draw clip. Empty rectangles suppress text as well.
/// Unlike `canvas_clip`, this retains full and empty clipping rectangles.
pub fn canvas_text_clip(image: u32) -> Option<[i32; 4]> {
    let jgl = Ptr32::<u32>::new(image.wrapping_add(4)).read().ok().filter(|&p| p != 0)?;
    obj::rect(jgl + 0x44)
}

