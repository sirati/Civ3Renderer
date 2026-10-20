//! The game's short map messages, anchored to their tile in every world
//! view. Their localized text, font and lifetime come from MapMessage, not
//! from guessing which unit commands failed or retaining recent TextOuts.

use civ3_assets::fonts::FontStyle;
use civ3_sys::map_messages::Message;
use menu::screen::Element;
use crate::{text::TextRun, ui_screen::Item};
use std::time::{Duration, Instant};

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

/// A notice of the renderer's own at `tile` (`crate::modal`: why a press
/// was not acted on), in the font and colours of the first of the game's
/// notice records that has them (shown or not).
#[cfg(windows)]
pub fn synthetic(game: &civ3_sys::Civ3, tile: (i32, i32), text: &str) -> Option<Notice> {
    use inproc::Ptr32;
    use civ3_sys::map_messages::{COUNT, FIRST, STRIDE};
    let first = game.addrs.main_screen_form.checked_add(FIRST)?;
    let (font_at, color, shadow, metrics) = (0..COUNT as u32).find_map(|k| {
        let word = |at: u32| Ptr32::<u32>::new(first + k * STRIDE as u32 + at).read().ok();
        let font_at = word(8)?;
        Some((font_at, word(0x14)?, word(0x10)?, civ3_sys::map_messages::font(font_at)?))
    })?;
    let font = inproc::gdi::font(metrics.handle)?;
    Some(Notice {
        message: Message { slot: usize::MAX, tile, text: text.into(), font: font_at, color, shadow, below: false, started: 0, duration: 0 },
        font: FontStyle { face: font.face, height: font.height.abs(), weight: font.weight, italic: font.italic },
        underline: font.underline,
        baseline: metrics.baseline,
        box_height: metrics.box_height,
    })
}

/// How long a game notice shows when its record gives no usable duration,
/// and the longest one may show.
const DEFAULT_FOR: Duration = Duration::from_millis(2500);
const LONGEST: Duration = Duration::from_secs(5);
/// The most notices shown at once (the most recently told).
pub const CAP: usize = 8;

/// What is shown of the game's and the renderer's notices: one per tile,
/// at most [`CAP`], each for its own time on the publisher's clock.
///
/// Every press on an exhausted unit makes the game add another record, and
/// a record is only freed by the game's animator. Mirroring the records
/// one-to-one stacked a plate per press, and records the animator did not
/// get to stayed forever. Here a notice told again only restarts its time,
/// and it goes when its time is up whatever the game's records still say.
#[derive(Default)]
pub struct Shown {
    /// The game's records seen last round: (slot, started, tile, text).
    seen: std::collections::HashSet<(usize, u32, (i32, i32), String)>,
    shown: Vec<Showing>,
    ours_seen: std::collections::HashSet<(Instant, (i32, i32), String)>,
    serial: u64,
}

struct Showing {
    notice: Notice,
    told: Instant,
    until: Instant,
    serial: u64,
    duration_ms: u32,
}

impl Shown {
    /// The notices to show at `now`, from the game's live records and the
    /// renderer's own (with when they were told).
    pub fn update(&mut self, now: Instant, game: Vec<Notice>, ours: Vec<(Instant, Notice)>) -> Vec<Notice> {
        let key = |n: &Notice| (n.message.tile, n.message.text.clone());
        let mut alive = std::collections::HashSet::new();
        let mut seen = std::collections::HashSet::new();
        for n in game {
            let m = &n.message;
            let id = (m.slot, m.started, m.tile, m.text.clone());
            alive.insert(key(&n));
            if !self.seen.contains(&id) {
                let lasts = Some(Duration::from_millis(m.duration.into())).filter(|d| !d.is_zero() && *d <= LONGEST).unwrap_or(DEFAULT_FOR);
                self.tell(n, now, now + lasts);
            }
            seen.insert(id);
        }
        self.seen = seen;
        let mut ours_seen = std::collections::HashSet::new();
        for (at, n) in ours {
            alive.insert(key(&n));
            let id = (at, n.message.tile, n.message.text.clone());
            // Polling is not another command. Evicted modal notices must
            // not compete for a slot again every publisher round.
            if !self.ours_seen.contains(&id) && now < at + crate::modal::NOTICE_FOR {
                self.tell(n, at, at + crate::modal::NOTICE_FOR);
            }
            ours_seen.insert(id);
        }
        self.ours_seen = ours_seen;
        // (Gone when the game or `crate::modal` dropped it, too.)
        self.shown.retain(|s| now < s.until && alive.contains(&key(&s.notice)));
        self.shown.iter().map(|s| s.notice.clone()).collect()
    }

    /// In the same order as `update`'s notices. Only another telling gets
    /// another serial, so unrelated scene updates cannot restart expiry.
    pub fn lifetimes(&self) -> Vec<(u64, u32)> {
        self.shown.iter().map(|s| (s.serial, s.duration_ms)).collect()
    }

    fn tell(&mut self, notice: Notice, told: Instant, until: Instant) {
        self.serial += 1;
        let serial = self.serial;
        let duration_ms = until.saturating_duration_since(told).as_millis() as u32;
        let m = &notice.message;
        match self.shown.iter().position(|s| s.notice.message.tile == m.tile) {
            // The same notice again: it stays where it is, for longer.
            Some(i) if self.shown[i].notice.message.text == m.text => {
                let s = &mut self.shown[i];
                s.told = s.told.max(told);
                if told >= s.told {
                    s.serial = serial;
                    s.duration_ms = duration_ms;
                }
                s.until = s.until.max(until);
            }
            // Another on the same tile replaces it, unless older.
            Some(i) => {
                if told >= self.shown[i].told {
                    self.shown[i] = Showing { notice: Self::alone(notice), told, until, serial, duration_ms };
                }
            }
            None => {
                self.shown.push(Showing { notice: Self::alone(notice), told, until, serial, duration_ms });
                if self.shown.len() > CAP {
                    let oldest = (0..self.shown.len()).min_by_key(|&i| self.shown[i].told).unwrap();
                    self.shown.remove(oldest);
                }
            }
        }
    }

    /// One per tile: never displaced below another.
    fn alone(mut notice: Notice) -> Notice {
        notice.message.below = false;
        notice
    }
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
pub fn sprites(items: &crate::world_items::WorldItems, fonts: &crate::text::Fonts, compiled: &render_desc::Compiled, outlines: bool) -> Vec<scene::FreeSprite> {
    let mut out = items.sprites(fonts, compiled, u32::MAX, outlines);
    for s in &mut out { s.category |= viewer_settings::clean_map::UNITS; }
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
            message: Message { slot: 1, tile: (100, 40), text: "This unit has already moved".into(), font: 1, color: 0x8000_7FFF, shadow: 0x8000_0000, below: false, started: 1000, duration: 2000 },
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
        assert!(!art.add(std::slice::from_ref(plate)), "a GPU solid plate needs no sprite art");
        let desc = format!("[grid]\nlayout = \"staggered\"\nstep = [64, 32]\n{}", crate::ui_screen::description(&art));
        let compiled = render_desc::Description::parse(&desc).unwrap().compile_layout(&NoFiles, 256).unwrap();
        let fonts = crate::text::Fonts::default();
        let layouts = crate::world_items::WorldItems::new(true, |reduced| {
            super::items(std::slice::from_ref(&notice()), reduced, [8, 16, 24], &mut width).into_iter().take(1).collect()
        });
        let out = sprites(&layouts, &fonts, &compiled, false);
        assert_eq!(out.len(), 2);
        for downscale in [1, 2, 1] {
            let camera = tile_render::Camera { downscale, ..tile_render::Camera::new((100, 100)) };
            let selected: Vec<_> = out.iter().filter(|s| s.category & camera.effective_hidden_categories() == 0).collect();
            assert_eq!(selected.len(), 1);
            let selected = selected[0];
            assert!(!selected.screen && selected.clip.is_none());
            assert_eq!(selected.not_in_view, None, "all independent views may draw this notice");
            assert_ne!(selected.category & viewer_settings::clean_map::UNITS, 0);
            assert_eq!(selected.after_pass, u32::MAX);
            let expected = super::items(std::slice::from_ref(&notice()), downscale > 1, [8, 16, 24], &mut width);
            let Item::Element(Element::Fill { rect, .. }, _) = expected[0] else { panic!() };
            assert_eq!((selected.x / downscale as i32, selected.y / downscale as i32), (rect[0], rect[1]));
            let solid = selected.solid.expect("notice plates are GPU rectangle commands");
            assert_eq!((solid.width, solid.height), (rect[2] as u32, rect[3] as u32));
        }
    }

    fn record(slot: usize, tile: (i32, i32), text: &str, started: u32) -> Notice {
        let mut n = notice();
        n.message = Message { slot, tile, text: text.into(), below: slot % 2 == 1, started, duration: 2000, ..n.message };
        n
    }

    #[test]
    fn spamming_one_notice_shows_it_once_for_longer() {
        let t0 = Instant::now();
        let mut shown = Shown::default();
        let mut records = Vec::new();
        let mut out = Vec::new();
        // 50 presses 100 ms apart, each a new game record (the game's slots
        // fill; none is ever freed).
        for k in 0..50u32 {
            records.push(record(k as usize, (10, 20), "This unit has already moved", 1000 + k * 100));
            out = shown.update(t0 + Duration::from_millis(k as u64 * 100), records.clone(), Vec::new());
            assert_eq!(out.len(), 1, "press {k}");
            assert!(!out[0].message.below, "it stays where it was");
        }
        // Lasts from the last press, not the first.
        let last = t0 + Duration::from_millis(4900);
        assert_eq!(shown.update(last + Duration::from_millis(1990), records.clone(), Vec::new()).len(), 1);
        assert_eq!(out[0].message.text, "This unit has already moved");
        assert!(shown.update(last + Duration::from_millis(2000), records.clone(), Vec::new()).is_empty());
    }

    #[test]
    fn notices_go_on_time_even_if_the_game_never_frees_them() {
        let t0 = Instant::now();
        let mut shown = Shown::default();
        let stuck = vec![record(0, (10, 20), "a", 5), record(1, (12, 20), "b", 6)];
        let ours = |at| vec![(at, record(usize::MAX, (14, 20), "Close the screen to attack", 0))];
        assert_eq!(shown.update(t0, stuck.clone(), ours(t0)).len(), 3);
        // Only the clock moves: the game's records stay as they were.
        for ms in [500, 1999] {
            assert_eq!(shown.update(t0 + Duration::from_millis(ms), stuck.clone(), ours(t0)).len(), 3);
        }
        assert_eq!(shown.update(t0 + Duration::from_millis(2000), stuck.clone(), ours(t0)).len(), 1);
        assert!(shown.update(t0 + crate::modal::NOTICE_FOR, stuck.clone(), ours(t0)).is_empty());
        assert!(shown.update(t0 + Duration::from_secs(60), stuck, Vec::new()).is_empty(), "stuck records do not come back");
        // A record that a duration is unusable for still goes.
        let mut forever = record(3, (1, 1), "x", 9);
        forever.message.duration = u32::MAX;
        let t1 = t0 + Duration::from_secs(61);
        assert_eq!(shown.update(t1, vec![forever.clone()], Vec::new()).len(), 1);
        assert!(shown.update(t1 + DEFAULT_FOR, vec![forever], Vec::new()).is_empty());
    }

    #[test]
    fn different_notices_on_different_units_each_show_once_and_are_capped() {
        let t0 = Instant::now();
        let mut shown = Shown::default();
        let records: Vec<_> = (0..3).flat_map(|i| [record(2 * i, (2 * i as i32, 0), "moved", 1), record(2 * i + 1, (2 * i as i32, 0), "moved", 2)]).collect();
        let out = shown.update(t0, records.clone(), Vec::new());
        assert_eq!(out.iter().map(|n| n.message.tile).collect::<Vec<_>>(), [(0, 0), (2, 0), (4, 0)]);
        // The game freeing one removes its copy at once.
        assert_eq!(shown.update(t0, records[2..].to_vec(), Vec::new()).len(), 2);
        // Another notice on a tile replaces the one there.
        let other = record(9, (2, 0), "other", 3);
        let out = shown.update(t0, [&records[2..], &[other][..]].concat(), Vec::new());
        assert_eq!(out.iter().map(|n| (n.message.tile, n.message.text.as_str())).collect::<Vec<_>>(), [((2, 0), "other"), ((4, 0), "moved")]);
        // Many tiles at once: the newest CAP.
        let many: Vec<_> = (0..20).map(|i| record(i, (2 * i as i32, 10), "m", 100 + i as u32)).collect();
        let mut shown = Shown::default();
        for k in 1..=many.len() {
            shown.update(t0 + Duration::from_millis(k as u64), many[..k].to_vec(), Vec::new());
        }
        let out = shown.update(t0 + Duration::from_millis(30), many, Vec::new());
        assert_eq!(out.len(), CAP);
        assert!(out.iter().all(|n| n.message.tile.0 >= 2 * (20 - CAP as i32)));
    }

    #[test]
    fn unchanged_modal_notices_do_not_evict_newer_game_notices() {
        let t0 = Instant::now();
        let mut shown = Shown::default();
        let ours = vec![(t0, record(usize::MAX, (100, 0), "modal", 0))];
        assert_eq!(shown.update(t0, Vec::new(), ours.clone()).len(), 1);
        let initial = shown.lifetimes();
        shown.update(t0, Vec::new(), ours.clone());
        assert_eq!(shown.lifetimes(), initial, "polling does not renew viewer lifetime");
        let game: Vec<_> = (0..CAP).map(|i| record(i, (2 * i as i32, 0), "game", i as u32)).collect();
        let t1 = t0 + Duration::from_millis(10);
        let first = shown.update(t1, game.clone(), ours.clone());
        assert_eq!(first.len(), CAP);
        assert!(first.iter().all(|n| n.message.text == "game"));
        assert_eq!(shown.update(t1, game, ours), first);
    }

    #[test]
    fn another_telling_renews_viewer_lifetime_even_when_text_and_tile_match() {
        let t0 = Instant::now();
        let mut shown = Shown::default();
        let n = record(0, (10, 20), "moved", 1);
        let first = shown.update(t0, vec![n.clone()], Vec::new());
        let lease = shown.lifetimes();
        assert_eq!(shown.update(t0, vec![n.clone()], Vec::new()), first);
        assert_eq!(shown.lifetimes(), lease);
        let mut restarted = n;
        restarted.message.started += 1;
        assert_eq!(shown.update(t0 + Duration::from_secs(1), vec![restarted], Vec::new()), first);
        assert!(shown.lifetimes()[0].0 > lease[0].0);
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
