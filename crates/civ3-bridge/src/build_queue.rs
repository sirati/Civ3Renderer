//! The city screen's build queue, reordered by dragging in the viewer
//! (`scene::Interface::build_queue`, `scene::Input::BuildQueue`).
//!
//! The publisher names the queue list's entries the screen shows (the
//! game's own list rows, `civ3_sys::build_queue::queue_list_form`), each
//! with a handle at its left (an interactable of its own, so that button
//! snapping can point at it), and the production box. A drop comes back
//! as an edit checked against the queue it was made for (its stamp) and is
//! done on the game's thread, between its messages, as the game's own
//! calls (`civ3_sys::build_queue::plan`): `City::set_production` for a
//! swap (with the game's confirmation of a switch; declined, nothing else
//! happens), then `City_Form::edit_queue` removals and appends. Only for
//! the player's own city, and not in a network game (there the queue's
//! edits go to the host, and a reorder of several would race it).

use civ3_sys::build_queue::{self as q, Drop, Order};
use civ3_sys::controls::Mem;
use scene::{BuildQueue, BuildQueueRow, Interactable, InteractableKind};

/// How wide a row's handle is (the viewer draws it there and moves the
/// row's text right of it).
pub const GRIP_WIDTH: i32 = 14;

/// Tags a handle's interactable id among the list's (`controls::part_id`
/// tags use the top nibble; entries are tag 0).
const GRIP_TAG: u64 = 0x7F00_0000;

/// The queue's rows among `interactables`: the entries of the list whose
/// form part is `form`, the first `count` of them (the one after them is
/// the game's empty "next" slot, not an entry).
pub fn rows(interactables: &[Interactable], form: u32, count: usize) -> Vec<BuildQueueRow> {
    let mut rows: Vec<BuildQueueRow> = interactables
        .iter()
        .filter(|i| i.kind == InteractableKind::ListItem && i.id >> 32 == u64::from(form) && i.rect[2] > GRIP_WIDTH && i.rect[3] > 0)
        .filter_map(|i| {
            let slot = i.id & 0x0FFF_FFFF;
            (slot < count as u64).then(|| {
                let [x, y, _, h] = i.rect;
                BuildQueueRow { slot: slot as u8, rect: i.rect, grip: [x, y, GRIP_WIDTH, h], grip_id: u64::from(form) << 32 | GRIP_TAG | slot }
            })
        })
        .collect();
    rows.sort_by_key(|r| r.slot);
    rows.dedup_by_key(|r| r.slot);
    rows
}

/// What the publisher sends: the open city screen's queue where it is
/// shown (`current`: the production box), and the handles to point at.
/// `None` without one, for another player's city, or in a network game.
pub fn publish(mem: &dyn Mem, interactables: &[Interactable], current: Option<[i32; 4]>) -> Option<(BuildQueue, Vec<Interactable>)> {
    let current = current?;
    let queue = q::Queue::read(mem)?;
    if q::network_game(mem) || q::player(mem) != Some(queue.civ) || queue.entries.is_empty() {
        return None;
    }
    let rows = rows(interactables, q::queue_list_form(mem)?, queue.entries.len());
    if rows.is_empty() {
        return None;
    }
    let grips = rows
        .iter()
        .map(|r| Interactable { region: None, alt_right_click: false, anchor: None, rect: r.grip, kind: InteractableKind::Button, id: r.grip_id })
        .collect();
    Some((BuildQueue { stamp: queue.stamp(), current, rows }, grips))
}

/// The drop an `Input::BuildQueue` names.
pub fn drop_of(to: u8) -> Drop {
    if to == scene::BUILD_QUEUE_CURRENT { Drop::Current } else { Drop::Before(usize::from(to)) }
}

/// The game's calls for the edit, against `queue` as it is now: `None` if
/// it is not for this queue (`stamp`) or changes nothing.
pub fn planned(queue: &q::Queue, stamp: u32, from: u8, to: u8) -> Option<q::Plan> {
    (queue.stamp() == stamp).then(|| q::plan(queue.current, &queue.entries, usize::from(from), drop_of(to))).flatten()
}

/// A short form for the log.
fn name(o: Order) -> String {
    match o.kind {
        q::IMPROVEMENT => format!("improvement {}", o.id),
        q::UNIT => format!("unit {}", o.id),
        k => format!("order {k}/{}", o.id),
    }
}

#[cfg(windows)]
pub use live::{handle_message, install, post};

#[cfg(windows)]
mod live {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};

    use civ3_sys::build_queue::{self as q, Edit};
    use civ3_sys::{Build, Civ3};
    use inproc::message::Message;
    use inproc::window::Window;

    struct Live {
        message: u32,
        edit_queue: q::city_form_edit_queue::Function,
        set_production: q::city_set_production::Function,
    }
    static LIVE: OnceLock<Live> = OnceLock::new();
    /// Edits posted, taken on the game's thread in order.
    static PENDING: Mutex<Vec<(u32, u8, u8)>> = Mutex::new(Vec::new());
    /// An edit is being made (the game may run a dialog's loop meanwhile).
    static RUNNING: AtomicBool = AtomicBool::new(false);

    pub fn install(game: Civ3) -> Result<(), String> {
        if game.build != Build::PcGames {
            return Err(format!("the build queue is not decoded for {:?}", game.build));
        }
        let sites = q::sites(game.build).ok_or("no build queue sites")?;
        let edit_queue = q::city_form_edit_queue::Function::at(sites.edit_queue.0, sites.edit_queue.1).map_err(|e| format!("City_Form::edit_queue: {e}"))?;
        let set_production = q::city_set_production::Function::at(sites.set_production.0, sites.set_production.1).map_err(|e| format!("City::set_production: {e}"))?;
        let message = inproc::message::register("civ3-renderer/build-queue/v1");
        if message == 0 {
            return Err("registering the build queue message failed".into());
        }
        LIVE.set(Live { message, edit_queue, set_production }).map_err(|_| "build queue installed twice".to_owned())?;
        log::info!("build queue: reordered by dragging in the viewer, through the city screen's own edits");
        Ok(())
    }

    /// Queues the edit for the game's thread, in order with the input
    /// posted before it.
    pub fn post(window: Window, stamp: u32, from: u8, to: u8) {
        let Some(live) = LIVE.get() else { return };
        PENDING.lock().unwrap_or_else(|e| e.into_inner()).push((stamp, from, to));
        window.post_message(live.message, 0);
    }

    /// Called by the shared game-window interceptor, on the game's thread.
    pub fn handle_message(message: Message) -> Option<isize> {
        let live = LIVE.get()?;
        if message.id != live.message {
            return None;
        }
        if RUNNING.swap(true, Ordering::AcqRel) {
            // (From a dialog's loop the edit opened: after it.)
            message.window.post_message(live.message, 0);
            return Some(0);
        }
        let edits = std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()));
        for (stamp, from, to) in edits {
            apply(live, stamp, from, to);
        }
        RUNNING.store(false, Ordering::Release);
        Some(0)
    }

    fn apply(live: &Live, stamp: u32, from: u8, to: u8) {
        let mem = civ3_sys::controls::Live;
        let Some(queue) = q::Queue::read(&mem) else { return log::debug!("build queue: no city screen for the edit") };
        if q::network_game(&mem) || q::player(&mem) != Some(queue.civ) {
            return log::debug!("build queue: not the player's city, or a network game");
        }
        let Some(plan) = super::planned(&queue, stamp, from, to) else {
            return log::debug!("build queue: edit {from} -> {to} is not for the queue now, or changes nothing");
        };
        let form = q::CITY_FORM;
        for edit in &plan.edits {
            match *edit {
                Edit::SetProduction(o) => {
                    live.set_production.call(queue.city, o.kind as u32, o.id as u32, 1);
                    let now = q::Queue::read(&mem).map(|q| q.current);
                    if !now.is_some_and(|n| plan.checked(n)) {
                        return log::info!("build queue: {} not built now ({now:?}): the queue stays", super::name(o));
                    }
                }
                Edit::Remove(i) => live.edit_queue.call(form, 0, u32::MAX, i as u32, 0),
                Edit::Append(i, o) => live.edit_queue.call(form, o.kind as u32, o.id as u32, i as u32, 1),
            }
        }
        let after = q::Queue::read(&mem).map(|q| (q.current, q.entries));
        if after.as_ref() == Some(&plan.result) {
            log::info!("build queue: entry {from} -> {to}: builds {}, then {:?}", super::name(plan.result.0), plan.result.1.iter().map(|&o| super::name(o)).collect::<Vec<_>>());
        } else {
            log::warn!("build queue: entry {from} -> {to}: expected {:?}, the city has {after:?}", plan.result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORM: u32 = 0x0074_8D50;

    fn item(id: u64, rect: [i32; 4]) -> Interactable {
        Interactable { region: None, alt_right_click: false, anchor: None, rect, kind: InteractableKind::ListItem, id }
    }

    #[test]
    fn rows_are_the_queue_lists_entries_without_the_next_slot() {
        let f = u64::from(FORM) << 32;
        let items = [
            item(f | 1, [100, 220, 150, 20]),
            item(f | 0, [100, 200, 150, 20]),
            item(f | 2, [100, 240, 150, 20]),
            // Another list's entry, and a button of this form.
            item(0x0011_0000_0000 | 1, [0, 0, 150, 20]),
            Interactable { kind: InteractableKind::Button, ..item(f | 3, [0, 0, 20, 20]) },
        ];
        let rows = rows(&items, FORM, 2);
        assert_eq!(rows.iter().map(|r| r.slot).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(rows[0].grip, [100, 200, GRIP_WIDTH, 20]);
        assert_eq!(rows[1].rect, [100, 220, 150, 20]);
        assert_ne!(rows[0].grip_id, f, "a handle is an interactable of its own");
        assert_eq!(rows[1].grip_id >> 32, u64::from(FORM));
    }

    /// Run explicitly with Civ III installed: a city screen's queue box
    /// (the game's own box and production button art, its font), drawn by
    /// the viewer's drag code on the CPU: the handles, a drag between rows,
    /// a drag over the production box, a row picked up by a click.
    #[test]
    #[ignore = "requires Civ III art; writes screenshots to C3R_QUEUE_SHOTS or /tmp/c3r-citylist-shots"]
    fn build_queue_headless_screenshots() {
        use crate::text::{Fonts, TextRun};
        use crate::ui_screen::Item;
        use menu::screen::{Element, Region, ScreenArt};
        let install = std::env::var("CIV3_INSTALL").unwrap_or_else(|_| "/mnt/datasets/winvm/civ3-src/Civilization III Complete".into());
        let files = civ3_assets::GameFiles::complete(&install);
        let directory = std::path::PathBuf::from(std::env::var("C3R_QUEUE_SHOTS").unwrap_or_else(|_| "/tmp/c3r-citylist-shots".into()));
        std::fs::create_dir_all(&directory).unwrap();
        let image = |source: &str, rect: [i32; 4], at: (i32, i32)| Item::Element(Element::Image {
            region: Region { source: source.into(), rect, keyed: true, mask: None, palette: None }, at, clip: [0, 0, 1024, 768] }, false);
        let text = |x: i32, y: i32, t: &str, height: i32| Item::Text(TextRun { x, y, color: [0, 0, 0], face: "Lucida Sans".into(), height, weight: 400,
            italic: false, underline: false, text: t.into(), backdrop: None }, [0, 0, 1024, 768], false);
        let (bx, by) = (560, 150);
        let names = ["Warrior", "Granary", "Settler", "Temple", "Worker"];
        let mut items = vec![
            image("Art/city screen/background.pcx", [0, 0, 1024, 768], (0, 0)),
            image("Art/city screen/ProductionQueueBox.pcx", [0, 0, 203, 360], (bx, by)),
            image("Art/city screen/ProdButton.pcx", [0, 0, 100, 95], (850, 520)),
            text(872, 600, "Curragh", -12),
        ];
        let row_h = 22;
        let mut rows = Vec::new();
        for (k, name) in names.iter().enumerate() {
            let y = by + 10 + row_h * k as i32;
            items.push(text(bx + 8, y + 16, &format!("{}. {name}", k + 2), -12));
            let rect = [bx + 4, y, 195, row_h];
            rows.push(BuildQueueRow { slot: k as u8, rect, grip: [rect[0], y, GRIP_WIDTH, row_h], grip_id: 0x7F00_0000 | k as u64 });
        }
        items.push(text(bx + 8, by + 10 + row_h * names.len() as i32 + 16, &format!("{}. ", names.len() + 2), -12));
        let mut fonts = Fonts::default();
        let runs: Vec<TextRun> = items.iter().filter_map(|i| match i { Item::Text(r, ..) => Some(r.clone()), _ => None }).collect();
        fonts.add(&files, &runs);
        let mut art = ScreenArt::default();
        art.add(&items.iter().filter_map(|i| match i { Item::Element(e, _) => Some(e.clone()), _ => None }).collect::<Vec<_>>());
        let description = format!("grid = {{ layout = 'orthogonal', step = [1, 1] }}\n{}\n{}", crate::ui_screen::description(&art), fonts.description());
        let compiled = crate::assets::compile(&description, civ3_assets::GameFiles::with_roots(files.roots().to_vec())).unwrap();
        let sprites = crate::ui_screen::sprites(&items, &fonts, &compiled, 0, false);
        let mut scene = scene::Scene { views: vec![scene::ViewState { id: 0, view: scene::View { interface_only: true, ..Default::default() }, overlay: sprites }], ..Default::default() };
        scene.interface.build_queue = Some(BuildQueue { stamp: 1, current: [850, 520, 100, 95], rows });
        let rect_of = |s: &tile_render::PlacedSprite| -> Option<[i32; 4]> {
            let (x0, y0, x1, y1) = if let Some(r) = s.solid {
                (s.x, s.y, s.x + r.width as i32, s.y + r.height as i32)
            } else if let Some(g) = s.text {
                let em = (g.em64 as i32 + 63) / 64;
                (s.x, s.y - em, s.x + g.advance.max(1), s.y + em / 3)
            } else {
                let id = compiled.sets.frame(s.set, 0)?;
                let sp = compiled.atlas.sprites.get(id.0 as usize)?;
                let (x0, y0) = (s.x + sp.anchor_x, s.y + sp.anchor_y);
                (x0, y0, x0 + sp.width as i32, y0 + sp.height as i32)
            };
            let (x0, y0, x1, y1) = match s.clip { Some(c) => (x0.max(c[0]), y0.max(c[1]), x1.min(c[2]), y1.min(c[3])), None => (x0, y0, x1, y1) };
            (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
        };
        let shot = |drag: &tile_viewer::build_queue::BuildQueueDrag, name: &str| {
            let mut world = tile_viewer::scene_world(&compiled, &scene);
            let free = drag.draw(&scene, &world.free, rect_of).expect("a queue is shown");
            world.free = free;
            let picture = tile_render::reference::render(&world, &compiled.atlas, &tile_render::Camera::new((1024, 768)), 0, [0, 0, 0, 255]);
            let full = image::RgbaImage::from_raw(picture.width, picture.height, picture.as_bytes().to_vec()).unwrap();
            let crop = image::imageops::crop_imm(&full, 520, 120, 460, 520).to_image();
            crop.save(directory.join(format!("queue-{name}.png"))).unwrap();
        };
        let press = |down: bool, (x, y): (i32, i32)| scene::Input::MouseButton { button: scene::Button::Left, down, x, y };
        let mut drag = tile_viewer::build_queue::BuildQueueDrag::default();
        shot(&drag, "grips");
        // "Warrior" (slot 0) dragged down between "Settler" and "Temple".
        drag.route(&press(true, (bx + 9, by + 20)), &scene);
        drag.route(&scene::Input::MouseMove { x: bx + 60, y: by + 10 + 3 * row_h - 4 }, &scene);
        shot(&drag, "drag");
        assert_eq!(drag.route(&press(false, (bx + 60, by + 10 + 3 * row_h - 4)), &scene), Some(vec![scene::Input::BuildQueue { stamp: 1, from: 0, to: 3 }]));
        // "Temple" (slot 3) over the production box.
        drag.route(&press(true, (bx + 9, by + 10 + 3 * row_h + 10)), &scene);
        drag.route(&scene::Input::MouseMove { x: 890, y: 560 }, &scene);
        shot(&drag, "swap-hover");
        assert_eq!(drag.route(&press(false, (890, 560)), &scene), Some(vec![scene::Input::BuildQueue { stamp: 1, from: 3, to: scene::BUILD_QUEUE_CURRENT }]));
        // Picked up by a click on its handle (the Alt mode's way).
        drag.route(&press(true, (bx + 9, by + 10 + row_h + 10)), &scene);
        drag.route(&press(false, (bx + 9, by + 10 + row_h + 10)), &scene);
        drag.route(&scene::Input::MouseMove { x: bx + 100, y: by + 10 + 4 * row_h + 10 }, &scene);
        shot(&drag, "click-picked");
    }

    #[test]
    fn an_edit_for_another_queue_is_dropped() {
        let queue = q::Queue { city: 1, city_id: 2, civ: 1, current: Order { kind: q::UNIT, id: 4 }, entries: vec![Order { kind: q::UNIT, id: 5 }, Order { kind: q::IMPROVEMENT, id: 6 }] };
        let stamp = queue.stamp();
        assert!(planned(&queue, stamp ^ 1, 1, 0).is_none());
        let swap = planned(&queue, stamp, 1, scene::BUILD_QUEUE_CURRENT).unwrap();
        assert_eq!(swap.result, (Order { kind: q::IMPROVEMENT, id: 6 }, vec![Order { kind: q::UNIT, id: 4 }, Order { kind: q::UNIT, id: 5 }]));
        assert_eq!(planned(&queue, stamp, 1, 0).unwrap().result.1, vec![Order { kind: q::IMPROVEMENT, id: 6 }, Order { kind: q::UNIT, id: 5 }]);
    }
}
