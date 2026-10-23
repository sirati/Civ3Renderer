//! A second ring around foreign units' team discs, by how the viewing
//! player stands with their owner: at war, in a mutual defence pact, at
//! peace. Each ring is a band just outside the disc's own ring (render-desc
//! `outline`), in a colour or inverting what is under it.
//!
//! `CIV3R_UNIT_RINGS`: `on` for the defaults (war red, defence blue, peace
//! green, 2 px), or comma-separated `war=`, `defense=`, `peace=`, `none=`
//! with `R,G,B` in `;`-separated form (`war=255;0;0`), `invert` or `off`,
//! and `width=N`. Unset uses Accessibility preferences; `off` disables rings.

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

impl Rings {
    /// A present environment variable overrides even an enabled setting.
    /// Invalid overrides retain the old behaviour: log and disable rings.
    pub fn configured(settings: &viewer_settings::accessibility::Accessibility, spec: Option<&str>) -> Option<Self> {
        if let Some(spec) = spec {
            return Self::parse(spec).inspect_err(|e| log::warn!("CIV3R_UNIT_RINGS: {e}")).ok().flatten();
        }
        let style = |c: &viewer_settings::accessibility::RingColor| Some(if c.invert { RingStyle::Invert } else { RingStyle::Color(c.color) });
        settings.unit_rings.then(|| Self { width: settings.ring_width.clamp(1, 6), war: style(&settings.war),
            defense: style(&settings.allied), peace: style(&settings.peace), none: None })
    }

    /// White coverage masks shared by every colour and relation. All widths
    /// are resident, so accepting Preferences never requires new ring art.
    pub fn all_description() -> String {
        Self::mask_image() + &(1..=8).map(Self::mask_sheet).collect::<String>()
    }

    fn mask_image() -> String {
        // Isolate precisely the native team-disc ink (palette index 247),
        // including its empty centre, before generating its outer outline.
        let transparent = (0..=255).filter(|&i| i != 247).map(|i| i.to_string()).collect::<Vec<_>>().join(",");
        format!("\n[images.unit_ring_mask]\nsource = \"Art/Cursor.pcx\"\ntransparent = [{transparent}]\nindexed = false\n")
    }

    fn mask_sheet(width: u32) -> String {
        let width = width.clamp(1, 8);
        format!("\n[sheets.unit_ring_width{width}]\nimage = \"unit_ring_mask\"\norigin = [259, 155]\ncell = [128, 64]\ngrid = [1, 1]\ngroup = \"UnitRing@width{width}\"\noutline = {{ width = {width}, color = [255, 255, 255, 255] }}\n\n")
    }
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

    /// Render description entries: `UnitRing@width<N>/0`, outlines of the
    /// team disc (§10.6), anchored like it.
    pub fn description(&self) -> String {
        Self::mask_image() + &Self::mask_sheet(self.width)
    }

    /// The ring for a unit of a civ the viewer stands `relation` with: its
    /// sprite name and `scale_den` (inverting or not), if it gets one.
    pub fn sprite(&self, relation: Relation) -> Option<(String, u32)> {
        let style = self.style(relation)?;
        let scale_den = match style {
            RingStyle::Invert => tile_render::Blend::Invert.scale_den(),
            RingStyle::Color(color) => tile_render::Blend::Subpixel { color }.scale_den(),
        };
        Some((format!("UnitRing@width{}/0", self.width.clamp(1, 8)), scale_den))
    }

    pub fn tint(&self, relation: Relation) -> u32 {
        match self.style(relation) {
            Some(RingStyle::Color(color)) => tile_render::Blend::Subpixel { color }.tint(),
            _ => 0,
        }
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
        assert!(r.description().contains("group = \"UnitRing@width3\""));
        assert!(Rings::parse("war=1;2").is_err());
        assert!(Rings::parse("width=0").is_err());
    }

    #[test]
    fn settings_and_explicit_environment_precedence() {
        use viewer_settings::accessibility::Accessibility;
        let mut a = Accessibility::default();
        assert_eq!(Rings::configured(&a, None), None);
        assert_eq!(Rings::configured(&a, Some("on")), Some(Rings::default()));
        a.unit_rings = true;
        a.war.color = [32, 208, 216];
        a.allied.invert = true;
        a.ring_width = 5;
        let r = Rings::configured(&a, None).unwrap();
        assert_eq!((r.war, r.defense, r.peace, r.width), (Some(RingStyle::Color(a.war.color)), Some(RingStyle::Invert), Some(RingStyle::Color(a.peace.color)), 5));
        assert_eq!(r.tint(Relation::War), 0x20d0d8);
        assert_eq!(r.sprite(Relation::Defense).unwrap().1, tile_render::Blend::Invert.scale_den());
        for spec in ["", "off", "invalid"] { assert_eq!(Rings::configured(&a, Some(spec)), None); }
        let env = "war=invert,none=12;34;56,width=8";
        assert_eq!(Rings::configured(&a, Some(env)), Rings::parse(env).unwrap());
    }
}
