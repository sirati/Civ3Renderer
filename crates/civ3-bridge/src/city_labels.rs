//! The labels the game draws beside cities on its map (docs §10.8), ported
//! from `Main_Screen_Form::draw_city_hud` (PCG 0x4E5640) so that they can be
//! drawn over any view of the world, not only where the game's own screen
//! shows them.
//!
//! [`labels`] gives, per city, the operations the game performs for its label —
//! the frame lines, the translucent plates, the texts, the capital's star and
//! the harbour, airport and barracks icons — in the order it performs them, at
//! the pixels of the world (the game's canvas position plus its camera).
//! [`LabelOp::draws`] turns them into the mirror's screen draws
//! ([`crate::ui::ScreenDraw`]) at a given offset, which `ui_screen` already
//! knows how to show; `civ3-tool labels` compares them with what the mirror
//! recorded from the game itself.

use std::collections::HashMap;

use civ3_assets::GameFiles;
use civ3_assets::fonts::{FontStyle, GlyphFont};
use civ3_sys::city_label::NEVER;

use crate::text::TextRun;
use crate::ui::{Frame, ScreenDraw, covered_alpha, line_fills, rgb555, shade_alpha, smooth_lines};
use crate::ui_screen::{self, Item};
use crate::world::{Civ3City, Civ3View, Civ3World};

/// The face of all the game's text (`LSANS.TTF`).
pub const FACE: &str = "Lucida Sans";

/// The art of the icons, as the game opens it (`Art\Cities`).
const CITY_ICONS: &str = "Art/Cities/city icons.pcx";
const HARBOR_AND_AIRPORT: &str = "Art/Cities/airAndHarb.pcx";
const BARRACKS: &str = "Art/Cities/barracks.pcx";

/// The cells the game cuts the icons from (`0x54B7E0`, `0x54A5B0`; an odd
/// width is made even, §16.3): the HUD icons 0 (the spies' wonder), 1 (the
/// capital's star) and 2 (the diplomats' coin), then the harbour, the airport
/// and the barracks.
const HUD_CELLS: [[i32; 4]; 3] = [[1, 1, 18, 18], [20, 1, 18, 18], [39, 1, 18, 18]];
const HARBOR_CELL: [i32; 4] = [27, 1, 24, 17];
const AIRPORT_CELL: [i32; 4] = [1, 1, 24, 17];
const BARRACKS_CELL: [i32; 4] = [1, 1, 24, 17];

/// The preference flags that change the label (`p_preferences`).
const P_COLOR_BLIND_HELP: u32 = 0x20_0000;
const P_SHOW_CITY_POP_DROPSHADOW: u32 = 0x200_0000;

/// `City.Common_Policy` bits the label shows: outlines of the size box and of
/// the production plate.
const POLICY_SIZE_BOX: i32 = 0x1;
const POLICY_PRODUCTION: i32 = 0x10;

/// The label's rectangles, relative to the city tile (docs §10.8).
const FRAME_HEIGHT: i32 = 21;
const NAME_PLATE_HEIGHT: i32 = 10;
const BOX_WIDTH: i32 = 24;
/// The plate is the text's width plus this, made even.
const TEXT_PADDING: i32 = 12;

/// The game's text colours (`0x8000xxxx` words in 15-bit colour, as the text
/// routine decodes them: `r = c >> 7 & 0xF8`, ...).
const WHITE: [u8; 3] = [248, 248, 248];
const YELLOW: [u8; 3] = [248, 248, 0];
const RED: [u8; 3] = [248, 0, 0];
const BLACK: [u8; 3] = [0, 0, 0];
/// What the production plate is shaded with (`0x80004945` in 565, `0x800024A5`
/// in 555 as the game computes it), as the translucent fills show it.
const PLATE_RGB: [u8; 3] = [74, 41, 41];
/// The outlines' colour (`0x80007FFF`) as the line and fill functions show it.
const OUTLINE_RGB: [u8; 3] = [255, 255, 255];

/// One drawing operation of a label.
#[derive(Clone, Debug, PartialEq)]
pub enum LabelOp {
    /// A line of the game's OpenGL line drawer (§10.9), `width` pixels wide,
    /// opaque.
    Line { from: (i32, i32), to: (i32, i32), rgb: [u8; 3], width: i32 },
    /// A rectangle (x, y, w, h) of `rgb`, mixed in `alpha` of 255 (255: opaque,
    /// else one of the game's translucent fills).
    Fill { rect: [i32; 4], rgb: [u8; 3], alpha: u8 },
    /// Region `rect` (x, y, w, h) of an art file with its top-left at `at`,
    /// colour index 255 transparent. (The game trims the transparent border
    /// and draws what is left: the same pixels.)
    Image { path: &'static str, rect: [i32; 4], at: (i32, i32) },
    /// Text with its pen at the run's position (a baseline).
    Text(TextRun),
}

impl LabelOp {
    /// The screen draws of the operation moved by `origin` (the world's offset
    /// on the screen: minus the camera), clipped to `clip` (left, top, right,
    /// bottom). A line is the rectangles of the game's line rule (smooth
    /// on Windows, see [`crate::ui::line_fills`]).
    pub fn draws(&self, origin: (i32, i32), clip: [i32; 4]) -> Vec<ScreenDraw> {
        self.draws_with(origin, clip, smooth_lines())
    }

    /// [`LabelOp::draws`], lines smooth or not.
    pub fn draws_with(&self, origin: (i32, i32), clip: [i32; 4], smooth: bool) -> Vec<ScreenDraw> {
        let (ox, oy) = origin;
        match self {
            LabelOp::Line { from, to, rgb, width } => line_fills((from.0 + ox, from.1 + oy), (to.0 + ox, to.1 + oy), *width, smooth)
                .into_iter()
                .map(|(rect, coverage)| ScreenDraw::Fill { rect, rgb: *rgb, alpha: covered_alpha(255, coverage), clip })
                .collect(),
            LabelOp::Fill { rect: [x, y, w, h], rgb, alpha } => vec![ScreenDraw::Fill { rect: [x + ox, y + oy, *w, *h], rgb: *rgb, alpha: *alpha, clip }],
            LabelOp::Image { path, rect, at } => {
                vec![ScreenDraw::Image { path: (*path).to_owned(), rect: *rect, x: at.0 + ox, y: at.1 + oy, keyed: true, clip, mask: None, palette: None, opacity: 255 }]
            }
            LabelOp::Text(run) => vec![ScreenDraw::Text { run: TextRun { x: run.x + ox, y: run.y + oy, ..run.clone() }, clip }],
        }
    }
}

/// What the game draws for one city's label, in drawing order.
#[derive(Clone, Debug, PartialEq)]
pub struct CityLabel {
    /// The city's id.
    pub city: i32,
    pub items: Vec<LabelOp>,
}

impl CityLabel {
    /// The screen draws of all the label's operations (see [`LabelOp::draws`]).
    pub fn draws(&self, origin: (i32, i32), clip: [i32; 4]) -> Vec<ScreenDraw> {
        self.items.iter().flat_map(|op| op.draws(origin, clip)).collect()
    }
}

/// The labels as the mirror's items, ready for `ui_screen::add_art` and
/// `ui_screen::sprites` (the art and fonts they need, the free sprites):
/// moved by `origin` (where the world's pixel (0, 0) is on the screen: minus the
/// camera) and clipped to `clip`. `under`: drawn under the fog of war, as the
/// game's own labels are (`Item`'s flag).
pub fn items(labels: &[CityLabel], origin: (i32, i32), clip: [i32; 4], under: bool) -> Vec<Item> {
    let draws: Vec<ScreenDraw> = labels.iter().flat_map(|l| l.draws(origin, clip)).collect();
    let frame = Frame { underlay_draws: if under { vec![0..draws.len()] } else { Vec::new() }, draws, ..Default::default() };
    // Our images are named as the art roots name them already.
    ui_screen::items(&frame, &[], &[], 0)
}

/// What a label needs from outside the world: the game's text routine, its
/// colour tables and the art of its icons.
pub trait Env {
    /// The width the game's text routine (`PCX_Image_process_text`, 0x5FE440)
    /// measures for `text` in `font`.
    fn text_width(&mut self, font: &FontStyle, text: &str) -> i32;
    /// Entry 68 of civ colour table `table` (`Art/Units/Palettes/ntpNN.pcx`) as
    /// 8-bit RGB: the colour of a civ's label (`Leader_get_label_color`, 0x55A3B0).
    fn table_color(&mut self, table: i32) -> [u8; 3];
    /// The width of the sprite the game cuts from region `cell` of the art
    /// file `path`: its transparent columns on both sides trimmed (`Sprite+0x10`).
    fn sprite_width(&mut self, path: &str, cell: [i32; 4]) -> i32;
}

/// [`Env`] reading the game's files: its fonts' advances (as `text` loads them
/// for drawing), palettes and icons. `F`: the files, owned (`Box`) or borrowed.
pub struct FileEnv<F: std::ops::Deref<Target = GameFiles>> {
    files: F,
    fonts: HashMap<FontStyle, Option<GlyphFont>>,
    colors: HashMap<i32, [u8; 3]>,
    widths: HashMap<(String, [i32; 4]), i32>,
}

impl<F: std::ops::Deref<Target = GameFiles>> FileEnv<F> {
    pub fn new(files: F) -> Self {
        Self { files, fonts: HashMap::new(), colors: HashMap::new(), widths: HashMap::new() }
    }
}

impl<F: std::ops::Deref<Target = GameFiles>> Env for FileEnv<F> {
    fn text_width(&mut self, font: &FontStyle, text: &str) -> i32 {
        let files = &*self.files;
        let loaded = self.fonts.entry(font.clone()).or_insert_with(|| civ3_assets::fonts::load(files, font, WHITE).inspect_err(|e| log::warn!("font {font:?}: {e}")).ok());
        // The game measures text on a monochrome memory DC (`gdi_text::Glyph::extent`), not as it
        // draws it; a character the font does not have takes no room.
        loaded.as_ref().map_or(0, |f| text.chars().filter_map(|c| f.glyphs.get(&c)).map(|g| g.extent).sum())
    }

    fn table_color(&mut self, table: i32) -> [u8; 3] {
        let files = &*self.files;
        *self.colors.entry(table).or_insert_with(|| {
            let image = files.read(&format!("Art/Units/Palettes/ntp{table:02}.pcx")).ok().and_then(|b| civ3_assets::pcx::decode(&b).ok());
            image.map_or([0; 3], |i| {
                let c = i.palette.get(68);
                [c.0, c.1, c.2]
            })
        })
    }

    fn sprite_width(&mut self, path: &str, cell: [i32; 4]) -> i32 {
        let files = &*self.files;
        *self.widths.entry((path.to_owned(), cell)).or_insert_with(|| {
            let Some(image) = files.read(path).ok().and_then(|b| civ3_assets::pcx::decode(&b).ok()) else { return cell[2] };
            trimmed(&image, cell).map_or(0, |t| t[2])
        })
    }
}

/// `E`, but text measured by the system's own text routine where there is
/// one (`civ3_assets::fonts::measure`): in the game's process the widths the
/// game measures, whatever system it runs on.
pub struct SystemTextEnv<E> {
    pub inner: E,
    widths: HashMap<(FontStyle, String), i32>,
}

impl<E> SystemTextEnv<E> {
    pub fn new(inner: E) -> Self {
        Self { inner, widths: HashMap::new() }
    }
}

impl<E: Env> Env for SystemTextEnv<E> {
    fn text_width(&mut self, font: &FontStyle, text: &str) -> i32 {
        if let Some(&w) = self.widths.get(&(font.clone(), text.to_owned())) {
            return w;
        }
        let w = civ3_assets::fonts::measure(font, text).unwrap_or_else(|| self.inner.text_width(font, text));
        self.widths.insert((font.clone(), text.to_owned()), w);
        w
    }

    fn table_color(&mut self, table: i32) -> [u8; 3] {
        self.inner.table_color(table)
    }

    fn sprite_width(&mut self, path: &str, cell: [i32; 4]) -> i32 {
        self.inner.sprite_width(path, cell)
    }
}

/// What the game keeps of the cell `cell` (x, y, w, h) of `image`: the rows
/// and columns that are not entirely colour 255 (§16.3, `0x62B170`), as
/// (x, y, w, h) of the image; `None` if nothing is left.
pub fn trimmed(image: &civ3_assets::Indexed, [x, y, w, h]: [i32; 4]) -> Option<[i32; 4]> {
    let mut bounds: Option<(i32, i32, i32, i32)> = None;
    for py in y.max(0)..(y + h).min(image.height as i32) {
        for px in x.max(0)..(x + w).min(image.width as i32) {
            if image.get(px as u32, py as u32) != 255 {
                let (l, t, r, b) = bounds.unwrap_or((px, py, px, py));
                bounds = Some((l.min(px), t.min(py), r.max(px), b.max(py)));
            }
        }
    }
    bounds.map(|(l, t, r, b)| [l, t, r - l + 1, b - t + 1])
}

/// A font the labels use (`get_font(size, style)`).
fn font(size: i32, bold: bool) -> FontStyle {
    FontStyle { face: FACE.to_owned(), height: size, weight: if bold { 700 } else { 0 }, italic: false }
}

/// `font[+0x10]`, which the text routines add to the top of a text's cell
/// (`PCX_Image_draw_text_in_rect`, 0x5FF0D0, and `0x5FD980`): the text's
/// `y` is the centre of the rectangle taking it into account. Measured: it
/// gives the baselines the game shows for the two sizes it uses.
fn text_height(size: i32) -> i32 {
    match size {
        10 => 7,
        _ => 14,
    }
}

/// The part of a label that depends on the world, drawing as the game does.
struct Painter<'e> {
    env: &'e mut dyn Env,
    ops: Vec<LabelOp>,
}

impl Painter<'_> {
    /// `OpenGLRenderer_draw_line` with the pen the label sets (black, 1 wide).
    fn line(&mut self, from: (i32, i32), to: (i32, i32)) {
        self.ops.push(LabelOp::Line { from, to, rgb: BLACK, width: 1 });
    }

    /// `JGL_Image::shade_rect(rect (l, t, r, b), colour, percent)`.
    fn shade(&mut self, [l, t, r, b]: [i32; 4], rgb: [u8; 3], percent: u32) {
        self.ops.push(LabelOp::Fill { rect: [l, t, r - l, b - t], rgb, alpha: shade_alpha(percent) });
    }

    /// `PCX_Image::fill_rect_outline` (0x6008D0) with the arguments the label gives: the
    /// rectangle's outline, one pixel outside its left and top edges.
    fn outline(&mut self, [l, t, r, b]: [i32; 4]) {
        // The game's `hline(x1, x2, y)` and `vline(x, y1, y2)` include both ends.
        let fill = |rect: [i32; 4]| LabelOp::Fill { rect, rgb: OUTLINE_RGB, alpha: 255 };
        self.ops.push(fill([l, t - 1, r - l, 1]));
        self.ops.push(fill([l - 1, b, r - l, 1]));
        self.ops.push(fill([l - 1, t - 1, 1, b - t + 1]));
        self.ops.push(fill([r - 1, t, 1, b - t + 1]));
    }

    /// `PCX_Image::draw_text_in_rect` (0x5FF0D0): `text` centred in the
    /// rectangle `(l, t, r, b)`, nothing if it is empty.
    fn text(&mut self, text: &str, [l, t, r, b]: [i32; 4], font: &FontStyle, color: [u8; 3]) {
        if text.is_empty() {
            return;
        }
        let width = self.env.text_width(font, text);
        let h = text_height(font.height);
        self.ops.push(LabelOp::Text(TextRun {
            x: ((r - l) - width) / 2 + l,
            y: ((b - t) - h) / 2 + t + h,
            color,
            face: font.face.clone(),
            height: -font.height,
            weight: font.weight,
            italic: false,
            underline: false,
            text: text.to_owned(),
            backdrop: None,
        }));
    }

    /// `Sprite_draw` of a sprite cut from `cell` of `path`.
    fn image(&mut self, path: &'static str, cell: [i32; 4], at: (i32, i32)) {
        self.ops.push(LabelOp::Image { path, rect: cell, at });
    }
}

/// The labels the game draws for the cities of `world`, in the order of its
/// city list: of the cities on tiles the viewing player has explored (all
/// with the debug "see everything" bit), whether or not the game's own view
/// shows them ([`in_game_view`]). `zoomed_out`: the world is the half-size one
/// of the zoomed-out view; the labels keep their size and are anchored at the
/// city tile as the game does. A world that wraps shows the map again beside
/// itself: a label near the seam belongs there too.
pub fn labels(world: &Civ3World, zoomed_out: bool, env: &mut dyn Env) -> Vec<CityLabel> {
    let bit = 1u32 << (world.player & 31);
    let see_all = world.debug_mode_bits & 8 != 0;
    world
        .cities
        .iter()
        .filter(|c| see_all || world.tile(c.x, c.y).is_some_and(|t| t.explored & bit != 0))
        .map(|c| CityLabel { city: c.id, items: city_label(world, zoomed_out, c, see_all, env) })
        .collect()
}

/// "turns" as the label writes them: `--` for the game's "never".
fn turns(t: Option<i32>) -> String {
    match t {
        Some(n) if n != NEVER => n.to_string(),
        _ => "--".to_owned(),
    }
}

/// The icons of the city's HUD box: what the game's rules gave, else only the
/// capital's star.
fn hud_icons(world: &Civ3World, city: &Civ3City) -> Vec<u8> {
    city.hud_icons.clone().unwrap_or_else(|| if world.capitals.get(city.civ as usize) == Some(&city.id) { vec![1] } else { Vec::new() })
}

/// The label of one city (`draw_city_hud`, 0x4E5640, per city).
fn city_label(world: &Civ3World, zoomed_out: bool, city: &Civ3City, see_all: bool, env: &mut dyn Env) -> Vec<LabelOp> {
    // The city tile's box (`Main_Screen_Form_tile_to_screen_coords`, 0x4E3BD0, without
    // the camera): half the tile's size per step; the label starts 10 below
    // its top. The tile is 128×64 (64×32 zoomed out).
    let (tile_w, tile_h) = if zoomed_out { (64, 32) } else { (128, 64) };
    let (sx, sy) = (tile_w / 2 * city.x, tile_h / 2 * city.y + 10);
    // The frame's top: 56 (28 zoomed out) below, `((tile_h / 4) * 7) / 2`.
    let top = sy + (tile_h / 4) * 7 / 2;
    let centre = sx + tile_w / 2;
    // The viewer's own cities (and all with the debug bit) show their food and production.
    let own = city.civ == world.player || see_all;
    let adjective = world.civ_adjectives.get(city.civ as usize).map_or("", String::as_str);
    let name_font = font(10, false);
    let mut name = city.name.clone();
    if world.preferences & P_COLOR_BLIND_HELP != 0 {
        name = format!("{name} ({adjective})");
    }
    let mut production = String::new();
    if own {
        name = format!("{name} :  {}", turns(city.grow_turns));
        if let Some(item) = city.order_name.as_deref().filter(|_| city.order.0 == 1 || city.order.0 == 2) {
            production = format!("{item} :  {}", turns(city.build_turns));
        }
    }
    let mut p = Painter { env, ops: Vec::new() };
    // The plate is as wide as the wider of the two lines, made even.
    let widest = if own { p.env.text_width(&name_font, &production).max(p.env.text_width(&name_font, &name)) } else { p.env.text_width(&name_font, &name) };
    let width = (widest + TEXT_PADDING + 1) & !1;
    let icons = hud_icons(world, city);
    let hud = if icons.is_empty() { 0 } else { BOX_WIDTH };
    let half = width / 2;
    let (left, right) = (centre - half - BOX_WIDTH, centre + half + hud);
    // The frame: black lines (the pen the function sets at its start).
    p.line((left, top), (right, top));
    p.line((left, top), (left, top + FRAME_HEIGHT));
    p.line((right, top), (right, top + FRAME_HEIGHT));
    p.line((left, top + FRAME_HEIGHT), (right, top + FRAME_HEIGHT));
    // The name on a plate that darkens the map by half.
    let color = {
        let table = world.color_tables.get(city.civ as usize).copied().unwrap_or(0);
        let [r, g, b] = p.env.table_color(table);
        rgb555(((r as u32 >> 3) << 10) | ((g as u32 >> 3) << 5) | (b as u32 >> 3))
    };
    let name_plate = [centre - half, top, centre + half, top + NAME_PLATE_HEIGHT];
    p.shade(name_plate, BLACK, 50);
    p.text(&name, name_plate, &name_font, WHITE);
    // Below it: the civ's colour in a line, the plate of the production, and
    // the size's box at the left, a quarter shaded each.
    p.shade([centre - half, top + NAME_PLATE_HEIGHT, centre + half, top + NAME_PLATE_HEIGHT + 1], color, 25);
    let production_plate = [centre - half, top + NAME_PLATE_HEIGHT + 1, centre + half, top + FRAME_HEIGHT];
    p.shade(production_plate, PLATE_RGB, 25);
    let size_box = [centre - half - BOX_WIDTH, top, centre - half, top + FRAME_HEIGHT];
    p.shade(size_box, color, 25);
    if own {
        if city.policy & POLICY_PRODUCTION != 0 {
            p.outline(production_plate);
        }
        p.text(&production, production_plate, &name_font, WHITE);
    }
    // The size: with the preference, first in black bold (a drop shadow).
    let size = city.size.to_string();
    if world.preferences & P_SHOW_CITY_POP_DROPSHADOW != 0 {
        p.text(&size, size_box, &font(16, true), BLACK);
    }
    let mut number = WHITE;
    if own {
        if city.policy & POLICY_SIZE_BOX != 0 {
            p.outline(size_box);
        }
        // A city that cannot grow shows its size in yellow, in red if it starves.
        if city.grow_turns.is_none_or(|t| t == NEVER) {
            number = if city.food_income < 0 { RED } else { YELLOW };
        }
    }
    p.text(&size, size_box, &font(16, false), number);
    // The HUD box on the right, with the icons, all drawn at one place.
    if !icons.is_empty() {
        p.shade([centre + half, top, centre + half + BOX_WIDTH, top + FRAME_HEIGHT], color, 25);
        for icon in icons {
            if let Some(cell) = HUD_CELLS.get(icon as usize) {
                p.image(CITY_ICONS, *cell, (centre + half + 2, top + 2));
            }
        }
    }
    // The icons of the buildings that train veterans: harbour and airport on
    // the tile's lower corners, the barracks (own cities) above the airport.
    let (icons_y, shift) = (sy - 10 + 47 + if zoomed_out { -32 } else { 0 }, if zoomed_out { 1 } else { 0 });
    let veteran = |bit: u8, flag: u32| city.veteran_icons.map_or_else(|| has_building_with(world, city, flag), |bits| bits & bit != 0);
    if veteran(civ3_sys::city_label::VETERAN_SEA, crate::city_hud::SEA) {
        let w = shift * p.env.sprite_width(HARBOR_AND_AIRPORT, HARBOR_CELL);
        p.image(HARBOR_AND_AIRPORT, HARBOR_CELL, (sx - w, icons_y));
    }
    if veteran(civ3_sys::city_label::VETERAN_AIR, crate::city_hud::AIR) {
        let w = shift * p.env.sprite_width(HARBOR_AND_AIRPORT, AIRPORT_CELL);
        p.image(HARBOR_AND_AIRPORT, AIRPORT_CELL, (sx + w + tile_w - 25, icons_y));
    }
    if city.civ == world.player && veteran(civ3_sys::city_label::VETERAN_GROUND, crate::city_hud::GROUND) {
        let w = shift * (p.env.sprite_width(BARRACKS, BARRACKS_CELL) - 64);
        p.image(BARRACKS, BARRACKS_CELL, (sx + w + 0x67, icons_y - 18));
    }
    p.ops
}

/// Whether `city` has a working, not obsolete improvement with `flag`
/// (`City_count_improvements_with_flag`, §10.8).
fn has_building_with(world: &Civ3World, city: &Civ3City, flag: u32) -> bool {
    world
        .icon_buildings
        .iter()
        .any(|b| b.flags & flag != 0 && city.has_improvement(b.id) && b.obsolete_for & (1 << (city.civ & 31)) == 0)
}

/// Where the game puts the top-left of the box of tile (`x`, `y`) on its map
/// canvas (`Main_Screen_Form_tile_to_screen_coords`, 0x4E3BD0): the tile's
/// distance from the first visible one, brought to the nearer side of a map
/// that wraps, in half tiles, less what the camera is past the grid. For a tile
/// that does not wrap this is `(64·x − camera_x, 32·y − camera_y)`; the labels of
/// [`labels`] are at `(64·x, 32·y)`, so this minus that is where to move them.
pub fn tile_to_screen(world: &Civ3World, view: &Civ3View, x: i32, y: i32) -> (i32, i32) {
    let (half_w, half_h) = if view.zoomed_out { (32, 16) } else { (64, 32) };
    let axis = |v: i32, min: i32, size: i32, wrap: bool, camera: i32, step: i32| {
        let mut d = v - min;
        if wrap {
            if d < 0 {
                d += size;
            } else if size <= d {
                d -= size;
            }
        }
        if size / 2 < d {
            d -= size;
        }
        step * (d - 1) - camera % step
    };
    (
        axis(x, view.tile_x.0, world.width, world.wrap_x, view.camera_x, half_w),
        axis(y, view.tile_y.0, world.height, world.wrap_y, view.camera_y, half_h),
    )
}

/// Whether the game draws `city`'s label in its own view: the part of its
/// test that depends on where the view is (`Main_Screen_Form_is_tile_on_screen`,
/// 0x4E6AB0, which keeps the tiles strictly inside the view's tile range, and a city on
/// column 0, 0x4E57BD, which it always draws).
pub fn in_game_view(world: &Civ3World, view: &Civ3View, city: &Civ3City) -> bool {
    if city.x == 0 {
        return true;
    }
    if (city.x ^ city.y) & 1 != 0 {
        return false;
    }
    // One axis: the view's range and the tile, wrapped as the map does.
    let axis = |wrap: bool, size: i32, (min, max): (i32, i32), tile: i32| {
        let norm = |v: i32| if !wrap { v } else if v < 0 { size + v } else if v >= size { v - size } else { v };
        let (mut min, max, mut tile) = (norm(min), norm(max), tile);
        if max < norm(min) {
            if norm(min) < tile && tile < size {
                tile -= size;
            }
            if min > 0 {
                min -= size;
            }
        }
        min < tile && tile < max
    };
    axis(world.wrap_x, world.width, view.tile_x, city.x) && axis(world.wrap_y, world.height, view.tile_y, city.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Civ3Tile, IconBuilding};

    /// Text as wide as it has characters, six pixels each (a stand-in for the font).
    struct Mono;

    impl Env for Mono {
        fn text_width(&mut self, _: &FontStyle, text: &str) -> i32 {
            6 * text.chars().count() as i32
        }
        fn table_color(&mut self, _: i32) -> [u8; 3] {
            [227, 10, 10]
        }
        fn sprite_width(&mut self, _: &str, cell: [i32; 4]) -> i32 {
            cell[2] - 1
        }
    }

    /// Rome, the viewer's capital, as in the first trace of the Wine runs
    /// (docs §10.8): city (44, 40), camera (1920, 772).
    fn rome() -> Civ3World {
        let mut world = Civ3World { player: 1, width: 100, height: 100, preferences: P_COLOR_BLIND_HELP, ..Default::default() };
        world.tiles = vec![Civ3Tile { explored: 2, ..Default::default() }; 100 * 50];
        world.capitals = vec![-1, 0];
        world.civ_adjectives = vec![String::new(), "Roman".into()];
        world.color_tables = vec![0, 1];
        world.cities.push(Civ3City {
            id: 0,
            x: 44,
            y: 40,
            civ: 1,
            size: 1,
            name: "Rome".into(),
            order: (2, 5),
            order_name: Some("Curragh".into()),
            policy: 1,
            grow_turns: Some(10),
            build_turns: Some(8),
            ..Default::default()
        });
        world
    }

    #[test]
    fn a_capital_label_has_the_games_frame_plates_and_texts() {
        let world = rome();
        let labels = labels(&world, false, &mut Mono);
        assert_eq!(labels.len(), 1);
        // The frame lines at screen y 574 (the camera's y is 772): 154 wide, with the star's box.
        let draws = labels[0].draws((-1920, -772), [0, 0, 1920, 1080]);
        let fills: Vec<[i32; 4]> = draws.iter().filter_map(|d| if let ScreenDraw::Fill { rect, alpha: 255, rgb: [0, 0, 0], .. } = d { Some(*rect) } else { None }).collect();
        // The text "Rome (Roman) :  10" is 18 characters (108 pixels here): a plate of 120, centred on x 960.
        assert_eq!(fills[0], [960 - 60 - 24, 574, 144 + 24, 1]);
        assert_eq!(fills.len(), 4);
        let shades: Vec<_> = draws.iter().filter_map(|d| if let ScreenDraw::Fill { rect, alpha, .. } = d { Some((*rect, *alpha)) } else { None }).filter(|(_, a)| *a != 255).collect();
        assert_eq!(shades[0], ([900, 574, 120, 10], 127));
        // The separator, the production plate, the size box, the star's box.
        assert_eq!(shades[1], ([900, 584, 120, 1], 191));
        assert_eq!(shades[2], ([900, 585, 120, 10], 191));
        assert_eq!(shades[3], ([876, 574, 24, 21], 191));
        assert_eq!(shades[4], ([1020, 574, 24, 21], 191));
        let texts: Vec<_> = draws.iter().filter_map(|d| if let ScreenDraw::Text { run, .. } = d { Some((run.x, run.y, run.height, run.text.as_str())) } else { None }).collect();
        assert_eq!(texts[0], (906, 582, -10, "Rome (Roman) :  10"));
        assert_eq!(texts[1], (900 + (120 - 72) / 2, 593, -10, "Curragh :  8"));
        assert_eq!(texts[2].3, "1");
    }

    #[test]
    fn labels_become_the_mirrors_items() {
        use menu::screen::Element;
        let world = rome();
        let labels = labels(&world, false, &mut Mono);
        let items = items(&labels, (-1920, -772), [0, 0, 1920, 1080], true);
        // Every draw of the label is one item: fills (lines too), texts, the star.
        assert_eq!(items.len(), labels[0].draws((0, 0), [0; 4]).len());
        assert!(items.iter().all(|i| matches!(i, Item::Text(_, _, true) | Item::Element(_, true))));
        let star = items.iter().find_map(|i| match i {
            Item::Element(Element::Image { region, at, .. }, _) => Some((region.source.clone(), *at)),
            _ => None,
        });
        assert_eq!(star, Some(("Art/Cities/city icons.pcx".to_owned(), (1020 + 2, 574 + 2))));
    }

    #[test]
    fn foreign_cities_show_name_and_size_only() {
        let mut world = rome();
        world.cities[0].civ = 2;
        world.color_tables.push(1);
        world.civ_adjectives.push("Greek".into());
        let ops = &labels(&world, false, &mut Mono)[0].items;
        let texts: Vec<&str> = ops.iter().filter_map(|o| if let LabelOp::Text(t) = o { Some(t.text.as_str()) } else { None }).collect();
        assert_eq!(texts, ["Rome (Greek)", "1"]);
        // No outline: only the four frame lines and the plates.
        assert!(!ops.iter().any(|o| matches!(o, LabelOp::Fill { alpha: 255, .. })));
    }

    #[test]
    fn labels_follow_the_view_only_where_the_game_draws_them() {
        let world = rome();
        let view = Civ3View { tile_x: (29, 61), tile_y: (23, 58), ..Default::default() };
        assert!(in_game_view(&world, &view, &world.cities[0]));
        // The edges are outside: the tile range is exclusive.
        let at = |x, y| Civ3City { x, y, ..Default::default() };
        assert!(!in_game_view(&world, &view, &at(29, 25)));
        assert!(in_game_view(&world, &view, &at(30, 24)));
        assert!(!in_game_view(&world, &view, &at(31, 24)));
        // Column 0 is always drawn.
        assert!(in_game_view(&world, &view, &at(0, 80)));
    }

    #[test]
    fn icons_sit_at_the_tiles_corners_and_zoomed_out_labels_move_with_the_tile() {
        let mut world = rome();
        world.icon_buildings = vec![IconBuilding { id: 3, flags: crate::city_hud::SEA | crate::city_hud::GROUND, obsolete_for: 0 }];
        world.cities[0].improvements[0] |= 1 << 3;
        let ops = &labels(&world, false, &mut Mono)[0].items;
        let images: Vec<(&str, (i32, i32))> = ops.iter().filter_map(|o| if let LabelOp::Image { path, at, .. } = o { Some((*path, *at)) } else { None }).collect();
        // The star (the HUD box's), the harbour and the barracks, in the tile's box (2816, 1280).
        assert_eq!(images[0], (CITY_ICONS, (2880 + 60 + 2, 1346 + 2)));
        assert_eq!(images[1], (HARBOR_AND_AIRPORT, (2816, 1280 + 47)));
        assert_eq!(images[2], (BARRACKS, (2816 + 103, 1280 + 29)));
        // Zoomed out the same label hangs 28 below the half-size tile's top plus 10.
        let ops = &labels(&world, true, &mut Mono)[0].items;
        assert_eq!(ops[0], LabelOp::Line { from: (1408 + 32 - 60 - 24, 640 + 10 + 28), to: (1408 + 32 + 60 + 24, 640 + 10 + 28), rgb: [0; 3], width: 1 });
    }
}
