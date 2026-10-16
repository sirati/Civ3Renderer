//! A second ring around foreign units' team discs, by how the viewing
//! player stands with their owner: at war, in a mutual defence pact, at
//! peace. Each ring is a band just outside the disc's own ring (render-desc
//! `outline`), in a colour or inverting what is under it.
//!
//! `CIV3R_UNIT_RINGS`: `on` for the defaults (war red, defence blue, peace
//! green, 2 px), or comma-separated `war=`, `defense=`, `peace=`, `none=`
//! with `R,G,B` in `;`-separated form (`war=255;0;0`), `invert` or `off`,
//! and `width=N`. Unset or `off`: no rings.

use civ3_sys::world::Relation;

/// How one kind of ring looks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RingStyle {
    Color([u8; 3]),
    /// Inverts the colours under it.
    Invert,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rings {
    pub width: u32,
    pub war: Option<RingStyle>,
    pub defense: Option<RingStyle>,
    pub peace: Option<RingStyle>,
    /// Units of civs with none of these relations.
    pub none: Option<RingStyle>,
}

impl Default for Rings {
    fn default() -> Self {
        Self {
            width: 2,
            war: Some(RingStyle::Color([255, 0, 0])),
            defense: Some(RingStyle::Color([0, 64, 255])),
            peace: Some(RingStyle::Color([0, 200, 0])),
            none: None,
        }
    }
}

const KINDS: [(&str, Relation); 4] = [("war", Relation::War), ("defense", Relation::Defense), ("peace", Relation::Peace), ("none", Relation::None)];

impl Rings {
    /// From `CIV3R_UNIT_RINGS`; `None` if unset, `off` or wrong (logged).
    pub fn from_env() -> Option<Rings> {
        let spec = std::env::var("CIV3R_UNIT_RINGS").ok()?;
        Rings::parse(&spec).inspect_err(|e| log::warn!("CIV3R_UNIT_RINGS: {e}")).ok().flatten()
    }

    pub fn parse(spec: &str) -> Result<Option<Rings>, String> {
        match spec.trim() {
            "" | "off" => return Ok(None),
            "on" => return Ok(Some(Rings::default())),
            _ => {}
        }
        let mut rings = Rings::default();
        for item in spec.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (key, value) = item.split_once('=').ok_or_else(|| format!("{item:?}: expected key=value"))?;
            if key == "width" {
                rings.width = value.parse().ok().filter(|w| (1..=8).contains(w)).ok_or_else(|| format!("width {value:?}: 1 to 8"))?;
                continue;
            }
            let style = match value {
                "off" => None,
                "invert" => Some(RingStyle::Invert),
                rgb => {
                    let c: Vec<u8> = rgb.split(';').map(|v| v.trim().parse().map_err(|_| format!("{key}: bad colour {rgb:?}"))).collect::<Result<_, _>>()?;
                    Some(RingStyle::Color(<[u8; 3]>::try_from(c).map_err(|_| format!("{key}: expected R;G;B"))?))
                }
            };
            *rings.style_mut(key).ok_or_else(|| format!("unknown key {key:?}"))? = style;
        }
        Ok(Some(rings))
    }

    fn style_mut(&mut self, key: &str) -> Option<&mut Option<RingStyle>> {
        Some(match key {
            "war" => &mut self.war,
            "defense" => &mut self.defense,
            "peace" => &mut self.peace,
            "none" => &mut self.none,
            _ => return None,
        })
    }

    fn style(&self, relation: Relation) -> Option<RingStyle> {
        match relation {
            Relation::War => self.war,
            Relation::Defense => self.defense,
            Relation::Peace => self.peace,
            Relation::None => self.none,
        }
    }

    /// Render description entries: `UnitRing@<kind>/0`, outlines of the
    /// team disc (§10.6), anchored like it.
    pub fn description(&self) -> String {
        let mut out = String::from("\n# Unit rings (generated, CIV3R_UNIT_RINGS).\n");
        for (kind, relation) in KINDS {
            let Some(style) = self.style(relation) else { continue };
            // An inverting ring only needs its shape.
            let [r, g, b] = match style {
                RingStyle::Color(c) => c,
                RingStyle::Invert => [255, 255, 255],
            };
            out += &format!(
                "[sheets.unit_ring_{kind}]\nimage = \"team_disc@0\"\norigin = [259, 155]\ncell = [128, 64]\ngrid = [1, 1]\ngroup = \"UnitRing@{kind}\"\noutline = {{ width = {}, color = [{r}, {g}, {b}, 255] }}\n\n",
                self.width
            );
        }
        out
    }

    /// The ring for a unit of a civ the viewer stands `relation` with: its
    /// sprite name and `scale_den` (inverting or not), if it gets one.
    pub fn sprite(&self, relation: Relation) -> Option<(String, u32)> {
        let style = self.style(relation)?;
        let kind = KINDS.iter().find(|k| k.1 == relation)?.0;
        let scale_den = if style == RingStyle::Invert { tile_render::Blend::Invert.scale_den() } else { 0 };
        Some((format!("UnitRing@{kind}/0"), scale_den))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs() {
        assert_eq!(Rings::parse("off").unwrap(), None);
        assert_eq!(Rings::parse("on").unwrap(), Some(Rings::default()));
        let r = Rings::parse("war=invert,peace=off,none=10;20;30,width=3").unwrap().unwrap();
        assert_eq!((r.war, r.peace, r.none, r.width), (Some(RingStyle::Invert), None, Some(RingStyle::Color([10, 20, 30])), 3));
        assert!(r.description().contains("group = \"UnitRing@none\""));
        assert!(!r.description().contains("UnitRing@peace"));
        assert!(Rings::parse("war=1;2").is_err());
        assert!(Rings::parse("width=0").is_err());
    }
}
