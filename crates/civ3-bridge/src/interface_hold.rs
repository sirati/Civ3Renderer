//! The interface is never shown with art that is not made yet.
//!
//! The game's interface is published as the game draws it. When a frame
//! shows art the renderer does not have yet (a button's hover state the
//! first time the pointer is on it, an icon first shown, a screen first
//! opened), that art would be drawn as nothing until it arrives: the
//! picture blinks out and back. Instead, while the missing art is being
//! made, the scene keeps the last interface whose art was complete (its
//! views, controls and looks) for at most [`LIMIT`]; the world goes on as
//! usual. The new picture shows the moment its art is there. Art that
//! does not come (an unreadable file) is not waited for longer than
//! [`LIMIT`].

use std::time::{Duration, Instant};

/// The longest an interface waits for its art.
pub const LIMIT: Duration = Duration::from_millis(500);

/// The parts of a scene that are the interface.
#[derive(Clone, Debug, Default)]
struct Interface {
    views: Vec<scene::ViewState>,
    interface: scene::Interface,
    interactables: Vec<scene::Interactable>,
    looks: Vec<scene::Looks>,
}

impl Interface {
    fn of(scene: &scene::Scene) -> Interface {
        Interface {
            views: scene.views.iter().filter(|v| v.view.interface_only).cloned().collect(),
            interface: scene.interface.clone(),
            interactables: scene.interactables.clone(),
            looks: scene.looks.clone(),
        }
    }

    fn put(&self, scene: &mut scene::Scene) {
        scene.views.retain(|v| !v.view.interface_only);
        scene.views.extend(self.views.iter().cloned());
        scene.interface = self.interface.clone();
        scene.interactables = self.interactables.clone();
        scene.looks = self.looks.clone();
    }
}

/// The last complete interface and how long a newer one has waited.
#[derive(Debug, Default)]
pub struct InterfaceHold {
    complete: Option<Interface>,
    waiting_since: Option<Instant>,
}

impl InterfaceHold {
    /// `scene`, made from a frame whose visible art is all there
    /// (`complete`) or is still being made (`pending`): kept as it is, or
    /// with the last complete interface in place of its own. Whether it
    /// was held.
    pub fn apply(&mut self, scene: &mut scene::Scene, complete: bool, pending: bool, now: Instant) -> bool {
        if complete {
            self.complete = Some(Interface::of(scene));
            self.waiting_since = None;
            return false;
        }
        let Some(last) = self.complete.as_ref().filter(|_| pending) else {
            self.waiting_since = None;
            return false;
        };
        let since = *self.waiting_since.get_or_insert(now);
        if now.duration_since(since) >= LIMIT {
            return false;
        }
        last.put(scene);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scene::{FreeSprite, View, ViewState};

    fn scene(ui_sprite: u32, world_camera: i32) -> scene::Scene {
        scene::Scene {
            views: vec![
                ViewState { id: 0, view: View { camera_x: world_camera, ..Default::default() }, overlay: vec![] },
                ViewState { id: 255, view: View { interface_only: true, ..Default::default() }, overlay: vec![FreeSprite { set: ui_sprite, ..Default::default() }] },
            ],
            ..Default::default()
        }
    }

    fn ui(s: &scene::Scene) -> u32 {
        s.views.iter().find(|v| v.view.interface_only).unwrap().overlay[0].set
    }

    #[test]
    fn a_frame_without_its_art_keeps_the_last_complete_interface_and_the_new_world() {
        let mut hold = InterfaceHold::default();
        let t = Instant::now();
        let mut first = scene(1, 10);
        assert!(!hold.apply(&mut first, true, false, t));
        // The pointer on a button: its hover art not made yet.
        let mut hover = scene(2, 20);
        assert!(hold.apply(&mut hover, false, true, t + Duration::from_millis(16)));
        assert_eq!(ui(&hover), 1, "the button as it was, not blank");
        assert_eq!(hover.views[0].view.camera_x, 20, "the world goes on");
        // Still waiting, within the limit.
        let mut later = scene(2, 30);
        assert!(hold.apply(&mut later, false, true, t + Duration::from_millis(300)));
        assert_eq!(ui(&later), 1);
        // The art arrived: the new picture.
        let mut arrived = scene(2, 40);
        assert!(!hold.apply(&mut arrived, true, false, t + Duration::from_millis(320)));
        assert_eq!(ui(&arrived), 2);
    }

    #[test]
    fn art_that_does_not_come_is_not_waited_for_past_the_limit() {
        let mut hold = InterfaceHold::default();
        let t = Instant::now();
        hold.apply(&mut scene(1, 0), true, false, t);
        assert!(hold.apply(&mut scene(2, 0), false, true, t));
        let mut late = scene(2, 0);
        assert!(!hold.apply(&mut late, false, true, t + LIMIT));
        assert_eq!(ui(&late), 2, "shown without it after the limit");
        // Not being made at all (unreadable): not held.
        let mut unavailable = scene(3, 0);
        assert!(!hold.apply(&mut unavailable, false, false, t + LIMIT));
        assert_eq!(ui(&unavailable), 3);
        // Nothing complete yet (the first frame): nothing to hold.
        let mut fresh = InterfaceHold::default();
        let mut s = scene(4, 0);
        assert!(!fresh.apply(&mut s, false, true, t));
        assert_eq!(ui(&s), 4);
    }
}
