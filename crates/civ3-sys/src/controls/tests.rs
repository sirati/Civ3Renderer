use super::*;
use std::collections::HashMap;

/// Game memory as bytes at addresses.
#[derive(Default)]
struct Fake(HashMap<u32, u8>);

impl Fake {
    fn put(&mut self, at: u32, v: u32) {
        for (i, b) in v.to_le_bytes().into_iter().enumerate() {
            self.0.insert(at + i as u32, b);
        }
    }
    fn bytes(&mut self, at: u32, bytes: &[u8]) {
        for (i, b) in bytes.iter().enumerate() {
            self.0.insert(at + i as u32, *b);
        }
    }
    fn rect(&mut self, at: u32, r: [i32; 4]) {
        for (i, v) in r.into_iter().enumerate() {
            self.put(at + 4 * i as u32, v as u32);
        }
    }

    /// A form at `addr`: its vtable, status, place (`left, top, right,
    /// bottom`, in its parent's pixels if `parent`), children.
    fn form(&mut self, addr: u32, vtable: u32, status1: u32, shown: bool, rect: [i32; 4], parent: u32, children: &[u32]) {
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

const BUTTON: u32 = 0x66_56C0;
const PANEL: u32 = 0x67_1218;

fn sites() -> Sites {
    Sites { roots: 0x100, root_count: 0x200, modal: 0x300, companion: 0x304, main_gui: 0x9F_DB10, advisor: 0x9A_FD58 }
}

fn with_roots(m: &mut Fake, roots: &[u32]) {
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
fn list(m: &mut Fake, f: u32, status1: u32, layout: ItemLayout, count: i32, first: i32, shown: i32) {
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

const ROWS: ItemLayout = ItemLayout { x0: 4, y0: 2, gap: 1, variable: false, rows: 8, width: 200, height: 19 };

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
    assert_eq!(lists[0].kind, ControlKind::List { scroll: [100, 100, 200, 160], step: 20 });
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
fn context_menu_rows_skip_hidden_and_disabled_entries() {
    // Shown, hidden, shown but disabled, shown.
    let rows = menu_rows(&[1, 0, 1 | 4, 1], 18, (120, 60));
    assert_eq!(rows, [(0, [0, 2, 120, 18]), (3, [0, 38, 120, 18])]);
    // Without a height: the menu's shared among its rows.
    assert_eq!(menu_rows(&[1, 1], 0, (100, 44)), [(0, [0, 2, 100, 20]), (1, [0, 22, 100, 20])]);
    assert!(menu_rows(&[0, 0], 18, (100, 44)).is_empty());
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
