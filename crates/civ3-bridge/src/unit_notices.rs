//! The game's short map messages, anchored to their tile in every world
//! view. Their localized text, font and lifetime come from MapMessage, not
//! from guessing which unit commands failed or retaining recent TextOuts.

use civ3_assets::fonts::FontStyle;
use civ3_sys::map_messages::Message;
use menu::screen::Element;
use crate::{text::TextRun, ui_screen::Item};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub message: Message,
    pub font: FontStyle,
    pub underline: bool,
    pub baseline: i32,
    pub box_height: i32,
}

/// Read every active record, including ones outside the engine camera.
#[cfg(windows)]
pub fn read(game: &civ3_sys::Civ3) -> Vec<Notice> {
    game.map_messages().unwrap_or_default().into_iter().filter_map(|message| {
        let metrics = civ3_sys::map_messages::font(message.font)?;
        let font = inproc::gdi::font(metrics.handle)?;
        Some(Notice {
            message,
            font: FontStyle { face: font.face, height: font.height.abs(), weight: font.weight, italic: font.italic },
            underline: font.underline,
            baseline: metrics.baseline,
            box_height: metrics.box_height,
        })
    }).collect()
}

const ALL: [i32; 4] = [-(1 << 24), -(1 << 24), 1 << 24, 1 << 24];

/// Font atlases and the game's TextOutA measurement use byte-valued glyph
/// keys. Decode semantic state to Unicode, then encode only at this boundary.
fn glyph_keys(text: &str) -> String {
    text.chars().map(|c| {
        (0x80u8..=0x9F).map(char::from)
            .find(|&key| gdi_text::windows_1252_char(key) == c).unwrap_or(c)
    }).collect()
}

/// Game-zoom coordinates, as for city labels. Conversion to canonical
/// coordinates happens once when these become world sprites.
pub fn items(notices: &[Notice], zoomed_out: bool, background: [u8; 3], width: &mut impl FnMut(&FontStyle, &str) -> i32) -> Vec<Item> {
    let (tile_w, tile_h) = if zoomed_out { (64, 32) } else { (128, 64) };
    let mut out = Vec::new();
    for notice in notices {
        let m = &notice.message;
        if m.tile.0 < 0 || m.tile.1 < 0 || (m.tile.0 + m.tile.1) & 1 != 0 { continue; }
        let text = glyph_keys(&m.text);
        let text_width = width(&notice.font, &text);
        if text_width <= 0 { continue; }
        let w = text_width.saturating_add(4);
        let x = m.tile.0.saturating_mul(tile_w / 2).saturating_add(tile_w / 2).saturating_sub(w / 2).saturating_sub(2);
        let y = m.tile.1.saturating_mul(tile_h / 2).saturating_sub(notice.box_height).saturating_sub(2)
            .saturating_add(if m.below { tile_h / 2 } else { 0 });
        out.push(Item::Element(Element::Fill { rect: [x, y, w, notice.box_height], rgb: background, alpha: crate::ui::shade_alpha(40), clip: ALL }, false));
        let tx = x + (w - text_width) / 2;
        let ty = y + (notice.box_height - notice.baseline) / 2 + notice.baseline;
        let run = |x, y, color| TextRun {
            x, y, color, face: notice.font.face.clone(), height: -notice.font.height,
            weight: notice.font.weight, italic: notice.font.italic, underline: notice.underline,
            text: text.clone(), backdrop: None,
        };
        // MapMessage::draw sets a (2, 2) shadow, then its foreground colour.
        if m.shadow != u32::MAX {
            out.push(Item::Text(run(tx + 2, ty + 2, color(m.shadow)), ALL, false));
        }
        out.push(Item::Text(run(tx, ty, color(m.color)), ALL, false));
    }
    out
}

fn color(word: u32) -> [u8; 3] {
    // TextOut receives the packed channels shifted left by three, unlike
    // UI fill pixels, whose 5-bit channels are expanded to the full range.
    if word & 0x8000_0000 != 0 {
        [((word >> 7) & 0xF8) as u8, ((word >> 2) & 0xF8) as u8, ((word << 3) & 0xF8) as u8]
    } else { [0; 3] }
}

/// These must remain world sprites, above units and fog, with the units'
/// per-view Clean Map category. They must not become an interface overlay.
pub fn sprites(items: &[Item], fonts: &crate::text::Fonts, compiled: &render_desc::Compiled, outlines: bool, zoomed_out: bool) -> Vec<scene::FreeSprite> {
    let mut out = crate::ui_screen::sprites(items, fonts, compiled, u32::MAX, outlines);
    for s in &mut out {
        s.screen = false;
        s.clip = None;
        s.category = viewer_settings::clean_map::UNITS;
    }
    if zoomed_out { crate::scene_map::from_zoomed_out(&mut out); }
    out
}

#[cfg(windows)]
thread_local! {
    static DRAWING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(windows)]
pub fn drawing() -> bool { DRAWING.with(std::cell::Cell::get) }

/// Consume this pass's mirrored drawing so it cannot also appear as a
/// screen overlay. All its text/fills are emitted from semantic records.
#[cfg(windows)]
pub fn install(game: civ3_sys::Civ3) -> Result<Option<civ3_sys::map_messages::draw::Installed>, inproc::hook::HookError> {
    let Some((at, prologue)) = civ3_sys::map_messages::draw_site(game.build) else { return Ok(None); };
    civ3_sys::map_messages::draw::install_at_start(at, prologue, |call| {
        struct Reset(bool);
        impl Drop for Reset { fn drop(&mut self) { DRAWING.with(|d| d.set(self.0)); } }
        let _reset = Reset(DRAWING.with(|d| d.replace(true)));
        call.original();
    }).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn notice() -> Notice {
        Notice {
            message: Message { slot: 1, tile: (100, 40), text: "This unit has already moved".into(), font: 1, color: 0x8000_7FFF, shadow: 0x8000_0000, below: false },
            font: FontStyle { face: "Lucida Sans".into(), height: 14, weight: 0, italic: false },
            underline: false, baseline: 10, box_height: 14,
        }
    }
    #[test]
    fn notice_follows_world_tile_in_both_game_zooms_without_camera_culling() {
        let n = notice();
        let mut measure = |_: &FontStyle, _: &str| 180;
        for (zoom, expected) in [(false, [6370, 1264, 184, 14]), (true, [3138, 624, 184, 14])] {
            let items = items(std::slice::from_ref(&n), zoom, [8, 16, 24], &mut measure);
            assert_eq!(items.len(), 3);
            assert!(matches!(&items[0], Item::Element(Element::Fill { rect, rgb, alpha: 153, .. }, false) if *rect == expected && *rgb == [8,16,24]));
            let Item::Text(shadow, _, false) = &items[1] else { panic!() };
            let Item::Text(text, _, false) = &items[2] else { panic!() };
            assert_eq!((shadow.x - text.x, shadow.y - text.y), (2, 2));
            assert_eq!(text.text, n.message.text);
            assert_eq!(text.color, [248; 3]);
            assert_eq!(text.x, expected[0] + 2);
            assert_eq!(text.y, expected[1] + 12);
        }
    }
    #[test]
    fn localized_unicode_uses_matching_byte_keys_for_measurement_and_art() {
        let mut n = notice();
        n.message.text = "Déjà “déplacé” – l’unité €".into();
        let expected = "Déjà \u{93}déplacé\u{94} \u{96} l\u{92}unité \u{80}";
        let mut measured = String::new();
        let items = items(&[n.clone()], false, [0; 3], &mut |_, text| {
            measured = text.to_owned();
            100
        });
        assert_eq!(measured, expected);
        for item in &items {
            if let Item::Text(run, _, _) = item {
                assert_eq!(run.text, measured);
                assert_eq!(run.text.chars().map(gdi_text::windows_1252_char).collect::<String>(), n.message.text);
            }
        }
    }

    #[test]
    fn text_keeps_game_color_font_and_shadow_disable_semantics() {
        let mut n = notice();
        n.font.weight = 700;
        n.font.italic = true;
        n.underline = true;
        n.message.color = 0x8000_7C00;
        n.message.shadow = u32::MAX;
        let items = items(&[n], false, [0; 3], &mut |_, _| 100);
        assert_eq!(items.len(), 2, "-1 disables the shadow instead of drawing white");
        let Item::Text(text, _, false) = &items[1] else { panic!() };
        assert_eq!((text.color, text.height, text.weight, text.italic, text.underline), ([248, 0, 0], -14, 700, true, true));
        assert_eq!(color(0), [0; 3]);
        assert_eq!(color(255), [0; 3], "palette-index text on 16-bit canvases is black");
    }

    #[test]
    fn generated_plate_is_world_art_above_units_in_each_independent_view() {
        struct NoFiles;
        impl render_desc::ImageLoader for NoFiles {
            fn load(&self, _: &str, _: &toml::Table) -> Result<Vec<tile_render::RgbaImage>, String> { Err("no files required".into()) }
        }
        let n = notice();
        let mut width = |_: &FontStyle, _: &str| 180;
        let items = items(&[n], false, [8, 16, 24], &mut width);
        let mut art = menu::screen::ScreenArt::default();
        let Item::Element(plate, _) = &items[0] else { panic!() };
        assert!(art.add(std::slice::from_ref(plate)));
        let desc = format!("[grid]\nlayout = \"staggered\"\nstep = [64, 32]\n{}", crate::ui_screen::description(&art));
        let compiled = render_desc::Description::parse(&desc).unwrap().compile_layout(&NoFiles, 256).unwrap();
        let fonts = crate::text::Fonts::default();
        let normal = sprites(&items[..1], &fonts, &compiled, false, false);
        let out = sprites(&items[..1], &fonts, &compiled, false, true);
        assert!(!normal.is_empty());
        for (normal, out) in normal.iter().zip(&out) {
            assert!(!normal.screen);
            assert_eq!(normal.clip, None);
            assert_eq!(normal.not_in_view, None, "all independent views may draw this notice");
            assert_eq!(normal.category, viewer_settings::clean_map::UNITS);
            assert_eq!(normal.after_pass, u32::MAX);
            assert_eq!((out.x, out.y), (normal.x * 2, normal.y * 2));
        }
    }

    #[test]
    fn overlapping_notices_shift_half_a_tile_and_absent_records_draw_nothing() {
        let n = notice();
        let b = Notice { message: Message { below: true, ..n.message.clone() }, ..n.clone() };
        for (zoom, dy) in [(false, 32), (true, 16)] {
            let mut width = |_: &FontStyle, _: &str| 180;
            let a = items(&[n.clone()], zoom, [0;3], &mut width);
            let b = items(&[b.clone()], zoom, [0;3], &mut width);
            let (Item::Text(a, _, _), Item::Text(b, _, _)) = (&a[2], &b[2]) else { panic!() };
            assert_eq!((b.x-a.x, b.y-a.y), (0, dy));
            assert!(items(&[], zoom, [0;3], &mut width).is_empty());
        }
    }
}
