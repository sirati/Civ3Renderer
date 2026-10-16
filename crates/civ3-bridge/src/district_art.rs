//! The art of C3X's districts as the render description needs it: every
//! slice of an image C3X draws on the map (`crate::districts`), a sprite
//! anchored where C3X puts it relative to the tile's cell, described as
//! districts appear (like unit art).

use std::collections::BTreeSet;
use std::fmt::Write as _;

/// A slice of an image drawn on a tile: `size` pixels from `origin` in the
/// image, its top-left `offset` from the tile's 128×64 cell origin.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slice {
    /// Relative to the art roots, as C3X loads it.
    pub image: String,
    pub origin: (u32, u32),
    pub size: (u32, u32),
    pub offset: (i32, i32),
    /// For the zoomed-out view: the art is halved, the offset is not (C3X
    /// does not scale what it adds to the drawing position).
    pub zoomed_out: bool,
}

impl Slice {
    pub fn of(d: &crate::districts::DistrictDraw, zoomed_out: bool) -> Slice {
        Slice { image: d.image.clone(), origin: d.origin, size: d.size, offset: d.offset, zoomed_out }
    }
}

/// Layers district slices go into, in drawing order (a tile shows at most
/// this many slices: e.g. a great wall's pillar and segments).
pub const LAYERS: [&str; 6] = ["district_0", "district_1", "district_2", "district_3", "district_4", "district_5"];
/// Layers for what C3X draws where the game draws resources.
pub const RESOURCE_LAYERS: [&str; 3] = ["district_resource_0", "district_resource_1", "district_resource_2"];

impl Slice {
    /// Its sprite set's name.
    pub fn sprite(&self) -> String {
        let ((x, y), (w, h), (dx, dy)) = (self.origin, self.size, self.offset);
        let z = if self.zoomed_out { "z" } else { "" };
        format!("District@{}#{x},{y},{w},{h}@{dx},{dy}{z}/0", self.image)
    }
}

/// The slices described so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DistrictArt {
    slices: BTreeSet<Slice>,
}

fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

impl DistrictArt {
    /// How many slices are described.
    pub fn len(&self) -> usize {
        self.slices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slices.is_empty()
    }

    /// Adds `slices`; whether any was new.
    pub fn add<'a>(&mut self, slices: impl IntoIterator<Item = &'a Slice>) -> bool {
        let mut new = false;
        for s in slices {
            if s.size.0 > 0 && s.size.1 > 0 && !self.slices.contains(s) {
                self.slices.insert(s.clone());
                new = true;
            }
        }
        new
    }

    /// Render description entries: an image per file, a sheet per slice.
    pub fn description(&self) -> String {
        let mut out = String::from("\n# District art (generated).\n");
        let images: BTreeSet<&str> = self.slices.iter().map(|s| s.image.as_str()).collect();
        let index = |image: &str| images.iter().position(|i| *i == image).unwrap_or(0);
        for (n, image) in images.iter().enumerate() {
            let _ = writeln!(out, "[images.district_image_{n}]\nsource = {}\n", toml_str(image));
        }
        for (n, s) in self.slices.iter().enumerate() {
            let ((x, y), (w, h), (dx, dy)) = (s.origin, s.size, s.offset);
            let _ = writeln!(
                out,
                "[sheets.district_{n}]\nimage = \"district_image_{}\"\norigin = [{x}, {y}]\ncell = [{w}, {h}]\ngrid = [1, 1]\nanchor = [{dx}, {dy}]\ngroup = {}\n{}",
                index(&s.image),
                toml_str(s.sprite().trim_end_matches("/0")),
                if s.zoomed_out { "scaling = \"scale_keep_anchor\"\n" } else { "" }
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_are_described_once() {
        let s = Slice { image: "Art/Districts/Summer/1200/Bridge.PCX".into(), origin: (176, 112), size: (176, 112), offset: (-24, -48), zoomed_out: false };
        let mut art = DistrictArt::default();
        assert!(art.add([&s]));
        assert!(!art.add([&s]));
        let d = art.description();
        assert!(d.contains("origin = [176, 112]\ncell = [176, 112]\ngrid = [1, 1]\nanchor = [-24, -48]"));
        assert!(d.contains(&format!("group = \"{}\"", s.sprite().trim_end_matches("/0"))));
    }
}
