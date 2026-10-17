//! Units (docs §10): which animation file each drawn unit shows, the render
//! description entries for those files in the owners' colours, and where
//! they go: the unit layers of the tiles they stand on (moved by per-cell
//! offsets where they are not at their tile's centre), or free sprites
//! where those cannot keep the game's order.
//!
//! Unit art is too large to load for every type and colour up front, so the
//! description entries are generated for the (animation file, colour) pairs
//! the world currently needs; a new pair means recompiling the description.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use civ3_assets::ini::Ini;
use civ3_assets::{GameFiles, flc};
use render_desc::Compiled;
use scene::Scene;

use crate::rules::terrain::COAST;
use crate::world::{Civ3Unit, Civ3View, Civ3World};

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
    pub fn civ(table: i32) -> Recolor {
        Recolor { table, army: None }
    }

    /// The palette row unit art is drawn with in these colours: the colour
    /// table's (`ntp@NN`, civ3.toml), or an army's (generated, see
    /// [`Needed::description`]).
    pub fn palette(&self) -> String {
        match self.army {
            None => format!("ntp@{}", self.table),
            Some((first, members)) => format!("UnitArmy@{}a{first}x{members}", self.table),
        }
    }

    /// Whether the art is the army's own (entries 64..127 recoloured too).
    fn army_art(&self) -> bool {
        self.army.is_some()
    }
}

/// The name part telling an army's own art (entries 0..127 palette
/// references) from the others' (0..63).
fn art_kind(army: bool) -> &'static str {
    if army { "#army" } else { "" }
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
    /// Files of which only some frames are drawn (units standing still,
    /// [`Needed::still`]): those chunks, the only ones made. A file not
    /// listed is made whole.
    pub stills: BTreeMap<String, BTreeSet<u32>>,
}

fn era_key(world: &Civ3World, civ: i32) -> &str {
    let era = world.eras.get(civ.max(0) as usize).copied().unwrap_or(0);
    world.era_keys.get(era.max(0) as usize).map_or("", String::as_str)
}

fn colors(world: &Civ3World, civ: i32) -> i32 {
    world.color_tables.get(civ.max(0) as usize).copied().unwrap_or(0)
}

/// The civ whose colours unit `u` (an army: also its member) is drawn in
/// (§10.2 step 2): its owner's, or the barbarians' (civ 0) for a unit of
/// hidden nationality (`UTA_Hidden_Nationality`) another real civ looks
/// at, unless the debug mode shows everything.
fn color_civ(world: &Civ3World, u: &Civ3Unit) -> i32 {
    let abilities = world.unit_type_stats.get(u.unit_type.max(0) as usize).map_or(0, |s| s[6]);
    let hidden = abilities >> civ3_sys::layout::UTA_HIDDEN_NATIONALITY & 1 != 0;
    if hidden && world.player > 0 && world.player != u.civ && world.debug_mode_bits & 8 == 0 { 0 } else { u.civ }
}

/// The member an army shows (§10.11), if `u` is an army with members.
fn army_member<'w>(world: &'w Civ3World, u: &Civ3Unit) -> Option<&'w Civ3Unit> {
    let id = u.army_member?;
    world.army_members.iter().find(|m| m.id == id)
}

/// The colours of an army's own art.
fn army_recolor(world: &Civ3World, u: &Civ3Unit) -> Recolor {
    Recolor { table: colors(world, color_civ(world, u)), army: Some((colors(world, 0), u.army_size.clamp(0, 4) as u8)) }
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
                    add(member, u.civ, Recolor::civ(colors(world, color_civ(world, u))), art);
                }
                None => add(u, u.civ, Recolor::civ(colors(world, color_civ(world, u))), art),
            }
        }
        for table in crate::city_site::needed(world) {
            if let Some(rgb) = art.table_color(table, 66) {
                out.site_colors.insert(table, rgb);
            }
        }
        out
    }

    /// What [`Needed::of`] says, for units that stand still: of their
    /// animations only the frame each unit shows (the many units of a whole
    /// scenario would otherwise bring all of theirs: eight directions of
    /// every animation, in every owner's colours).
    pub fn still(world: &Civ3World, art: &mut UnitArt) -> Needed {
        let mut out = Needed::of(world, art);
        for u in &world.units {
            let Some(key) = world.unit_type_keys.get(u.unit_type.max(0) as usize) else { continue };
            let Some(file) = art.file(key, era_key(world, u.civ), u.animation) else { continue };
            let block = DIRECTION_BLOCK.get(u.direction.max(0) as usize).copied().unwrap_or(2);
            let chunk = (block * (file.frames_per_direction + 1) + u.frame.max(0) as u32).min(file.chunks.saturating_sub(1));
            out.stills.entry(file.path.clone()).or_default().insert(chunk);
        }
        out
    }

    /// Whether everything `other` needs is already here.
    pub fn covers(&self, other: &Needed) -> bool {
        // (A file made whole has every frame; one made in part the frames
        // listed.)
        let frames = |p: &String| match (self.stills.get(p), other.stills.get(p)) {
            (None, _) => true,
            (Some(mine), Some(theirs)) => theirs.is_subset(mine),
            (Some(_), None) => false,
        };
        other.files.iter().all(|(p, (_, c))| self.files.get(p).is_some_and(|(_, mine)| c.is_subset(mine)) && frames(p))
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
        for p in other.files.keys() {
            let mine_whole = self.files.contains_key(p) && !self.stills.contains_key(p);
            match other.stills.get(p) {
                _ if mine_whole => {}
                None => drop(self.stills.remove(p)),
                Some(theirs) => self.stills.entry(p.clone()).or_default().extend(theirs),
            }
        }
        for (p, (f, c)) in &other.files {
            self.files.entry(p.clone()).or_insert_with(|| (f.clone(), BTreeSet::new())).1.extend(c);
        }
        self.site_colors.extend(other.site_colors.iter().map(|(k, v)| (*k, *v)));
        self.plain.extend(other.plain.iter().map(|(k, v)| (k.clone(), v.clone())));
    }

    /// Render description entries for this art: sprites
    /// `UnitArt@<file>/<chunk>` (indexed, its indices 0..63 remapped: drawn
    /// in the colours a draw picks, [`Recolor::palette`]; an army's own art
    /// `UnitArt@<file>#army/<chunk>`, 0..127) and
    /// `UnitShadow@<file>/<chunk>`, per direction block the animations
    /// `UnitAnim@<file>[#army]/<block>` and `UnitShadowAnim@<file>/<block>`,
    /// the armies' palette rows, and the HUD's art ([`hud`]). One copy of
    /// each file's art serves every nation.
    pub fn description(&self) -> String {
        let mut out = String::from("\n# Unit art (generated, docs §10).\n");
        let all: Vec<String> = (0..256).map(|i| i.to_string()).collect();
        let body_transparent = format!("[{}, 255]", (240..=254).map(|i: u32| i.to_string()).collect::<Vec<_>>().join(", "));
        let shadow_transparent = format!(
            "[{}]",
            all.iter().enumerate().filter(|(i, _)| !SHADOWS.contains(&(*i as u32))).map(|(_, s)| s.as_str()).collect::<Vec<_>>().join(", ")
        );
        let shadow_rgba: Vec<String> = SHADOWS.map(|i| format!("{i} = [{}, 0, 0, 255]", i - 239)).collect();
        // Armies' own art (§10.11): entries 64..127 repeat the first 16
        // colours of the table, the first `members`·16 of them from another
        // table (the game reads civ 0's).
        let armies: BTreeSet<Recolor> = self.files.values().flat_map(|(_, c)| c.iter().copied()).filter(Recolor::army_art).collect();
        for r in &armies {
            let Some((first, members)) = r.army else { continue };
            let copies: Vec<String> = (0..4u8)
                .map(|k| format!("[\"ntp@{}\", 0, {}, 16]", if k < members { first } else { r.table }, 64 + 16 * u32::from(k)))
                .collect();
            let _ = writeln!(out, "[palettes.{}]\nbase = \"ntp@{}\"\ncopy = [{}]\n", toml_str(&r.palette()), r.table, copies.join(", "));
        }
        for (n, (path, (file, colors))) in self.files.iter().enumerate() {
            let source = toml_str(path);
            // Every frame (chunk) of the file, one sprite each (of a file drawn
            // still, the frames shown).
            let made = |k: &u32| self.stills.get(path).is_none_or(|s| s.contains(k));
            let sheets = |out: &mut String, image: &str, group: &str| match self.stills.get(path) {
                None => {
                    let _ = writeln!(
                        out,
                        "[sheets.\"{image}\"]\nimage = \"{image}\"\nall_frames = true\ncell = [{}, {}]\nanchor = [{}, {}]\ngroup = {}\n",
                        file.size.0,
                        file.size.1,
                        file.anchor.0,
                        file.anchor.1,
                        toml_str(group)
                    );
                }
                Some(frames) => {
                    for k in frames {
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
                }
            };
            // Per direction block, its frames (the ring frame last, §10.4) as
            // one animation, for the unit layers (a cell's phase is the frame).
            let blocks = |out: &mut String, set: &str, sprite: &str| {
                let per = file.frames_per_direction + 1;
                for b in 0..file.chunks.div_ceil(per) {
                    let frames: Vec<String> = (b * per..((b + 1) * per).min(file.chunks)).filter(made).map(|k| toml_str(&format!("{sprite}/{k}"))).collect();
                    if frames.is_empty() {
                        continue;
                    }
                    let _ = writeln!(out, "[sets.{}]\nframes = [{}]\n", toml_str(&format!("{set}/{b}")), frames.join(", "));
                }
            };
            let shadow = format!("unitshadow{n}");
            // (Of a file drawn still, only its frames shown are made.)
            let only = self.stills.get(path).map_or(String::new(), |s| format!("frames = [{}]\n", s.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")));
            let _ = writeln!(out, "[images.{shadow}]\nsource = {source}\n{only}transparent = {shadow_transparent}\nrgba = {{ {} }}\n", shadow_rgba.join(", "));
            sheets(&mut out, &shadow, &format!("UnitShadow@{path}"));
            blocks(&mut out, &format!("UnitShadowAnim@{path}"), &format!("UnitShadow@{path}"));
            let kinds: BTreeSet<bool> = colors.iter().map(Recolor::army_art).collect();
            for army in kinds {
                let image = format!("unit{n}{}", if army { "a" } else { "" });
                let remap = if army { 128 } else { 64 };
                let _ = writeln!(out, "[images.{image}]\nsource = {source}\n{only}remap = [0, {remap}]\ntransparent = {body_transparent}\n");
                let kind = art_kind(army);
                sheets(&mut out, &image, &format!("UnitArt@{path}{kind}"));
                blocks(&mut out, &format!("UnitAnim@{path}{kind}"), &format!("UnitArt@{path}{kind}"));
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
        out.push_str(&hud::description());
        out
    }
}

fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// `P_SHOW_TEAM_COLOR_DISC`.
const SHOW_TEAM_COLOR_DISC: u32 = 0x800;

/// Where the selected unit stands (its tile centre, or where it is on its
/// way), if one is selected and shown.
pub fn focus(world: &Civ3World) -> Option<(i32, i32)> {
    let (id, _) = world.selection?;
    world.units.iter().find(|u| u.id == id && u.visible).map(|u| u.pixel)
}

/// The layers a tile's units are drawn on (civ3.toml), one set per unit
/// the tile shows, in drawing order: the animator draws a tile's units one
/// after the other, at most two (a moving unit over the one under it,
/// §10.2).
pub const SLOTS: [&str; 2] = ["unit", "unit_over"];

/// What `Unit_draw` puts on the map for one unit, in its order (§10.2):
/// each is a layer of a slot (`<slot>_<part>`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Part {
    Disc,
    /// Our ring around a foreign unit's disc (`crate::rings`).
    Ring,
    Cursor,
    /// An army's own (banner) art, beside the member it shows (§10.11).
    Banner,
    Shadow,
    Body,
    Bar,
    Frame,
    Led,
    Stack,
}

impl Part {
    fn layer(self) -> &'static str {
        match self {
            Part::Disc => "disc",
            Part::Ring => "ring",
            Part::Cursor => "cursor",
            Part::Banner => "banner",
            Part::Shadow => "shadow",
            Part::Body => "body",
            Part::Bar => "hud_bar",
            Part::Frame => "hud_frame",
            Part::Led => "hud_led",
            Part::Stack => "hud_stack",
        }
    }

    /// The layer's offset from its cell's origin (the tile box's top-left,
    /// civ3.toml): where [`Piece::at`] is for a unit at its tile's centre.
    /// The disc's box is the tile's, the HUD's origin 32 px left of and
    /// above the unit's anchor (zoomed out 16: the layer offset is halved,
    /// the HUD's sprites keep their size and anchors, §10.2).
    fn origin(self) -> (i32, i32) {
        match self {
            Part::Disc | Part::Ring => (0, 0),
            Part::Cursor | Part::Banner | Part::Shadow | Part::Body => (64, 32),
            Part::Bar | Part::Frame | Part::Led | Part::Stack => (32, 0),
        }
    }
}

/// One thing drawn for a unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub part: Part,
    /// The set its layer draws, and the frame of it shown (the phase).
    pub set: String,
    pub phase: u8,
    /// The single sprite of that frame, for drawing it as a free sprite
    /// (`None`: the set has one frame).
    pub sprite: Option<String>,
    /// The world pixel (full size) its sprites' anchors are relative to.
    pub at: (i32, i32),
    /// 0 alpha, else as `scene::FreeSprite::scale_den`.
    pub scale_den: u32,
    /// The named palette its indexed art is drawn with (a palette of the
    /// description; empty: none).
    pub palette: String,
}

impl Piece {
    fn new(part: Part, set: String, at: (i32, i32)) -> Piece {
        Piece { part, set, phase: 0, sprite: None, at, scale_den: 0, palette: String::new() }
    }
}

/// What the animator draws for unit `u`, in its order (§10.2 `Unit_draw`,
/// §10.11 for armies): team disc (our ring), selection cursor, body with
/// shadow (an army: its banner art, then its member), HUD.
pub fn pieces(world: &Civ3World, art: &mut UnitArt, u: &Civ3Unit, rings: Option<&crate::rings::Rings>) -> Vec<Piece> {
    let mut out = Vec::new();
    let (x, y) = u.pixel;
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
        let palette = format!("disc@{}", colors(world, color_civ(world, u)));
        out.push(Piece { palette, ..Piece::new(Part::Disc, "TeamDisc/0".into(), (mx - 64, my - 32)) });
    }
    // Around it, how the viewer stands with a foreign owner.
    if let (Some(rings), false, false) = (rings, in_city, u.civ == world.player) {
        let relation = world.relations.get(u.civ.max(0) as usize).copied().unwrap_or(civ3_sys::world::Relation::None);
        if let Some((name, scale_den)) = rings.sprite(relation) {
            out.push(Piece { scale_den, ..Piece::new(Part::Ring, name, (mx - 64, my - 32)) });
        }
    }
    if let (Some((_, frame)), Some(cursor)) = (world.selection.filter(|s| s.0 == u.id), art.cursor()) {
        let block = DIRECTION_BLOCK[CURSOR_DIRECTION as usize];
        out.push(animation(Part::Cursor, cursor, Some(Recolor::civ(colors(world, world.player))), block, frame, (mx, my)));
    }
    let mut body = |unit: &Civ3Unit, recolor: Recolor, at: (i32, i32), part: Part, shadow: bool, out: &mut Vec<Piece>| {
        let type_key = world.unit_type_keys.get(unit.unit_type.max(0) as usize);
        if let Some(file) = type_key.and_then(|k| art.file(k, era_key(world, u.civ), unit.animation)) {
            let block = DIRECTION_BLOCK.get(unit.direction.max(0) as usize).copied().unwrap_or(2);
            if shadow {
                let mut p = animation(Part::Shadow, file, None, block, unit.frame, at);
                p.scale_den = 16;
                out.push(p);
            }
            out.push(animation(part, file, Some(recolor), block, unit.frame, at));
        }
    };
    match member {
        Some(m) => {
            let attacking_east = u.animation == ANIMATION_ATTACK1 && u.direction == DIRECTION_EAST;
            body(u, army_recolor(world, u), (x + 40, y - if attacking_east { 20 } else { 0 }), Part::Banner, false, &mut out);
            body(m, Recolor::civ(colors(world, color_civ(world, u))), (mx, my), Part::Body, false, &mut out);
        }
        None => body(u, Recolor::civ(colors(world, color_civ(world, u))), (x, y), Part::Body, true, &mut out),
    }
    out.extend(hud(world, u, (mx, my)));
    out
}

/// Frame `frame` of `file`'s block `block` (in `colors`; `None`: its
/// shadow) at `at`: the set of the block's frames (its last one the ring
/// frame, §10.4) and the single frame's sprite.
fn animation(part: Part, file: &AnimFile, colors: Option<Recolor>, block: u32, frame: i32, at: (i32, i32)) -> Piece {
    let n = file.frames_per_direction;
    // (The chunk as the game seeks it, clamped to the file.)
    let chunk = (block * (n + 1) + frame.max(0) as u32).min(file.chunks.saturating_sub(1));
    let (set, sprite, palette) = match colors {
        None => (format!("UnitShadowAnim@{}/{block}", file.path), format!("UnitShadow@{}/{chunk}", file.path), String::new()),
        Some(c) => {
            let kind = art_kind(c.army_art());
            (format!("UnitAnim@{}{kind}/{block}", file.path), format!("UnitArt@{}{kind}/{chunk}", file.path), c.palette())
        }
    };
    Piece { phase: chunk.saturating_sub(block * (n + 1)).min(n).min(255) as u8, sprite: Some(sprite), palette, ..Piece::new(part, set, at) }
}

/// The palette row `p` is drawn with (0: none, or not in the description).
fn piece_palette(compiled: &Compiled, p: &Piece) -> u32 {
    if p.palette.is_empty() { 0 } else { compiled.palette(&p.palette).unwrap_or(0) }
}

/// The index of the render pass that `layer` is in (among the passes that
/// have layers, as `tile_render::TileWorld::passes` counts them).
pub fn pass_of(compiled: &Compiled, layer: &str) -> Option<u32> {
    let pass = compiled.layers.iter().find(|l| l.name == layer)?.pass;
    let mut passes: Vec<u32> = compiled.layers.iter().map(|l| l.pass).collect();
    passes.dedup();
    passes.iter().position(|&p| p == pass).map(|i| i as u32)
}

/// The pass the units are drawn in (§10.2: after the terrain and the fog
/// of war), and the one before it: what goes over the fog but under the
/// units follows that one (the city-site preview, §10.9).
pub fn unit_passes(compiled: &Compiled) -> (u32, u32) {
    let units = pass_of(compiled, "unit_body").unwrap_or(u32::MAX);
    (units, if units == u32::MAX { u32::MAX } else { units.saturating_sub(1) })
}

/// Puts the world's units into `scene` (built by `scene_map::Mapper` for
/// `world` seen by `view`): each unit the animator draws on the layers of
/// its tile's next slot (`SLOTS`), with the offset of where it is from where
/// it would stand (a unit on its way, an army's banner), in the animator's
/// order: the units pass sweeps the rows top to bottom, each from its right
/// end (§10.2: x descending), a tile's slots in order. What does not fit
/// that order (always-on-top units, appended at the end of the game's
/// list; a third unit on a tile) is drawn, with all units after it, as free
/// sprites after the units pass, in the list's order; inverting rings
/// (which a layer cannot draw) as free sprites under the units.
pub fn place(world: &Civ3World, view: &Civ3View, art: &mut UnitArt, compiled: &Compiled, rings: Option<&crate::rings::Rings>, scene: &mut Scene) {
    let (units_pass, under_units) = unit_passes(compiled);
    // (A description without unit layers: all free.)
    let layered = compiled.layer_index("unit_body").is_some();
    let shape = scene.shape;
    let units: Vec<&Civ3Unit> = world.units.iter().filter(|u| u.visible).collect();
    let slots = if layered { slots(world, view, &units, (shape.cols, shape.rows)) } else { vec![None; units.len()] };
    let invert = tile_render::Blend::Invert.scale_den();
    log::trace!("units: {} on layers, {} as free sprites", slots.iter().filter(|s| s.is_some()).count(), slots.iter().filter(|s| s.is_none()).count());
    for (u, slot) in units.into_iter().zip(slots) {
        let (col, row) = ((u.x >> 1) as u32, u.y as u32);
        for p in pieces(world, art, u, rings) {
            // Inverting rings under the units (a layer cannot invert).
            let Some(slot) = slot.filter(|_| p.scale_den != invert) else {
                scene.sprites.extend(free_sprite(compiled, &p, if p.scale_den == invert { under_units } else { units_pass }));
                continue;
            };
            let layer = format!("{}_{}", SLOTS[slot], p.part.layer());
            let palette = piece_palette(compiled, &p);
            let (Some(index), Some(value)) = (compiled.layer_index(&layer), compiled.value_in(&layer, &p.set, palette)) else { continue };
            scene.set(index, col, row, value);
            scene.set_phase(index, col, row, p.phase);
            let (ox, oy) = p.part.origin();
            let dx = wrapped_offset(p.at.0 - (64 * u.x + ox), world.wrap_x, 64 * world.width);
            let dy = wrapped_offset(p.at.1 - (32 * u.y + oy), world.wrap_y, 32 * world.height);
            scene.set_offset(index, col, row, (dx, dy));
        }
    }
}

/// The slot (of [`SLOTS`]) each of `units` (in the animator's order) is
/// drawn in on its tile's cell (`cells`: the grid's columns and rows), if
/// the layers can draw it in that order: rows top to bottom (as the game's
/// view sweeps them, unwrapped from its first tile), each from its right
/// end, a tile's slots in order. From the first that does not fit on, none.
pub fn slots(world: &Civ3World, view: &Civ3View, units: &[&Civ3Unit], (cols, rows): (u32, u32)) -> Vec<Option<usize>> {
    let unwrapped = |v: i32, wraps: bool, from: i32, size: i32| if wraps && size > 0 { from + (v - from).rem_euclid(size) } else { v };
    let mut used: HashMap<(i32, i32), usize> = HashMap::new();
    let mut last: Option<(i32, i32, usize)> = None;
    let mut fits = true;
    units
        .iter()
        .map(|u| {
            let on_grid = u.x >= 0 && u.y >= 0 && ((u.x >> 1) as u32) < cols && (u.y as u32) < rows;
            let order = (unwrapped(u.y, world.wrap_y, view.tile_y.0, world.height), -unwrapped(u.x, world.wrap_x, view.tile_x.0, world.width));
            let first = used.get(&(u.x, u.y)).copied().unwrap_or(0);
            let slot = (first..SLOTS.len()).find(|&s| last < Some((order.0, order.1, s))).filter(|_| on_grid && fits);
            match slot {
                Some(s) => {
                    used.insert((u.x, u.y), s + 1);
                    last = Some((order.0, order.1, s));
                }
                None => fits = false,
            }
            slot
        })
        .collect()
}

/// An offset from a unit's tile on a map repeating every `period` pixels
/// (when it `wraps`): the shortest way, as a cell's offset.
fn wrapped_offset(d: i32, wraps: bool, period: i32) -> i16 {
    let d = if wraps && period > 0 && d.abs() > period / 2 { d - period * (d as f64 / period as f64).round() as i32 } else { d };
    d.clamp(i16::MIN.into(), i16::MAX.into()) as i16
}

/// `p` as a free sprite after pass `after_pass` (its frame's own sprite).
fn free_sprite(compiled: &Compiled, p: &Piece, after_pass: u32) -> Option<scene::FreeSprite> {
    let (set, _) = compiled.set(p.sprite.as_deref().unwrap_or(&p.set))?;
    Some(scene::FreeSprite {
        set: set.raw(),
        x: p.at.0,
        y: p.at.1,
        screen: false,
        after_pass,
        phase: 0,
        ticks_per_frame: 1,
        scale_den: p.scale_den,
        tint: 0,
        zoom_art: 0,
        zoom: (0, 0),
        zoom_offset: (0, 0),
        clip: None,
        not_in_view: None,
        glyph: None,
        palette: 0,
    })
}

/// The free sprites drawing the world's units as `place` would put them
/// on layers, in the animator's order, after the units pass (and inverting
/// rings under it): for comparing the two, and for a description without
/// unit layers.
pub fn free_sprites(world: &Civ3World, art: &mut UnitArt, compiled: &Compiled, rings: Option<&crate::rings::Rings>) -> Vec<scene::FreeSprite> {
    let (units_pass, under_units) = unit_passes(compiled);
    let mut out = Vec::new();
    for u in world.units.iter().filter(|u| u.visible) {
        for p in pieces(world, art, u, rings) {
            let after = if p.scale_den == tile_render::Blend::Invert.scale_den() { under_units } else { units_pass };
            out.extend(free_sprite(compiled, &p, after));
        }
    }
    out
}

/// §10.6: health bar, fortified frame, movement LED and stack lines of a
/// unit, drawn at `anchor` (an army's HUD sits at its member), as pieces at
/// the HUD's origin (32 px left of and above the anchor): sprites of the
/// generated HUD art (`hud::description`), which keeps its size zoomed out.
fn hud(world: &Civ3World, u: &Civ3Unit, anchor: (i32, i32)) -> Vec<Piece> {
    let mut out = Vec::new();
    let Some(&[attack, defence, movement, hp_bonus, class, _, _]) = world.unit_type_stats.get(u.unit_type.max(0) as usize) else {
        return out;
    };
    let at = (anchor.0 - 32, anchor.1 - 32);
    let has_bar = attack > 0 || defence > 0;
    let base_hp = world.experience_hit_points.get(u.experience.max(0) as usize).copied().unwrap_or(1);
    let max = (base_hp + hp_bonus).max(1);
    let hp = (max - u.damage).clamp(0, 9999);
    let bar = hud::bar_height(max);
    let color = if hp <= 1 {
        hud::RED
    } else if 3 * u.damage < max || u.damage == 0 {
        hud::GREEN
    } else if 3 * u.damage < 2 * max {
        hud::YELLOW
    } else {
        hud::RED
    };
    if has_bar {
        out.push(Piece::new(Part::Bar, format!("UnitHud_bar/{}", hud::bar_index(max, hp, color)), at));
    }
    // §10.10: fortified land units on land with moves left get a white
    // outline around the bar (the LED drawn next covers most of its top).
    let left_moves = movement * 3 - u.moves;
    let on_water = world.tile(u.x, u.y).is_some_and(|t| t.terrain() >= COAST);
    if has_bar && class == 0 && !on_water && u.state == STATE_FORTIFIED && left_moves > 0 {
        out.push(Piece::new(Part::Frame, format!("UnitHud_frame/{bar}"), at));
    }
    // Own units: how much of their move is left (moves count thirds).
    if u.civ == world.player {
        let led = if left_moves <= 0 {
            2
        } else if u.moves != 0 {
            1
        } else {
            0
        };
        out.push(Piece::new(Part::Led, format!("UnitHud_led@{bar}/{led}"), at));
    }
    // Stack lines: one per unit on the tile (up to 8), when there are
    // several and the unit is not moving.
    if !u.always_on_top {
        let n = world.unit_tiles.iter().filter(|&&(x, y, c)| x == u.x && y == u.y && c < 0).count().clamp(1, 8);
        if n > 1 {
            out.push(Piece::new(Part::Stack, format!("UnitHud_stack/{n}"), at));
        }
    }
    out
}

/// The unit HUD's art (§10.6), made once for every state it can show:
/// health bars, fortified frames, LEDs at every bar height, stack lines.
/// Sprites are anchored at the HUD's origin and keep their size zoomed out.
pub mod hud {
    use std::fmt::Write as _;

    pub const GREEN: usize = 0;
    pub const YELLOW: usize = 1;
    pub const RED: usize = 2;
    const COLORS: [&str; 3] = ["0,255,0,255", "255,255,0,255", "255,0,0,255"];
    const BLACK: &str = "0,0,0,255";
    const WHITE: &str = "255,255,255,255";
    /// From this many hit points on, bars look the same (2 px a point, 40 px
    /// high, at most 20 segments).
    const MAX_DISTINCT: i32 = 21;
    /// Bars are at most this high.
    const TALLEST: i32 = 40;

    /// Pixels per hit point and the bar's height for `max` hit points.
    pub fn pitch_and_height(max: i32) -> (i32, i32) {
        let max = max.max(1);
        let pitch = if 5 * max - 1 > TALLEST { (41 / max).max(2) } else { 5 };
        (pitch, (pitch * max - 1).min(TALLEST))
    }

    pub fn bar_height(max: i32) -> i32 {
        pitch_and_height(max).1
    }

    /// Bar sprite of a unit with `max` hit points, `hp` of them left, in
    /// `color`: bars are numbered by (max up to `MAX_DISTINCT`, segments,
    /// colour).
    pub fn bar_index(max: i32, hp: i32, color: usize) -> u32 {
        let m = max.clamp(1, MAX_DISTINCT);
        let segments = hp.clamp(0, 20).min(m);
        // Bars of fewer hit points before: m' + 1 of them for each m' < m.
        let before = (m - 1) * (m + 2) / 2;
        (3 * (before + segments) + color as i32) as u32
    }

    fn bar_heights() -> Vec<i32> {
        let mut h: Vec<i32> = (1..=MAX_DISTINCT).map(bar_height).collect();
        h.sort();
        h.dedup();
        h
    }

    /// Render description entries: images and sheets `UnitHud_bar/<i>`,
    /// `UnitHud_frame/<height>`, `UnitHud_led@<height>/<0 green, 1
    /// yellow, 2 red>`, `UnitHud_stack/<lines>`.
    pub fn description() -> String {
        let mut out = String::from("\n# Unit HUD (generated, docs §10.6): sprites at the HUD's origin.\n");
        // Bars: 2 px wide cells, 40 high, their bottom row 23 px below the
        // origin; black, then the segments from the bottom up.
        let mut bars = Vec::new();
        for m in 1..=MAX_DISTINCT {
            let (pitch, height) = pitch_and_height(m);
            for segments in 0..=m.min(20) {
                for color in COLORS {
                    let x = 2 * bars.len();
                    let mut cell = format!("{BLACK}:{x},{},2,{height}", TALLEST - height);
                    if segments > 0 {
                        let _ = write!(cell, ";{color}:{x},{},2,{}*{segments},0,{}", TALLEST + 1 - pitch, pitch - 1, -pitch);
                    }
                    bars.push(cell);
                }
            }
        }
        debug_assert_eq!(bars.len() as u32, bar_index(MAX_DISTINCT, 20, RED) + 1);
        // One sheet cutting image `name` into `cells` cells in a row, named
        // `group/<first>`, `group/<first + 1>`, ...
        let sheet = |out: &mut String, (name, group): (&str, &str), cell: (usize, i32), cells: usize, anchor: (i32, i32), first: i32| {
            let _ = writeln!(
                out,
                "[sheets.{name}]\nimage = \"{name}\"\ncell = [{}, {}]\ngrid = [{cells}, 1]\nanchor = [{}, {}]\ngroup = \"{group}\"\nfirst_index = {first}\nscaling = \"keep\"\n",
                cell.0,
                cell.1,
                anchor.0,
                anchor.1,
            );
        };
        let _ = writeln!(out, "[images.unit_hud_bar]\nsource = \"rects:{}x{TALLEST};{}\"\n", 2 * bars.len(), bars.join(";"));
        sheet(&mut out, ("unit_hud_bar", "UnitHud_bar"), (2, TALLEST), bars.len(), (6, 24 - TALLEST), 0);
        // Fortified frames: the outline of [5, 8] × [23 − height, 24], by
        // height (cell k for height k + 1), bottom row 24 below the origin.
        let tall = TALLEST + 2;
        let frames: Vec<String> = (1..=TALLEST)
            .map(|h| {
                let x = 4 * (h - 1);
                let top = tall - 2 - h;
                format!("{WHITE}:{x},{top},4,1;{WHITE}:{x},{},4,1;{WHITE}:{x},{},1,{h};{WHITE}:{},{},1,{h}", tall - 1, top + 1, x + 3, top + 1)
            })
            .collect();
        let _ = writeln!(out, "[images.unit_hud_frame]\nsource = \"rects:{}x{tall};{}\"\n", 4 * TALLEST, frames.join(";"));
        sheet(&mut out, ("unit_hud_frame", "UnitHud_frame"), (4, tall), TALLEST as usize, (5, 25 - tall), 1);
        // The LED above the bar, at every height a bar has.
        for h in bar_heights() {
            let _ = writeln!(
                out,
                "[sheets.unit_hud_led_{h}]\nimage = \"movement_led\"\norigin = [1, 1]\ncell = [6, 6]\nspacing = [8, 0]\ngrid = [3, 1]\nanchor = [4, {}]\ngroup = \"UnitHud_led@{h}\"\nscaling = \"keep\"\n",
                18 - h
            );
        }
        // Stack lines: 2 to 8 white lines 4 px long, 2 px apart, from (5, 26).
        let stacks: Vec<String> = (2..=8).map(|n| format!("{WHITE}:{},0,4,1*{n},0,2", 4 * (n - 2))).collect();
        let _ = writeln!(out, "[images.unit_hud_stack]\nsource = \"rects:28x15;{}\"\n", stacks.join(";"));
        sheet(&mut out, ("unit_hud_stack", "UnitHud_stack"), (4, 15), 7, (5, 26), 2);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn still_frames_cover_only_themselves_and_a_whole_file_wins() {
        let file = AnimFile { path: "a.flc".into(), frames_per_direction: 2, chunks: 24, size: (8, 8), anchor: (0, 0) };
        let colors = BTreeSet::from([Recolor::civ(3)]);
        let with = |stills: Option<&[u32]>| Needed {
            files: BTreeMap::from([("a.flc".to_owned(), (file.clone(), colors.clone()))]),
            stills: stills.map(|s| BTreeMap::from([("a.flc".to_owned(), s.iter().copied().collect())])).unwrap_or_default(),
            ..Default::default()
        };
        let (whole, one, two) = (with(None), with(Some(&[9])), with(Some(&[9, 12])));
        assert!(whole.covers(&one) && two.covers(&one));
        assert!(!one.covers(&two) && !one.covers(&whole));
        // Only the frames listed are made: chunk 9 and its block (block 3 of
        // three frames each).
        let d = one.description();
        assert!(d.contains("frame = 9\n") && !d.contains("frame = 10\n"));
        assert!(d.contains("frames = [9]\n"), "only frame 9 is decoded");
        assert!(d.contains("UnitAnim@a.flc/3") && !d.contains("UnitAnim@a.flc/2\""));
        let mut merged = one.clone();
        merged.merge(&two);
        assert_eq!(merged.stills["a.flc"], BTreeSet::from([9, 12]));
        merged.merge(&whole);
        assert!(merged.stills.is_empty() && merged.covers(&whole));
        let mut kept = whole.clone();
        kept.merge(&one);
        assert!(kept.stills.is_empty());
    }

    /// One unit type (attack 1, defence 1, one move) and one experience level
    /// (3 hit points).
    fn world() -> Civ3World {
        Civ3World { unit_type_stats: vec![[1, 1, 1, 0, 0, 0, 0]], experience_hit_points: vec![3], ..Default::default() }
    }

    /// What `Unit_draw_hud` puts where (§10.6, §10.10), pixel by pixel, from
    /// the HUD's origin: `led` the LED cells' colours.
    fn game_hud(world: &Civ3World, u: &Civ3Unit, led: [[u8; 4]; 3]) -> BTreeMap<(i32, i32), [u8; 4]> {
        let mut out = BTreeMap::new();
        let [attack, defence, movement, hp_bonus, class, _, _] = world.unit_type_stats[u.unit_type as usize];
        let mut rect = |c: [u8; 4], (x0, y0, x1, y1): (i32, i32, i32, i32)| {
            for y in y0..y1 {
                for x in x0..x1 {
                    out.insert((x, y), c);
                }
            }
        };
        let max = (world.experience_hit_points[u.experience as usize] + hp_bonus).max(1);
        let hp = (max - u.damage).clamp(0, 9999);
        let pitch = if 5 * max - 1 > 40 { (41 / max).max(2) } else { 5 };
        let bar = (pitch * max - 1).min(40);
        let (left, bottom) = (6, 24);
        let has_bar = attack > 0 || defence > 0;
        if has_bar {
            rect([0, 0, 0, 255], (left, bottom - bar, left + 2, bottom));
        }
        let color = if hp <= 1 {
            [255, 0, 0, 255]
        } else if 3 * u.damage < max || u.damage == 0 {
            [0, 255, 0, 255]
        } else if 3 * u.damage < 2 * max {
            [255, 255, 0, 255]
        } else {
            [255, 0, 0, 255]
        };
        let mut seg_bottom = bottom;
        for _ in 0..if has_bar { hp.min(20) } else { 0 } {
            rect(color, (left, seg_bottom - pitch + 1, left + 2, seg_bottom));
            seg_bottom -= pitch;
        }
        let left_moves = movement * 3 - u.moves;
        let white = [255; 4];
        if has_bar && class == 0 && u.state == STATE_FORTIFIED && left_moves > 0 {
            let (x0, y0, x1, y1) = (left - 1, bottom - bar - 1, left + 2, bottom);
            rect(white, (x0, y0, x1 + 1, y0 + 1));
            rect(white, (x0, y1, x1 + 1, y1 + 1));
            rect(white, (x0, y0 + 1, x0 + 1, y1));
            rect(white, (x1, y0 + 1, x1 + 1, y1));
        }
        if u.civ == world.player {
            let k = if left_moves <= 0 { 2 } else if u.moves != 0 { 1 } else { 0 };
            rect(led[k], (4, 18 - bar, 10, 24 - bar));
        }
        let n = world.unit_tiles.iter().filter(|&&(x, y, c)| x == u.x && y == u.y && c < 0).count().clamp(1, 8) as i32;
        if !u.always_on_top && n > 1 {
            for i in 0..n {
                rect(white, (5, 26 + 2 * i, 9, 27 + 2 * i));
            }
        }
        out
    }

    #[test]
    fn the_hud_art_draws_what_the_game_does() {
        // The LED sheet's image as civ3.toml cuts it, each LED one colour.
        let led = [[0, 200, 0, 255], [200, 200, 0, 255], [200, 0, 0, 255]];
        let desc = format!(
            "grid = {{ layout = \"staggered\", step = [64, 32] }}\n[images.movement_led]\nsource = \"rects:44x8;0,200,0,255:1,1,6,6;200,200,0,255:15,1,6,6;200,0,0,255:29,1,6,6\"\n{}",
            hud::description()
        );
        struct NoFiles;
        impl render_desc::ImageLoader for NoFiles {
            fn load(&self, source: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> {
                Err(format!("no file {source}"))
            }
        }
        let compiled = render_desc::Description::parse(&desc).unwrap().compile(&NoFiles, 4096).unwrap();
        // Our pieces' sprites drawn in order, from the HUD's origin.
        let ours = |world: &Civ3World, u: &Civ3Unit| {
            let mut out = BTreeMap::new();
            for p in hud(world, u, (32, 32)) {
                assert_eq!(p.at, (0, 0));
                let (set, _) = compiled.set(&p.set).unwrap_or_else(|| panic!("no sprite {}", p.set));
                let s = compiled.atlas.sprites[compiled.sets.frame(set, 0).unwrap().0 as usize];
                assert_eq!(s.scaling, tile_render::KEEP_SIZE, "{}: the HUD keeps its size zoomed out", p.set);
                let page = &compiled.atlas.pages[s.page as usize];
                for y in 0..s.height {
                    for x in 0..s.width {
                        let c = page.get(s.atlas_x + x, s.atlas_y + y);
                        if c[3] != 0 {
                            out.insert((s.anchor_x + x as i32, s.anchor_y + y as i32), c);
                        }
                    }
                }
            }
            out
        };
        let mut w = world();
        w.unit_type_stats.push([0, 0, 1, 0, 0, 0, 0]);
        let mut checked = 0;
        for bonus in [0, 1, 4, 6, 9, 13, 17, 30] {
            w.unit_type_stats[0][3] = bonus;
            for damage in [0, 1, 2, 3, 5, 8, 12, 40] {
                for (state, moves, civ, stack) in [(0, 0, 0, 1), (STATE_FORTIFIED, 1, 0, 3), (STATE_FORTIFIED, 3, 1, 9), (0, 2, 0, 2)] {
                    for unit_type in [0, 1] {
                        w.unit_tiles = vec![(4, 6, -1); stack];
                        let u = Civ3Unit { x: 4, y: 6, unit_type, damage, state, moves, civ, ..Default::default() };
                        assert_eq!(ours(&w, &u), game_hud(&w, &u, led), "bonus {bonus}, damage {damage}, state {state}, moves {moves}, civ {civ}, stack {stack}");
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 512);
    }

    #[test]
    fn units_pick_their_colours_hidden_nationality_and_armies_included() {
        // Player 1 looks at civ 2's units (colour tables 5, 7, 9): one of
        // hidden nationality (barbarian colours, ntp 5), one not.
        let hidden = 1 << civ3_sys::layout::UTA_HIDDEN_NATIONALITY;
        let mut w = Civ3World { player: 1, color_tables: vec![5, 7, 9], unit_type_stats: vec![[1, 1, 1, 0, 0, 0, hidden], [1, 1, 1, 0, 0, 0, 0]], ..Default::default() };
        let u = |unit_type| Civ3Unit { civ: 2, unit_type, army_size: 2, ..Default::default() };
        let palette = |w: &Civ3World, unit: &Civ3Unit| Recolor::civ(colors(w, color_civ(w, unit))).palette();
        assert_eq!((palette(&w, &u(0)), palette(&w, &u(1))), ("ntp@5".to_owned(), "ntp@9".to_owned()));
        // Its owner, and the debug mode showing everything, see its colours.
        w.player = 2;
        assert_eq!(palette(&w, &u(0)), "ntp@9");
        (w.player, w.debug_mode_bits) = (1, 8);
        assert_eq!(palette(&w, &u(0)), "ntp@9");
        // An army: entries 64..127 from civ 0's table for its two members,
        // its own after; one image of the file for every colour.
        w.debug_mode_bits = 0;
        let army = army_recolor(&w, &u(1));
        assert_eq!(army.palette(), "UnitArmy@9a5x2");
        let file = AnimFile { path: "Art/Units/A/A.flc".into(), frames_per_direction: 1, chunks: 16, size: (8, 8), anchor: (0, 0) };
        let mut needed = Needed::default();
        needed.files.insert(file.path.clone(), (file, [army, Recolor::civ(9), Recolor::civ(7)].into()));
        let d = needed.description();
        assert!(d.contains("[palettes.\"UnitArmy@9a5x2\"]\nbase = \"ntp@9\"\ncopy = [[\"ntp@5\", 0, 64, 16], [\"ntp@5\", 0, 80, 16], [\"ntp@9\", 0, 96, 16], [\"ntp@9\", 0, 112, 16]]"), "{d}");
        assert_eq!(d.matches("source = \"Art/Units/A/A.flc\"").count(), 3, "shadow, body, army body: {d}");
        assert!(d.contains("remap = [0, 64]") && d.contains("remap = [0, 128]"));
    }

    #[test]
    fn bars_are_numbered_without_gaps() {
        let mut seen = BTreeSet::new();
        for max in 1..=40 {
            for hp in 0..=max {
                for color in [hud::GREEN, hud::YELLOW, hud::RED] {
                    seen.insert(hud::bar_index(max, hp, color));
                }
            }
        }
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), (0..753).collect::<Vec<_>>());
    }

    #[test]
    fn units_take_their_tiles_slots_in_the_animators_order() {
        let world = Civ3World { width: 20, height: 20, wrap_x: true, ..Default::default() };
        let view = Civ3View { tile_x: (16, 30), tile_y: (2, 12), ..Default::default() };
        let at = |x, y| Civ3Unit { x, y, ..Default::default() };
        // Rows top to bottom, right to left (across the wrap: 1 is 21, right
        // of 19); a moving unit over the one under it; then an always-on-top
        // unit back on an earlier tile, and all after it, free.
        let units = [at(19, 3), at(1, 5), at(17, 5), at(17, 5), at(4, 6), at(2, 6), at(5, 3), at(8, 8)];
        let refs: Vec<&Civ3Unit> = units.iter().collect();
        let got = slots(&world, &view, &refs, (10, 20));
        assert_eq!(got, vec![Some(0), Some(0), Some(0), Some(1), Some(0), Some(0), None, None]);
        // A third unit on a tile does not fit either.
        let three = [at(4, 6), at(4, 6), at(4, 6), at(2, 6)];
        let refs: Vec<&Civ3Unit> = three.iter().collect();
        assert_eq!(slots(&world, &view, &refs, (10, 20)), vec![Some(0), Some(1), None, None]);
    }

    #[test]
    fn offsets_take_the_short_way_round_the_map() {
        assert_eq!(wrapped_offset(-40, true, 1280), -40);
        assert_eq!(wrapped_offset(1280 - 40, true, 1280), -40);
        assert_eq!(wrapped_offset(-1280 + 64, true, 1280), 64);
        assert_eq!(wrapped_offset(1240, false, 1280), 1240);
        assert_eq!(wrapped_offset(100_000, false, 0), i16::MAX);
    }
}
