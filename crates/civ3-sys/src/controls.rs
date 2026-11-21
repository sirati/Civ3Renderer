//! The game's controls on screen that take the mouse: buttons, list
//! entries, lists, check boxes, context menu entries, the advisors' tabs,
//! text links (docs §16.11). Found the way the mouse hit test (0x609030)
//! finds them: the form tree from the roots (only the modal form's tree and
//! its companion while a modal form is open), forms hidden along their
//! parent chain left out.
//!
//! Reads go through [`Mem`] so the walk can be tested without the game.
//! Only the PCGames build is decoded.

use inproc::Ptr32;

use crate::Build;

mod setup;
pub use setup::{TitleLayout, title_layout};
pub mod advisors;
pub mod end_screens;
pub mod form_hit;
pub mod screens;
pub mod wheel;

/// Reads the game's memory (`None` where it cannot be read).
pub trait Mem {
    fn u32(&self, addr: u32) -> Option<u32>;
    fn u8(&self, addr: u32) -> Option<u8>;
    /// Semantic regions from the interface's file geometry, when the native
    /// pixel map is intentionally absent. None uses the game's bytes.
    fn index_regions(&self, _pcx: u32) -> Option<Vec<(u8, [i32; 4])>> { None }
    /// One guarded block read, used for the native setup screens' indexed
    /// hit maps. The live implementation avoids one protection check per pixel.
    fn read_bytes(&self, addr: u32, len: usize) -> Option<Vec<u8>> {
        (0..len).map(|i| self.u8(addr.checked_add(u32::try_from(i).ok()?)?)).collect()
    }
    fn i32(&self, addr: u32) -> Option<i32> {
        self.u32(addr).map(|v| v as i32)
    }
}

/// This process's memory, read guarded.
pub struct Live;

impl Mem for Live {
    fn u32(&self, addr: u32) -> Option<u32> {
        Ptr32::<u32>::new(addr).read().ok()
    }
    fn u8(&self, addr: u32) -> Option<u8> {
        Ptr32::<u8>::new(addr).read().ok()
    }
    fn read_bytes(&self, addr: u32, len: usize) -> Option<Vec<u8>> {
        Ptr32::<u8>::new(addr).read_array(len).ok()
    }
}

impl form_hit::HitMem for Live {
    fn write_u32(&self, addr: u32, value: u32) -> Option<()> {
        Ptr32::<u32>::new(addr).write(&value).ok()
    }
}

/// What a control is.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ControlKind {
    /// Clicked: buttons, check box items, context menu entries, tabs, links.
    Button,
    /// An entry of a list box.
    ListItem,
    /// A native slider, with normalized knob position and screen travel.
    Slider { track: [i32; 4], value: i32 },
    /// A list box whose entries scroll with the wheel: they scroll in
    /// `scroll` (screen x, y, w, h), `step` pixels a wheel message.
    List { scroll: [i32; 4], step: i32 },
}

/// A control in screen (game client) pixels: x, y, w, h.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub rect: [i32; 4],
    pub kind: ControlKind,
    /// The same while the control is there: its object's address, with the
    /// entry's index above it for parts of one.
    pub id: u64,
}

/// Where the game keeps its forms, and the classes it clicks (PCGames).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sites {
    /// `Base_Form*` roots (index 0 topmost) and their count.
    pub roots: u32,
    pub root_count: u32,
    /// The modal form and its companion (both 0 when none).
    pub modal: u32,
    pub companion: u32,
    /// The interface panel (`Main_GUI`, Main_Screen_Form+0x5450): its
    /// buttons are known by their places in it.
    pub main_gui: u32,
    /// The advisors' form (its six tabs are tested in its code, 0x49D6D0).
    pub advisor: u32,
}

pub fn sites(build: Build) -> Option<Sites> {
    match build {
        Build::PcGames => Some(Sites {
            roots: 0xCA_CB10,
            root_count: 0xCA_D3C4,
            modal: 0xCA_D384,
            companion: 0xCA_D380,
            main_gui: 0x9F_DB10,
            advisor: 0x9A_FD58,
        }),
        Build::Gog | Build::Steam => None,
    }
}

// `Base_Form` (docs §16.5; offsets from the object's start).
const STATUS1: u32 = 0x9C;
const STATUS2: u32 = 0xA0;
const PARENT: u32 = 0x130;
const LEFT: u32 = 0x1AC;
const CLIENT: u32 = 0x1BC;
const CLIP: u32 = 0x1DC;
const CHILDREN: u32 = 0x224;
const CHILD_COUNT: u32 = 0x22C;
/// Text links (e.g. the Civilopedia's): 32-byte entries, a RECT at +4, in
/// the form's own pixels; their count.
const LINKS: u32 = 0x378;
const LINK_COUNT: u32 = 0x380;

/// Status1: coordinates relative to the parent; relative to the parent's
/// frame; the form itself never takes the mouse; the wheel scrolls it (a
/// list); composited colour-keyed.
const RELATIVE: u32 = 0x20;
const FRAME_RELATIVE: u32 = 0x8000;
const NO_MOUSE: u32 = 0x2;
const WHEEL: u32 = 0x8;
const KEYED: u32 = 0x100_0000;

/// Controls activated by a click: `Button`, its city screen subclass,
/// their base, and the editable `TextBox` (focus/caret via native input).
const BUTTON_VTABLES: [u32; 4] = [0x66_56C0, 0x66_5F20, 0x67_1550, 0x66_7188];
/// `Context_Menu` and its subclass.
pub const CONTEXT_MENU_VTABLES: [u32; 2] = [0x67_1AC0, 0x66_B240];
/// The list a combo box opens (measured: the Science Advisor's "need
/// direction" tech box): a modal form of its own, composed over the dialog
/// holding the box (its frame; the entries are a list form, 0x666908,
/// inside it). The box itself is a form of vtable 0x672328.
pub const COMBO_LIST_VTABLES: [u32; 1] = [0x67_2160];
/// A check box group's form part (`CheckBox` + 0x20).
const CHECKBOX_VTABLE: u32 = 0x67_1C70;
/// Where every list box's draw thunk (vtable +0x58) ends.
const LIST_DRAW: u32 = 0x61_81E0;

/// Deepest form tree walked, most children of a form, entries of a list.
const MAX_DEPTH: u32 = 16;
const MAX_CHILDREN: u32 = 256;
const MAX_ITEMS: u32 = 512;

/// Ids of parts of a control: the control's address above, the part
/// below, tagged by kind.
fn part_id(object: u32, tag: u32, index: u32) -> u64 {
    (object as u64) << 32 | (tag | index) as u64
}
const TAG_ITEM: u32 = 0;
const TAG_LINK: u32 = 0x1000_0000;
pub(crate) const TAG_MENU: u32 = 0x2000_0000;
/// Regions clicked by Main_GUI's own mouse handler, with no Button form.
pub(crate) const TAG_STATUS: u32 = 0x3000_0000;
pub(crate) const STATUS_ORB: u32 = 0;
pub(crate) const STATUS_UNIT_PICTURE: u32 = 1;
pub(crate) const TAG_CHECK: u32 = 0x4000_0000;
pub(crate) const TAG_TAB: u32 = 0x8000_0000;
pub(crate) const TAG_CHOICE: u32 = 0x0800_0000;

fn rect_at(mem: &dyn Mem, at: u32) -> Option<[i32; 4]> {
    Some([mem.i32(at)?, mem.i32(at + 4)?, mem.i32(at + 8)?, mem.i32(at + 12)?])
}

/// x, y, w, h of the intersection (empty: w or h 0).
fn intersect([ax, ay, aw, ah]: [i32; 4], [bx, by, bw, bh]: [i32; 4]) -> [i32; 4] {
    let (x0, y0) = (ax.max(bx), ay.max(by));
    let (x1, y1) = ((ax + aw).min(bx + bw), (ay + ah).min(by + bh));
    [x0, y0, (x1 - x0).max(0), (y1 - y0).max(0)]
}

fn empty(r: [i32; 4]) -> bool {
    r[2] <= 0 || r[3] <= 0
}

fn contains([x, y, w, h]: [i32; 4], (px, py): (i32, i32)) -> bool {
    px >= x && py >= y && px < x + w && py < y + h
}

/// `Base_Form::local_to_screen(0, 0)` (0x604A00).
fn origin(mem: &dyn Mem, form: u32, depth: u32) -> Option<(i32, i32)> {
    let mut x = mem.i32(form + LEFT)? + mem.i32(form + CLIENT)?;
    let mut y = mem.i32(form + LEFT + 4)? + mem.i32(form + CLIENT + 4)?;
    let status = mem.u32(form + STATUS1)?;
    let parent = mem.u32(form + PARENT)?;
    if status & RELATIVE != 0 && parent != 0 && depth < MAX_DEPTH {
        let (px, py) = origin(mem, parent, depth + 1)?;
        x += px;
        y += py;
        if status & FRAME_RELATIVE != 0 {
            x -= mem.i32(parent + LEFT)?;
            y -= mem.i32(parent + LEFT + 4)?;
        }
    }
    Some((x, y))
}

/// A form as the mouse sees it.
#[derive(Copy, Clone, Debug)]
struct Form {
    addr: u32,
    vtable: u32,
    status1: u32,
    /// Screen position of its pixel (0, 0), its size.
    origin: (i32, i32),
    size: (i32, i32),
    /// Where it shows on the screen (its rectangle within its clip).
    shown: [i32; 4],
}

impl Form {
    fn read(mem: &dyn Mem, addr: u32) -> Option<Form> {
        let [l, t, r, b] = rect_at(mem, addr + LEFT)?;
        let origin = origin(mem, addr, 0)?;
        let size = (r - l, b - t);
        let [cl, ct, cr, cb] = rect_at(mem, addr + CLIP)?;
        let shown = intersect([origin.0, origin.1, size.0, size.1], [cl, ct, cr - cl, cb - ct]);
        Some(Form { addr, vtable: mem.u32(addr)?, status1: mem.u32(addr + STATUS1)?, origin, size, shown })
    }

    /// A rectangle in its own pixels on the screen, cut to where it shows.
    fn on_screen(&self, [x, y, w, h]: [i32; 4]) -> [i32; 4] {
        intersect([self.origin.0 + x, self.origin.1 + y, w, h], self.shown)
    }
}

/// Where form `addr` (a `Base_Form*`) shows on the screen (x, y, w, h).
pub fn form_rect(mem: &dyn Mem, addr: u32) -> Option<[i32; 4]> {
    Form::read(mem, addr).map(|f| [f.origin.0, f.origin.1, f.size.0, f.size.1])
}

fn visible(mem: &dyn Mem, form: u32) -> bool {
    mem.u32(form + STATUS2).is_some_and(|s| s & 1 != 0)
}

/// The game's clickable controls now, topmost first. Controls covered by
/// an opaque form above them (another root) are left out.
pub fn controls(mem: &dyn Mem, sites: &Sites) -> Vec<Control> {
    let modal = mem.u32(sites.modal).unwrap_or(0);
    let roots: Vec<u32> = if modal != 0 {
        // Only the modal form's tree and its companion take the mouse.
        [modal, mem.u32(sites.companion).unwrap_or(0)].into_iter().filter(|&f| f != 0).collect()
    } else {
        let n = mem.u32(sites.root_count).unwrap_or(0).min(MAX_CHILDREN);
        (0..n).filter_map(|i| mem.u32(sites.roots + 4 * i)).filter(|&f| f != 0).collect()
    };
    let known = known_buttons(sites);
    let mut out = Vec::new();
    // Opaque roots above: what they cover does not take the mouse.
    let mut covers: Vec<[i32; 4]> = Vec::new();
    for root in roots {
        let mut found = Vec::new();
        walk(mem, sites, &known, root, 0, &mut found);
        out.extend(found.into_iter().filter(|c: &Control| {
            let [x, y, w, h] = c.rect;
            !covers.iter().any(|&r| contains(r, (x + w / 2, y + h / 2)))
        }));
        if let Some(f) = Form::read(mem, root).filter(|f| visible(mem, root) && f.status1 & KEYED == 0) {
            covers.push(f.shown);
        }
    }
    out
}

/// One form of the tree the mouse sees, for tracing ([`form_tree`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormLine {
    pub depth: u32,
    pub addr: u32,
    pub vtable: u32,
    pub status1: u32,
    pub visible: bool,
    /// x, y, w, h on the screen (within its clip).
    pub shown: [i32; 4],
    pub children: u32,
    /// As a list box: its parts, entry count, first shown, shown, and
    /// whether its layout reads sanely.
    pub list: Option<(ListParts, i32, i32, i32, bool)>,
    /// As a control of a kind by a type field: where its draw goes.
    pub typed: Option<u32>,
}

/// Every form the walk of [`controls`] visits (hidden ones too, not their
/// children), depth first: what a trace shows to find controls it misses.
pub fn form_tree(mem: &dyn Mem, sites: &Sites) -> Vec<FormLine> {
    let modal = mem.u32(sites.modal).unwrap_or(0);
    let roots: Vec<u32> = if modal != 0 {
        [modal, mem.u32(sites.companion).unwrap_or(0)].into_iter().filter(|&f| f != 0).collect()
    } else {
        let n = mem.u32(sites.root_count).unwrap_or(0).min(MAX_CHILDREN);
        (0..n).filter_map(|i| mem.u32(sites.roots + 4 * i)).filter(|&f| f != 0).collect()
    };
    fn visit(mem: &dyn Mem, addr: u32, depth: u32, out: &mut Vec<FormLine>) {
        if depth > MAX_DEPTH || out.len() > 4096 {
            return;
        }
        let Some(form) = Form::read(mem, addr) else { return };
        let visible = visible(mem, addr);
        let n = mem.u32(addr + CHILD_COUNT).unwrap_or(0).min(MAX_CHILDREN);
        let list = list_parts(mem, &form).map(|p| {
            let count = mem.i32(p.items + L_COUNT).unwrap_or(-1);
            let first = mem.i32(p.list + G_FIRST).unwrap_or(-1);
            let shown = mem.i32(p.list + G_SHOWN).unwrap_or(-1);
            let sane = ItemLayout::read(mem, p.items).is_some_and(|l| l.sane());
            (p, count, first, shown, sane)
        });
        let typed = typed_draw(mem, &form).map(|(_, target)| target);
        out.push(FormLine { depth, addr, vtable: form.vtable, status1: form.status1, visible, shown: form.shown, children: n, list, typed });
        if !visible {
            return;
        }
        let children = mem.u32(addr + CHILDREN).unwrap_or(0);
        if children != 0 {
            for i in 0..n {
                if let Some(c) = mem.u32(children + 4 * i).filter(|&c| c != 0 && c != addr) {
                    visit(mem, c, depth + 1, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    for root in roots {
        visit(mem, root, 0, &mut out);
    }
    out
}

/// The interface panel's buttons by place: Advisors, Menu, Geography,
/// Civilopedia, the 8 map navigator buttons, the 7 right panel buttons, the
/// 42 unit command buttons (`Button` 0x6D4 bytes, `Command_Button` 0x6DC).
fn known_buttons(sites: &Sites) -> Vec<u32> {
    let g = sites.main_gui;
    let mut out = vec![g + 0x845C, g + 0x8B30, g + 0x9204, g + 0x98D8];
    out.extend((0..8).map(|i| g + 0x9FAC + i * 0x6D4));
    out.extend((0..7).map(|i| g + 0xD64C + i * 0x6D4));
    out.extend((0..42).map(|i| g + 0x1_6CE4 + i * 0x6DC));
    out
}

fn walk(mem: &dyn Mem, sites: &Sites, known: &[u32], addr: u32, depth: u32, out: &mut Vec<Control>) {
    if depth > MAX_DEPTH || !visible(mem, addr) {
        return;
    }
    let Some(form) = Form::read(mem, addr) else { return };
    // Children first: the hit test tries them before their parent, from
    // the first, the first hit winning. What an opaque child (one taking
    // the mouse on all of its rectangle) covers of a later one does not
    // take the mouse: e.g. `Main_GUI`'s buttons under the victory screen
    // or the ranking, earlier children of the main screen form.
    let n = mem.u32(addr + CHILD_COUNT).unwrap_or(0).min(MAX_CHILDREN);
    let children = mem.u32(addr + CHILDREN).unwrap_or(0);
    if children != 0 {
        let mut covers: Vec<[i32; 4]> = Vec::new();
        for i in 0..n {
            if let Some(c) = mem.u32(children + 4 * i).filter(|&c| c != 0 && c != addr) {
                let mut found = Vec::new();
                walk(mem, sites, known, c, depth + 1, &mut found);
                out.extend(found.into_iter().filter(|c| {
                    let [x, y, w, h] = c.rect;
                    !covers.iter().any(|&r| contains(r, (x + w / 2, y + h / 2)))
                }));
                if let Some(f) = Form::read(mem, c).filter(|f| visible(mem, c) && f.status1 & (KEYED | NO_MOUSE) == 0) {
                    covers.push(f.shown);
                }
            }
        }
    }
    if empty(form.shown) {
        return;
    }
    if form.status1 & NO_MOUSE != 0 {
        return;
    }
    links(mem, &form, out);
    if addr == sites.advisor {
        tabs(&form, out);
    }
    if addr == sites.main_gui {
        status_panel(mem, &form, out);
    }
    setup::controls(mem, &form, out);
    advisors::controls(mem, &form, out);
    screens::controls(mem, &form, out);
    end_screens::controls(mem, &form, out);
    if let Some(slider) = crate::sliders::Slider::read(mem, addr) {
        let [x, y, w, h] = slider.track;
        out.push(Control {
            rect: form.shown,
            kind: ControlKind::Slider { track: [form.origin.0 + x, form.origin.1 + y, w, h], value: slider.normalized() },
            id: addr as u64,
        });
    } else if BUTTON_VTABLES.contains(&form.vtable) || known.contains(&addr) {
        out.push(Control { rect: form.shown, kind: ControlKind::Button, id: addr as u64 });
    } else if CONTEXT_MENU_VTABLES.contains(&form.vtable) {
        context_menu(mem, &form, out);
    } else if form.vtable == CHECKBOX_VTABLE {
        check_boxes(mem, &form, out);
    } else if let Some(list) = list_parts(mem, &form) {
        list_box(mem, &form, list, out);
    } else {
        choice_group(mem, &form, out);
    }
}

// Main_GUI::handle_click_in_status_panel (PCGames 0x555F30) tests these
// regions itself; neither is a Button in the form tree. Its status orb
// skips the active unit, or ends a ready turn. The unit picture centres
// the map on the active unit. Keep their native coordinates and let the
// normal mouse dispatch run that handler, including C3X's wrapper.
const MAIN_GUI_IN_SCREEN: u32 = 0x5450;
const TURN_END_FLAG: u32 = 0x2E198;
const GUI_ENABLED: u32 = 0x585;
const UNIT_STATUS_RECT: u32 = 0x13FE8;

fn status_panel(mem: &dyn Mem, form: &Form, out: &mut Vec<Control>) {
    if !mem.u8(form.addr + GUI_ENABLED).is_some_and(|v| v != 0) {
        return;
    }
    let Some(main) = form.addr.checked_sub(MAIN_GUI_IN_SCREEN) else { return };
    let unit = mem.u32(main + crate::layout::MAIN_SCREEN_FORM_CURRENT_UNIT).unwrap_or(0);
    let end_turn = mem.u8(main + TURN_END_FLAG).is_some_and(|v| v != 0);
    if unit == 0 && !end_turn {
        return;
    }
    let Some([l, t, r, b]) = rect_at(mem, form.addr + UNIT_STATUS_RECT) else { return };
    if r <= l || b <= t {
        return;
    }
    let middle = t + (b - t) / 2;
    // The native tests include x=L..L+47, y=middle..middle+28, but
    // exactly y=middle goes to the upper status row in its outer branch.
    let orb = form.on_screen([l, middle + 1, 48, 28]);
    if !empty(orb) {
        out.push(Control { rect: orb, kind: ControlKind::Button, id: part_id(form.addr, TAG_STATUS, STATUS_ORB) });
    }
    if unit != 0 {
        // Beyond the orb, the lower half's native action centres the
        // current unit. The upper half contains individual stack icons
        // and is deliberately not one large, misleading target.
        let picture = form.on_screen([l + 48, middle + 1, r - l - 47, b - middle]);
        if !empty(picture) {
            out.push(Control { rect: picture, kind: ControlKind::Button, id: part_id(form.addr, TAG_STATUS, STATUS_UNIT_PICTURE) });
        }
    }
}

/// Native hit strip of the unit/cargo icons in Main_GUI's upper status
/// panel (0x556220). The individual icons are identified in the captured
/// canvas, because their artwork may use scenario-specific icon indices.
/// Only the main HUD's own published actions permit this region.
pub fn status_unit_row(mem: &dyn Mem, sites: &Sites, controls: &[Control]) -> Option<(u32, [i32; 4])> {
    if !controls.iter().any(|c| c.id == part_id(sites.main_gui, TAG_STATUS, STATUS_UNIT_PICTURE)) {
        return None;
    }
    let form = Form::read(mem, sites.main_gui)?;
    let [l, t, r, b] = rect_at(mem, sites.main_gui + UNIT_STATUS_RECT)?;
    // All units_32 images are sliced to the same cell dimensions. Refuse
    // another format instead of guessing native icon geometry.
    let icons = mem.u32(0xB4_2184).filter(|&p| p != 0)?;
    let (w, h) = (mem.i32(icons + crate::layout::SPRITE_WIDTH)?, mem.i32(icons + crate::layout::SPRITE_HEIGHT)?);
    if w != 32 || h != 32 { return None; }
    let y = t + (b - t) / 4 - h / 2 - 20;
    let row = form.on_screen([l + 27, y, r - l - 53, h - 10]);
    (!empty(row)).then_some((sites.main_gui + crate::layout::BASE_FORM_DATA_CANVAS + 4, row))
}

/// The text links of a form (32-byte entries, a RECT at +4). The game
/// tests these last first (0x60F170), including links crossing its clip.
fn links(mem: &dyn Mem, form: &Form, out: &mut Vec<Control>) {
    let n = mem.u32(form.addr + LINK_COUNT).unwrap_or(0);
    let Some(base) = mem.u32(form.addr + LINKS).filter(|&b| b != 0 && n > 0 && n <= MAX_ITEMS) else { return };
    for i in (0..n).rev() {
        let Some(at) = base.checked_add(32 * i).and_then(|a| a.checked_add(4)).filter(|a| *a <= u32::MAX - 12) else { continue };
        let Some([l, t, r, b]) = rect_at(mem, at) else { continue };
        let (Some(w), Some(h)) = (r.checked_sub(l), b.checked_sub(t)) else { continue };
        // Long or partially scrolled links can exceed the form's size.
        // Their visible intersection, rather than their uncut width,
        // decides whether they can be selected.
        if w <= 0 || h <= 0 || l.checked_add(form.origin.0).is_none() || t.checked_add(form.origin.1).is_none() || r.checked_add(form.origin.0).is_none() || b.checked_add(form.origin.1).is_none() {
            continue;
        }
        let rect = form.on_screen([l, t, w, h]);
        if !empty(rect) {
            out.push(Control { rect, kind: ControlKind::Button, id: part_id(form.addr, TAG_LINK, i) });
        }
    }
}

impl Control {
    /// A semantic row or text link whose hit rectangle may include blank
    /// space after its visible icon or text. Decorations are not controls.
    pub fn content_row(&self) -> bool {
        matches!(self.kind, ControlKind::ListItem) ||
            (self.id >> 32 != 0 && matches!(self.id as u32 & 0xF800_0000, TAG_LINK | TAG_MENU | TAG_CHOICE)) ||
            advisors::cell_kind(self.id).is_some_and(|k| k != advisors::CELL_LIST)
    }

    /// Its only action is on the right button, so a button-snapping left
    /// click is sent as a right click: the domestic advisor's production
    /// cells (the quick build chooser has no left click equivalent).
    pub fn alt_right_click(&self) -> bool {
        advisors::cell_kind(self.id) == Some(advisors::CELL_PRODUCTION)
    }

    /// A box of a check box group: its hit cell is the box alone, its
    /// label is drawn beside it.
    pub fn check_box(&self) -> bool {
        self.id >> 32 != 0 && self.id as u32 & 0xF800_0000 == TAG_CHECK
    }

    /// A link from the form's rich-text hit table.
    pub fn text_link(&self) -> bool {
        self.id >> 32 != 0 && self.id as u32 & 0xF800_0000 == TAG_LINK
    }
}

/// The advisors' six tabs down their left edge (x 4..=0x44, y 242 + 62k
/// ..= 304 + 62k in the form's pixels, 0x49D6D0).
pub fn tab_rects() -> [[i32; 4]; 6] {
    std::array::from_fn(|k| [4, 242 + 62 * k as i32, 0x44 - 4 + 1, 62])
}

fn tabs(form: &Form, out: &mut Vec<Control>) {
    for (k, r) in tab_rects().into_iter().enumerate() {
        let rect = form.on_screen(r);
        if !empty(rect) {
            out.push(Control { rect, kind: ControlKind::Button, id: part_id(form.addr, TAG_TAB, k as u32) });
        }
    }
}

// `Context_Menu` (docs §16.11): items, their count and height.
const MENU_ITEMS: u32 = 0x578;
const MENU_ITEM_BYTES: u32 = 28;
const MENU_ITEM_STATUS: u32 = 0xC;
const MENU_COUNT: u32 = 0x2184;
const MENU_ITEM_HEIGHT: u32 = 0x218C;
/// The frame border width used by both drawing and mouse hit testing.
const MENU_BORDER: u32 = 0x73_0318;
const DEFAULT_FONT: u32 = 0xCC_38B8;

/// A context menu's rows: per shown item (status bit 0), its row
/// rectangle in the menu's pixels, except separators (bit 2). Disabled
/// items (bit 1) still take hover. `item_height` includes the font's line
/// height, and `border` is the game's current frame width.
pub fn menu_rows(statuses: &[u32], item_height: i32, size: (i32, i32), border: i32) -> Vec<(usize, [i32; 4])> {
    let shown: Vec<usize> = statuses.iter().enumerate().filter(|(_, s)| *s & 1 != 0).map(|(i, _)| i).collect();
    if shown.is_empty() {
        return vec![];
    }
    let h = if item_height > 0 { item_height } else { (size.1 - 2 * border) / shown.len() as i32 };
    if h <= 0 {
        return vec![];
    }
    shown
        .iter()
        .enumerate()
        .filter(|(_, i)| statuses[**i] & 4 == 0)
        .map(|(row, &i)| (i, [0, border + row as i32 * h, size.0, h]))
        .collect()
}

/// Canvas::line_height (0x5FE780), used by Context_Menu's drawing and
/// hit testing (0x61B250, 0x61A9C0). A larger font expands every row.
fn canvas_line_height(mem: &dyn Mem, canvas: u32) -> Option<i32> {
    let font = mem.u32(canvas + 0x5C).filter(|&f| f != 0).or_else(|| mem.u32(DEFAULT_FONT).filter(|&f| f != 0))?;
    let height = mem.i32(font + 8)?;
    if height < 0 { mem.i32(font + 0xC) } else { height.checked_add(mem.i32(font + 0x10)?) }
}

fn context_menu(mem: &dyn Mem, form: &Form, out: &mut Vec<Control>) {
    let n = mem.u32(form.addr + MENU_COUNT).unwrap_or(0).min(MAX_ITEMS);
    let statuses: Vec<u32> = (0..n).map(|i| mem.u32(form.addr + MENU_ITEMS + i * MENU_ITEM_BYTES + MENU_ITEM_STATUS).unwrap_or(0)).collect();
    let canvas = form.addr + crate::layout::BASE_FORM_DATA_CANVAS + 4;
    let height = mem.i32(form.addr + MENU_ITEM_HEIGHT).unwrap_or(0).max(canvas_line_height(mem, canvas).unwrap_or(0));
    let border = mem.i32(MENU_BORDER).filter(|b| (0..128).contains(b)).unwrap_or(2);
    for (i, r) in menu_rows(&statuses, height, form.size, border) {
        let rect = form.on_screen(r);
        if !empty(rect) {
            out.push(Control { rect, kind: ControlKind::Button, id: part_id(form.addr, TAG_MENU, i as u32) });
        }
    }
}

/// How a list (or check box group) lays its items out (its item-list
/// part, docs §16.11).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemLayout {
    pub x0: i32,
    pub y0: i32,
    pub gap: i32,
    /// 1: one column of items of different heights (lines).
    pub variable: bool,
    pub rows: i32,
    pub width: i32,
    pub height: i32,
}

// The item-list part's fields.
const L_Y0: u32 = 0x3C;
const L_X0: u32 = 0x44;
const L_GAP: u32 = 0x48;
const L_LAYOUT: u32 = 0x50;
const L_ROWS: u32 = 0x5C;
const L_WIDTH: u32 = 0x68;
const L_HEIGHT: u32 = 0x6C;
const L_FIRST_NODE: u32 = 0xC8;
const L_COUNT: u32 = 0xD0;
// The list object's: the first item shown, how many are shown.
const G_FIRST: u32 = 0x2C;
const G_SHOWN: u32 = 0x30;

impl ItemLayout {
    fn read(mem: &dyn Mem, l: u32) -> Option<ItemLayout> {
        Some(ItemLayout {
            x0: mem.i32(l + L_X0)?,
            y0: mem.i32(l + L_Y0)?,
            gap: mem.i32(l + L_GAP)?,
            variable: mem.i32(l + L_LAYOUT)? == 1,
            rows: mem.i32(l + L_ROWS)?,
            width: mem.i32(l + L_WIDTH)?,
            height: mem.i32(l + L_HEIGHT)?,
        })
    }

    fn sane(&self) -> bool {
        (1..=4096).contains(&self.width) && (1..=1024).contains(&self.height) && (0..=256).contains(&self.gap)
    }

    /// The rectangle of the `k`-th item shown (0x617470), in the list's
    /// pixels; `lines_before` / `lines`: lines of the items shown before it
    /// and its own (variable layout only).
    pub fn item(&self, k: i32, lines_before: i32, lines: i32) -> [i32; 4] {
        let step = self.height + self.gap;
        if self.variable {
            let top = self.y0 + step * lines_before;
            return [self.x0, top, self.width - 2 * self.x0, step * lines.max(1) - self.gap];
        }
        let rows = self.rows.max(1);
        let (col, row) = (k / rows, k % rows);
        [self.x0 + col * (self.width + self.gap), self.y0 + row * step, self.width - 2 * self.x0, self.height]
    }
}

/// A list box's parts: the list object G and its item-list part L (MSVC
/// virtual bases), when `form` is a list box's form part.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ListParts {
    pub list: u32,
    pub items: u32,
}

/// Recognised by its draw thunk (vtable +0x58): `sub ecx,[ecx-4]`,
/// maybe `sub ecx,k`, `jmp 0x6181E0`; then G = F − [F−4] − k − 0x64,
/// checked against G's virtual base table (F = G + 4 + vb[1]).
fn list_parts(mem: &dyn Mem, form: &Form) -> Option<ListParts> {
    if let Some((ecx, LIST_DRAW)) = typed_draw(mem, form) {
        // (A typed control that is a list box.)
        return list_at(mem, form, ecx.wrapping_sub(0x64));
    }
    let thunk = mem.u32(form.vtable + 0x58)?;
    let code: Vec<u8> = (0..11).map(|i| mem.u8(thunk + i)).collect::<Option<_>>()?;
    if code[..3] != [0x2B, 0x49, 0xFC] {
        return None;
    }
    let (k, jmp) = if code[3..5] == [0x83, 0xE9] { (code[5] as u32, 6) } else { (0, 3) };
    if code[jmp] != 0xE9 {
        return None;
    }
    let rel = u32::from_le_bytes(code[jmp + 1..jmp + 5].try_into().ok()?);
    if thunk.wrapping_add(jmp as u32 + 5).wrapping_add(rel) != LIST_DRAW {
        return None;
    }
    let f = form.addr;
    list_at(mem, form, f.wrapping_sub(mem.u32(f.wrapping_sub(4))?).wrapping_sub(k).wrapping_sub(0x64))
}

/// The list object G of a list box's form part, if G's virtual base table
/// points back at it (F = G + 4 + vb[1]); its item list at G + 4 + vb[2].
fn list_at(mem: &dyn Mem, form: &Form, g: u32) -> Option<ListParts> {
    let vb = mem.u32(g.wrapping_add(4))?;
    let (to_form, to_items) = (mem.u32(vb + 4)?, mem.u32(vb + 8)?);
    (g.wrapping_add(4).wrapping_add(to_form) == form.addr).then(|| ListParts { list: g, items: g.wrapping_add(4).wrapping_add(to_items) })
}

/// The draws of a choice group: a grid of items (0x615080 each), or rows
/// of text (0x61CC20 each; a popup's choices).
const CHOICE_DRAW: u32 = 0x61_56F0;
const LINES_DRAW: u32 = 0x61_D7C0;

/// A control of one of several kinds by a type field (class 0x666908, e.g.
/// a popup's choices): its draw thunk `sub ecx,[ecx-4]`, `jmp D`, where D
/// dispatches on the type at `[ecx-8]` through a jump table to
/// `add ecx,imm`, `jmp T` (0x619570: type 2 a list box, 8 a choice group).
/// Where `ecx` ends up and the draw it jumps to.
fn typed_draw(mem: &dyn Mem, form: &Form) -> Option<(u32, u32)> {
    let thunk = mem.u32(form.vtable + 0x58)?;
    if [mem.u8(thunk)?, mem.u8(thunk + 1)?, mem.u8(thunk + 2)?] != [0x2B, 0x49, 0xFC] {
        return None;
    }
    let (subtract, jmp) = match (mem.u8(thunk + 3)?, mem.u8(thunk + 4)?) {
        (0x83, 0xE9) => (mem.u8(thunk + 5)? as i8 as i32 as u32, 6),
        (0x81, 0xE9) => (mem.u32(thunk + 5)?, 9),
        _ => (0, 3),
    };
    if mem.u8(thunk + jmp)? != 0xE9 { return None; }
    let d = thunk.wrapping_add(jmp + 5).wrapping_add(mem.u32(thunk + jmp + 1)?);
    let ecx = form.addr.wrapping_sub(mem.u32(form.addr.wrapping_sub(4))?).wrapping_sub(subtract);
    // Direct choice classes use the same virtual-base thunk without the
    // type dispatcher: icon grids (0x671730) and radio rows (0x666778).
    if matches!(d, CHOICE_DRAW | LINES_DRAW) {
        return Some((ecx, d));
    }
    // mov eax,[ecx-8]; dec eax; cmp eax,n; ja; xor edx,edx; mov dl,[eax+idx]; jmp [edx*4+table]
    let head: Vec<u8> = (0..24).map(|i| mem.u8(d + i)).collect::<Option<_>>()?;
    if head[..5] != [0x8B, 0x41, 0xF8, 0x48, 0x83] || head[5] != 0xF8 || head[7] != 0x77 || head[9..13] != [0x33, 0xD2, 0x8A, 0x90] || head[17..20] != [0xFF, 0x24, 0x95] {
        return None;
    }
    let types = head[6] as u32 + 1;
    let idx = u32::from_le_bytes(head[13..17].try_into().ok()?);
    let table = u32::from_le_bytes(head[20..24].try_into().ok()?);
    let t = mem.u32(ecx.wrapping_sub(8))?;
    if t == 0 || t > types {
        return None;
    }
    let entry = mem.u32(table + 4 * mem.u8(idx + t - 1)? as u32)?;
    let at: Vec<u8> = (0..11).map(|i| mem.u8(entry + i)).collect::<Option<_>>()?;
    let (add, jmp) = match at[..2] {
        [0x81, 0xC1] => (u32::from_le_bytes(at[2..6].try_into().ok()?), 6),
        [0x83, 0xC1] => (at[2] as i8 as i32 as u32, 3),
        _ => return None,
    };
    if at[jmp] != 0xE9 {
        return None;
    }
    let target = entry.wrapping_add(jmp as u32 + 5).wrapping_add(u32::from_le_bytes(at[jmp + 1..jmp + 5].try_into().ok()?));
    Some((ecx.wrapping_add(add), target))
}

/// A choice group (a typed control): a grid of items drawn with 0x615080
/// (type 8: object B = `ecx` − 0xE0; item `k` at column `k / rows`: x
/// `col·(W+gap)`, y `row·H + gap/2`, W×H), or rows of text drawn with
/// 0x61CC20 (type 16, a popup's choices: B = `ecx` − 0x28; each row as high
/// as its lines, at least H: the form's height shared among the rows by
/// their lines). Its form part at B + vb[1], its item list at B + vb[2]
/// (vb = [B]).
fn choice_group(mem: &dyn Mem, form: &Form, out: &mut Vec<Control>) -> bool {
    let (b, rows_of_text) = match typed_draw(mem, form) {
        Some((ecx, CHOICE_DRAW)) => (ecx.wrapping_sub(0xE0), false),
        Some((ecx, LINES_DRAW)) => (ecx.wrapping_sub(0x28), true),
        _ => return false,
    };
    let Some(vb) = mem.u32(b) else { return false };
    let (Some(to_form), Some(to_items)) = (mem.u32(vb + 4), mem.u32(vb + 8)) else { return false };
    if b.wrapping_add(to_form) != form.addr {
        return false;
    }
    let items = b.wrapping_add(to_items);
    let Some(l) = ItemLayout::read(mem, items).filter(ItemLayout::sane) else { return false };
    let count = mem.i32(items + L_COUNT).unwrap_or(0).clamp(0, 64);
    let rects: Vec<[i32; 4]> = if rows_of_text {
        // Lines per row: the item nodes (+8 data, whose +8 is its lines; +0xC
        // the next node).
        let mut node = mem.u32(items + L_FIRST_NODE).unwrap_or(0);
        let mut lines = Vec::new();
        for _ in 0..count {
            let n = if node == 0 { 1 } else { mem.u32(node + 8).and_then(|d| mem.i32(d + 8)).unwrap_or(1) };
            lines.push(n.clamp(1, 16));
            node = if node == 0 { 0 } else { mem.u32(node + 0xC).unwrap_or(0) };
        }
        let total: i32 = lines.iter().sum::<i32>().max(1);
        let mut before = 0;
        lines
            .iter()
            .map(|n| {
                let (top, bottom) = (form.size.1 * before / total, form.size.1 * (before + n) / total);
                before += n;
                [0, top, form.size.0, bottom - top]
            })
            .collect()
    } else {
        let rows = l.rows.max(1);
        (0..count)
            .map(|k| {
                let (col, row) = (k / rows, k % rows);
                [col * (l.width + l.gap), row * l.height + l.gap / 2, l.width, l.height]
            })
            .filter(|r| r[0] + r[2] <= form.size.0 + 1 && r[1] + r[3] <= form.size.1 + 1)
            .collect()
    };
    for (k, r) in rects.into_iter().enumerate() {
        let rect = form.on_screen(r);
        if !empty(rect) {
            out.push(Control { rect, kind: ControlKind::Button, id: part_id(form.addr, TAG_CHOICE, k as u32) });
        }
    }
    true
}

fn list_box(mem: &dyn Mem, form: &Form, parts: ListParts, out: &mut Vec<Control>) {
    let Some(layout) = ItemLayout::read(mem, parts.items).filter(ItemLayout::sane) else { return };
    let count = mem.i32(parts.items + L_COUNT).unwrap_or(0).clamp(0, MAX_ITEMS as i32);
    let first = mem.i32(parts.list + G_FIRST).unwrap_or(0).clamp(0, count);
    let shown = mem.i32(parts.list + G_SHOWN).unwrap_or(0).clamp(0, count - first);
    // Lines per item shown (variable layout): from the item nodes.
    let lines: Vec<i32> = if layout.variable { item_lines(mem, parts.items, first, shown) } else { vec![1; shown as usize] };
    let mut before = 0;
    let mut content: Option<[i32; 4]> = None;
    for k in 0..shown {
        let own = lines.get(k as usize).copied().unwrap_or(1);
        let r = layout.item(k, before, own);
        before += own;
        let rect = form.on_screen(r);
        if !empty(rect) {
            content = Some(match content {
                None => rect,
                Some([x, y, w, h]) => {
                    let (left, top) = (x.min(rect[0]), y.min(rect[1]));
                    [left, top, (x + w).max(rect[0] + rect[2]) - left, (y + h).max(rect[1] + rect[3]) - top]
                }
            });
            out.push(Control { rect, kind: ControlKind::ListItem, id: part_id(form.addr, TAG_ITEM, (first + k) as u32) });
        }
    }
    // Scrolled with the wheel (one item a message), if it has more items
    // than it shows.
    if form.status1 & WHEEL != 0 && count > shown {
        let step = layout.height + layout.gap;
        // Child scrollbar arrows and frames are outside the item layout.
        // Keep the form's full wheel hit area, but animate only its content.
        let scroll = content.map(|[x, _, w, _]| [x, form.shown[1], w, form.shown[3]]).unwrap_or(form.shown);
        out.push(Control { rect: form.shown, kind: ControlKind::List { scroll, step }, id: form.addr as u64 });
    }
}

/// Lines of the items `first..first + shown` (item nodes: +8 data, whose
/// +0xC is its line count; +0xC the next node).
fn item_lines(mem: &dyn Mem, items: u32, first: i32, shown: i32) -> Vec<i32> {
    let mut node = mem.u32(items + L_FIRST_NODE).unwrap_or(0);
    for _ in 0..first {
        if node == 0 {
            break;
        }
        node = mem.u32(node + 0xC).unwrap_or(0);
    }
    let mut out = Vec::new();
    for _ in 0..shown {
        if node == 0 {
            break;
        }
        let lines = mem.u32(node + 8).and_then(|d| mem.i32(d + 0xC)).unwrap_or(1);
        out.push(lines.clamp(1, 64));
        node = mem.u32(node + 0xC).unwrap_or(0);
    }
    out
}

/// A check box group (the form part of a `CheckBox`, its item list 0x578
/// further on): an item per box, laid out in columns of `rows`; the whole
/// group if its layout does not read sanely.
fn check_boxes(mem: &dyn Mem, form: &Form, out: &mut Vec<Control>) {
    let items = form.addr + 0x578;
    let layout = ItemLayout::read(mem, items).filter(ItemLayout::sane);
    let count = mem.i32(items + L_COUNT).unwrap_or(0);
    let boxes: Vec<[i32; 4]> = match layout {
        Some(l) if (1..=64).contains(&count) => {
            let rows = l.rows.max(1);
            (0..count).map(|i| [(i / rows) * (l.width + l.gap), (i % rows) * (l.height + l.gap), l.width, l.height]).collect()
        }
        _ => vec![],
    };
    let fits = !boxes.is_empty() && boxes.iter().all(|r| r[0] + r[2] <= form.size.0 + 1 && r[1] + r[3] <= form.size.1 + 1);
    if !fits {
        out.push(Control { rect: form.shown, kind: ControlKind::Button, id: form.addr as u64 });
        return;
    }
    for (i, r) in boxes.into_iter().enumerate() {
        let rect = form.on_screen(r);
        if !empty(rect) {
            out.push(Control { rect, kind: ControlKind::Button, id: part_id(form.addr, TAG_CHECK, i as u32) });
        }
    }
}

#[cfg(test)]
mod tests;

/// A popup's choice rows (a choice group of rows of text, type 16): `c`,
/// its own part B (the bullet sprites, the hover and selected text
/// colours), `items` its item list.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BulletList {
    pub c: u32,
    pub items: u32,
}

/// The rows of text of the choice group whose form part is at `f`, if it
/// is one (see [`choice_group`]).
pub fn bullet_list(mem: &dyn Mem, f: u32) -> Option<BulletList> {
    let form = Form::read(mem, f)?;
    let (ecx, draw) = typed_draw(mem, &form)?;
    if draw != LINES_DRAW {
        return None;
    }
    let b = ecx.wrapping_sub(0x28);
    let vb = mem.u32(b)?;
    (b.wrapping_add(mem.u32(vb + 4)?) == f).then(|| BulletList { c: b, items: b.wrapping_add(mem.u32(vb + 8).unwrap_or(0)) })
}
