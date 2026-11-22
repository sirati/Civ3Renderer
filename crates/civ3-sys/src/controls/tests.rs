use super::*;
use std::collections::HashMap;

/// Game memory as bytes at addresses.
#[derive(Default)]
pub(super) struct Fake(HashMap<u32, u8>);

impl Fake {
    pub(super) fn put(&mut self, at: u32, v: u32) {
        for (i, b) in v.to_le_bytes().into_iter().enumerate() {
            self.0.insert(at + i as u32, b);
        }
    }
    pub(super) fn bytes(&mut self, at: u32, bytes: &[u8]) {
        for (i, b) in bytes.iter().enumerate() {
            self.0.insert(at + i as u32, *b);
        }
    }
    pub(super) fn rect(&mut self, at: u32, r: [i32; 4]) {
        for (i, v) in r.into_iter().enumerate() {
            self.put(at + 4 * i as u32, v as u32);
        }
    }

    /// A form at `addr`: its vtable, status, place (`left, top, right,
    /// bottom`, in its parent's pixels if `parent`), children.
    pub(super) fn form(&mut self, addr: u32, vtable: u32, status1: u32, shown: bool, rect: [i32; 4], parent: u32, children: &[u32]) {
        self.put(addr, vtable);
        self.put(addr + STATUS1, status1 | if parent != 0 { RELATIVE } else { 0 });
        self.put(addr + STATUS2, shown as u32);
        self.put(addr + PARENT, parent);
        self.rect(addr + LEFT, rect);
        self.rect(addr + CLIENT, [0, 0, 0, 0]);
        self.rect(addr + CLIP, [0, 0, 1024, 768]);
        let array = addr + 0x4000;
        self.put(addr + CHILDREN, if children.is_empty() { 0 } else { array });
        self.put(addr + CHILD_COUNT, children.len() as u32);
        for (i, c) in children.iter().enumerate() {
            self.put(array + 4 * i as u32, *c);
        }
        self.put(addr + LINK_COUNT, 0);
    }
}

impl Mem for Fake {
    fn u32(&self, addr: u32) -> Option<u32> {
        let b: Vec<u8> = (0..4).map(|i| self.0.get(&(addr + i)).copied()).collect::<Option<_>>()?;
        Some(u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn u8(&self, addr: u32) -> Option<u8> {
        self.0.get(&addr).copied()
    }
}

pub(super) const BUTTON: u32 = 0x66_56C0;
pub(super) const PANEL: u32 = 0x67_1218;

pub(super) fn sites() -> Sites {
    Sites { roots: 0x100, root_count: 0x200, modal: 0x300, companion: 0x304, main_gui: 0x9F_DB10, advisor: 0x9A_FD58 }
}

pub(super) fn with_roots(m: &mut Fake, roots: &[u32]) {
    m.put(0x200, roots.len() as u32);
    for (i, r) in roots.iter().enumerate() {
        m.put(0x100 + 4 * i as u32, *r);
    }
    m.put(0x300, 0);
    m.put(0x304, 0);
}

#[test]
fn buttons_are_found_down_the_tree_where_they_show() {
    let mut m = Fake::default();
    // A panel at (100, 50) with a button at (10, 20) in it, a hidden
    // button, and a hidden group whose button is hidden with it.
    m.form(0x10_0000, PANEL, KEYED, true, [100, 50, 400, 250], 0, &[0x20_0000, 0x30_0000, 0x40_0000]);
    m.form(0x20_0000, BUTTON, 0, true, [10, 20, 70, 44], 0x10_0000, &[]);
    m.form(0x30_0000, BUTTON, 0, false, [80, 20, 140, 44], 0x10_0000, &[]);
    m.form(0x40_0000, PANEL, 0, false, [0, 100, 200, 200], 0x10_0000, &[0x50_0000]);
    m.form(0x50_0000, BUTTON, 0, true, [0, 0, 60, 24], 0x40_0000, &[]);
    with_roots(&mut m, &[0x10_0000]);
    let c = controls(&m, &sites());
    assert_eq!(c, [Control { rect: [110, 70, 60, 24], kind: ControlKind::Button, id: 0x20_0000 }]);
    // Clipped to where it shows.
    m.rect(0x20_0000 + CLIP, [0, 0, 150, 80]);
    assert_eq!(controls(&m, &sites())[0].rect, [110, 70, 40, 10]);
    // A button that takes no mouse is none.
    m.rect(0x20_0000 + CLIP, [0, 0, 1024, 768]);
    m.put(0x20_0000 + STATUS1, RELATIVE | NO_MOUSE);
    assert!(controls(&m, &sites()).is_empty());
}

#[test]
fn a_modal_form_leaves_only_its_own_tree_and_its_companion() {
    let mut m = Fake::default();
    m.form(0x10_0000, BUTTON, 0, true, [0, 0, 50, 20], 0, &[]);
    m.form(0x20_0000, PANEL, KEYED, true, [200, 200, 600, 500], 0, &[0x30_0000]);
    m.form(0x30_0000, BUTTON, 0, true, [10, 10, 60, 30], 0x20_0000, &[]);
    m.form(0x40_0000, BUTTON, 0, true, [700, 0, 750, 20], 0, &[]);
    with_roots(&mut m, &[0x10_0000, 0x20_0000, 0x40_0000]);
    assert_eq!(controls(&m, &sites()).len(), 3);
    m.put(0x300, 0x20_0000);
    let ids: Vec<u64> = controls(&m, &sites()).iter().map(|c| c.id).collect();
    assert_eq!(ids, [0x30_0000]);
    m.put(0x304, 0x40_0000);
    let ids: Vec<u64> = controls(&m, &sites()).iter().map(|c| c.id).collect();
    assert_eq!(ids, [0x30_0000, 0x40_0000]);
}

#[test]
fn an_opaque_form_above_hides_what_it_covers() {
    let mut m = Fake::default();
    // A dialog (root 0, topmost) over a button of a lower root.
    m.form(0x10_0000, PANEL, 0, true, [0, 0, 300, 300], 0, &[]);
    m.form(0x20_0000, PANEL, KEYED, true, [0, 0, 1024, 768], 0, &[0x30_0000, 0x40_0000]);
    m.form(0x30_0000, BUTTON, 0, true, [10, 10, 60, 30], 0x20_0000, &[]);
    m.form(0x40_0000, BUTTON, 0, true, [400, 10, 460, 30], 0x20_0000, &[]);
    with_roots(&mut m, &[0x10_0000, 0x20_0000]);
    let ids: Vec<u64> = controls(&m, &sites()).iter().map(|c| c.id).collect();
    assert_eq!(ids, [0x40_0000]);
    // A colour-keyed one lets the mouse through.
    m.put(0x10_0000 + STATUS1, KEYED);
    assert_eq!(controls(&m, &sites()).len(), 2);
}

#[test]
fn the_interface_panel_s_buttons_are_known_by_their_place() {
    let mut m = Fake::default();
    let gui = 0x9F_DB10;
    let command = gui + 0x1_6CE4 + 3 * 0x6DC;
    m.form(gui, 0x66_B570, KEYED, true, [0, 600, 1024, 768], 0, &[command]);
    // (A class of its own.)
    m.form(command, 0x66_0000, 0, true, [100, 10, 132, 42], gui, &[]);
    with_roots(&mut m, &[gui]);
    assert_eq!(controls(&m, &sites()), [Control { rect: [100, 610, 32, 32], kind: ControlKind::Button, id: command as u64 }]);
}

/// A list box's form part at `f` (G = f − 0x64, its item list at
/// f + 0x578, as the generic list box), its draw thunk at `thunk`.
pub(super) fn list(m: &mut Fake, f: u32, status1: u32, layout: ItemLayout, count: i32, first: i32, shown: i32) {
    let vtable = 0x70_0000;
    let thunk = 0x71_0000;
    m.form(f, vtable, status1, true, [100, 100, 300, 260], 0, &[]);
    m.put(vtable + 0x58, thunk);
    let rel = LIST_DRAW.wrapping_sub(thunk + 8);
    m.bytes(thunk, &[0x2B, 0x49, 0xFC, 0xE9]);
    m.put(thunk + 4, rel);
    // (Trailing bytes the thunk read covers.)
    m.bytes(thunk + 8, &[0xCC; 3]);
    let g = f - 0x64;
    m.put(f - 4, 0);
    let vb = 0x72_0000;
    m.put(g + 4, vb);
    m.put(vb + 4, 0x60);
    m.put(vb + 8, 0x5D8);
    let l = g + 4 + 0x5D8;
    m.put(l + L_X0, layout.x0 as u32);
    m.put(l + L_Y0, layout.y0 as u32);
    m.put(l + L_GAP, layout.gap as u32);
    m.put(l + L_LAYOUT, if layout.variable { 1 } else { 0 });
    m.put(l + L_ROWS, layout.rows as u32);
    m.put(l + L_WIDTH, layout.width as u32);
    m.put(l + L_HEIGHT, layout.height as u32);
    m.put(l + L_COUNT, count as u32);
    m.put(g + G_FIRST, first as u32);
    m.put(g + G_SHOWN, shown as u32);
    with_roots(m, &[f]);
}

pub(super) const ROWS: ItemLayout = ItemLayout { x0: 4, y0: 2, gap: 1, variable: false, rows: 8, width: 200, height: 19 };

#[test]
fn a_list_box_shows_its_entries_and_scrolls_with_the_wheel() {
    let mut m = Fake::default();
    let f = 0x10_0000;
    list(&mut m, f, WHEEL, ROWS, 30, 5, 8);
    let c = controls(&m, &sites());
    let items: Vec<&Control> = c.iter().filter(|c| c.kind == ControlKind::ListItem).collect();
    assert_eq!(items.len(), 8);
    // Entry 5 first, at (100 + 4, 100 + 2), 192 wide; the next 20 below.
    assert_eq!(items[0].rect, [104, 102, 192, 19]);
    assert_eq!(items[1].rect, [104, 122, 192, 19]);
    assert_eq!(items[0].id, (f as u64) << 32 | 5);
    assert_eq!(items[7].id, (f as u64) << 32 | 12);
    // The list: the wheel moves it an entry (20 px).
    let lists: Vec<&Control> = c.iter().filter(|c| matches!(c.kind, ControlKind::List { .. })).collect();
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].kind, ControlKind::List { scroll: [104, 100, 192, 160], step: 20 });
    assert_eq!((lists[0].rect, lists[0].id), ([100, 100, 200, 160], f as u64));
    // All shown, or no wheel: its entries only.
    list(&mut m, f, WHEEL, ROWS, 8, 0, 8);
    assert!(controls(&m, &sites()).iter().all(|c| c.kind == ControlKind::ListItem));
    list(&mut m, f, 0, ROWS, 30, 0, 8);
    assert!(controls(&m, &sites()).iter().all(|c| c.kind == ControlKind::ListItem));
}

#[test]
fn a_form_whose_draw_thunk_differs_is_no_list() {
    let mut m = Fake::default();
    list(&mut m, 0x10_0000, WHEEL, ROWS, 30, 5, 8);
    // Its virtual base table disagrees: not taken for one.
    m.put(0x72_0000 + 4, 0x64);
    assert!(controls(&m, &sites()).is_empty());
    list(&mut m, 0x10_0000, WHEEL, ROWS, 30, 5, 8);
    m.bytes(0x71_0000, &[0x90]);
    assert!(controls(&m, &sites()).is_empty());
}

#[test]
fn list_entries_of_several_lines_stack() {
    let mut m = Fake::default();
    let f = 0x10_0000;
    list(&mut m, f, 0, ItemLayout { variable: true, ..ROWS }, 3, 1, 2);
    // Nodes: 3 items of 1, 2, 3 lines.
    let l = f + 0x578;
    let nodes = [0x80_0000, 0x80_0100, 0x80_0200];
    m.put(l + L_FIRST_NODE, nodes[0]);
    for (i, n) in nodes.iter().enumerate() {
        m.put(n + 8, n + 0x80);
        m.put(n + 0x80 + 0xC, i as u32 + 1);
        m.put(n + 0xC, nodes.get(i + 1).copied().unwrap_or(0));
    }
    let c = controls(&m, &sites());
    // Item 1 (2 lines) at the top, then item 2 (3 lines).
    assert_eq!(c[0].rect, [104, 102, 192, 39]);
    assert_eq!(c[1].rect, [104, 142, 192, 59]);
}

#[test]
fn grid_items_go_down_then_across() {
    let l = ItemLayout { x0: 0, y0: 0, gap: 2, variable: false, rows: 3, width: 50, height: 10 };
    assert_eq!(l.item(0, 0, 1), [0, 0, 50, 10]);
    assert_eq!(l.item(2, 0, 1), [0, 24, 50, 10]);
    assert_eq!(l.item(3, 0, 1), [52, 0, 50, 10]);
}

#[test]
fn context_menu_rows_skip_hidden_entries_and_separators() {
    // Shown, hidden, shown separator, shown.
    let rows = menu_rows(&[1, 0, 1 | 4, 1], 18, (120, 60), 2);
    assert_eq!(rows, [(0, [0, 2, 120, 18]), (3, [0, 38, 120, 18])]);
    // Without a height: the menu's shared among its rows.
    assert_eq!(menu_rows(&[1, 1], 0, (100, 44), 2), [(0, [0, 2, 100, 20]), (1, [0, 22, 100, 20])]);
    assert!(menu_rows(&[0, 0], 18, (100, 44), 2).is_empty());
}

#[test]
fn context_menu_hit_rectangles_follow_the_font_and_current_border() {
    let mut m = Fake::default();
    let (form, font) = (0x20_0000, 0x30_0000);
    m.form(form, CONTEXT_MENU_VTABLES[0], 0, true, [100, 200, 344, 287], 0, &[]);
    with_roots(&mut m, &[form]);
    m.put(form + MENU_COUNT, 5);
    m.put(form + MENU_ITEM_HEIGHT, 15);
    m.put(MENU_BORDER, 1);
    m.put(form + crate::layout::BASE_FORM_DATA_CANVAS + 4 + 0x5C, font);
    m.put(font + 8, 12);
    m.put(font + 0x10, 5);
    for (i, status) in [1, 3, 5, 1, 1].into_iter().enumerate() {
        m.put(form + MENU_ITEMS + i as u32 * MENU_ITEM_BYTES + MENU_ITEM_STATUS, status);
    }
    let rects = || controls(&m, &sites()).iter().map(|c| c.rect).collect::<Vec<_>>();
    // The original 15-pixel rows with a fixed two-pixel border missed the
    // first row's fill and the bottom of later rows, so no hover appeared.
    assert_eq!(rects(), [[100, 201, 244, 17], [100, 218, 244, 17], [100, 252, 244, 17], [100, 269, 244, 17]]);
    // A default font and negative-height metrics take the other branch.
    m.put(form + crate::layout::BASE_FORM_DATA_CANVAS + 4 + 0x5C, 0);
    m.put(DEFAULT_FONT, font);
    m.put(font + 8, (-12i32) as u32);
    m.put(font + 0xC, 19);
    assert_eq!(controls(&m, &sites())[0].rect, [100, 201, 244, 19]);
}

#[test]
fn context_menus_check_boxes_tabs_and_links() {
    let mut m = Fake::default();
    // A context menu at (300, 200) with two entries 18 high.
    let menu = 0x10_0000;
    m.form(menu, 0x67_1AC0, 0, true, [300, 200, 420, 242], 0, &[]);
    m.put(menu + MENU_COUNT, 2);
    m.put(menu + MENU_ITEM_HEIGHT, 18);
    m.put(menu + MENU_ITEMS + MENU_ITEM_STATUS, 1);
    m.put(menu + MENU_ITEMS + MENU_ITEM_BYTES + MENU_ITEM_STATUS, 1);
    // Check boxes: 4 in 2 rows of 2, 100×20, gap 4, at (0, 400).
    let check = 0x20_0000;
    m.form(check, CHECKBOX_VTABLE, 0, true, [0, 400, 300, 450], 0, &[]);
    let l = check + 0x578;
    for (field, v) in [(L_X0, 0), (L_Y0, 0), (L_GAP, 4), (L_LAYOUT, 0), (L_ROWS, 2), (L_WIDTH, 100), (L_HEIGHT, 20), (L_COUNT, 4)] {
        m.put(l + field, v);
    }
    // The advisor with a text link at (500, 300)-(560, 312).
    let advisor = 0x9A_FD58;
    m.form(advisor, 0x66_88B8, 0, true, [0, 0, 1024, 768], 0, &[]);
    m.put(advisor + LINK_COUNT, 1);
    m.put(advisor + LINKS, 0x30_0000);
    m.rect(0x30_0000 + 4, [500, 300, 560, 312]);
    with_roots(&mut m, &[menu, check, advisor]);
    let c = controls(&m, &sites());
    let rects: Vec<[i32; 4]> = c.iter().map(|c| c.rect).collect();
    assert!(c.iter().all(|c| c.kind == ControlKind::Button));
    assert_eq!(rects[..2], [[300, 202, 120, 18], [300, 220, 120, 18]]);
    assert_eq!(rects[2..6], [[0, 400, 100, 20], [0, 424, 100, 20], [104, 400, 100, 20], [104, 424, 100, 20]]);
    // The link, then the six tabs.
    assert_eq!(rects[6], [500, 300, 60, 12]);
    assert_eq!(tab_rects()[0], [4, 242, 65, 62]);
    assert_eq!(rects[7..], tab_rects());
    // Ids tell the parts apart.
    let mut ids: Vec<u64> = c.iter().map(|c| c.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), c.len());
    // Check boxes that do not read sanely: the group as one.
    m.put(l + L_WIDTH, 0);
    let c = controls(&m, &sites());
    assert!(c.iter().any(|c| c.id == check as u64 && c.rect == [0, 400, 300, 50]));
}

#[test]
fn the_form_tree_lists_every_form_visited() {
    let mut m = Fake::default();
    m.form(0x10_0000, PANEL, KEYED, true, [100, 50, 400, 250], 0, &[0x20_0000, 0x40_0000]);
    m.form(0x20_0000, BUTTON, 0, true, [10, 20, 70, 44], 0x10_0000, &[]);
    // Hidden: listed, its children not.
    m.form(0x40_0000, PANEL, 0, false, [0, 100, 200, 200], 0x10_0000, &[0x50_0000]);
    m.form(0x50_0000, BUTTON, 0, true, [0, 0, 60, 24], 0x40_0000, &[]);
    with_roots(&mut m, &[0x10_0000]);
    let tree = form_tree(&m, &sites());
    let got: Vec<(u32, u32, bool, [i32; 4], u32)> = tree.iter().map(|f| (f.depth, f.addr, f.visible, f.shown, f.children)).collect();
    assert_eq!(got, [(0, 0x10_0000, true, [100, 50, 300, 200], 2), (1, 0x20_0000, true, [110, 70, 60, 24], 0), (1, 0x40_0000, false, [100, 150, 200, 100], 1)]);
    assert!(tree.iter().all(|f| f.list.is_none()));
}

/// A typed control at `f` (class 0x666908): its draw thunk to a dispatcher
/// on the type at `[ecx-8]` (`kind`), type `of` to `add ecx,add; jmp draw`;
/// its object B `back` below that, form part F, `count` items of the
/// layout, item nodes of `lines` each.
#[allow(clippy::too_many_arguments)]
fn typed(m: &mut Fake, f: u32, size: (i32, i32), layout: ItemLayout, count: i32, kind: u32, of: u32, draw: u32, add: i32, back: u32, lines: &[i32]) {
    let (vtable, thunk, d, idx, table, entry): (u32, u32, u32, u32, u32, u32) = (0x70_0000, 0x71_0000, 0x71_1000, 0x71_2000, 0x71_3000, 0x71_4000);
    m.form(f, vtable, 0, true, [100, 100, 100 + size.0, 100 + size.1], 0, &[]);
    m.put(vtable + 0x58, thunk);
    m.bytes(thunk, &[0x2B, 0x49, 0xFC, 0xE9]);
    m.put(thunk + 4, d.wrapping_sub(thunk + 8));
    m.bytes(d, &[0x8B, 0x41, 0xF8, 0x48, 0x83, 0xF8, 0x0F, 0x77, 0x43, 0x33, 0xD2, 0x8A, 0x90]);
    m.put(d + 13, idx);
    m.bytes(d + 17, &[0xFF, 0x24, 0x95]);
    m.put(d + 20, table);
    // Types 1..16: `of` at entry 1, the rest at entry 0 (a `ret`).
    for t in 0..16u32 {
        m.bytes(idx + t, &[if t + 1 == of { 1 } else { 0 }]);
    }
    m.put(table, entry + 0x100);
    m.put(table + 4, entry);
    m.bytes(entry, &[0x81, 0xC1]);
    m.put(entry + 2, add as u32);
    m.bytes(entry + 6, &[0xE9]);
    m.put(entry + 7, draw.wrapping_sub(entry + 11));
    m.bytes(entry + 0x100, &[0xC3; 11]);
    // F − [F−4] = ecx; the type at ecx − 8.
    m.put(f - 4, 0x10);
    let ecx = f - 0x10;
    m.put(ecx - 8, kind);
    let b = ecx.wrapping_add(add as u32) - back;
    let vb = 0x72_0000;
    m.put(b, vb);
    m.put(vb + 4, f - b);
    m.put(vb + 8, 0x200);
    let l = b + 0x200;
    m.put(l + L_X0, layout.x0 as u32);
    m.put(l + L_Y0, layout.y0 as u32);
    m.put(l + L_LAYOUT, 0);
    m.put(l + L_GAP, layout.gap as u32);
    m.put(l + L_ROWS, layout.rows as u32);
    m.put(l + L_WIDTH, layout.width as u32);
    m.put(l + L_HEIGHT, layout.height as u32);
    m.put(l + L_COUNT, count as u32);
    // Item nodes: +8 data (its +8 the lines), +0xC the next.
    let mut node = 0;
    for (k, n) in lines.iter().enumerate().rev() {
        let (at, data) = (0x73_0000 + 0x100 * k as u32, 0x74_0000 + 0x100 * k as u32);
        m.put(at + 8, data);
        m.put(data + 8, *n as u32);
        m.put(at + 0xC, node);
        node = at;
    }
    m.put(l + L_FIRST_NODE, node);
    with_roots(m, &[f]);
}

#[test]
fn a_choice_grid_s_items_are_buttons() {
    let mut m = Fake::default();
    let f = 0x10_0000;
    let grid = ItemLayout { gap: 2, rows: 2, width: 300, height: 25, ..Default::default() };
    typed(&mut m, f, (314, 52), grid, 2, 8, 8, CHOICE_DRAW, -0x8C, 0xE0, &[]);
    let c = controls(&m, &sites());
    assert_eq!(c.iter().map(|c| (c.rect, c.kind)).collect::<Vec<_>>(), [([100, 101, 300, 25], ControlKind::Button), ([100, 126, 300, 25], ControlKind::Button)]);
    assert_eq!(c[1].id, (f as u64) << 32 | TAG_CHOICE as u64 | 1);
    // Of another type: none.
    let mut m = Fake::default();
    typed(&mut m, f, (314, 52), grid, 2, 4, 8, CHOICE_DRAW, -0x8C, 0xE0, &[]);
    assert!(controls(&m, &sites()).is_empty());
}

#[test]
fn a_popup_s_choices_share_its_height_by_their_lines() {
    // Type 16 (the quit popup's "No, not really." / "Yes, immediately!"):
    // 314×52, rows of one and of three lines.
    let mut m = Fake::default();
    let f = 0x10_0000;
    typed(&mut m, f, (314, 52), ItemLayout { rows: 1, width: 314, height: 13, ..Default::default() }, 2, 16, 16, LINES_DRAW, -0x184, 0x28, &[1, 3]);
    let c = controls(&m, &sites());
    assert_eq!(c.iter().map(|c| c.rect).collect::<Vec<_>>(), [[100, 100, 314, 13], [100, 113, 314, 39]]);
    assert!(c.iter().all(|c| c.kind == ControlKind::Button));
    // Two of one line: half each.
    typed(&mut m, f, (314, 52), ItemLayout { rows: 1, width: 314, height: 26, ..Default::default() }, 2, 16, 16, LINES_DRAW, -0x184, 0x28, &[1, 1]);
    assert_eq!(controls(&m, &sites()).iter().map(|c| c.rect).collect::<Vec<_>>(), [[100, 100, 314, 26], [100, 126, 314, 26]]);
}

#[test]
fn rich_text_links_keep_the_visible_fragment_and_hit_order() {
    let mut m = Fake::default();
    let form = 0x10_0000;
    m.form(form, PANEL, 0, true, [100, 50, 300, 150], 0, &[]);
    m.put(form + LINKS, 0x30_0000);
    m.put(form + LINK_COUNT, 5);
    for (k, r) in [
        [-30, 10, 250, 26], // A long link clipped on both sides.
        [15, 40, 65, 56],
        [30, 40, 90, 56], // Drawn later, so tested first by the game.
        [10, 120, 60, 136], // Entirely outside the visible form.
        [80, 70, 60, 86], // Invalid reversed rectangle.
    ].into_iter().enumerate() {
        m.rect(0x30_0000 + 32 * k as u32 + 4, r);
    }
    with_roots(&mut m, &[form]);
    let got = controls(&m, &sites());
    assert_eq!(got.iter().map(|c| c.rect).collect::<Vec<_>>(), [[130,90,60,16], [115,90,50,16], [100,60,200,16]]);
    assert!(got.iter().all(|c| c.content_row() && c.text_link()));
    assert_eq!(got.iter().map(|c| c.id).collect::<Vec<_>>(), [part_id(form,TAG_LINK,2), part_id(form,TAG_LINK,1), part_id(form,TAG_LINK,0)]);
    m.put(form + STATUS1, NO_MOUSE);
    assert!(controls(&m, &sites()).is_empty(), "a form which never receives clicks has no active text links");
    m.put(form + STATUS1, 0);
    m.put(form + STATUS2, 0);
    assert!(controls(&m, &sites()).is_empty(), "hidden rich text never becomes an Alt target");
}


/// The orb and unit picture are painted directly into Main_GUI rather
/// than constructed as Button objects.
#[test]
fn main_gui_status_actions_are_targets_only_while_the_native_actions_are_available() {
    let mut m = Fake::default();
    let g = sites().main_gui;
    let main = g - MAIN_GUI_IN_SCREEN;
    m.form(g, 0x66_B570, KEYED, true, [10, 5, 1034, 773], 0, &[]);
    m.rect(g + UNIT_STATUS_RECT, [700, 500, 1000, 700]);
    m.put(g + GUI_ENABLED - 1, 0x100);
    m.put(main + crate::layout::MAIN_SCREEN_FORM_CURRENT_UNIT, 0);
    m.put(main + TURN_END_FLAG, 0);
    with_roots(&mut m, &[g]);
    // A passive status panel or a waiting AI turn is not a button.
    assert!(controls(&m, &sites()).is_empty());
    m.put(main + TURN_END_FLAG, 1);
    assert_eq!(controls(&m, &sites()), [Control {
        rect: [710, 606, 48, 28], kind: ControlKind::Button,
        id: part_id(g, TAG_STATUS, STATUS_ORB),
    }]);
    m.put(main + crate::layout::MAIN_SCREEN_FORM_CURRENT_UNIT, 0x51_0000);
    let expected = [
        Control { rect: [710, 606, 48, 28], kind: ControlKind::Button, id: part_id(g, TAG_STATUS, STATUS_ORB) },
        Control { rect: [758, 606, 253, 100], kind: ControlKind::Button, id: part_id(g, TAG_STATUS, STATUS_UNIT_PICTURE) },
    ];
    assert_eq!(controls(&m, &sites()), expected);
    m.put(main + TURN_END_FLAG, 0);
    assert_eq!(controls(&m, &sites()), expected);
    // The centre of either region follows the corresponding native
    // branch: the orb's 48 by 29 hit box or the unit picture beside it.
    for (k, c) in expected.iter().enumerate() {
        let (x, y) = (c.rect[0] + c.rect[2] / 2 - 10, c.rect[1] + c.rect[3] / 2 - 5);
        assert!(y > 600 && y <= 700);
        assert_eq!((700..=747).contains(&x) && y <= 628, k == 0);
    }
    m.put(g + GUI_ENABLED - 1, 0);
    assert!(controls(&m, &sites()).is_empty());
    m.put(g + GUI_ENABLED - 1, 0x100);
    m.put(g + STATUS2, 0);
    assert!(controls(&m, &sites()).is_empty());
    m.put(g + STATUS2, 1);
    m.put(g + STATUS1, KEYED | NO_MOUSE);
    assert!(controls(&m, &sites()).is_empty());
    m.put(g + STATUS1, KEYED);
    // Modal roots also remove both HUD actions.
    m.form(0x70_0000, PANEL, KEYED, true, [0, 0, 100, 100], 0, &[]);
    m.put(sites().modal, 0x70_0000);
    assert!(controls(&m, &sites()).is_empty());
}

#[test]
fn main_gui_status_actions_use_the_runtime_rect_and_clip() {
    let mut m = Fake::default();
    let g = sites().main_gui;
    m.form(g, 0x66_B570, KEYED, true, [0, 0, 1024, 768], 0, &[]);
    m.rect(g + UNIT_STATUS_RECT, [800, 500, 1000, 700]);
    m.put(g + GUI_ENABLED - 1, 0x100);
    m.put(g - MAIN_GUI_IN_SCREEN + crate::layout::MAIN_SCREEN_FORM_CURRENT_UNIT, 0x51_0000);
    with_roots(&mut m, &[g]);
    m.rect(g + CLIP, [0, 0, 840, 620]);
    assert_eq!(controls(&m, &sites()), [Control {
        rect: [800, 601, 40, 19], kind: ControlKind::Button,
        id: part_id(g, TAG_STATUS, STATUS_ORB),
    }]);
    m.rect(g + UNIT_STATUS_RECT, [1000, 700, 800, 500]);
    assert!(controls(&m, &sites()).is_empty());
}

/// Native New Game screens paint option indices into a separate eight-bit
/// image. Their mouse handlers read these pixels, not child Button forms.
fn setup_mask(m: &mut Fake, f: u32, vtable: u32, offset: u32, size: (usize, usize), rects: &[(u8, [i32; 4])]) {
    let (image, bits) = (0x90_0000, 0x91_0000);
    m.form(f, vtable, 0, true, [40, 20, 40 + size.0 as i32, 20 + size.1 as i32], 0, &[]);
    with_roots(m, &[f]);
    m.put(f + offset + 4, image);
    m.put(image + 0x24, 8);
    let stride = (size.0 + 3) & !3;
    m.put(image + 0x40, stride as u32);
    m.rect(image + 0x54, [0, 0, size.0 as i32, size.1 as i32]);
    m.put(image + 0x4C0, bits);
    let mut data = vec![255; stride * size.1];
    for &(index, [x, y, w, h]) in rects {
        for yy in y..y+h {
            for xx in x..x+w { data[yy as usize * stride + xx as usize] = index; }
        }
    }
    m.bytes(bits, &data);
}

#[test]
fn every_world_and_player_option_uses_its_actual_native_hit_mask() {
    let f = 0x10_0000;
    for (vtable, offset, count) in [(0x67_0390, 0x3958, 34), (0x66_E2F0, 0x2B60, 67)] {
        let mut m = Fake::default();
        m.put(0x9C_3D74, 32);
        m.put(0x9C_3D54, 8);
        // World size, barbarians, landmass/water, climate, temperature and
        // age; player civilization, difficulty, game rules and aggression.
        let rects: Vec<_> = (0..count).map(|i| (i as u8, [i % 10 * 19, i / 10 * 13, 17, 11])).collect();
        setup_mask(&mut m, f, vtable, offset, (197, 97), &rects);
        let got = controls(&m, &sites());
        assert_eq!(got.len(), count as usize);
        for (control, &(index, rect)) in got.iter().zip(&rects) {
            assert_eq!(control.id, (f as u64) << 32 | 0x5000_0000 | index as u64);
            assert_eq!(control.rect, [rect[0] + 40, rect[1] + 20, rect[2], rect[3]]);
            let (x, y) = (rect[0] + rect[2] / 2, rect[1] + rect[3] / 2);
            assert_eq!(m.u8(0x91_0000 + y as u32 * 200 + x as u32), Some(index));
        }
        // A scenario-disabled choice is not in the game's mask. It must
        // disappear instead of keeping an English/static-layout target.
        let r = rects[3].1;
        for y in r[1]..r[1]+r[3] { for x in r[0]..r[0]+r[2] { m.0.insert(0x91_0000 + y as u32 * 200 + x as u32, 255); } }
        let got = controls(&m, &sites());
        assert_eq!(got.len(), count as usize - 1);
        assert!(!got.iter().any(|c| c.id as u32 == 0x5000_0003));
        m.put(f + STATUS2, 0);
        assert!(controls(&m, &sites()).is_empty());
    }
}

#[test]
fn native_setup_targets_never_land_in_holes_or_over_another_option() {
    let mut m = Fake::default();
    // Index 1 has an L shape around index 2. Its bounding-box midpoint
    // lies on 2, so selecting the largest solid run rectangle matters.
    setup_mask(&mut m, 0x10_0000, 0x67_0390, 0x3958, (21, 20), &[(1, [0, 0, 20, 20]), (2, [4, 4, 16, 16])]);
    let got = controls(&m, &sites());
    assert_eq!(got.len(), 2);
    let (one, two) = (&got[0], &got[1]);
    assert_eq!(one.rect, [40, 20, 20, 4]);
    assert_eq!(two.rect, [44, 24, 16, 16]);
    // Invalid formats, strides or unreadable pixels fail closed.
    m.put(0x90_0000 + 0x24, 16);
    assert!(controls(&m, &sites()).is_empty());
    m.put(0x90_0000 + 0x24, 8);
    m.put(0x90_0000 + 0x40, 2);
    assert!(controls(&m, &sites()).is_empty());
    m.put(0x90_0000 + 0x40, 24);
    m.put(0x90_0000 + 0x4C0, 0xFF_FFFF);
    assert!(controls(&m, &sites()).is_empty());
}

#[test]
fn without_mask_pixels_the_setup_targets_are_the_recorded_fills() {
    // Without jgl no fill writes the mask: its memory stays 0, which the
    // pixel walk would take for one choice 0 over the whole screen.
    struct Filled(Fake, u32);
    impl Mem for Filled {
        fn u32(&self, addr: u32) -> Option<u32> { self.0.u32(addr) }
        fn u8(&self, addr: u32) -> Option<u8> { self.0.u8(addr) }
        fn index_regions(&self, pcx: u32) -> Option<Vec<(u8, [i32; 4])>> {
            // 255 is never a choice; 40 is past the world screen's last.
            (pcx == self.1).then(|| vec![(0, [2, 3, 17, 11]), (5, [30, 3, 17, 11]), (40, [60, 3, 17, 11])])
        }
    }
    let (f, offset) = (0x10_0000, 0x3958);
    let mut m = Fake::default();
    setup_mask(&mut m, f, 0x67_0390, offset, (97, 47), &[(0, [0, 0, 97, 47])]);
    let got = controls(&Filled(m, f + offset), &sites());
    let got: Vec<_> = got.iter().map(|c| (c.id as u32, c.rect)).collect();
    assert_eq!(got, [(0x5000_0000, [42, 23, 17, 11]), (0x5000_0005, [70, 23, 17, 11])]);
}

#[test]
fn native_title_layout_uses_actual_resolution_rows_and_modal_state() {
    let mut m = Fake::default();
    m.bytes(0xB4_25CA, &[1]);
    assert_eq!(title_layout(&m, Build::Gog), None);
    assert_eq!(title_layout(&m, Build::Steam), None);
    m.put(0xCA_D384, 0);
    m.put(0x9C_72F8, 1920);
    m.put(0x9C_72FC, 1080);
    m.put(0x72_C2A4, 200);
    m.put(0x72_C2A8, 428);
    m.put(0xB3_E274, 38);
    m.put(0xA5_2CE8, 0);
    assert_eq!(title_layout(&m, Build::PcGames), Some(TitleLayout { origin: (648, 456), row_height: 38, first: 1 }));
    // A larger title viewport has the same client-pixel controls; the
    // viewer applies its selected adaptive UI scaling afterward.
    m.put(0x9C_72F8, 3840);
    m.put(0x9C_72FC, 2160);
    m.put(0xA5_2CE8, 1);
    assert_eq!(title_layout(&m, Build::PcGames), Some(TitleLayout { origin: (1608, 996), row_height: 38, first: 0 }));
    m.put(0xCA_D384, 0x12_3456);
    assert_eq!(title_layout(&m, Build::PcGames), None);
    m.put(0xCA_D384, 0);
    m.bytes(0xB4_25CA, &[0]);
    assert_eq!(title_layout(&m, Build::PcGames), None);
}

#[test]
fn direct_native_grid_and_radio_classes_bypass_the_type_dispatcher() {
    for (vtable, draw, back, subtract) in [(0x67_1730, CHOICE_DRAW, 0xE0, 0), (0x66_6778, LINES_DRAW, 0x28, 0x578)] {
        let mut m = Fake::default();
        let (f, thunk, vb) = (0x10_0000, 0x71_0000, 0x72_0000);
        m.form(f, vtable, 0, true, [100, 100, 300, 160], 0, &[]);
        m.put(vtable + 0x58, thunk);
        m.put(f - 4, 0);
        m.bytes(thunk, &[0x2B, 0x49, 0xFC]);
        let jump = if subtract == 0 { thunk + 3 } else {
            m.bytes(thunk + 3, &[0x81, 0xE9]); m.put(thunk + 5, subtract); thunk + 9
        };
        m.bytes(jump, &[0xE9]);
        m.put(jump + 1, draw.wrapping_sub(jump + 5));
        let b = f - subtract - back;
        m.put(b, vb);
        m.put(vb + 4, f - b);
        m.put(vb + 8, f + 0x578 - b);
        let l = f + 0x578;
        for (field, value) in [(L_X0, 0), (L_Y0, 0), (L_LAYOUT, 0), (L_GAP, 0), (L_ROWS, 2), (L_WIDTH, 200), (L_HEIGHT, 30), (L_COUNT, 2)] { m.put(l + field, value); }
        m.put(l + L_FIRST_NODE, 0);
        with_roots(&mut m, &[f]);
        let controls = controls(&m, &sites());
        assert_eq!(controls.iter().map(|c| c.rect).collect::<Vec<_>>(), [[100, 100, 200, 30], [100, 130, 200, 30]]);
        assert_eq!(controls[1].id, (f as u64) << 32 | TAG_CHOICE as u64 | 1);
        m.put(f + STATUS1, NO_MOUSE);
        assert!(super::controls(&m, &sites()).is_empty());
    }
}


#[test]
fn status_unit_icons_use_the_native_strip_and_need_an_active_visible_hud() {
    let mut m = Fake::default();
    let g = sites().main_gui;
    m.form(g, 0x66_B570, KEYED, true, [10, 5, 1034, 773], 0, &[]);
    m.rect(g + UNIT_STATUS_RECT, [700, 500, 1000, 700]);
    m.put(g + GUI_ENABLED - 1, 0x100);
    m.put(g - MAIN_GUI_IN_SCREEN + crate::layout::MAIN_SCREEN_FORM_CURRENT_UNIT, 0x51_0000);
    m.put(0xB4_2184, 0x73_0000);
    m.put(0x73_0000 + crate::layout::SPRITE_WIDTH, 32);
    m.put(0x73_0000 + crate::layout::SPRITE_HEIGHT, 32);
    with_roots(&mut m, &[g]);
    assert_eq!(status_unit_row(&m, &sites(), &controls(&m, &sites())), Some((g + 0x274, [737, 519, 247, 22])));
    assert_eq!(status_unit_row(&m, &sites(), &[]), None);
    m.put(g + GUI_ENABLED - 1, 0);
    assert_eq!(status_unit_row(&m, &sites(), &controls(&m, &sites())), None);
    m.put(g + GUI_ENABLED - 1, 0x100);
    m.put(0x73_0000 + crate::layout::SPRITE_HEIGHT, 24);
    assert_eq!(status_unit_row(&m, &sites(), &controls(&m, &sites())), None);
}

fn native_slider(m: &mut Fake, f: u32, size: (i32, i32), arrows: bool, border: bool, reverse: bool) {
    let parent = f + 0x20000;
    m.form(parent, PANEL, KEYED, true, [100, 50, 1000, 750], 0, &[f]);
    m.form(f, crate::sliders::VTABLE, 0, true, [10, 20, 10 + size.0, 20 + size.1], parent, &[]);
    m.put(f + 0x278, 0x500000);
    m.put(f + 0x574, if arrows { 0 } else { 2 });
    m.put(f + 0x578, 1);
    m.put(f + 0x57C, if border { 0 } else { u32::MAX });
    m.put(f + 0x580, 10);
    m.put(f + 0x584, 137);
    m.put(f + 0x588, u32::from(reverse));
    m.put(f + crate::sliders::VALUE, 51);
    let inset = i32::from(border);
    let thickness = size.0.min(size.1);
    m.rect(f + 0x5AC, [inset, inset, thickness - inset, thickness - inset]);
    with_roots(m, &[parent]);
}

#[test]
fn native_slider_tracks_match_knob_travel_with_arrows_borders_and_both_axes() {
    let f = 0x100000;
    for horizontal in [false, true] {
        for arrows in [false, true] {
            for border in [false, true] {
                let mut m = Fake::default();
                let size = if horizontal { (406, 20) } else { (20, 406) };
                native_slider(&mut m, f, size, arrows, border, false);
                let slider = crate::sliders::Slider::read(&m, f).unwrap();
                let knob = 20 - i32::from(border) * 2;
                let start = i32::from(border) + knob / 2 + if arrows { knob + 1 } else { 0 };
                let travel = 406 - knob * if arrows { 3 } else { 1 } - i32::from(border) * 2;
                let track = if horizontal { [110 + start, 80, travel, 1] } else { [120, 70 + start, 1, travel] };
                assert_eq!(controls(&m, &sites()), [Control { id: u64::from(f), rect: [110, 70, size.0, size.1], kind: ControlKind::Slider { track, value: slider.normalized() } }]);
                assert_eq!(slider.exact_at(0), 10);
                assert_eq!(slider.exact_at(1_000_000), 137);
            }
        }
    }
}

#[test]
fn native_slider_reverse_changes_parent_value_but_not_the_drawn_position() {
    let f = 0x100000;
    let mut m = Fake::default();
    native_slider(&mut m, f, (406, 20), true, true, false);
    let normal = crate::sliders::Slider::read(&m, f).unwrap();
    m.put(f + 0x588, 1);
    let reverse = crate::sliders::Slider::read(&m, f).unwrap();
    assert_eq!(normal.normalized(), reverse.normalized());
    assert_eq!(normal.track, reverse.track);
    assert!(!normal.same_control(reverse));
}

#[test]
fn native_slider_identity_rejects_reused_canvas_parent_and_range() {
    let f = 0x100000;
    for offset in [0x130, 0x278, 0x578, 0x580, 0x584] {
        let mut m = Fake::default();
        native_slider(&mut m, f, (406, 20), true, true, false);
        let before = crate::sliders::Slider::read(&m, f).unwrap();
        let old = m.u32(f + offset).unwrap();
        m.put(f + offset, old + 1);
        assert!(!crate::sliders::Slider::read(&m, f).is_some_and(|after| after.same_control(before)));
    }
}

#[test]
fn hidden_empty_and_malformed_native_sliders_are_not_alt_targets() {
    let f = 0x100000;
    for (offset, value) in [(STATUS2, 0), (STATUS1, RELATIVE | NO_MOUSE), (0x278, 0), (0x584, 10), (0x58C, 138), (0x1B4, 10)] {
        let mut m = Fake::default();
        native_slider(&mut m, f, (406, 20), true, true, false);
        m.put(f + offset, value);
        assert!(controls(&m, &sites()).is_empty(), "offset {offset:x}");
    }
}

#[test]
fn malformed_native_slider_knob_geometry_cannot_overflow_track_arithmetic() {
    let f = 0x100000;
    for right in [i32::MAX, i32::MIN, 0, 100_000] {
        let mut m = Fake::default();
        native_slider(&mut m, f, (406, 20), true, true, false);
        m.put(f + 0x5AC, 0);
        m.put(f + 0x5B4, right as u32);
        assert!(crate::sliders::Slider::read(&m, f).is_none());
        assert!(controls(&m, &sites()).is_empty());
    }
}


#[test]
fn the_native_map_seed_text_box_can_receive_alt_focus_and_keyboard_editing() {
    let mut m = Fake::default();
    let (world, seed) = (0x10_0000, 0x10_120C);
    m.form(world, 0x67_0390, 0, true, [100, 80, 900, 680], 0, &[seed]);
    // Real New_Game_Map_Form setup calls TextBox::setup and selects its
    // editable buffer/max-length. Native clicks position the caret; Alt
    // must focus this form, without pretending that it is a palette item.
    m.form(seed, 0x66_7188, 0x102020, true, [420, 550, 620, 575], world, &[]);
    with_roots(&mut m, &[world]);
    assert_eq!(controls(&m, &sites()), [Control { rect: [520, 630, 200, 25], kind: ControlKind::Button, id: seed as u64 }]);
    m.put(seed + STATUS1, 0x102020 | NO_MOUSE);
    assert!(controls(&m, &sites()).is_empty());
    m.put(seed + STATUS1, 0x102020);
    m.put(seed + STATUS2, 0);
    assert!(controls(&m, &sites()).is_empty());
}

#[test]
fn every_form_a_click_acts_on_is_a_control_whatever_its_class() {
    // A dialog (class not decoded here; rectangles left, top, right,
    // bottom) with three children of unknown
    // classes: one with a click callback (+0x244), one whose class has its
    // own left button handler, one that does nothing with a click.
    let (dialog, cb, own, inert, screen) = (0x10_0000, 0x11_0000, 0x12_0000, 0x13_0000, 0x14_0000);
    let mut m = Fake::default();
    m.form(dialog, 0x66_9990, 0, true, [100, 100, 600, 400], 0, &[cb, own, inert, screen]);
    m.form(cb, 0x66_9994, 0, true, [10, 200, 90, 230], dialog, &[]);
    m.put(cb + 0x244, 0x51_2340);
    m.form(own, 0x66_9998, 0, true, [100, 200, 180, 230], dialog, &[]);
    m.put(0x66_9998 + 0x150, 0x41_0000);
    m.put(0x66_9998 + 0x154, 0x60_8A40);
    m.form(inert, 0x66_999C, 0, true, [200, 200, 280, 230], dialog, &[]);
    m.put(0x66_999C + 0x150, 0x60_8960);
    m.put(0x66_999C + 0x154, 0x60_8A40);
    // Too large for a control (a panel), though it has a callback.
    m.form(screen, 0x66_99A0, 0, true, [0, 0, 450, 180], dialog, &[]);
    m.put(screen + 0x244, 0x51_2340);
    with_roots(&mut m, &[dialog]);
    let got: Vec<(u64, ControlKind)> = controls(&m, &sites()).iter().map(|c| (c.id, c.kind)).collect();
    assert_eq!(got, [(cb as u64, ControlKind::Button), (own as u64, ControlKind::Button)]);
}
