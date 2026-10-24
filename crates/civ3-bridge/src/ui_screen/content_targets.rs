//! Pointer positions on semantic rows, derived from their drawn content.
use civ3_sys::controls::{Control, ControlKind};
use scene::Interactable;

use crate::text::{Fonts, TextRun};
use crate::ui::{Frame, ScreenDraw};

/// Set a pointer tip just after a list entry's name (in the middle of its
/// row), else at an icon's lower right corner or at the end of its
/// text, and give a check box its label as the region it draws button
/// snapping from. Only existing game controls qualify. Draws never create
/// targets.
pub fn content_anchors(frame: &Frame, fonts: &Fonts, controls: &[Control], targets: &mut [Interactable]) {
    annotate(frame, controls, targets, |run| fonts.ink_bounds(run));
    label_regions(frame, controls, targets, |run| fonts.ink_bounds(run));
}

/// A check box's label (the game draws it beside the box, outside the
/// box's hit cell): the text on the box's row starting within one and a
/// half box heights of it, and text following that as closely (several
/// runs), up to the next box on the row. Lines drawn on the row bridge
/// the way to it (the foreign advisor's treaty boxes: a legend line, its
/// name in the middle). With the box it is the control's region
/// (`scene::Interactable::region`); the box stays where the pointer goes
/// and clicks, which toggles it.
fn label_regions(frame: &Frame, controls: &[Control], targets: &mut [Interactable], text_bounds: impl Fn(&TextRun) -> Option<[i32; 4]>) {
    let boxes: Vec<&Control> = controls.iter().filter(|c| c.check_box() && c.rect[2] > 0 && c.rect[3] > 0).collect();
    if boxes.is_empty() {
        return;
    }
    // Text (true), and fills: lines or ticks that may lead to it.
    let mut runs: Vec<([i32; 4], bool)> = frame.draws.iter().filter_map(|d| match d {
        ScreenDraw::Text { run, clip } => text_bounds(run).and_then(|r| intersection(r, *clip)).map(|r| (r, true)),
        ScreenDraw::Fill { rect, clip, alpha, .. } if *alpha != 0 => intersection(*rect, *clip).map(|r| (r, false)),
        _ => None,
    }).collect();
    runs.sort_unstable_by_key(|(r, _)| (r[0], r[1]));
    for c in &boxes {
        let [x, y, w, h] = c.rect;
        let gap = h.max(8) * 3 / 2;
        // (Half its ink, or half the box, on the box's row.)
        let on_row = |r: &[i32; 4]| (r[1] + r[3]).min(y + h) - r[1].max(y) >= r[3].min(h) / 2 + 1;
        // Up to the next box on its row.
        let stop = boxes.iter().filter(|o| o.id != c.id && o.rect[0] > x && on_row(&o.rect)).map(|o| o.rect[0]).min().unwrap_or(i32::MAX);
        let [mut l, mut t, mut r, mut b] = [x, y, x + w, y + h];
        // How far the row's content reaches, lines included; the label ends
        // with its last text.
        let mut reach = r;
        // (A line no taller than the box: not a panel behind the row.)
        for (run, text) in runs.iter().filter(|(run, text)| on_row(run) && run[0] >= x && run[0] + run[2] <= stop && (*text || run[3] <= h)) {
            if run[0] > reach + gap {
                break;
            }
            reach = reach.max(run[0] + run[2]);
            if *text {
                (l, t, r, b) = (l.min(run[0]), t.min(run[1]), reach, b.max(run[1] + run[3]));
            }
        }
        let region = [l, t, r - l, b - t];
        if region != c.rect {
            if let Some(target) = targets.iter_mut().find(|t| t.id == c.id) {
                target.region = Some(region);
            }
        }
    }
}

/// The military advisor's unit columns that hold a drawn unit icon
/// (`units_32.pcx` on its canvas within a column strip).
pub fn military_unit_controls(frame: &Frame, strips: &[civ3_sys::controls::screens::UnitStrip]) -> Vec<Control> {
    let mut out = Vec::new();
    for strip in strips {
        let Some((_, range)) = frame.form_draws.iter().find(|(c, _)| *c == strip.canvas) else { continue };
        let icons = frame.draws.get(range.clone()).unwrap_or(&[]).iter().filter_map(|d| {
            let (path, x, y, w, h, clip) = match d {
                ScreenDraw::Image { path, rect, x, y, clip, opacity, .. } if *opacity != 0 => (path, *x, *y, rect[2], rect[3], *clip),
                ScreenDraw::ResizedImage { path, x, y, size: (w, h), clip, opacity, .. } if *opacity != 0 => (path, *x, *y, *w, *h, *clip),
                _ => return None,
            };
            if !path.to_ascii_lowercase().replace('/', "\\").ends_with("units_32.pcx") {
                return None;
            }
            let [cl, ct, cr, cb] = clip;
            let r = intersection([x, y, w, h], [cl, ct, cr, cb])?;
            intersection(r, [strip.rect[0], strip.rect[1], strip.rect[0] + strip.rect[2], strip.rect[1] + strip.rect[3]])
        });
        out.extend(civ3_sys::controls::screens::unit_columns(strip, icons));
    }
    out
}

fn intersection([x, y, w, h]: [i32; 4], [l, t, r, b]: [i32; 4]) -> Option<[i32; 4]> {
    let (left, top) = (x.max(l), y.max(t));
    let (right, bottom) = (x.saturating_add(w).min(r), y.saturating_add(h).min(b));
    (right > left && bottom > top).then_some([left, top, right - left, bottom - top])
}

fn annotate(frame: &Frame, controls: &[Control], targets: &mut [Interactable], text_bounds: impl Fn(&TextRun) -> Option<[i32; 4]>) {
    for control in controls.iter().filter(|c| c.content_row()) {
        let Some(target) = targets.iter_mut().find(|t| t.id == control.id) else {
            continue;
        };
        let [x, y, w, h] = control.rect;
        if w <= 0 || h <= 0 {
            continue;
        }
        let clip = [x, y, x.saturating_add(w), y.saturating_add(h)];
        let owner = (control.id >> 32) as u32;
        let canvas = owner.checked_add(civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4);
        let draws = canvas
            .and_then(|canvas| frame.form_draws.iter().find(|(c, _)| *c == canvas))
            .and_then(|(_, range)| frame.draws.get(range.clone()))
            .unwrap_or(&[]);
        let mut icon: Option<[i32; 4]> = None;
        let mut text: Option<[i32; 4]> = None;
        // A list entry's name: its first text (leftmost, then topmost).
        let mut name: Option<[i32; 4]> = None;
        for draw in draws {
            match draw {
                ScreenDraw::Image {
                    path,
                    rect,
                    x,
                    y,
                    clip: image_clip,
                    opacity,
                    ..
                } if *opacity != 0 && rect[2] <= h.saturating_mul(2) && rect[3] <= h.saturating_mul(2) && !path.to_ascii_lowercase().contains("background") => {
                    // A row's icon, never a large panel behind the row.
                    if let Some(r) = intersection([*x, *y, rect[2], rect[3]], *image_clip).and_then(|r| intersection(r, clip)) {
                        if icon.is_none_or(|old| (r[0], -(r[2] as i64 * r[3] as i64)) < (old[0], -(old[2] as i64 * old[3] as i64))) {
                            icon = Some(r);
                        }
                    }
                }
                ScreenDraw::ResizedImage { path, x, y, size: (dw, dh), clip: image_clip, opacity, .. }
                    if *opacity != 0 && *dw <= h.saturating_mul(2) && *dh <= h.saturating_mul(2)
                        && !path.to_ascii_lowercase().contains("background") => {
                    if let Some(r) = intersection([*x, *y, *dw, *dh], *image_clip).and_then(|r| intersection(r, clip)) {
                        if icon.is_none_or(|old| (r[0], -(r[2] as i64 * r[3] as i64)) < (old[0], -(old[2] as i64 * old[3] as i64))) {
                            icon = Some(r);
                        }
                    }
                }
                // (Text whose baseline is a line or more away from the row
                // has no ink in it: not measured.)
                ScreenDraw::Text { run, clip: text_clip } if (clip[1] - 2 * run.height.abs().max(16)..clip[3] + 2 * run.height.abs().max(16)).contains(&run.y) => {
                    if let Some(r) = text_bounds(run).and_then(|r| intersection(r, *text_clip)).and_then(|r| intersection(r, clip)) {
                        if text.is_none_or(|old| (r[0] + r[2], r[1]) > (old[0] + old[2], old[1])) {
                            text = Some(r);
                        }
                        if name.is_none_or(|old| (r[0], r[1]) < (old[0], old[1])) {
                            name = Some(r);
                        }
                    }
                }
                _ => {}
            }
        }
        // An advisor's speech box is all panel art: its words hold content.
        use civ3_sys::controls::advisors::{CELL_SPEECH, cell_kind};
        let icon = icon.filter(|_| cell_kind(control.id) != Some(CELL_SPEECH));
        target.anchor = if let (ControlKind::ListItem, Some([nx, _, nw, _])) = (control.kind, name) {
            // A list entry: just after the end of its name, in the middle
            // of its row (its icon, if any, before the name; other columns
            // after it), so it is clear which row a click takes.
            Some([(nx + nw).clamp(x, x + w - 1), y + h / 2])
        } else if let Some([x, y, w, h]) = icon {
            Some([x + w - 1, y + h - 1])
        } else if let Some([x, y, w, h]) = text {
            Some([x + w - 1, y + h / 2])
        } else if control.text_link() {
            // The link table already gives its exact rendered text region.
            Some([x + w - 1, y + h / 2])
        } else {
            None
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use civ3_sys::controls::ControlKind;
    const FORM: u32 = 0x10_0000;
    fn row(index: u32, y: i32, link: bool) -> Control {
        Control {
            rect: [20, y, 400, 70],
            kind: if link { ControlKind::Button } else { ControlKind::ListItem },
            id: (FORM as u64) << 32 | (if link { 0x1000_0000 } else { 0 }) | index as u64,
        }
    }
    fn label(text: &str, x: i32, baseline: i32) -> ScreenDraw {
        ScreenDraw::Text {
            run: TextRun {
                x,
                y: baseline,
                text: text.into(),
                face: "fixture".into(),
                height: 16,
                color: [255; 3],
                weight: 400,
                italic: false,
                underline: false,
                backdrop: None,
            },
            clip: [0, 0, 1024, 768],
        }
    }
    fn bounds(run: &TextRun) -> Option<[i32; 4]> {
        Some([run.x, run.y - 12, run.text.len() as i32 * 7, 16])
    }
    fn targets(controls: &[Control]) -> Vec<Interactable> {
        crate::ui_screen::interactables(controls)
    }
    #[test]
    fn civilopedia_icons_and_scenario_text_snap_to_their_content() {
        let controls = [row(0, 20, false), row(1, 100, false), row(2, 180, true)];
        let draws = vec![
            ScreenDraw::Image {
                path: "Art/Civilopedia/Terrain.pcx".into(),
                rect: [0, 0, 64, 64],
                x: 22,
                y: 22,
                keyed: true,
                clip: [0, 0, 1024, 768],
                mask: None,
                palette: None,
                opacity: 255,
            },
            label("Terrain", 100, 60),
            label("A scenario", 22, 130),
            label("Units", 22, 210),
        ];
        let frame = Frame {
            form_draws: vec![(FORM + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4, 0..draws.len())],
            draws,
            ..Default::default()
        };
        let mut got = targets(&controls);
        annotate(&frame, &controls, &mut got, bounds);
        assert_eq!(
            got.iter().map(|t| t.anchor).collect::<Vec<_>>(),
            // (List entries: after their names, mid-row; a link: its text.)
            [Some([149, 55]), Some([92, 135]), Some([56, 206])]
        );
        assert_eq!(
            got.iter().map(|t| t.rect).collect::<Vec<_>>(),
            controls.map(|c| c.rect),
            "anchors preserve real game hit boxes"
        );
    }
    #[test]
    fn clip_to_visible_content_and_ignore_other_forms_and_decorations() {
        let control = row(0, 20, false);
        let draws = vec![
            ScreenDraw::Image {
                path: "Art/Civilopedia/Terrain.pcx".into(),
                rect: [0, 0, 64, 64],
                x: 22,
                y: 22,
                keyed: true,
                clip: [0, 0, 50, 45],
                mask: None,
                palette: None,
                opacity: 255,
            },
            label("Unrelated", 200, 60),
        ];
        let frame = Frame {
            form_draws: vec![(FORM + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4, 0..1), (0x999, 1..2)],
            draws,
            ..Default::default()
        };
        let mut got = targets(&[control]);
        annotate(&frame, &[control], &mut got, bounds);
        assert_eq!(got[0].anchor, Some([49, 44]));
        assert_eq!(got.len(), 1);
        let mut only_decoration = Vec::new();
        annotate(&frame, &[], &mut only_decoration, bounds);
        assert!(only_decoration.is_empty());
    }
    #[test]
    fn exact_rich_text_hit_rect_is_a_valid_anchor_before_fonts_arrive() {
        let control = row(0, 180, true);
        let mut got = targets(&[control]);
        annotate(&Frame::default(), &[control], &mut got, |_| None);
        assert_eq!(got[0].anchor, Some([419, 215]));
    }
    #[test]
    fn a_unit_list_entry_with_a_scaled_icon_anchors_after_its_name() {
        let controls = [row(0, 20, false)];
        let frame = Frame { draws: vec![
            ScreenDraw::ResizedImage { path: "Art/units32.pcx".into(), rect: [0,0,240,240], x:22, y:22,
                size:(31,31), sample_step:[500000,500000], keyed:true, clip:[0,0,1024,768], mask:None,palette:None,opacity:255 },
            label("Unit",100,60),
        ], form_draws: vec![(FORM+4+civ3_sys::layout::BASE_FORM_DATA_CANVAS,0..2)], ..Default::default() };
        let mut targets = targets(&controls);
        annotate(&frame, &controls, &mut targets, bounds);
        assert_eq!(targets[0].anchor, Some([128, 55]));
    }

    /// The city screen's production list: icon, name, its statistics, the
    /// turns at the right; a long name on two lines.
    #[test]
    fn production_entries_point_just_after_their_name_in_the_middle_of_the_row() {
        let controls = [row(0, 20, false), row(1, 100, false)];
        let icon = |y| ScreenDraw::Image { path: "Art/units/units_32.pcx".into(), rect: [0, 0, 32, 32], x: 24, y, keyed: true,
            clip: [0, 0, 1024, 768], mask: None, palette: None, opacity: 255 };
        let draws = vec![
            icon(40), label("Settler", 70, 60), label("0.0.1", 140, 60), label("14 turns", 350, 60),
            icon(120), label("Longbowman", 70, 132), label("4(2).1.1", 70, 148), label("19 turns", 350, 140),
        ];
        let frame = Frame { form_draws: vec![(FORM + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4, 0..draws.len())], draws, ..Default::default() };
        let mut got = targets(&controls);
        annotate(&frame, &controls, &mut got, bounds);
        // "Settler" ends at 70 + 7 * 7; "Longbowman" at 70 + 10 * 7.
        assert_eq!(got.iter().map(|t| t.anchor).collect::<Vec<_>>(), [Some([119, 55]), Some([140, 135])]);
        for (t, c) in got.iter().zip(&controls) {
            let [x, y, w, h] = c.rect;
            let [ax, ay] = t.anchor.unwrap();
            assert!(ax >= x && ax < x + w && ay >= y && ay < y + h, "inside its row");
        }
    }

    #[test]
    fn domestic_advisor_cells_point_at_their_icon_or_words() {
        use civ3_sys::controls::advisors::{CELL_CITY, CELL_PRODUCTION, CELL_SPEECH, cell_id};
        let cell = |kind, rect| Control { rect, kind: ControlKind::Button, id: cell_id(FORM, kind, 0) };
        let controls = [cell(CELL_CITY, [96, 305, 145, 37]), cell(CELL_PRODUCTION, [811, 306, 114, 34]), cell(CELL_SPEECH, [812, 117, 191, 123])];
        let image = |path: &str, x, y, w, h| ScreenDraw::Image { path: path.into(), rect: [0, 0, w, h], x, y, keyed: true,
            clip: [0, 0, 1024, 768], mask: None, palette: None, opacity: 255 };
        let draws = vec![
            label("Rome", 111, 341),
            image("art/city screen/buildings-small.pcx", 864, 309, 27, 26),
            label("Wealth", 895, 335),
            image("Art/Advisors/dialogbox.pcx", 804, 108, 206, 132),
            label("More", 969, 244),
        ];
        let frame = Frame { form_draws: vec![(FORM + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4, 0..draws.len())], draws, ..Default::default() };
        let mut got = targets(&controls);
        annotate(&frame, &controls, &mut got, bounds);
        assert_eq!(got.iter().map(|t| t.anchor).collect::<Vec<_>>(), [Some([138, 335]), Some([890, 334]), Some([996, 236])]);
    }

    #[test]
    fn military_unit_columns_come_from_drawn_unit_icons_only() {
        use civ3_sys::controls::screens::UnitStrip;
        let canvas = FORM + civ3_sys::layout::BASE_FORM_DATA_CANVAS + 4;
        let strip = UnitStrip { form: FORM, canvas, side: 0, row: 2, rect: [631, 600, 346, 45], left: 631 };
        let image = |path: &str, x| ScreenDraw::Image { path: path.into(), rect: [0, 0, 32, 32], x, y: 606, keyed: true,
            clip: [0, 0, 1920, 1080], mask: None, palette: None, opacity: 255 };
        let draws = vec![image("Art/units_32.pcx", 633), image("Art\\Units_32.PCX", 633 + 64), image("Art/other.pcx", 633 + 32)];
        let frame = Frame { form_draws: vec![(canvas, 0..draws.len())], draws, ..Default::default() };
        let got = military_unit_controls(&frame, &[strip]);
        assert_eq!(got.iter().map(|c| (c.rect, c.id as u32 & 0xFFF)).collect::<Vec<_>>(), [([632, 600, 32, 45], 0x200), ([696, 600, 32, 45], 0x202)]);
        assert!(military_unit_controls(&Frame::default(), &[strip]).is_empty());
    }

    /// The foreign advisor's Treaties tab as the game drew it (the trace
    /// `featfix/logs/trace-c-f4.txt`, 1920×1080): six check boxes, each a
    /// form of its own, their hit cells the 22×16 boxes; each label a
    /// legend line on the box's row, the treaty's name in its middle.
    const TREATIES: &str = r#"ui_text 448 156 1472 924 text dc 0 at 1191 461 color 0 112 184 font "Lucida Sans"/-12/0/0/0 str Treaties
ui_text 448 156 1472 924 text dc 0 at 1263 461 color 0 0 0 font "Lucida Sans"/-12/0/0/0 str Trades
ui_text 448 156 1472 924 text dc 0 at 1295 501 color 248 0 0 font "Lucida Sans"/-14/0/0/0 str War
ui_fill 1218 495 66 3 255 0 0 448 156 1472 924 128
ui_fill 1284 490 1 10 255 0 0 448 156 1472 924 128
ui_fill 1332 490 1 10 255 0 0 448 156 1472 924 128
ui_fill 1332 495 66 3 255 0 0 448 156 1472 924 128
ui_text 448 156 1472 924 text dc 0 at 1288 534 color 0 0 248 font "Lucida Sans"/-14/0/0/0 str Peace
ui_fill 1218 528 60 3 0 0 255 448 156 1472 924 128
ui_fill 1278 523 1 10 0 0 255 448 156 1472 924 128
ui_fill 1338 523 1 10 0 0 255 448 156 1472 924 128
ui_fill 1338 528 60 3 0 0 255 448 156 1472 924 128
ui_text 448 156 1472 924 text dc 0 at 1247 567 color 0 128 128 font "Lucida Sans"/-14/0/0/0 str Mutual Protection
ui_fill 1218 561 28 3 0 132 132 448 156 1472 924 128
ui_fill 1246 556 1 10 0 132 132 448 156 1472 924 128
ui_fill 1370 556 1 10 0 132 132 448 156 1472 924 128
ui_fill 1370 561 28 3 0 132 132 448 156 1472 924 128
ui_text 448 156 1472 924 text dc 0 at 1251 600 color 0 128 0 font "Lucida Sans"/-14/0/0/0 str Right of Passage
ui_fill 1218 594 30 3 0 132 0 448 156 1472 924 128
ui_fill 1248 589 1 10 0 132 0 448 156 1472 924 128
ui_fill 1368 589 1 10 0 132 0 448 156 1472 924 128
ui_fill 1368 594 30 3 0 132 0 448 156 1472 924 128
ui_text 448 156 1472 924 text dc 0 at 1254 633 color 240 96 32 font "Lucida Sans"/-14/0/0/0 str Military Alliance
ui_fill 1218 627 33 3 247 99 33 448 156 1472 924 128
ui_fill 1251 622 1 10 247 99 33 448 156 1472 924 128
ui_fill 1365 622 1 10 247 99 33 448 156 1472 924 128
ui_fill 1365 627 33 3 247 99 33 448 156 1472 924 128
ui_text 448 156 1472 924 text dc 0 at 1218 639 color 240 96 32 font "Lucida Sans"/-10/0/0/0 str With
ui_text 448 156 1472 924 text dc 0 at 1361 639 color 240 96 32 font "Lucida Sans"/-10/0/0/0 str Against
ui_text 448 156 1472 924 text dc 0 at 1270 662 color 128 128 128 font "Lucida Sans"/-10/0/0/0 str Locked Alliance
ui_fill 1218 660 33 3 132 132 132 448 156 1472 924 128
ui_fill 1251 655 1 10 132 132 132 448 156 1472 924 128
ui_fill 1365 655 1 10 132 132 132 448 156 1472 924 128
ui_fill 1365 660 33 3 132 132 132 448 156 1472 924 128
ui_text 448 156 1472 924 text dc 0 at 1193 705 color 0 0 0 font "Lucida Sans"/-12/0/0/0 str Select All
ui_text 448 156 1472 924 text dc 0 at 1363 705 color 0 0 0 font "Lucida Sans"/-12/0/0/0 str All Off
"#;

    /// Its six boxes (the trace's `control` lines), top to bottom.
    fn treaty_boxes() -> Vec<Control> {
        [(485, 0xa4_aff8u64), (518, 0xa4_b68c), (551, 0xa4_bd20), (584, 0xa4_c3b4), (617, 0xa4_ca48), (650, 0xa4_d0dc)]
            .map(|(y, form)| Control { rect: [1193, y, 22, 16], kind: ControlKind::Button, id: form << 32 | 0x4000_0000 })
            .to_vec()
    }

    #[test]
    fn the_foreign_advisors_treaty_boxes_take_their_legend_names() {
        // (Ink: three quarters of the height above the baseline, half of
        // it a letter.)
        let ink = |run: &TextRun| {
            let h = run.height.abs();
            Some([run.x, run.y - h * 3 / 4, run.text.chars().count() as i32 * h / 2, h * 3 / 4])
        };
        let frame = crate::world::Trace::parse(TREATIES).unwrap().ui.unwrap();
        let controls = treaty_boxes();
        let mut got = targets(&controls);
        label_regions(&frame, &controls, &mut got, ink);
        let names = ["War", "Peace", "Mutual Protection", "Right of Passage", "Military Alliance", "Locked Alliance"];
        for ((t, c), name) in got.iter().zip(&controls).zip(names) {
            let region = t.region.unwrap_or_else(|| panic!("{name}: the box alone"));
            let run = frame.draws.iter().find_map(|d| match d { ScreenDraw::Text { run, .. } if run.text == name => Some(run), _ => None }).unwrap();
            let [nx, ny, nw, nh] = ink(run).unwrap();
            let [x, y, w, h] = region;
            assert!(x <= c.rect[0] && y <= c.rect[1] && x + w >= c.rect[0] + c.rect[2] && y + h >= c.rect[1] + c.rect[3], "{name}: {region:?} holds the box");
            assert!(x <= nx && y <= ny && x + w >= nx + nw && y + h >= ny + nh, "{name}: {region:?} holds its name");
            // The box is still where it points and clicks.
            assert_eq!((t.rect, t.anchor), (c.rect, None));
        }
        // Rows apart: no region reaches into the next box's row.
        for pair in got.windows(2) {
            let (a, b) = (pair[0].region.unwrap(), pair[1].region.unwrap());
            assert!(a[1] + a[3] <= b[1], "{a:?} over {b:?}");
        }
    }

    #[test]
    fn a_check_boxs_label_joins_its_region_and_the_box_stays_its_hit_cell() {
        // Two boxes on a row, a third below; each label just right of its box.
        let check = |i: u32, x: i32, y: i32| Control { rect: [x, y, 16, 16], kind: ControlKind::Button, id: (FORM as u64) << 32 | 0x4000_0000 | i as u64 };
        let controls = [check(0, 100, 200), check(1, 300, 200), check(2, 100, 240), row(3, 400, false)];
        // (Ink: x, baseline - 12, 7 a letter, 16 high.)
        let draws = vec![
            label("Show FPS", 122, 214),
            // Further on, apart from the label, before the next box: not it.
            label("Far", 250, 214),
            label("VSync", 322, 214),
            // A label in two runs.
            label("Limit", 122, 254),
            label("FPS", 162, 254),
            // On no box's row.
            label("Title", 100, 300),
        ];
        let frame = Frame { draws, ..Default::default() };
        let mut got = targets(&controls);
        label_regions(&frame, &controls, &mut got, bounds);
        assert_eq!(
            got.iter().map(|t| t.region).collect::<Vec<_>>(),
            [Some([100, 200, 78, 18]), Some([300, 200, 57, 18]), Some([100, 240, 83, 18]), None]
        );
        assert_eq!(got.iter().map(|t| (t.rect, t.anchor)).collect::<Vec<_>>(), controls.map(|c| (c.rect, None)), "the box is where it points and clicks");
        // A line on a row leads to nothing without text.
        let lone = Frame { draws: vec![ScreenDraw::Fill { rect: [120, 206, 60, 3], rgb: [0; 3], alpha: 128, clip: [0, 0, 1024, 768] }], ..Default::default() };
        let mut lined = targets(&controls);
        label_regions(&lone, &controls, &mut lined, bounds);
        assert!(lined.iter().all(|t| t.region.is_none()));
        // Without text: the box alone.
        let mut bare = targets(&controls);
        label_regions(&Frame::default(), &controls, &mut bare, bounds);
        assert!(bare.iter().all(|t| t.region.is_none()));
    }
}
