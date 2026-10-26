//! Native file art for C3X's improvement-row mill icons. All raw game reads
//! and calls are in civ3-sys. The retained palette is decoded at file load;
//! drawing never opens a file or reads pixels from a game image/DC.

use std::{cell::RefCell, collections::HashMap};
use civ3_ui::mill_icons;
use civ3_ui::mill_icons::Candidate;
#[cfg(windows)]
use crate::ui::Event;

#[derive(Clone)]
struct Sheet { path: String, transparent: Vec<u8> }
thread_local! { static SHEETS: RefCell<HashMap<u32, Sheet>> = RefCell::new(HashMap::new()); }

#[derive(Clone, Copy)]
struct Answer { mill: u32, generate: bool, active: Option<bool>, inputs: Option<bool> }
struct Capture { row: civ3_sys::mill_icons::Row, answers: Vec<Answer>, ready: bool, error: Option<&'static str> }
thread_local! { static ROWS: RefCell<Vec<Option<Capture>>> = const { RefCell::new(Vec::new()) }; }

fn drawing_ready() {
    ROWS.with_borrow_mut(|rows| {
        if let Some(Some(c)) = rows.last_mut() {
            if c.ready { c.error = Some("duplicate mill draw boundary"); }
            c.ready = true;
        }
    });
}

fn generation(civ: u32, mill: u32, value: bool) {
    ROWS.with_borrow_mut(|rows| {
        let Some(Some(c)) = rows.last_mut() else { return };
        if civ as i32 != c.row.civ || !c.row.mills.iter().any(|m| m.address == mill) {
            c.error = Some("query does not name this row's mill/civ"); return;
        }
        c.answers.push(Answer { mill, generate: value, active: None, inputs: None });
    });
}

fn building_query(city: u32, improvement: u32, value: bool, inputs: bool) {
    ROWS.with_borrow_mut(|rows| {
        let Some(Some(c)) = rows.last_mut() else { return };
        let Some(last) = c.answers.last_mut() else { c.error = Some("building query without a mill"); return };
        if city != c.row.city || improvement as i32 != c.row.improvement || !last.generate
            || (inputs && (last.active != Some(true) || last.inputs.is_some())) || (!inputs && last.active.is_some()) {
            c.error = Some("unexpected native query order or row identity"); return;
        }
        if inputs { last.inputs = Some(value); } else { last.active = Some(value); }
    });
}

impl Capture {
    fn selected(&self) -> Result<Vec<(i32, i32)>, &'static str> {
        if let Some(e) = self.error { return Err(e); }
        let mut candidates = Vec::new();
        for a in &self.answers {
            let m = self.row.mills.iter().find(|m| m.address == a.mill).ok_or("mill source lost")?;
            if a.generate && a.active.is_none() || a.active == Some(true) && a.inputs.is_none() { return Err("native query result missing"); }
            candidates.push(Candidate { improvement: m.improvement, resource: m.resource, icon: m.icon,
                flags: m.flags, bonus: m.class == 0, can_generate: a.generate, active: a.active == Some(true), has_inputs: a.inputs == Some(true) });
        }
        // The original availability gates ran after all native queries.
        // No DC is needed to establish whether C3X enabled this draw.
        Ok(if self.ready { mill_icons::selected(self.row.improvement, candidates) } else { Vec::new() })
    }
}

/// No RefCell borrow spans a foreign call. Nested row draws restore their
/// enclosing capture, including an unreadable inner row or an unwind.
struct RowScope { active: bool }
impl RowScope {
    fn new(row: Option<civ3_sys::mill_icons::Row>) -> Self {
        ROWS.with_borrow_mut(|rows| rows.push(row.map(|row| Capture { row, answers: Vec::new(), ready: false, error: None })));
        Self { active: true }
    }
    fn finish(mut self) -> Option<Capture> {
        self.active = false;
        ROWS.with_borrow_mut(|rows| rows.pop().flatten())
    }
}
impl Drop for RowScope {
    fn drop(&mut self) { if self.active { ROWS.with_borrow_mut(|rows| { rows.pop(); }); } }
}

pub(crate) fn loaded(image: u32, path: &str) {
    if !path.replace('\\', "/").to_ascii_lowercase().ends_with("art/resources.pcx") { return; }
    SHEETS.with_borrow_mut(|s| { s.remove(&image); });
    let decoded = civ3_assets::files::read_file(path).map_err(|e| e.to_string())
        .and_then(|b| civ3_assets::pcx::decode(&b).map_err(|e| e.to_string()));
    match decoded {
        Ok(file) => {
            let palette = std::array::from_fn(|i| { let civ3_assets::Rgb(r, g, b) = file.palette.get(i as u8); [r, g, b] });
            SHEETS.with_borrow_mut(|s| { s.insert(image, Sheet { path: path.to_owned(), transparent: mill_icons::transparent(&palette) }); });
        }
        Err(e) => log::error!("C3X mill icons: no file art source for {path}: {e}"),
    }
}

pub(crate) fn cleared(image: u32) { SHEETS.with_borrow_mut(|s| { s.remove(&image); }); }

#[cfg(windows)]
pub(crate) struct Installed {
    _draw: civ3_sys::mill_icons::draw::Installed,
    _generate: civ3_sys::mill_icons::can_generate::Installed,
    _active: civ3_sys::mill_icons::active_building::Installed,
    _inputs: civ3_sys::mill_icons::has_inputs::Installed,
    _drawing: civ3_sys::mill_icons::drawing::Installed,
}

#[cfg(windows)]
pub(crate) fn install(build: civ3_sys::Build) -> Result<Option<Installed>, inproc::hook::HookError> {
    use civ3_sys::mill_icons as native;
    let Some(producer) = native::Producer::find(build)? else { return Ok(None) };
    let drawing = producer.intercept_drawing(crate::jgl_hooks::own_mode(), drawing_ready)?;
    let generate = native::can_generate::install_call_site(native::GENERATE_SITE, &native::GENERATE_BYTES, |c| {
        let result = c.original(); generation(c.civ, c.mill, result != 0); result
    })?;
    let active = native::active_building::install_call_site(native::ACTIVE_SITE, &native::ACTIVE_BYTES, |c| {
        let result = c.original(); building_query(c.city, c.improvement, result != 0, false); result
    })?;
    let inputs = native::has_inputs::install_call_site(native::INPUTS_SITE, &native::INPUTS_BYTES, |c| {
        let result = c.original(); building_query(c.city, c.improvement, result != 0, true); result
    })?;
    let hook = native::draw::install_call_site(native::SITE, &native::SITE_BYTES, move |c| {
        let scope = RowScope::new(producer.row());
        // C3X records the building and runs its actual queries. The checked
        // draw boundary skips all mill DC/GDI work in owned mode.
        // Our icons must precede
        // the tourism/yield and maintenance decorations that follow this call.
        let result = c.original();
        let Some(capture) = scope.finish() else { log::error!("C3X mill icons: unreadable producer state"); return result };
        let resources = match capture.selected() {
            Ok(resources) => resources,
            Err(e) => { log::error!("C3X mill icons: {e}"); return result; }
        };
        if !resources.is_empty() {
            // This GDI caller uses the actual destination DC clip. The
            // command helper clip_of intentionally omits a full clip, so
            // read the exact ABI field here, including an empty rectangle.
            let Some(clip) = civ3_sys::ui_hooks::canvas_text_clip(c.canvas) else {
                log::error!("C3X mill icons: unreadable canvas clip at {:#x}", c.canvas);
                return result;
            };
            let sheet = SHEETS.with_borrow(|s| s.get(&capture.row.sheet).cloned());
            match sheet {
                Some(sheet) => crate::ui_trace::mill_icons(Event::MillIcons {
                    canvas: c.canvas, path: sheet.path, resources, at: (c.x as i32, c.y as i32),
                    clip, transparent: sheet.transparent,
                }),
                None => log::error!("C3X mill icons: sheet {:#x} has no retained file source", capture.row.sheet),
            }
        }
        result
    })?;
    log::info!("C3X mill icons: native resource state and file art at PCG {:#x}", native::SITE);
    Ok(Some(Installed { _draw: hook, _generate: generate, _active: active, _inputs: inputs, _drawing: drawing }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{Event, ScreenDraw, UiModel};

    #[test]
    fn actual_query_answers_keep_short_circuit_order_and_nested_row_lifetimes() {
        use civ3_sys::mill_icons::{Mill, Row};
        let row = || Row { city: 100, city_id: 23, civ: 5, improvement: 2, sheet: 200,
            mills: (1..=3).map(|i| Mill { address: i * 10, improvement: 2, resource: i as i32,
                icon: i as i32 + 4, flags: 0, class: 2 }).collect() };
        let scope = RowScope::new(Some(row()));
        generation(5, 10, true); building_query(100, 2, true, false); building_query(100, 2, false, true);
        { let _unknown_inner = RowScope::new(None); generation(99, 999, true); }
        generation(5, 20, false);
        { let inner = RowScope::new(Some(row())); generation(5, 10, true); building_query(100, 2, false, false);
          assert!(inner.finish().unwrap().selected().unwrap().is_empty()); }
        generation(5, 30, true); building_query(100, 2, true, false); building_query(100, 2, true, true);
        drawing_ready();
        assert_eq!(scope.finish().unwrap().selected().unwrap(), [(3, 7)]);
        let incomplete = RowScope::new(Some(row())); generation(5, 10, true);
        assert_eq!(incomplete.finish().unwrap().selected(), Err("native query result missing"));
        let wrong = RowScope::new(Some(row())); generation(5, 10, true); building_query(101, 2, true, false);
        assert!(wrong.finish().unwrap().selected().is_err());
        let unavailable = RowScope::new(Some(row()));
        generation(5, 10, true); building_query(100, 2, true, false); building_query(100, 2, true, true);
        assert!(unavailable.finish().unwrap().selected().unwrap().is_empty(), "original source-availability gate did not enable drawing");
        let outer = RowScope::new(Some(row()));
        generation(5, 10, true); building_query(100, 2, true, false); building_query(100, 2, true, true);
        { let inner = RowScope::new(Some(row()));
          generation(5, 20, true); building_query(100, 2, true, false); building_query(100, 2, true, true);
          drawing_ready(); assert_eq!(inner.finish().unwrap().selected().unwrap(), [(2, 6)]); }
        assert!(outer.finish().unwrap().selected().unwrap().is_empty(), "nested draw readiness cannot enable its outer row");
        assert!(ROWS.with_borrow(|rows| rows.is_empty()));
    }

    #[test]
    #[ignore = "requires C3R_MILL_RULES pointing to conquests.biq"]
    fn enabled_and_disabled_config_fixture_uses_resolved_game_ids() {
        use civ3_ui::mill_icons::Candidate;
        let path = std::path::PathBuf::from(std::env::var_os("C3R_MILL_RULES").expect("C3R_MILL_RULES"));
        let rules = civ3_assets::biq::Biq::open(&path).unwrap().rules().unwrap();
        // C3X parse_mill resolves the first exact matching name, then stores
        // signed short IDs. No icon is selected by guessing a name.
        let improvement = rules.buildings.iter().position(|b| b.name == "Granary").unwrap() as i32;
        let resource = rules.resources.iter().position(|r| r.name == "Iron").unwrap() as i32;
        let icon = rules.resources[resource as usize].icon;
        let on = Candidate { improvement, resource, icon, flags: 0, bonus: false,
            can_generate: true, active: true, has_inputs: true };
        assert_eq!(mill_icons::selected(improvement, [on]), [(resource, icon)]);
        for off in [Candidate { can_generate: false, ..on }, Candidate { active: false, ..on }, Candidate { has_inputs: false, ..on }] {
            assert!(mill_icons::selected(improvement, [off]).is_empty());
        }
        assert!(mill_icons::selected(improvement, []).is_empty());
        let at = mill_icons::layout(&[(resource, icon)], (30, 40));
        eprintln!("fixture Granary improvement={improvement}, Iron resource={resource}, IconID={icon}, source={:?}, at={:?}; active/known/inputs=true", at[0].rect, at[0].at);
    }

    #[test]
    fn row_icons_keep_source_phase_canvas_order_and_list_clip_through_composition() {
        let mut model = UiModel::new(vec![], vec![]);
        model.apply(Event::Size { image: 30, size: (100, 100) });
        model.apply(Event::MillIcons { canvas: 30, path: "resources.pcx".into(), resources: vec![(82, 7), (2, 19)],
            at: (-3, 1), clip: [20, 10, 35, 30], transparent: vec![7, 255] });
        model.apply(Event::Fill { canvas: 30, rect: [22, 28, 2, 2], rgb: [1, 2, 3], alpha: 128, clip: None });
        model.apply(Event::Size { image: 40, size: (300, 300) });
        model.apply(Event::Copy { from: 30, src: [20, 10, 15, 20], canvas: 40, x: 80, y: 90, keyed: false });
        let frame = model.canvas_frame(40, 1).unwrap();
        let draws: Vec<_> = frame.draws.iter().filter_map(|d| match d {
            ScreenDraw::Indexed { rect, x, y, clip, indices, .. } => Some((*rect, *x, *y, *clip, indices)), _ => None,
        }).collect();
        assert_eq!(draws.len(), 2);
        assert_eq!((draws[0].0, draws[0].1, draws[0].2, draws[0].3), ([59, 59, 33, 33], 72, 81, [80, 90, 95, 105]));
        assert_eq!((draws[1].0, draws[1].1, draws[1].2, draws[1].3), ([59, 159, 33, 33], 72, 97, [80, 97, 95, 110]));
        assert!(draws.iter().all(|d| d.4.sample_offset == [65535, 32768] && d.4.transparent == [7, 255]));
        assert!(matches!(frame.draws.last(), Some(ScreenDraw::Fill { alpha: 128, .. })), "later row decorations stay above the icons");
        let items = crate::ui_screen::items(&frame, &[], &[], 0);
        let mut art = menu::screen::ScreenArt::default();
        art.add(&items.iter().filter_map(|i| match i { crate::ui_screen::Item::Element(e, _) => Some(e.clone()), _ => None }).collect::<Vec<_>>());
        assert_eq!(menu::screen::ScreenArt::parse_lines(&art.to_lines()), art, "preload keeps the sampling phase");
    }

    #[test]
    #[ignore = "requires C3R_MILL_ART and C3R_MILL_GDI from tools/mill-icon-gdi"]
    fn file_art_matches_original_transparent_blt_for_all_36_icons() {
        use civ3_assets::{GameFiles, loader::Civ3Loader};
        let path = std::path::PathBuf::from(std::env::var_os("C3R_MILL_ART").expect("C3R_MILL_ART"));
        let refs = std::path::PathBuf::from(std::env::var_os("C3R_MILL_GDI").expect("C3R_MILL_GDI"));
        let file = civ3_assets::pcx::decode(&std::fs::read(&path).unwrap()).unwrap();
        let palette = std::array::from_fn(|i| { let civ3_assets::Rgb(r,g,b)=file.palette.get(i as u8); [r,g,b] });
        let transparent = mill_icons::transparent(&palette);
        loaded(0x1234, path.to_str().unwrap());
        let retained = SHEETS.with_borrow(|s| s.get(&0x1234).cloned()).expect("load-time file source");
        assert_eq!(retained.transparent, transparent);
        assert_eq!(retained.path, path.to_str().unwrap());
        cleared(0x1234);
        assert!(SHEETS.with_borrow(|s| !s.contains_key(&0x1234)));
        let files = GameFiles::with_roots(vec![path.parent().unwrap().into()]);
        let loader = Civ3Loader::new(files);
        let mut compared = 0;
        for icon in 0..36 {
            let mut model = UiModel::new(vec![], vec![]);
            model.apply(Event::Size { image: 30, size: (100, 100) });
            model.apply(Event::MillIcons { canvas: 30, path: "resources.pcx".into(), resources: vec![(82, icon)],
                at: (0, 0), clip: [0, 0, 100, 100], transparent: transparent.clone() });
            let items = crate::ui_screen::items(&model.canvas_frame(30, 1).unwrap(), &[], &[], 0);
            let mut art = menu::screen::ScreenArt::default();
            art.add(&items.iter().filter_map(|i| match i { crate::ui_screen::Item::Element(e, _) => Some(e.clone()), _ => None }).collect::<Vec<_>>());
            let desc = format!("grid = {{ layout = 'orthogonal', step = [1,1] }}\n{}", crate::ui_screen::description(&art));
            let compiled = render_desc::Description::parse(&desc).unwrap().compile(&loader, 512).unwrap();
            let s = compiled.atlas.sprites.iter().find(|s| s.width == 24 && s.height == 24).unwrap();
            let page = &compiled.atlas.pages[s.page as usize];
            let reference = std::fs::read(refs.join(format!("icon{icon:02}.555"))).unwrap();
            assert_eq!(reference.len(), 1152);
            for y in 0..24 { for x in 0..24 {
                let [r,g,b,a] = page.get(s.atlas_x + x, s.atlas_y + y);
                assert!(a == 0 || a == 255);
                let word = if a == 0 { 0x1234 } else { (u16::from(r >> 3) << 10) | (u16::from(g >> 3) << 5) | u16::from(b >> 3) };
                let p = ((y * 24 + x) * 2) as usize;
                assert_eq!(word, u16::from_le_bytes([reference[p],reference[p+1]]), "icon {icon} at {x},{y}");
                compared += 1;
            }}
        }
        assert_eq!(compared, 20736);
        eprintln!("36 file icons, {compared} original GDI/native RGB555 pixels, zero differences");
    }
}
