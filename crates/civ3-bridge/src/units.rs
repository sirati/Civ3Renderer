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

use crate::world::{Civ3Unit, Civ3World};

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

/// "Colour table" of art drawn in its own palette (the selection cursor).
const OWN_PALETTE: i32 = -1;

/// Resolves unit types to animation files.
pub struct UnitArt {
    files: GameFiles,
    /// `#ANIMNAME_...` keys of `Text/PediaIcons.txt` to folder names.
    anim_names: HashMap<String, String>,
    /// (unit type key, era key, animation) → file, cached.
    resolved: HashMap<(String, String, i32), Option<AnimFile>>,
    cursor: Option<Option<AnimFile>>,
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
        Self { files, anim_names, resolved: HashMap::new(), cursor: None }
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

/// The unit art a world needs: animation files and, per file, the colour
/// tables it is drawn in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Needed {
    pub files: BTreeMap<String, (AnimFile, BTreeSet<i32>)>,
}

fn era_key(world: &Civ3World, civ: i32) -> &str {
    let era = world.eras.get(civ.max(0) as usize).copied().unwrap_or(0);
    world.era_keys.get(era.max(0) as usize).map_or("", String::as_str)
}

fn colors(world: &Civ3World, civ: i32) -> i32 {
    world.color_tables.get(civ.max(0) as usize).copied().unwrap_or(0)
}

impl Needed {
    /// The animations the world's units show now, plus each type's DEFAULT
    /// (what idle units return to), in the colours of the civs showing
    /// them. Other animations are added when a unit starts them: loading
    /// all of them up front does not fit a 32-bit process.
    pub fn of(world: &Civ3World, art: &mut UnitArt) -> Needed {
        let mut out = Needed::default();
        if let Some(cursor) = art.cursor() {
            out.files.insert(cursor.path.clone(), (cursor.clone(), [colors(world, world.player)].into()));
        }
        for u in &world.units {
            let Some(type_key) = world.unit_type_keys.get(u.unit_type.max(0) as usize) else { continue };
            for animation in [1, u.animation] {
                if let Some(file) = art.file(type_key, era_key(world, u.civ), animation) {
                    out.files.entry(file.path.clone()).or_insert_with(|| (file.clone(), BTreeSet::new())).1.insert(colors(world, u.civ));
                }
            }
        }
        out
    }

    /// Whether everything `other` needs is already here.
    pub fn covers(&self, other: &Needed) -> bool {
        other.files.iter().all(|(p, (_, c))| self.files.get(p).is_some_and(|(_, mine)| c.is_subset(mine)))
    }

    pub fn merge(&mut self, other: &Needed) {
        for (p, (f, c)) in &other.files {
            self.files.entry(p.clone()).or_insert_with(|| (f.clone(), BTreeSet::new())).1.extend(c);
        }
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
                let palette = if *c == OWN_PALETTE {
                    String::new()
                } else {
                    format!("palette = \"Art/Units/Palettes/ntp{c:02}.pcx\"\npalette_range = [0, 64]\n")
                };
                let _ = writeln!(out, "[images.{image}]\nsource = {source}\n{palette}transparent = {body_transparent}\n");
                sheets(&mut out, &image, &format!("UnitArt@{path}#{c}"));
            }
        }
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
pub fn sprites(world: &Civ3World, art: &mut UnitArt, compiled: &Compiled) -> Vec<scene::FreeSprite> {
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
                });
            }
        };
        let in_city = world.tile(u.x, u.y).is_some_and(|t| t.city >= 0);
        if world.preferences & SHOW_TEAM_COLOR_DISC != 0 && !in_city {
            put(&format!("TeamDisc@{}/0", colors(world, u.civ)), (x - 64, y - 32), 0);
        }
        if let (Some((_, frame)), Some(cursor)) = (world.selection.filter(|s| s.0 == u.id), art.cursor()) {
            let block = DIRECTION_BLOCK[CURSOR_DIRECTION as usize];
            let chunk = (block * (cursor.frames_per_direction + 1) + frame.max(0) as u32).min(cursor.chunks.saturating_sub(1));
            put(&format!("UnitArt@{}#{}/{chunk}", cursor.path, colors(world, world.player)), (x, y), 0);
        }
        let type_key = world.unit_type_keys.get(u.unit_type.max(0) as usize);
        if let Some(file) = type_key.and_then(|k| art.file(k, era_key(world, u.civ), u.animation)) {
            let chunk = chunk(file, u);
            put(&format!("UnitShadow@{}/{chunk}", file.path), (x, y), 16);
            put(&format!("UnitArt@{}#{}/{chunk}", file.path, colors(world, u.civ)), (x, y), 0);
        }
        for (name, at) in hud(world, u) {
            put(name, at, 0);
        }
    }
    out
}

/// §10.6: health bar, movement LED and stack lines of a unit, as 1×1
/// colour pixels and LED sprites at world positions.
fn hud(world: &Civ3World, u: &Civ3Unit) -> Vec<(&'static str, (i32, i32))> {
    let mut out = Vec::new();
    let Some(&[attack, defence, movement, hp_bonus]) = world.unit_type_stats.get(u.unit_type.max(0) as usize) else {
        return out;
    };
    let has_bar = attack > 0 || defence > 0;
    let (hx, hy) = (u.pixel.0 - 32, u.pixel.1 - 32);
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
    // Own units: how much of their move is left (moves count thirds).
    if u.civ == world.player {
        let left_moves = movement * 3 - u.moves;
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
