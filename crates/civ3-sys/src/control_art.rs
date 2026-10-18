//! The art of the game's controls in their states (docs §16.11): which of
//! the game's sprites a control draws when the pointer is over it, presses
//! it, or not, and selected or not, so that a renderer can draw those
//! states itself. Found where the controls' draw functions take them from:
//! `Button`s (0x60A010), the advisors' tabs (0x49D960), check boxes
//! (0x61E610).
//!
//! Reads go through [`Mem`] so they can be tested without the game. Only
//! the PCGames build is decoded.

use crate::controls::{Control, ControlKind, Mem, Sites, TAG_CHECK, TAG_CHOICE, TAG_MENU, TAG_TAB, TAG_STATUS, STATUS_ORB};
use crate::layout;

/// The draw (vtable +0x58) of every `Button` that picks its art by its
/// state (0x60A010; the city screen's production button 0x423470 calls it
/// and writes its text over it).
pub const BUTTON_DRAWS: [u32; 2] = [0x60_A010, 0x42_3470];

/// `Button` fields (docs §16.11).
const IMAGES: u32 = layout::BUTTON_IMAGES;
/// The nine frame pieces (`Sprite*`), three states each: piece `p`, state
/// `s` at `+ 0xC·p + 4·s`.
const PIECES: u32 = layout::BUTTON_FIELD_668;
/// Hovered; pressed; latched on (a radio or toggle button: drawn pressed);
/// the state forced (0..3 overrides the rest, never set by the game).
const HOVERED: u32 = layout::BUTTON_FIELD_574_28;
const PRESSED: u32 = layout::BUTTON_FIELD_5FC_6;
const LATCHED: u32 = layout::BUTTON_FIELD_574_1;
const FORCED: u32 = layout::BUTTON_FIELD_630_1;
/// How far its label moves while pressed (x, y).
const PRESSED_TEXT: u32 = 0x5EC;

/// The advisors' tabs: their sprites (`Sprite`, 0x2C bytes) per tab and
/// column (normal, hover, selected), the tab selected and the one hovered
/// (0x49CF16, 0x49D960, 0x49E4C0).
const TAB_SPRITES: u32 = 0x26F0;
const TAB_STRIDE: u32 = 0x84;
const SPRITE_BYTES: u32 = 0x2C;
const TAB_SELECTED: u32 = 0x2510;
const TAB_HOVERED: u32 = 0x2514;

/// A check box group's form part F: its box sprites (`Sprite*`: unchecked,
/// hovered, checked) at F−0x10, F−0x0C, F−0x08; its item list's checked
/// bits and hovered item at F+0x668, F+0x66C (0x61E610, 0x61F070).
const CHECK_SPRITES: u32 = 0x10;
const CHECK_BITS: u32 = 0x668;
const CHECK_HOVERED: u32 = 0x66C;

/// Where the pointer is, as a control's art knows it.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum Pointer {
    #[default]
    Normal,
    Hover,
    Pressed,
}

/// A state a control has art for.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Look {
    pub pointer: Pointer,
    pub selected: bool,
}

const fn look(pointer: Pointer, selected: bool) -> Look {
    Look { pointer, selected }
}

/// What a click does to a control's selection.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Select {
    None,
    /// Flips it (a check box).
    Toggle,
    /// Selects it and unselects the others of the group (the object's
    /// address).
    Pick(u32),
}

/// A control's state art.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateArt {
    /// The canvas the control is drawn on: its draws on the composed
    /// screen are the control's.
    pub canvas: u32,
    /// Which of the canvas' draws are the control's.
    pub part: Part,
    /// The state the game's fields say it is in.
    pub state: Look,
    /// Whether that state is the game's own choice rather than the
    /// pointer's (forced, or latched): then the game's drawing stays.
    pub forced: bool,
    /// Per piece of art it is drawn from, the sprite (game `Sprite*`) of
    /// each state it has art for.
    pub pieces: Vec<Vec<(Look, u32)>>,
    /// How far its text moves in a state (x, y), where it does.
    pub text_offsets: Vec<(Look, (i32, i32))>,
    /// The colours of its text and fills in a state, where they change (the
    /// i-th of one state becomes the i-th of another).
    pub colors: Vec<(Look, Vec<Color>)>,
    pub select: Select,
}

/// A colour the game draws a control's text or a fill in, as it has it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Color {
    /// A text colour: packed (bit 31 and RGB555, as `SetTextColor` gets
    /// it), else a palette index, black on the game's canvases (docs
    /// §16.10).
    Text(u32),
    /// A fill's RGB555 colour.
    Fill(u32),
}

/// Which of a canvas' draws are a control's.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// All of them (the canvas is the control's own).
    Whole,
    /// The last draw of its art within its rectangle (a tab, a box: drawn
    /// over what was there).
    LastArt,
    /// All within its rectangle (a row, redrawn as a whole).
    Within,
}

/// The state art of `control`, if the game draws it by its state from
/// sprites: a `Button`, an advisors' tab, a check box.
pub fn of(mem: &dyn Mem, sites: &Sites, control: &Control) -> Option<StateArt> {
    let object = (control.id >> 32) as u32;
    let part = control.id as u32;
    if control.kind != ControlKind::Button {
        return None;
    }
    match (object, part & 0xF000_0000) {
        (0, _) => button(mem, part),
        (o, TAG_TAB) if o == sites.advisor => advisor_tab(mem, o, part & 0x0FFF_FFFF),
        (o, TAG_CHECK) => check_box(mem, o, part & 0x0FFF_FFFF),
        (o, 0) if part & TAG_CHOICE != 0 => bullet_row(mem, o, part & !TAG_CHOICE),
        (o, TAG_MENU) => menu_row(mem, o, part & 0x0FFF_FFFF),
        (o, TAG_STATUS) if o == sites.main_gui && part & 0x0FFF_FFFF == STATUS_ORB => status_orb(mem, o),
        _ => None,
    }
}

/// Main_GUI paints the status orb directly (0x554DA1, 0x555050), and
/// changes its art in hover (0x558169) and click (0x5560F6). These are
/// the real sprites, on Main_GUI's canvas; the unit portrait has no hover
/// or pressed artwork of its own.
fn status_orb(mem: &dyn Mem, gui: u32) -> Option<StateArt> {
    let main = gui.checked_sub(0x5450)?;
    let selected = mem.u32(main + layout::MAIN_SCREEN_FORM_CURRENT_UNIT)? != 0;
    let normal = if selected { 0 } else { mem.u32(gui + 0x10654)?.min(2) };
    let state = look(if mem.u32(gui + 0x1064C)? != 0 { Pointer::Hover } else { Pointer::Normal }, false);
    Some(StateArt {
        canvas: gui + layout::BASE_FORM_DATA_CANVAS + 4,
        part: Part::LastArt,
        state,
        forced: false,
        pieces: vec![vec![
            (look(Pointer::Normal, false), gui + 0x147E4 + normal * layout::SPRITE_SIZE),
            (look(Pointer::Hover, false), gui + 0x14810),
            (look(Pointer::Pressed, false), gui + 0x1483C),
        ]],
        text_offsets: Vec::new(),
        colors: Vec::new(),
        select: Select::None,
    })
}

/// The state art of the `Button` at `addr`, if it is one that draws by its
/// state (its draw is one of [`BUTTON_DRAWS`]) with images or frame pieces.
pub fn button(mem: &dyn Mem, addr: u32) -> Option<StateArt> {
    let vtable = mem.u32(addr)?;
    if !BUTTON_DRAWS.contains(&mem.u32(vtable + 0x58)?) {
        return None;
    }
    // The state, as the draw picks it.
    let forced = mem.i32(addr + FORCED)?;
    let latched = mem.u32(addr + LATCHED)? != 0;
    let (state, forced) = if (0..3).contains(&forced) {
        (forced as usize, true)
    } else if latched || mem.u32(addr + PRESSED)? != 0 {
        (2, latched)
    } else {
        (usize::from(mem.u32(addr + HOVERED)? != 0), false)
    };
    const LOOKS: [Look; 3] = [look(Pointer::Normal, false), look(Pointer::Hover, false), look(Pointer::Pressed, false)];
    let image = |s: u32| mem.u32(addr + IMAGES + 4 * s);
    let images = [image(0)?, image(1)?, image(2)?];
    // Its image of each state, else (none in its state) its frame pieces.
    let sprites: Vec<[u32; 3]> = if images[state] != 0 {
        vec![images]
    } else {
        (0..9).map(|p| Some([0, 1, 2].map(|s| mem.u32(addr + PIECES + 0xC * p + 4 * s).unwrap_or(0)))).collect::<Option<Vec<_>>>()?
    };
    let pieces: Vec<Vec<(Look, u32)>> = sprites
        .into_iter()
        .filter(|p| p[state] != 0)
        .map(|p| (0..3).filter(|&s| p[s] != 0).map(|s| (LOOKS[s], p[s])).collect())
        .collect();
    if pieces.is_empty() {
        return None;
    }
    let text = (mem.i32(addr + PRESSED_TEXT)?, mem.i32(addr + PRESSED_TEXT + 4)?);
    Some(StateArt {
        canvas: addr + layout::BASE_FORM_DATA_CANVAS + 4,
        part: Part::Whole,
        state: LOOKS[state],
        forced,
        pieces,
        text_offsets: vec![(LOOKS[2], text)],
        colors: Vec::new(),
        select: Select::None,
    })
}

/// Tab `k` of the advisors' form at `advisor`: normal, hover and selected
/// art (selected whatever the pointer does; pressing it selects it).
pub fn advisor_tab(mem: &dyn Mem, advisor: u32, k: u32) -> Option<StateArt> {
    if k >= 6 {
        return None;
    }
    let sprite = |col: u32| advisor + TAB_SPRITES + TAB_STRIDE * k + SPRITE_BYTES * col;
    let selected = mem.i32(advisor + TAB_SELECTED)? == k as i32;
    let hovered = mem.i32(advisor + TAB_HOVERED)? == k as i32;
    let state = look(if hovered { Pointer::Hover } else { Pointer::Normal }, selected);
    let pieces = vec![vec![
        (look(Pointer::Normal, false), sprite(0)),
        (look(Pointer::Hover, false), sprite(1)),
        (look(Pointer::Normal, true), sprite(2)),
        (look(Pointer::Hover, true), sprite(2)),
    ]];
    Some(StateArt {
        canvas: advisor + layout::BASE_FORM_DATA_CANVAS + 4,
        part: Part::LastArt,
        state,
        forced: false,
        pieces,
        text_offsets: Vec::new(),
        colors: Vec::new(),
        select: Select::Pick(advisor),
    })
}

/// Box `i` of the check box group whose form part is at `form`: unchecked,
/// hovered (only drawn unchecked: hovering a checked box shows it checked)
/// and checked art.
pub fn check_box(mem: &dyn Mem, form: u32, i: u32) -> Option<StateArt> {
    if i >= 32 {
        return None;
    }
    let [unchecked, hovered, checked] = [0, 1, 2].map(|s| mem.u32(form.wrapping_sub(CHECK_SPRITES) + 4 * s).unwrap_or(0));
    if unchecked == 0 || hovered == 0 || checked == 0 {
        return None;
    }
    let on = mem.u32(form + CHECK_BITS)? & (1 << i) != 0;
    let hover = mem.i32(form + CHECK_HOVERED)? == i as i32;
    let pieces = vec![vec![
        (look(Pointer::Normal, false), unchecked),
        (look(Pointer::Hover, false), hovered),
        (look(Pointer::Normal, true), checked),
        (look(Pointer::Hover, true), checked),
    ]];
    Some(StateArt {
        canvas: form + layout::BASE_FORM_DATA_CANVAS + 4,
        part: Part::LastArt,
        state: look(if hover { Pointer::Hover } else { Pointer::Normal }, on),
        forced: false,
        pieces,
        text_offsets: Vec::new(),
        colors: Vec::new(),
        select: Select::Toggle,
    })
}

/// A context menu's items (28 bytes: +0xC status, bit 1 disabled, bit 2 a
/// separator; +0x18 bit 0 red), the item under the pointer, and a hook that
/// draws its rows instead (0x61B250).
const MENU_ITEMS: u32 = 0x578;
const MENU_ITEM_BYTES: u32 = 28;
const MENU_HOVERED: u32 = 0x2190;
const MENU_ROW_HOOK: u32 = 0x21AC;
/// The globals holding a row's colours: fill, text and its shadow, normal
/// (black or red) and highlighted (0x61B250).
const MENU_FILL: u32 = 0x73_02F8;
const MENU_FILL_HIGHLIGHT: u32 = 0xCA_DF8C;
const MENU_TEXT: [u32; 2] = [0xCA_DF90, 0x73_0304];
const MENU_TEXT_RED: [u32; 2] = [0x73_031C, 0x73_0320];
const MENU_TEXT_HIGHLIGHT: [u32; 2] = [0x73_02FC, 0x73_0308];

/// Row `i` of the context menu at `form`: its fill and text colours,
/// highlighted while the pointer is over it (disabled rows, highlighted
/// without the text's shadow, and rows drawn by a hook are left out).
pub fn menu_row(mem: &dyn Mem, form: u32, i: u32) -> Option<StateArt> {
    if i >= 512 || mem.u32(form + MENU_ROW_HOOK)? != 0 {
        return None;
    }
    let item = form + MENU_ITEMS + MENU_ITEM_BYTES * i;
    if mem.u32(item + 0xC)? & 6 != 0 {
        return None;
    }
    let read = |a: u32| mem.u32(a);
    let text = if mem.u32(item + 0x18)? & 1 != 0 { MENU_TEXT_RED } else { MENU_TEXT };
    let colors = |fill: u32, [t, s]: [u32; 2]| -> Option<Vec<Color>> {
        let (t, s) = (read(t)?, read(s)?);
        let mut v = vec![Color::Fill(read(fill)?), Color::Text(t)];
        if s != u32::MAX {
            v.push(Color::Text(s));
        }
        Some(v)
    };
    let (normal, highlight) = (colors(MENU_FILL, text)?, colors(MENU_FILL_HIGHLIGHT, MENU_TEXT_HIGHLIGHT)?);
    if normal.len() != highlight.len() {
        return None;
    }
    let hovered = mem.i32(form + MENU_HOVERED)? == i as i32;
    Some(StateArt {
        canvas: form + layout::BASE_FORM_DATA_CANVAS + 4,
        part: Part::Within,
        state: look(if hovered { Pointer::Hover } else { Pointer::Normal }, false),
        forced: false,
        pieces: Vec::new(),
        text_offsets: Vec::new(),
        colors: vec![(look(Pointer::Normal, false), normal), (look(Pointer::Hover, false), highlight.clone()), (look(Pointer::Pressed, false), highlight)],
        select: Select::None,
    })
}

/// The bullet list's sprites (`Sprite*`: normal, selected, hover) and text
/// colours (hover, selected) in its own part C (0x61CC20); its item list's
/// normal colour, selected and hovered rows, and the outline drawn around a
/// selected row's text (−1: none).
const BULLETS: u32 = 4;
const BULLET_HOVER_COLOR: u32 = 0x18;
const BULLET_SELECTED_COLOR: u32 = 0x1C;
const L_NORMAL_COLOR: u32 = 0x80;
const L_SELECTED: u32 = 0xF0;
const L_HOVERED: u32 = 0xF4;
const L_OUTLINE: u32 = 0xBC;

/// Row `r` of the bullet list whose form part is at `form` (a popup's
/// choices): its bullet and text colour normal, hovered (the selected row
/// never shows hover) and selected. A press selects (on the button going
/// down), so pressed looks selected.
pub fn bullet_row(mem: &dyn Mem, form: u32, r: u32) -> Option<StateArt> {
    bullet_row_of(mem, crate::controls::bullet_list(mem, form)?, form, r)
}

/// [`bullet_row`] of the rows `list`.
fn bullet_row_of(mem: &dyn Mem, list: crate::controls::BulletList, form: u32, r: u32) -> Option<StateArt> {
    // (An outline around the selected row's text: not drawn here.)
    if mem.i32(list.items + L_OUTLINE)? != -1 {
        return None;
    }
    let [normal, selected, hover] = [0, 1, 2].map(|i| mem.u32(list.c + BULLETS + 4 * i).unwrap_or(0));
    if normal == 0 || selected == 0 || hover == 0 {
        return None;
    }
    let on = mem.i32(list.items + L_SELECTED)? == r as i32;
    let over = mem.i32(list.items + L_HOVERED)? == r as i32;
    let colors = [mem.u32(list.items + L_NORMAL_COLOR)?, mem.u32(list.c + BULLET_HOVER_COLOR)?, mem.u32(list.c + BULLET_SELECTED_COLOR)?];
    let looks: Vec<(Look, u32, u32)> = [
        (look(Pointer::Normal, false), normal, colors[0]),
        (look(Pointer::Hover, false), hover, colors[1]),
        (look(Pointer::Pressed, false), selected, colors[2]),
        (look(Pointer::Normal, true), selected, colors[2]),
        (look(Pointer::Hover, true), selected, colors[2]),
        (look(Pointer::Pressed, true), selected, colors[2]),
    ]
    .into();
    Some(StateArt {
        canvas: form + layout::BASE_FORM_DATA_CANVAS + 4,
        part: Part::Within,
        state: look(if over && !on { Pointer::Hover } else { Pointer::Normal }, on),
        forced: false,
        pieces: vec![looks.iter().map(|&(l, s, _)| (l, s)).collect()],
        text_offsets: Vec::new(),
        colors: looks.iter().map(|&(l, _, c)| (l, vec![Color::Text(c)])).collect(),
        select: Select::Pick(form),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake(HashMap<u32, u32>);

    impl Mem for Fake {
        fn u32(&self, addr: u32) -> Option<u32> {
            Some(self.0.get(&addr).copied().unwrap_or(0))
        }
        fn u8(&self, addr: u32) -> Option<u8> {
            self.u32(addr & !3).map(|v| (v >> (8 * (addr & 3))) as u8)
        }
    }

    const NORMAL: Look = look(Pointer::Normal, false);
    const HOVER: Look = look(Pointer::Hover, false);
    const PRESSED_LOOK: Look = look(Pointer::Pressed, false);

    fn button() -> Fake {
        let mut m = Fake::default();
        m.0.insert(0x1000, 0x66_56C0);
        m.0.insert(0x66_56C0 + 0x58, BUTTON_DRAWS[0]);
        m.0.insert(0x1000 + FORCED, u32::MAX);
        m.0.insert(0x1000 + PRESSED_TEXT, 2);
        m.0.insert(0x1000 + PRESSED_TEXT + 4, 2);
        m
    }

    #[test]
    fn a_buttons_images_by_state() {
        let mut m = button();
        for (s, sprite) in [0xA0, 0xA1, 0xA2].into_iter().enumerate() {
            m.0.insert(0x1000 + IMAGES + 4 * s as u32, sprite);
        }
        let art = super::button(&m, 0x1000).unwrap();
        assert_eq!(
            art,
            StateArt {
                canvas: 0x1000 + 0x274,
                part: Part::Whole,
                state: NORMAL,
                forced: false,
                pieces: vec![vec![(NORMAL, 0xA0), (HOVER, 0xA1), (PRESSED_LOOK, 0xA2)]],
                text_offsets: vec![(PRESSED_LOOK, (2, 2))],
                colors: vec![],
                select: Select::None,
            }
        );
        m.0.insert(0x1000 + HOVERED, 1);
        assert_eq!(super::button(&m, 0x1000).unwrap().state, HOVER);
        m.0.insert(0x1000 + PRESSED, 1);
        assert_eq!(super::button(&m, 0x1000).unwrap().state, PRESSED_LOOK);
        // Latched on (a radio button): the game's.
        m.0.insert(0x1000 + PRESSED, 0);
        m.0.insert(0x1000 + LATCHED, 1);
        let art = super::button(&m, 0x1000).unwrap();
        assert_eq!((art.state, art.forced), (PRESSED_LOOK, true));
        // Not a state-drawing button.
        m.0.insert(0x66_56C0 + 0x58, 0x60_7A30);
        assert_eq!(super::button(&m, 0x1000), None);
    }

    #[test]
    fn a_framed_buttons_pieces_by_state() {
        let mut m = button();
        for p in 0..9 {
            for s in 0..3 {
                m.0.insert(0x1000 + PIECES + 0xC * p + 4 * s, 0x100 * (p + 1) + s);
            }
        }
        let art = super::button(&m, 0x1000).unwrap();
        assert_eq!(art.pieces.len(), 9);
        assert_eq!(art.pieces[4], [(NORMAL, 0x500), (HOVER, 0x501), (PRESSED_LOOK, 0x502)]);
        // No art at all: none.
        assert_eq!(super::button(&super::tests::button(), 0x1000), None);
    }

    #[test]
    fn tabs_and_boxes_by_their_ids() {
        let sites = Sites { roots: 0, root_count: 0, modal: 0, companion: 0, main_gui: 0x9F_DB10, advisor: 0x9A_FD58 };
        let mut m = Fake::default();
        m.0.insert(0x9A_FD58 + TAB_SELECTED, 2);
        m.0.insert(0x9A_FD58 + TAB_HOVERED, 3);
        let tab = |k: u64| Control { rect: [4, 0, 65, 62], kind: ControlKind::Button, id: 0x9A_FD58 << 32 | 0x8000_0000 | k };
        let art = of(&m, &sites, &tab(3)).unwrap();
        assert_eq!((art.state, art.part, art.select), (HOVER, Part::LastArt, Select::Pick(0x9A_FD58)));
        assert_eq!(art.pieces[0][1], (HOVER, 0x9A_FD58 + 0x26F0 + 3 * 0x84 + 0x2C));
        assert_eq!(of(&m, &sites, &tab(2)).unwrap().state, look(Pointer::Normal, true));
        let boxes = 0x5000u32;
        for (s, v) in [0xB0, 0xB1, 0xB2].into_iter().enumerate() {
            m.0.insert(boxes - 0x10 + 4 * s as u32, v);
        }
        m.0.insert(boxes + CHECK_BITS, 0b100);
        m.0.insert(boxes + CHECK_HOVERED, 1);
        let check = |i: u64| Control { rect: [0, 0, 16, 16], kind: ControlKind::Button, id: (boxes as u64) << 32 | 0x4000_0000 | i };
        let art = of(&m, &sites, &check(1)).unwrap();
        assert_eq!((art.state, art.select), (HOVER, Select::Toggle));
        assert_eq!(of(&m, &sites, &check(2)).unwrap().state, look(Pointer::Normal, true));
        // A list entry: none.
        assert_eq!(of(&m, &sites, &Control { kind: ControlKind::ListItem, ..check(0) }), None);
    }
}

#[cfg(test)]
mod bullet_tests {
    use super::*;
    use crate::controls::BulletList;

    #[derive(Default)]
    struct Fake(std::collections::HashMap<u32, u32>);

    impl Mem for Fake {
        fn u32(&self, addr: u32) -> Option<u32> {
            Some(self.0.get(&addr).copied().unwrap_or(0))
        }
        fn u8(&self, addr: u32) -> Option<u8> {
            self.u32(addr & !3).map(|v| (v >> (8 * (addr & 3))) as u8)
        }
    }

    #[test]
    fn a_popups_choice_rows_by_state() {
        let f = 0x10_0000;
        let mut m = Fake::default();
        let (c, l) = (f - 0x28, f + 0x578);
        let list = BulletList { c, items: l };
        for (i, s) in [0xB3_DD28, 0xB3_DD80, 0xB3_DD54].into_iter().enumerate() {
            m.0.insert(c + 4 + 4 * i as u32, s);
        }
        m.0.insert(c + 0x18, 0x8000_5960);
        m.0.insert(c + 0x1C, 0x8000_01D7);
        m.0.insert(l + 0x80, 0);
        m.0.insert(l + 0xBC, u32::MAX);
        m.0.insert(l + 0xF0, 0);
        m.0.insert(l + 0xF4, 2);
        let row = |r| bullet_row_of(&m, list, f, r).unwrap();
        let (sel, over, plain) = (row(0), row(2), row(1));
        assert_eq!(sel.state, look(Pointer::Normal, true));
        assert_eq!(over.state, look(Pointer::Hover, false));
        assert_eq!((plain.state, plain.part, plain.select), (look(Pointer::Normal, false), Part::Within, Select::Pick(f)));
        assert_eq!(plain.pieces[0][1], (look(Pointer::Hover, false), 0xB3_DD54));
        assert_eq!(plain.colors[1], (look(Pointer::Hover, false), vec![Color::Text(0x8000_5960)]));
        // Pressed: selected.
        assert_eq!(plain.pieces[0][2], (look(Pointer::Pressed, false), 0xB3_DD80));
        // An outline around the selected text: none.
        m.0.insert(l + 0xBC, 5);
        assert_eq!(bullet_row_of(&m, list, f, 1), None);
    }
}

#[cfg(test)]
mod menu_tests {
    use super::*;

    #[derive(Default)]
    struct Fake(std::collections::HashMap<u32, u32>);

    impl Mem for Fake {
        fn u32(&self, addr: u32) -> Option<u32> {
            Some(self.0.get(&addr).copied().unwrap_or(0))
        }
        fn u8(&self, addr: u32) -> Option<u8> {
            self.u32(addr & !3).map(|v| (v >> (8 * (addr & 3))) as u8)
        }
    }

    #[test]
    fn a_context_menus_rows_highlight_under_the_pointer() {
        let form = 0x20_0000;
        let mut m = Fake::default();
        for (a, v) in [(MENU_FILL, 0x111), (MENU_FILL_HIGHLIGHT, 0x222), (MENU_TEXT[0], 0x8000_0000), (MENU_TEXT[1], u32::MAX), (MENU_TEXT_HIGHLIGHT[0], 0x8000_7FFF), (MENU_TEXT_HIGHLIGHT[1], u32::MAX)] {
            m.0.insert(a, v);
        }
        m.0.insert(form + MENU_HOVERED, 1);
        let row = menu_row(&m, form, 1).unwrap();
        assert_eq!(row.state, look(Pointer::Hover, false));
        assert_eq!(row.colors[0], (look(Pointer::Normal, false), vec![Color::Fill(0x111), Color::Text(0x8000_0000)]));
        assert_eq!(row.colors[1], (look(Pointer::Hover, false), vec![Color::Fill(0x222), Color::Text(0x8000_7FFF)]));
        assert_eq!(menu_row(&m, form, 0).unwrap().state, look(Pointer::Normal, false));
        // Disabled, a separator, a hook drawing the rows: none.
        m.0.insert(form + MENU_ITEMS + MENU_ITEM_BYTES * 2 + 0xC, 2);
        assert_eq!(menu_row(&m, form, 2), None);
        m.0.insert(form + MENU_ROW_HOOK, 0x40_0000);
        assert_eq!(menu_row(&m, form, 1), None);
    }
}


#[cfg(test)]
mod status_tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake(HashMap<u32, u32>);
    impl Mem for Fake {
        fn u32(&self, addr: u32) -> Option<u32> { self.0.get(&addr).copied() }
        fn u8(&self, addr: u32) -> Option<u8> { self.u32(addr).map(|v| v as u8) }
    }

    #[test]
    fn status_orb_uses_main_gui_art_instead_of_guessing_a_button_object() {
        let sites = crate::controls::sites(crate::Build::PcGames).unwrap();
        let g = sites.main_gui;
        let mut m = Fake::default();
        m.0.insert(g - 0x5450 + layout::MAIN_SCREEN_FORM_CURRENT_UNIT, 0);
        m.0.insert(g + 0x10654, 2);
        m.0.insert(g + 0x1064C, 0);
        let control = Control { rect: [700, 601, 48, 28], kind: ControlKind::Button,
            id: (g as u64) << 32 | TAG_STATUS as u64 | STATUS_ORB as u64 };
        let art = of(&m, &sites, &control).unwrap();
        assert_eq!((art.canvas, art.part, art.state, art.forced, art.select),
            (g + 0x274, Part::LastArt, look(Pointer::Normal, false), false, Select::None));
        assert_eq!(art.pieces, [vec![
            (look(Pointer::Normal, false), g + 0x1483C),
            (look(Pointer::Hover, false), g + 0x14810),
            (look(Pointer::Pressed, false), g + 0x1483C),
        ]]);
        m.0.insert(g - 0x5450 + layout::MAIN_SCREEN_FORM_CURRENT_UNIT, 0x5000);
        m.0.insert(g + 0x1064C, 1);
        let art = of(&m, &sites, &control).unwrap();
        assert_eq!(art.state, look(Pointer::Hover, false));
        assert_eq!(art.pieces[0][0].1, g + 0x147E4);
        // The portrait only has its normal game drawing. It must not get
        // the neighbouring orb's hover or pressed art.
        assert_eq!(of(&m, &sites, &Control { id: control.id | 1, ..control }), None);
    }
}
