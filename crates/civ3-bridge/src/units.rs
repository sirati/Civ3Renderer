//! Units (docs §10): which animation file each drawn unit shows, the render
//! description entries for those files in the owners' colours, and the
//! free sprites that draw them.
//!
//! Unit art is too large to load for every type and colour up front, so the
//! description entries are generated for the (animation file, colour) pairs
//! the world currently needs; a new pair means recompiling the description.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use civ3_assets::ini::Ini;
use civ3_assets::{GameFiles, flc};
use render_desc::Compiled;

use crate::rules::terrain::COAST;
use crate::world::{Civ3Unit, Civ3World};

/// `UnitState` of a fortified unit.
const STATE_FORTIFIED: i32 = 1;

/// `AnimationType` names, by value (docs §10.4).
const ANIMATION_NAMES: [&str; 19] = [
    "BLANK", "DEFAULT", "RUN", "ATTACK1", "ATTACK2", "ATTACK3", "DEATH", "FORTIFY", "FIDGET", "VICTORY", "CAPTURE",
    "FORTRESS", "BUILD", "ROAD", "MINE", "IRRIGATE", "JUNGLE", "FOREST", "PLANT",
];

/// FLC block of each `enum direction` value (table 0x73DD7C): the blocks
/// in a file are SW, S, SE, E, NE, N, NW, W.
const DIRECTION_BLOCK: [u32; 9] = [0, 4, 3, 2, 1, 0, 7, 6, 5];

/// Shadow palette indices and their darkening (`c·(i − 239)/16`).
const SHADOWS: std::ops::RangeInclusive<u32> = 248..=254;

/// The layout of one animation file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimFile {
    /// Game-relative path.
    pub path: String,
    pub frames_per_direction: u32,
    /// Frame chunks in the file (8 × (frames + 1)).
    pub chunks: u32,
    /// Frame size, and where a frame lies relative to the unit's anchor.
    pub size: (u32, u32),
    pub anchor: (i32, i32),
}

impl AnimFile {
    fn read(files: &GameFiles, path: &str) -> Option<AnimFile> {
        let anim = flc::decode(&files.read(path).ok()?).ok()?;
        let h = &anim.header;
        let u16_at = |o: usize| u16::from_le_bytes([h[o], h[o + 1]]) as i32;
        let (left, top, w, hgt) = (u16_at(0x64), u16_at(0x66), u16_at(0x68), u16_at(0x6A));
        Some(AnimFile {
            path: path.to_owned(),
            frames_per_direction: u16_at(0x62).max(1) as u32,
            chunks: anim.frames.len() as u32,
            size: (anim.width, anim.height),
            anchor: (left - (w >> 1), top - (hgt >> 1)),
        })
    }
}

/// The selection cursor's animation (`Art/Animations/Cursor/Cursor.INI`).
const CURSOR_FLC: &str = "Art/Animations/Cursor/Cursor.flc";
/// The cursor shows the block of direction 5.
const CURSOR_DIRECTION: i32 = 5;
/// `AnimationType` ATTACK1 and `enum direction` east (armies, §10.11).
const ANIMATION_ATTACK1: i32 = 3;
const DIRECTION_EAST: i32 = 2;

/// Resolves unit types to animation files.
pub struct UnitArt {
    files: GameFiles,
    /// `#ANIMNAME_...` keys of `Text/PediaIcons.txt` to folder names.
    anim_names: HashMap<String, String>,
    /// (unit type key, era key, animation) → file, cached.
    resolved: HashMap<(String, String, i32), Option<AnimFile>>,
    cursor: Option<Option<AnimFile>>,
    /// Files asked for by path, cached.
    by_path: HashMap<String, Option<AnimFile>>,
}

impl UnitArt {
    pub fn new(files: GameFiles) -> Self {
        let text = files.read("Text/PediaIcons.txt").map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let mut anim_names = HashMap::new();
        let mut lines = text.lines().map(str::trim);
        while let Some(line) = lines.next() {
            if let Some(key) = line.strip_prefix("#ANIMNAME_") {
                if let Some(value) = lines.next() {
                    anim_names.insert(key.to_owned(), value.to_owned());
                }
            }
        }
        Self { files, anim_names, resolved: HashMap::new(), cursor: None, by_path: HashMap::new() }
    }

    /// Entry `index` of colour table `table` (`ntpNN.pcx`).
    pub fn table_color(&self, table: i32, index: usize) -> Option<[u8; 3]> {
        let bytes = self.files.read(&format!("Art/Units/Palettes/ntp{table:02}.pcx")).ok()?;
        let c = civ3_assets::pcx::decode(&bytes).ok()?.palette.0[index.min(255)];
        Some([c.0, c.1, c.2])
    }

    /// The selection cursor's animation file.
    pub fn cursor(&mut self) -> Option<&AnimFile> {
        if self.cursor.is_none() {
            self.cursor = Some(AnimFile::read(&self.files, CURSOR_FLC));
        }
        self.cursor.as_ref().and_then(Option::as_ref)
    }

    /// The animation file a unit of type `type_key` (`PRTO_...`) shows for
    /// `animation` in `era_key`, falling back to DEFAULT.
    /// The animation file at `path` (e.g. one the game draws in a panel).
    pub fn file_at(&mut self, path: &str) -> Option<&AnimFile> {
        let files = &self.files;
        self.by_path.entry(path.to_owned()).or_insert_with(|| AnimFile::read(files, path)).as_ref()
    }

    pub fn file(&mut self, type_key: &str, era_key: &str, animation: i32) -> Option<&AnimFile> {
        let key = (type_key.to_owned(), era_key.to_owned(), animation);
        if !self.resolved.contains_key(&key) {
            let found = self.lookup(type_key, era_key, animation).or_else(|| self.lookup(type_key, era_key, 1));
            self.resolved.insert(key.clone(), found);
        }
        self.resolved[&key].as_ref()
    }

    fn lookup(&self, type_key: &str, era_key: &str, animation: i32) -> Option<AnimFile> {
        let name = self
            .anim_names
            .get(&format!("{type_key}_{era_key}"))
            .or_else(|| self.anim_names.get(type_key))?;
        let ini_path = format!("Art/Units/{name}/{name}.INI");
        let ini = Ini::parse_bytes(&self.files.read(&ini_path).ok()?);
        let file = ini.get("Animations", ANIMATION_NAMES.get(animation as usize)?)?.trim();
        if file.is_empty() {
            return None;
        }
        AnimFile::read(&self.files, &format!("Art/Units/{name}/{file}"))
    }
}

/// The colours unit art is drawn in: a colour table (`ntpNN.pcx`, palette
/// entries 0..63), and for an army's own art also entries 64..127 (§10.11):
/// the first 16 colours of its table, repeated, the first `members`·16 of
/// them overwritten from table `first_civ_table` (the game reads civ 0's).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Recolor {
    pub table: i32,
    pub army: Option<(i32, u8)>,
}

impl Recolor {
    fn civ(table: i32) -> Recolor {
        Recolor { table, army: None }
    }
}

impl std::fmt::Display for Recolor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.army {
            None => write!(f, "{}", self.table),
            Some((first, members)) => write!(f, "{}a{first}x{members}", self.table),
        }
    }
}

/// The unit art a world needs: animation files and, per file, the colours
/// it is drawn in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Needed {
    pub files: BTreeMap<String, (AnimFile, BTreeSet<Recolor>)>,
    /// Colour tables of city-site previews, with their entry 66 (§10.9).
    pub site_colors: BTreeMap<i32, [u8; 3]>,
    /// Animations drawn in their own colours, without shadow (leader
    /// heads): sprites `Flc@<file>/<chunk>`. Only those shown (they are
    /// big).
    pub plain: BTreeMap<String, AnimFile>,
}

fn era_key(world: &Civ3World, civ: i32) -> &str {
    let era = world.eras.get(civ.max(0) as usize).copied().unwrap_or(0);
    world.era_keys.get(era.max(0) as usize).map_or("", String::as_str)
}

fn colors(world: &Civ3World, civ: i32) -> i32 {
    world.color_tables.get(civ.max(0) as usize).copied().unwrap_or(0)
}

/// The member an army shows (§10.11), if `u` is an army with members.
fn army_member<'w>(world: &'w Civ3World, u: &Civ3Unit) -> Option<&'w Civ3Unit> {
    let id = u.army_member?;
    world.army_members.iter().find(|m| m.id == id)
}

/// The colours of an army's own art.
fn army_recolor(world: &Civ3World, u: &Civ3Unit) -> Recolor {
    Recolor { table: colors(world, u.civ), army: Some((colors(world, 0), u.army_size.clamp(0, 4) as u8)) }
}

impl Needed {
    /// The animations the world's units show now, plus each type's DEFAULT
    /// (what idle units return to), in the colours of the civs showing
    /// them. Other animations are added when a unit starts them: loading
    /// all of them up front does not fit a 32-bit process.
    pub fn of(world: &Civ3World, art: &mut UnitArt) -> Needed {
        let mut out = Needed::default();
        if let Some(cursor) = art.cursor() {
            out.files.insert(cursor.path.clone(), (cursor.clone(), [Recolor::civ(colors(world, world.player))].into()));
        }
        let mut add = |u: &Civ3Unit, civ: i32, recolor: Recolor, art: &mut UnitArt| {
            let Some(type_key) = world.unit_type_keys.get(u.unit_type.max(0) as usize) else { return };
            for animation in [1, u.animation] {
                if let Some(file) = art.file(type_key, era_key(world, civ), animation) {
                    out.files.entry(file.path.clone()).or_insert_with(|| (file.clone(), BTreeSet::new())).1.insert(recolor);
                }
            }
        };
        for u in &world.units {
            match army_member(world, u) {
                Some(member) => {
                    add(u, u.civ, army_recolor(world, u), art);
                    add(member, u.civ, Recolor::civ(colors(world, u.civ)), art);
                }
                None => add(u, u.civ, Recolor::civ(colors(world, u.civ)), art),
            }
        }
        for table in crate::city_site::needed(world) {
            if let Some(rgb) = art.table_color(table, 66) {
                out.site_colors.insert(table, rgb);
            }
        }
        out
    }

    /// Whether everything `other` needs is already here.
    pub fn covers(&self, other: &Needed) -> bool {
        other.files.iter().all(|(p, (_, c))| self.files.get(p).is_some_and(|(_, mine)| c.is_subset(mine)))
            && other.site_colors.keys().all(|c| self.site_colors.contains_key(c))
            && other.plain.keys().all(|p| self.plain.contains_key(p))
    }

    /// Animation file `path` in its own colours (replacing those before:
    /// only the ones shown are kept).
    pub fn set_plain(&mut self, art: &mut UnitArt, paths: &[String]) {
        self.plain = paths.iter().filter_map(|p| art.file_at(p).map(|f| (f.path.clone(), f.clone()))).collect();
    }

    /// Adds animation file `path` in colour table `table`.
    pub fn add_file(&mut self, art: &mut UnitArt, path: &str, table: i32) {
        if let Some(file) = art.file_at(path) {
            self.files.entry(file.path.clone()).or_insert_with(|| (file.clone(), BTreeSet::new())).1.insert(Recolor::civ(table));
        }
    }

    pub fn merge(&mut self, other: &Needed) {
        for (p, (f, c)) in &other.files {
            self.files.entry(p.clone()).or_insert_with(|| (f.clone(), BTreeSet::new())).1.extend(c);
        }
        self.site_colors.extend(other.site_colors.iter().map(|(k, v)| (*k, *v)));
        self.plain.extend(other.plain.iter().map(|(k, v)| (k.clone(), v.clone())));
    }

    /// Render description entries for this art: sprites
    /// `UnitArt@<file>#<colours>/<chunk>` and `UnitShadow@<file>/<chunk>`.
    pub fn description(&self) -> String {
        let mut out = String::from("\n# Unit art (generated, docs §10).\n");
        let all: Vec<String> = (0..256).map(|i| i.to_string()).collect();
        let body_transparent = format!("[{}, 255]", (240..=254).map(|i: u32| i.to_string()).collect::<Vec<_>>().join(", "));
        let shadow_transparent = format!(
            "[{}]",
            all.iter().enumerate().filter(|(i, _)| !SHADOWS.contains(&(*i as u32))).map(|(_, s)| s.as_str()).collect::<Vec<_>>().join(", ")
        );
        let shadow_rgba: Vec<String> = SHADOWS.map(|i| format!("{i} = [{}, 0, 0, 255]", i - 239)).collect();
        for (n, (path, (file, colors))) in self.files.iter().enumerate() {
            let source = toml_str(path);
            let sheets = |out: &mut String, image: &str, group: &str| {
                for k in 0..file.chunks {
                    let _ = writeln!(
                        out,
                        "[sheets.\"{image}_{k}\"]\nimage = \"{image}\"\nframe = {k}\ncell = [{}, {}]\nanchor = [{}, {}]\ngroup = {}\nfirst_index = {k}\n",
                        file.size.0,
                        file.size.1,
                        file.anchor.0,
                        file.anchor.1,
                        toml_str(group)
                    );
                }
            };
            let shadow = format!("unitshadow{n}");
            let _ = writeln!(out, "[images.{shadow}]\nsource = {source}\ntransparent = {shadow_transparent}\nrgba = {{ {} }}\n", shadow_rgba.join(", "));
            sheets(&mut out, &shadow, &format!("UnitShadow@{path}"));
            for c in colors {
                let image = format!("unit{n}c{c}");
                let mut palette = format!("palette = \"Art/Units/Palettes/ntp{:02}.pcx\"\npalette_range = [0, 64]\n", c.table);
                // Armies: entries 64..127 repeat the first 16 colours, the
                // first `members`·16 of them from another table (§10.11).
                if let Some((first, members)) = c.army {
                    let split = 64 + 16 * u32::from(members);
                    let map = |r: std::ops::Range<u32>| r.map(|i| format!("{i} = {}", (i - 64) % 16)).collect::<Vec<_>>().join(", ");
                    let _ = writeln!(palette, "palette_map = {{ {} }}", map(split..128));
                    let _ = writeln!(palette, "palette2 = \"Art/Units/Palettes/ntp{first:02}.pcx\"\npalette2_map = {{ {} }}", map(64..split));
                }
                let _ = writeln!(out, "[images.{image}]\nsource = {source}\n{palette}transparent = {body_transparent}\n");
                sheets(&mut out, &image, &format!("UnitArt@{path}#{c}"));
            }
        }
        // In the file's own colours, frames at their top-left.
        for (n, (path, file)) in self.plain.iter().enumerate() {
            let image = format!("flc{n}");
            let _ = writeln!(out, "[images.{image}]\nsource = {}\n", toml_str(path));
            for k in 0..file.chunks {
                let _ = writeln!(
                    out,
                    "[sheets.\"{image}_{k}\"]\nimage = \"{image}\"\nframe = {k}\ncell = [{}, {}]\ngroup = {}\nfirst_index = {k}\n",
                    file.size.0,
                    file.size.1,
                    toml_str(&format!("Flc@{path}"))
                );
            }
        }
        let sites: Vec<(i32, [u8; 3])> = self.site_colors.iter().map(|(k, v)| (*k, *v)).collect();
        out.push_str(&crate::city_site::description(&sites));
        out
    }
}

fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `P_SHOW_TEAM_COLOR_DISC`.
const SHOW_TEAM_COLOR_DISC: u32 = 0x800;

/// The free sprites drawing the world's units, after all passes (over the
/// fog of war), in the animator's order: per unit its team disc, its body
/// with shadow, then its HUD (§10.2 `Unit_draw`).
/// Where the selected unit stands (its tile centre, or where it is on its
/// way), if one is selected and shown.
pub fn focus(world: &Civ3World) -> Option<(i32, i32)> {
    let (id, _) = world.selection?;
    world.units.iter().find(|u| u.id == id && u.visible).map(|u| u.pixel)
}

/// `rings`: second rings around foreign units (`crate::rings`).
/// `zoomed_out`: the positions are those the zoomed-out view halves; the
/// HUD is not drawn smaller there, so its pieces keep their distances from
/// its origin by being placed twice as far from it here.
pub fn sprites(world: &Civ3World, art: &mut UnitArt, compiled: &Compiled, rings: Option<&crate::rings::Rings>, zoomed_out: bool) -> Vec<scene::FreeSprite> {
    let mut out = Vec::new();
    for u in world.units.iter().filter(|u| u.visible) {
        let (x, y) = u.pixel;
        let mut put = |name: &str, (px, py): (i32, i32), scale_den| {
            if let Some((set, _)) = compiled.set(name) {
                out.push(scene::FreeSprite {
                    set: set.raw(),
                    x: px,
                    y: py,
                    screen: false,
                    after_pass: u32::MAX,
                    phase: 0,
                    ticks_per_frame: 1,
                    scale_den,
                    tint: 0,
                    zoom_art: 0,
                    zoom: (0, 0),
                    zoom_offset: (0, 0),
                    clip: None,
                    not_in_view: None,
                    glyph: None,
                });
            }
        };
        let in_city = world.tile(u.x, u.y).is_some_and(|t| t.city >= 0);
        // An army shows its strongest member at the tile, its own (banner)
        // art 40 px to the right behind it, and its HUD at the member
        // (§10.11).
        let member = army_member(world, u);
        let (mx, my) = match member {
            Some(m) if !u.moving => m.pixel,
            _ => (x, y),
        };
        if world.preferences & SHOW_TEAM_COLOR_DISC != 0 && !in_city {
            put(&format!("TeamDisc@{}/0", colors(world, u.civ)), (mx - 64, my - 32), 0);
        }
        // Around it, how the viewer stands with a foreign owner.
        if let (Some(rings), false, false) = (rings, in_city, u.civ == world.player) {
            let relation = world.relations.get(u.civ.max(0) as usize).copied().unwrap_or(civ3_sys::world::Relation::None);
            if let Some((name, scale_den)) = rings.sprite(relation) {
                put(&name, (mx - 64, my - 32), scale_den);
            }
        }
        if let (Some((_, frame)), Some(cursor)) = (world.selection.filter(|s| s.0 == u.id), art.cursor()) {
            let block = DIRECTION_BLOCK[CURSOR_DIRECTION as usize];
            let chunk = (block * (cursor.frames_per_direction + 1) + frame.max(0) as u32).min(cursor.chunks.saturating_sub(1));
            put(&format!("UnitArt@{}#{}/{chunk}", cursor.path, colors(world, world.player)), (mx, my), 0);
        }
        let mut body = |unit: &Civ3Unit, recolor: Recolor, at: (i32, i32), shadow: bool| {
            let type_key = world.unit_type_keys.get(unit.unit_type.max(0) as usize);
            if let Some(file) = type_key.and_then(|k| art.file(k, era_key(world, u.civ), unit.animation)) {
                let chunk = chunk(file, unit);
                if shadow {
                    put(&format!("UnitShadow@{}/{chunk}", file.path), at, 16);
                }
                put(&format!("UnitArt@{}#{recolor}/{chunk}", file.path), at, 0);
            }
        };
        match member {
            Some(m) => {
                let attacking_east = u.animation == ANIMATION_ATTACK1 && u.direction == DIRECTION_EAST;
                body(u, army_recolor(world, u), (x + 40, y - if attacking_east { 20 } else { 0 }), false);
                body(m, Recolor::civ(colors(world, u.civ)), (mx, my), false);
            }
            None => body(u, Recolor::civ(colors(world, u.civ)), (x, y), true),
        }
        for (name, at) in hud(world, u, (mx, my), if zoomed_out { 2 } else { 1 }) {
            put(name, at, 0);
        }
    }
    out
}

/// §10.6: health bar, movement LED and stack lines of a unit, as 1×1
/// colour pixels and LED sprites at world positions.
/// `anchor`: where it is drawn (an army's HUD sits at its member). `spread`:
/// the factor the pieces' distances from the HUD's origin are multiplied by
/// (2 where positions are halved afterwards and the HUD keeps its size).
fn hud(world: &Civ3World, u: &Civ3Unit, anchor: (i32, i32), spread: i32) -> Vec<(&'static str, (i32, i32))> {
    let mut out = hud_pieces(world, u, anchor);
    let origin = (anchor.0 - 32, anchor.1 - 32);
    if spread != 1 {
        for (_, (x, y)) in &mut out {
            *x = origin.0 + spread * (*x - origin.0);
            *y = origin.1 + spread * (*y - origin.1);
        }
    }
    out
}

fn hud_pieces(world: &Civ3World, u: &Civ3Unit, anchor: (i32, i32)) -> Vec<(&'static str, (i32, i32))> {
    let mut out = Vec::new();
    let Some(&[attack, defence, movement, hp_bonus, class, _]) = world.unit_type_stats.get(u.unit_type.max(0) as usize) else {
        return out;
    };
    let has_bar = attack > 0 || defence > 0;
    let (hx, hy) = (anchor.0 - 32, anchor.1 - 32);
    let base_hp = world.experience_hit_points.get(u.experience.max(0) as usize).copied().unwrap_or(1);
    let max = (base_hp + hp_bonus).max(1);
    let hp = (max - u.damage).clamp(0, 9999);
    let pitch = if 5 * max - 1 > 40 { (41 / max).max(2) } else { 5 };
    let bar = (pitch * max - 1).min(40);
    let (left, bottom) = (hx + 6, hy + 24);
    let rect = |name: &'static str, (x0, y0, x1, y1): (i32, i32, i32, i32), out: &mut Vec<_>| {
        for py in y0..y1 {
            for px in x0..x1 {
                out.push((name, (px, py)));
            }
        }
    };
    if has_bar {
        rect("Hud_black/0", (left, bottom - bar, left + 2, bottom), &mut out);
    }
    let color = if hp <= 1 {
        "Hud_red/0"
    } else if 3 * u.damage < max || u.damage == 0 {
        "Hud_green/0"
    } else if 3 * u.damage < 2 * max {
        "Hud_yellow/0"
    } else {
        "Hud_red/0"
    };
    let mut seg_bottom = bottom;
    for _ in 0..if has_bar { hp.min(20) } else { 0 } {
        rect(color, (left, seg_bottom - pitch + 1, left + 2, seg_bottom), &mut out);
        seg_bottom -= pitch;
    }
    // §10.10: fortified land units on land with moves left get a white
    // outline around the bar (the LED drawn next covers most of its top).
    let left_moves = movement * 3 - u.moves;
    let on_water = world.tile(u.x, u.y).is_some_and(|t| t.terrain() >= COAST);
    if has_bar && class == 0 && !on_water && u.state == STATE_FORTIFIED && left_moves > 0 {
        let (x0, y0, x1, y1) = (left - 1, bottom - bar - 1, left + 2, bottom);
        rect("Hud_white/0", (x0, y0, x1 + 1, y0 + 1), &mut out);
        rect("Hud_white/0", (x0, y1, x1 + 1, y1 + 1), &mut out);
        rect("Hud_white/0", (x0, y0 + 1, x0 + 1, y1), &mut out);
        rect("Hud_white/0", (x1, y0 + 1, x1 + 1, y1), &mut out);
    }
    // Own units: how much of their move is left (moves count thirds).
    if u.civ == world.player {
        let led = if left_moves <= 0 {
            "MovementLED/2"
        } else if u.moves != 0 {
            "MovementLED/1"
        } else {
            "MovementLED/0"
        };
        out.push((led, (hx + 4, hy + 18 - bar)));
    }
    // Stack lines: one per unit on the tile (up to 8), when there are
    // several and the unit is not moving.
    if !u.always_on_top {
        let n = world.unit_tiles.iter().filter(|&&(x, y, c)| x == u.x && y == u.y && c < 0).count().clamp(1, 8) as i32;
        if n > 1 {
            for i in 0..n {
                rect("Hud_white/0", (hx + 5, hy + 26 + 2 * i, hx + 9, hy + 27 + 2 * i), &mut out);
            }
        }
    }
    out
}

/// The frame chunk a unit shows: its facing's block, its frame.
fn chunk(file: &AnimFile, u: &Civ3Unit) -> u32 {
    let block = DIRECTION_BLOCK.get(u.direction.max(0) as usize).copied().unwrap_or(2);
    (block * (file.frames_per_direction + 1) + u.frame.max(0) as u32).min(file.chunks.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One unit type (attack 1, defence 1, one move) and one experience level
    /// (3 hit points).
    fn world() -> Civ3World {
        Civ3World { unit_type_stats: vec![[1, 1, 1, 0, 0, 0]], experience_hit_points: vec![3], ..Default::default() }
    }

    #[test]
    fn a_zoomed_out_hud_keeps_its_size_at_half_the_distance() {
        // Zoomed out the sprites' positions are halved afterwards, but the
        // HUD is not drawn smaller: its pieces are 16 px from the unit's
        // half-scale anchor, at the distances they have 32 px from the
        // anchor at full scale.
        let (world, unit) = (world(), Civ3Unit::default());
        let anchor = (200, 100);
        let full = hud(&world, &unit, anchor, 1);
        let spread = hud(&world, &unit, anchor, 2);
        assert!(full.len() > 4);
        assert_eq!(full.len(), spread.len());
        let origin = (anchor.0 - 32, anchor.1 - 32);
        for ((name, (x, y)), (spread_name, (sx, sy))) in full.into_iter().zip(spread) {
            assert_eq!(name, spread_name);
            assert_eq!((sx / 2, sy / 2), (origin.0 / 2 + x - origin.0, origin.1 / 2 + y - origin.1), "{name} at ({x}, {y})");
        }
    }
}
