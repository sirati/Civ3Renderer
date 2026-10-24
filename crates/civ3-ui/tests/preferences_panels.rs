//! Civilization III's Preferences panels (`assets/civ3/preferences.toml`)
//! against the generic menu's layout rules: rulers, fits, cards. The
//! menu's own tests are game-agnostic; these check the game's description.

use menu::panel::*;
use menu::screen::Element;

#[test]
fn preference_rulers_follow_handle_centres_and_stay_inside_each_slider() {
    let panels = Panels::parse(include_str!("../../../assets/civ3/preferences.toml")).unwrap();
    let panel = panels.panel("preferences").unwrap();
    let mut state = PanelState::default();
    let area = [0, 0, 1024, 768];
    let mut count = 0;
    for (index, item) in panel.items.iter().enumerate() {
        let Kind::Slider { control, min, max, knob, ticks, .. } = &item.kind else { continue };
        count += 1;
        assert!(!ticks.is_empty(), "{control} needs a ruler");
        let (w, h) = item.size.unwrap();
        let (x, y) = item.at;
        let kw = panels.art_named(knob.as_ref().unwrap(), &state).unwrap().size.unwrap().0 as i32;
        for &(value, label) in ticks {
            let cx = x + slider_center(w as i32, kw, (value - min) / (max - min));
            assert!(cx >= x + kw / 2 && cx <= x + w as i32 - kw / 2, "{control}: {value}");
            state.values.insert(control.clone(), Value::Number(value));
            let draws = panels.draw_item(panel, &state, area, &estimate, index, item);
            let (top, bottom, len) = ruler_marks(y, h as i32, label);
            for mark in [top, bottom] {
                assert!(draws.iter().any(|d| matches!(d, Draw::Element(Element::Fill { rect, .. })
                    if *rect == [cx, mark, 1, len])), "{control}: tick at {value}");
            }
            assert!(draws.iter().any(|d| matches!(d, Draw::Element(Element::Image { at, region, .. })
                if region.source == "Art/SCROLL.PCX" && at.0 + kw / 2 == cx)), "{control}: handle at {value}");
            for draw in &draws {
                if let Draw::Text(t) = draw {
                    if t.style.height != 9 { continue; }
                    let m = estimate(&t.style, &t.text);
                    assert_eq!(t.clip, [x, y, x + w as i32, y + h as i32]);
                    assert!(t.x >= x && t.x + m.width <= x + w as i32, "{control}: {}", t.text);
                    assert!(t.y - m.ascent >= y && t.y + m.descent <= y + h as i32);
                    assert!(t.text.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '×' || c == '%'));
                }
            }
        }
    }
    assert_eq!(count, 18);
}

#[test]
fn zoom_preferences_are_separate_and_controls_fit_without_overlap() {
    let panels = Panels::parse(include_str!("../../../assets/civ3/preferences.toml")).unwrap();
    let panel = panels.panel("preferences").unwrap();
    let modes = ["Off", "Floating131", "Floating171", "Mouse111", "Mouse131", "Beside", "TopStrip"];
    for content in ["World", "WorldAndMenus"] {
        for split in ["Standard", "Vertical1To1"] {
            for tab in ["layout", "zoom"] {
                for mode in modes {
                    let mut state = PanelState::default();
                    state.values.insert("tab".into(), Value::Text(tab.into()));
                    state.values.insert("layout.zoom".into(), Value::Text(mode.into()));
                    state.values.insert("layout.zoom_content".into(), Value::Text(content.into()));
                    state.values.insert("layout.split".into(), Value::Text(split.into()));
                    state.values.insert("layout.zoom_controls".into(), Value::On(mode != "Off"));
                    state.values.insert("layout.zoom_lens_controls".into(), Value::On(!matches!(mode, "Off" | "TopStrip")));
                    let looks = panels.looks(panel, &state, [0, 0, 1024, 768], &estimate);
                    for (i, look) in looks.iter().enumerate() {
                        let [x, y, w, h] = look.rect;
                        assert!(x >= 49 && x+w <= 973 && y >= 40 && y+h <= 690, "{tab} {mode}: {:?}", look.rect);
                        for other in &looks[..i] {
                            let [a, b, c, d] = other.rect;
                            assert!(x+w <= a || a+c <= x || y+h <= b || b+d <= y, "{tab} {mode}: {:?} overlaps {:?}", look.rect, other.rect);
                        }
                    }
                    // The pictures: every placement always a target (Alt
                    // reaches them); what it shows unless the zoom is
                    // off; the place in a split only where it counts.
                    let targets = |control: &str| looks.iter().filter(|l| {
                        panel.items.iter().enumerate().any(|(n, i)| i.control() == Some(control) && id_hash(&panel.name, &n.to_string()) == l.id)
                    }).count();
                    if tab == "zoom" {
                        assert_eq!(targets("layout.zoom"), modes.len());
                        assert_eq!(targets("layout.zoom_content"), if mode == "Off" { 0 } else { 2 }, "{mode}");
                        let placed = split != "Standard" && !matches!(mode, "Off" | "Beside");
                        assert_eq!(targets("layout.zoom_place"), if placed { 3 } else { 0 }, "{split} {mode}");
                        assert!(looks.iter().all(|l| l.kind == scene::InteractableKind::Button || matches!(l.kind, scene::InteractableKind::Slider { .. })));
                    }
                    for item in &panel.items {
                        if item.control().is_some_and(|k| k.starts_with("layout.zoom")) {
                            let cond = item.show_if.as_ref().unwrap();
                            let mut layout = state.clone();
                            layout.values.insert("tab".into(), Value::Text("layout".into()));
                            assert!(!cond.holds(&layout.values), "zoom control on Layout");
                        }
                    }
                }
            }
        }
    }
    // Every placement, region and content picture is generated art of
    // one size, with a tooltip naming it.
    for item in panel.items.iter().filter(|i| matches!(i.control(), Some("layout.zoom" | "layout.zoom_place" | "layout.zoom_content"))) {
        let icon = item.icon.as_ref().unwrap();
        assert_eq!(panels.art.get(icon).and_then(|a| a.size), Some((84, 52)), "{icon}");
        assert!(panels.generated.iter().any(|(name, source, _)| name == icon && source.starts_with("rects:84x52;")), "{icon}");
        assert!(item.tooltip.as_ref().is_some_and(|t| !t.is_empty()), "{icon}");
        assert_eq!(item.size, Some((104, 64)), "{icon}");
    }
    let note = "Rendering menus doesn't render in a higher resolution. It just shows the pixels.";
    assert!(panel.items.iter().any(|i| i.text.as_deref() == Some(note) && i.show_if.as_ref().is_some_and(|c| c.holds(&[("tab".to_string(), Value::Text("zoom".into()))].into()))));
    let item = panel.items.iter().find(|i| i.control() == Some("layout.zoom_magnification_step")).unwrap();
    let Kind::Slider { tick_labels, .. } = &item.kind else { panic!("magnification slider") };
    assert_eq!(tick_labels.as_ref().unwrap(), &["1.5×", "2×", "3×", "4×"]);
}

#[test]
fn preferences_undo_controls_fit_beside_values_and_tooltips_stay_in_the_panel() {
    let panels = Panels::parse(include_str!("../../../assets/civ3/preferences.toml")).unwrap();
    let panel = panels.panel("preferences").unwrap();
    let area = [200, 100, 1024, 768];
    let mut state = PanelState::default();
    for item in &panel.items {
        if let Kind::Slider { control, max, .. } = &item.kind {
            state.values.insert(control.clone(), Value::Number(*max));
            state.values.insert(format!("dialog.changed.{control}"), Value::On(true));
        }
    }
    for key in ["resolution.scale_menus", "display.limit_fps", "layout.zoom_controls", "layout.zoom_lens_controls"] {
        state.values.insert(key.into(), Value::On(true));
    }
    state.values.insert("layout.zoom".into(), Value::Text("Floating131".into()));
    state.values.insert("layout.zoom_magnification_label".into(), Value::Text("4×".into()));
    state.values.insert("layout.split".into(), Value::Text("Vertical1To1".into()));
    let mut checked = 0;
    let overlap = |a: [i32; 4], b: [i32; 4]| a[0] < b[0]+b[2] && b[0] < a[0]+a[2] && a[1] < b[1]+b[3] && b[1] < a[1]+a[3];
    for tab in ["resolution", "layout", "zoom", "accessibility"] {
        state.values.insert("tab".into(), Value::Text(tab.into()));
        let looks = panels.looks(panel, &state, area, &estimate);
        for (i, look) in looks.iter().enumerate() {
            assert!(looks[..i].iter().all(|other| !overlap(look.rect, other.rect)), "{tab}: {:?} overlaps a control", look.rect);
        }
        let undos: Vec<_> = panel.items.iter().enumerate().filter(|(_, i)| i.control() == Some("dialog.undo") && Panels::visible(i, &state)).collect();
        for (index, undo) in undos {
            checked += 1;
            let Kind::Choice { value: control, .. } = &undo.kind else { unreachable!() };
            let rect = panels.rect(undo, &state, panels.origin(panel, area)).unwrap();
            let (n, slider) = panel.items.iter().enumerate().find(|(_, i)| matches!(&i.kind, Kind::Slider { control: c, .. } if c == control)).unwrap();
            for draw in panels.draw_item(panel, &state, area, &estimate, n, slider) {
                if let Draw::Text(t) = draw {
                    let m = estimate(&t.style, &t.text);
                    assert!(!overlap(rect, [t.x, t.y-m.ascent, m.width, m.ascent+m.descent]), "{control}: undo overlaps {}", t.text);
                }
            }
            state.hover = Some(index);
            let draws = panels.draw(panel, &state, area, &estimate);
            let look = panels.looks(panel, &state, area, &estimate).into_iter().find(|l| l.id == id_hash(&panel.name, &index.to_string())).unwrap();
            assert!(draws.windows(look.draws.len()).any(|w| w == look.draws), "{control}: hovered control and tooltip must form one replacement range");
            let Draw::Text(t) = draws.last().unwrap() else { panic!("tooltip must paint above all items") };
            assert_eq!(t.text, "Undo to current state");
            let m = estimate(&t.style, &t.text);
            assert!(t.x >= area[0] && t.x + m.width <= area[0]+area[2]);
            assert!(t.y-m.ascent >= area[1] && t.y+m.descent <= area[1]+area[3]);
        }
        state.hover = None;
    }
    assert_eq!(checked, 16);
}

#[test]
fn accessibility_controls_are_visible_hit_testable_and_fit() {
    let panels = Panels::parse(include_str!("../../../assets/civ3/preferences.toml")).unwrap();
    let p = panels.panel("preferences").unwrap();
    let mut state = PanelState::default();
    state.values.insert("tab".into(), Value::Text("accessibility".into()));
    state.values.insert("accessibility.unit_rings".into(), Value::On(true));
    state.values.insert("accessibility.active_unit_arrow".into(), Value::On(true));
    state.values.insert("accessibility.stack_count".into(), Value::On(true));
    for item in p.items.iter().filter(|i| i.control().is_some_and(|k| k.starts_with("accessibility."))) {
        assert!(Panels::visible(item, &state));
        assert!(Panels::enabled(item, &state));
        let [x, y, w, h] = panels.rect(item, &state, (0, 0)).unwrap();
        assert!(x >= 70 && y >= 95 && x+w <= 952 && y+h <= 655);
        let hit = panels.hit(p, &state, [0, 0, 1024, 768], x+w/2, y+h/2);
        assert_eq!(hit, Some(p.items.iter().position(|i| std::ptr::eq(i, item)).unwrap()));
    }
    let looks = panels.looks(p, &state, [0, 0, 1024, 768], &estimate);
    for (i, a) in looks.iter().enumerate() {
        let [x, y, w, h] = a.rect;
        for b in &looks[..i] {
            let [l, t, bw, bh] = b.rect;
            assert!(x+w <= l || l+bw <= x || y+h <= t || t+bh <= y, "{:?} overlaps {:?}", a.rect, b.rect);
        }
    }
    let bob = p.items.iter().find(|i| i.control() == Some("accessibility.arrow_bobbing")).unwrap();
    state.values.insert("accessibility.active_unit_arrow".into(), Value::On(false));
    assert!(!Panels::visible(bob, &state));
    // The stack count's backdrop: shown, greyed out without the count.
    let backdrop = p.items.iter().find(|i| i.control() == Some("accessibility.stack_backdrop")).unwrap();
    state.values.insert("accessibility.stack_count".into(), Value::On(false));
    assert!(Panels::visible(backdrop, &state) && !Panels::enabled(backdrop, &state));
    for tab in ["civ3", "resolution", "layout", "zoom", "clean_map"] {
        state.values.insert("tab".into(), Value::Text(tab.into()));
        assert!(p.items.iter().filter(|i| i.control().is_some_and(|k| k.starts_with("accessibility."))).all(|i| !Panels::visible(i, &state)));
    }
}

#[test]
fn base_ui_slider_centres_line_up_with_pixel_scale_buttons() {
    let panels = Panels::parse(include_str!("../../../assets/civ3/preferences.toml")).unwrap();
    let panel = panels.panel("preferences").unwrap();
    let item = panel.items.iter().find(|i| i.control() == Some("resolution.base_ui_scale")).unwrap();
    let Kind::Slider { min, max, knob, .. } = &item.kind else { panic!("base scale slider") };
    let kw = panels.art_named(knob.as_ref().unwrap(), &PanelState::default()).unwrap().size.unwrap().0 as i32;
    let centre = |v| item.at.0 + slider_center(item.size.unwrap().0 as i32, kw, (v - min) / (max - min));
    for button in &panel.items {
        let Kind::Choice { control, value } = &button.kind else { continue };
        if control != "resolution.pixel_scale" { continue; }
        let k = value.parse::<f32>().unwrap();
        assert_eq!(centre(k), button.at.0 + button.size.unwrap().0 as i32 / 2);
        if k < *max { assert_eq!(centre(k + 0.5), (centre(k) + centre(k + 1.0)) / 2); }
    }
}

/// Every renderer tab of the Preferences, with its conditional
/// controls, undo buttons and either Base UI scale button shown: each
/// item (a control with its label and value, or a text) lies inside
/// the tab's parchment and overlaps no other item. On Accessibility
/// every item sits in a card with a heading, each control in the card of
/// the feature it belongs to, its undo button beside it.
#[test]
fn preferences_tabs_fit_without_overlaps_and_accessibility_controls_sit_in_their_cards() {
    let panels = Panels::parse(include_str!("../../../assets/civ3/preferences.toml")).unwrap();
    let panel = panels.panel("preferences").unwrap();
    let area = [0, 0, 1024, 768];
    let origin = panels.origin(panel, area);
    let mut state = PanelState::default();
    for item in &panel.items {
        if let Kind::Slider { control, max, .. } = &item.kind {
            state.values.insert(control.clone(), Value::Number(*max));
            state.values.insert(format!("dialog.changed.{control}"), Value::On(true));
        }
    }
    for key in ["resolution.scale_menus", "display.limit_fps", "layout.zoom_controls", "layout.zoom_lens_controls",
        "accessibility.unit_rings", "accessibility.active_unit_arrow", "layout.menu_follows_pointer"] {
        state.values.insert(key.into(), Value::On(true));
    }
    for (key, value) in [("layout.zoom", "Floating131"), ("layout.zoom_magnification_label", "4×"), ("layout.split", "Vertical1To1"),
        ("layout.orientation", "vertical"), ("layout.screen_place", "Floating"), ("accessibility.hud_percent", "300"), ("accessibility.arrow_percent", "300")] {
        state.values.insert(key.into(), Value::Text(value.into()));
    }
    let overlap = |a: [i32; 4], b: [i32; 4]| a[0] < b[0]+b[2] && b[0] < a[0]+a[2] && a[1] < b[1]+b[3] && b[1] < a[1]+a[3];
    let inside = |a: [i32; 4], b: [i32; 4]| a[0] >= b[0] && a[1] >= b[1] && a[0]+a[2] <= b[0]+b[2] && a[1]+a[3] <= b[1]+b[3];
    let union = |a: [i32; 4], b: [i32; 4]| {
        let (l, t) = (a[0].min(b[0]), a[1].min(b[1]));
        [l, t, (a[0]+a[2]).max(b[0]+b[2]) - l, (a[1]+a[3]).max(b[1]+b[3]) - t]
    };
    let background = |item: &Item| matches!(&item.kind, Kind::Image(art) if art == "cover" || art == "section");
    for previewing in [false, true] {
        state.values.insert("dialog.previewing_base".into(), Value::On(previewing));
        for tab in ["resolution", "layout", "zoom", "clean_map", "accessibility"] {
            state.values.insert("tab".into(), Value::Text(tab.into()));
            let visible: Vec<(usize, &Item)> = panel.items.iter().enumerate().filter(|(_, i)| Panels::visible(i, &state)).collect();
            let rect_of = |n: usize, item: &Item| {
                let mut rect = panels.rect(item, &state, origin);
                for draw in panels.draw_item(panel, &state, area, &estimate, n, item) {
                    if let Draw::Text(t) = draw {
                        let m = estimate(&t.style, &t.text);
                        let text = [t.x, t.y - m.ascent, m.width, m.ascent + m.descent];
                        rect = Some(rect.map_or(text, |r| union(r, text)));
                    }
                }
                rect.unwrap()
            };
            let parchment = visible.iter().find(|(_, i)| matches!(&i.kind, Kind::Image(a) if a == "cover")).map(|&(n, i)| rect_of(n, i)).unwrap();
            let cards: Vec<[i32; 4]> = visible.iter().filter(|(_, i)| matches!(&i.kind, Kind::Image(a) if a == "section")).map(|&(n, i)| rect_of(n, i)).collect();
            // (The tab buttons lie above the parchment.)
            let items: Vec<(&Item, [i32; 4])> = visible.iter().filter(|(_, i)| !background(i) && !matches!(&i.kind, Kind::Choice { control, .. } if control == "tab"))
                .map(|&(n, i)| (i, rect_of(n, i))).collect();
            let name = |item: &Item| item.control().map(str::to_owned).or(item.text.clone()).unwrap_or_default();
            for (i, (item, rect)) in items.iter().enumerate() {
                assert!(inside(*rect, parchment), "{tab}: {} at {rect:?} leaves the panel {parchment:?}", name(item));
                for (other, other_rect) in &items[..i] {
                    assert!(!overlap(*rect, *other_rect), "{tab}: {} at {rect:?} overlaps {} at {other_rect:?}", name(item), name(other));
                }
            }
            if tab != "accessibility" { continue; }
            let card_of = |rect: [i32; 4]| {
                let holding: Vec<_> = cards.iter().copied().filter(|card| inside(rect, *card)).collect();
                assert_eq!(holding.len(), 1, "accessibility: {rect:?} in exactly one card");
                holding[0]
            };
            let heading = |card: [i32; 4]| items.iter().find(|(i, r)| i.style.as_deref() == Some("heading") && inside(*r, card)).and_then(|(i, _)| i.text.clone());
            for (item, rect) in &items {
                assert!(heading(card_of(*rect)).is_some(), "accessibility: {} in a card without a heading", name(item));
            }
            for card in &cards {
                assert!(heading(*card).is_some(), "accessibility: card {card:?} without a heading");
            }
            let card_named = |control: &str| {
                let (_, rect) = items.iter().find(|(i, _)| i.control() == Some(control)).unwrap();
                heading(card_of(*rect)).unwrap()
            };
            for (control, card) in [("accessibility.active_unit_arrow", "Active unit"), ("accessibility.arrow_bobbing", "Active unit"),
                ("accessibility.arrow_size", "Active unit"),
                ("accessibility.hud_size", "Unit markers"), ("accessibility.stack_count", "Unit markers"),
                ("accessibility.stack_backdrop", "Unit markers"),
                ("accessibility.unit_rings", "Diplomacy rings"), ("accessibility.war.color", "Diplomacy rings"),
                ("accessibility.peace.invert", "Diplomacy rings"), ("accessibility.ring_width", "Diplomacy rings"),
                ("input.alt_in_menus", "Mouse (Alt mode)"), ("input.snap_updates_per_second", "Mouse (Alt mode)"),
                ("input.record_alt_mode", "Mouse (Alt mode)")] {
                assert_eq!(card_named(control), card, "{control}");
            }
            // Each undo button in its slider's card, on its row.
            for (undo, rect) in items.iter().filter(|(i, _)| i.control() == Some("dialog.undo")) {
                let Kind::Choice { value: control, .. } = &undo.kind else { unreachable!() };
                let slider = items.iter().find(|(i, _)| i.control() == Some(control.as_str())).unwrap();
                assert_eq!(card_of(*rect), card_of(slider.1), "{control}: undo in its card");
                assert_eq!(rect[1], origin.1 + slider.0.at.1, "{control}: undo on the slider's row");
            }
        }
    }
    // The Alt mode settings left the Layout tab for Accessibility.
    state.values.insert("tab".into(), Value::Text("layout".into()));
    assert!(!panel.items.iter().any(|i| Panels::visible(i, &state) && matches!(i.control(), Some("input.alt_in_menus" | "input.snap_updates_per_second" | "input.record_alt_mode"))));
}

