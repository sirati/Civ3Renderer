//! Input the game reaches only after a stall is dropped, not replayed.
//!
//! The viewer shows hover and presses at once and forwards input into the
//! game window's queue. When the game thread stalls (seen under ARM64
//! Wine/FEX: seconds waiting for a heap lock), everything posted meanwhile
//! would run in one burst once it resumes: a dozen "Next City" clicks at
//! once. A press, wheel notch or typed key older than [`STALE_AFTER_MS`]
//! when the game dispatches it is dropped, together with its release.
//! Releases of presses the game did get always go through, so nothing stays
//! held; moves and every other message go through untouched.

/// How old (ms since posting) input may be when the game gets to it.
pub const STALE_AFTER_MS: u32 = 1000;

/// What a forwarded message is to the filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Press(Source),
    Release(Source),
    Wheel,
    /// A character of a key (translated, or posted on its own).
    Char,
    Other,
}

/// What was pressed: a game-window mouse button (0 left, 1 right,
/// 2 middle), a key, or a button of the bridge's own world input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Mouse(u8),
    Key(u32),
    World(u8),
}

/// The filter's state on the game window's thread.
#[derive(Debug, Default)]
pub struct StaleFilter {
    /// Presses dropped whose releases are still to come.
    dropped: Vec<Source>,
    /// A key press was dropped: its characters follow in the same turn.
    drop_chars: bool,
    /// How many messages were dropped (for the log).
    pub dropped_total: u64,
}

impl StaleFilter {
    /// Whether the message goes on to the game (`false`: consumed).
    pub fn admit(&mut self, gesture: Gesture, age_ms: u32) -> bool {
        let stale = age_ms > STALE_AFTER_MS;
        // Characters follow their key directly; anything else ends them.
        let drop_chars = std::mem::take(&mut self.drop_chars);
        let admit = match gesture {
            Gesture::Press(source) => {
                self.dropped.retain(|&s| s != source);
                if stale {
                    self.dropped.push(source);
                }
                self.drop_chars = stale && matches!(source, Source::Key(_));
                !stale
            }
            Gesture::Release(source) => match self.dropped.iter().position(|&s| s == source) {
                Some(index) => {
                    self.dropped.swap_remove(index);
                    false
                }
                None => true,
            },
            Gesture::Wheel => !stale,
            Gesture::Char => {
                self.drop_chars = drop_chars;
                !(drop_chars || stale)
            }
            Gesture::Other => true,
        };
        if !admit {
            self.dropped_total += 1;
        }
        admit
    }
}

const WM_KEYDOWN: u32 = 0x0100;
const WM_KEYUP: u32 = 0x0101;
const WM_CHAR: u32 = 0x0102;
const WM_DEADCHAR: u32 = 0x0103;
const WM_SYSKEYDOWN: u32 = 0x0104;
const WM_SYSKEYUP: u32 = 0x0105;
const WM_SYSCHAR: u32 = 0x0106;
const WM_SYSDEADCHAR: u32 = 0x0107;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_RBUTTONDBLCLK: u32 = 0x0206;
const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_MBUTTONUP: u32 = 0x0208;
const WM_MBUTTONDBLCLK: u32 = 0x0209;
const WM_MOUSEWHEEL: u32 = 0x020A;

/// Modifiers and lock keys are never dropped: the game reads their state
/// from the system as well, which must not disagree with what it was sent.
fn modifier(vk: u32) -> bool {
    matches!(vk, 0x10..=0x12 | 0x14 | 0x5B | 0x5C | 0x90 | 0x91 | 0xA0..=0xA5)
}

/// What a window message is to the filter.
pub fn gesture(id: u32, wparam: usize) -> Gesture {
    let vk = (wparam & 0xFFFF) as u32;
    match id {
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => Gesture::Press(Source::Mouse(0)),
        WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => Gesture::Press(Source::Mouse(1)),
        WM_MBUTTONDOWN | WM_MBUTTONDBLCLK => Gesture::Press(Source::Mouse(2)),
        WM_LBUTTONUP => Gesture::Release(Source::Mouse(0)),
        WM_RBUTTONUP => Gesture::Release(Source::Mouse(1)),
        WM_MBUTTONUP => Gesture::Release(Source::Mouse(2)),
        WM_MOUSEWHEEL => Gesture::Wheel,
        WM_KEYDOWN | WM_SYSKEYDOWN if !modifier(vk) => Gesture::Press(Source::Key(vk)),
        WM_KEYUP | WM_SYSKEYUP if !modifier(vk) => Gesture::Release(Source::Key(vk)),
        WM_CHAR | WM_DEADCHAR | WM_SYSCHAR | WM_SYSDEADCHAR => Gesture::Char,
        _ => Gesture::Other,
    }
}

/// The same on the bridge's side: a batch of viewer input the publisher
/// takes after it was itself held up for longer than [`STALE_AFTER_MS`]
/// (its previous take was that long ago) may be that old. Its presses and
/// wheel turns are dropped with their releases; they would otherwise be
/// posted to the game all at once, freshly stamped.
#[derive(Debug, Default)]
pub struct BatchFilter {
    filter: StaleFilter,
    /// After a stall: how much longer later batches still count as stale.
    /// When the whole process stalled, input the viewer sent meanwhile is
    /// still in the socket when the publisher first resumes and arrives in
    /// the next few batches.
    grace_ms: u32,
}

/// How long after a stall input still counts as sent during it.
pub const RESUME_GRACE_MS: u32 = 300;

impl BatchFilter {
    pub fn filter(&mut self, inputs: Vec<scene::Input>, waited_ms: u32) -> Vec<scene::Input> {
        use scene::Input;
        let waited_ms = if waited_ms > STALE_AFTER_MS {
            self.grace_ms = RESUME_GRACE_MS;
            waited_ms
        } else if self.grace_ms > 0 {
            self.grace_ms = self.grace_ms.saturating_sub(waited_ms.max(1));
            STALE_AFTER_MS + 1
        } else {
            waited_ms
        };
        let dropped_before = self.filter.dropped_total;
        let kept: Vec<Input> = inputs
            .into_iter()
            .filter(|input| {
                let button = |b: scene::Button| match b {
                    scene::Button::Left => 0,
                    scene::Button::Right => 1,
                    scene::Button::Middle => 2,
                };
                let gesture = match *input {
                    Input::MouseButton { button: b, down, .. } | Input::WorldButton { button: b, down, .. } => {
                        let source = Source::Mouse(button(b));
                        if down { Gesture::Press(source) } else { Gesture::Release(source) }
                    }
                    Input::Key { vk, down, .. } | Input::WorldKey { vk, down, .. } if vk != 0 && !modifier(vk) => {
                        if down { Gesture::Press(Source::Key(vk)) } else { Gesture::Release(Source::Key(vk)) }
                    }
                    Input::Key { vk: 0, down: true, .. } | Input::WorldKey { vk: 0, down: true, .. } => Gesture::Char,
                    Input::Wheel { .. } => Gesture::Wheel,
                    _ => Gesture::Other,
                };
                // Text without a key stands alone: no key press before it.
                if gesture == Gesture::Char {
                    let admit = waited_ms <= STALE_AFTER_MS;
                    if !admit { self.filter.dropped_total += 1; }
                    return admit;
                }
                self.filter.admit(gesture, waited_ms)
            })
            .collect();
        let dropped = self.filter.dropped_total - dropped_before;
        if dropped != 0 {
            log::info!("input: dropped {dropped} viewer inputs that waited up to {waited_ms} ms for the bridge; not replayed late");
        }
        kept
    }
}

/// The game window thread's filter (`world_input`'s interceptor calls
/// these; the state is that thread's own, no lock).
#[cfg(windows)]
pub mod live {
    use super::*;
    use std::cell::RefCell;

    thread_local! {
        static FILTER: RefCell<StaleFilter> = RefCell::new(StaleFilter::default());
    }

    fn admit(gesture: Gesture, what: impl FnOnce() -> String) -> bool {
        if gesture == Gesture::Other {
            // Cheap path for the bulk of the game's messages.
            return FILTER.with(|f| f.borrow_mut().admit(gesture, 0));
        }
        let age = inproc::message::queued_age_ms();
        let admit = FILTER.with(|f| f.borrow_mut().admit(gesture, age));
        if !admit {
            log::info!("input: dropped {} posted {age} ms ago (the game was busy); not replayed late", what());
        }
        admit
    }

    /// For a native message to the game window: whether it goes on.
    pub fn admit_native(message: inproc::message::Message) -> bool {
        let gesture = gesture(message.id, message.wparam);
        admit(gesture, || format!("{gesture:?} (message {:#x})", message.id))
    }

    /// For a button of the bridge's own world input (map clicks).
    pub fn admit_world_button(button: scene::Button, down: bool) -> bool {
        let source = Source::World(match button {
            scene::Button::Left => 0,
            scene::Button::Right => 1,
            scene::Button::Middle => 2,
        });
        let gesture = if down { Gesture::Press(source) } else { Gesture::Release(source) };
        admit(gesture, || format!("world {gesture:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRESH: u32 = 30;
    const STALE: u32 = STALE_AFTER_MS + 1;

    #[test]
    fn stale_clicks_are_dropped_with_their_releases_fresh_ones_pass() {
        let mut f = StaleFilter::default();
        let (down, up) = (gesture(WM_LBUTTONDOWN, 1), gesture(WM_LBUTTONUP, 0));
        // A burst after a 5 s stall: every click is dropped, press and release.
        for _ in 0..12 {
            assert!(!f.admit(down, 5000));
            assert!(f.admit(gesture(0x0200, 1), 5000), "moves always pass");
            assert!(!f.admit(up, 5000));
        }
        assert_eq!(f.dropped_total, 24);
        // Input after the stall works normally at once.
        assert!(f.admit(down, FRESH));
        assert!(f.admit(up, FRESH));
    }

    #[test]
    fn a_release_of_a_delivered_press_always_passes() {
        let mut f = StaleFilter::default();
        assert!(f.admit(gesture(WM_RBUTTONDOWN, 2), FRESH));
        // The game stalls between press and release: the release still goes,
        // so the game never keeps a button held.
        assert!(f.admit(gesture(WM_RBUTTONUP, 0), STALE));
        assert!(f.admit(gesture(WM_KEYDOWN, 0x41), FRESH));
        assert!(f.admit(gesture(WM_KEYUP, 0x41), STALE));
        // A release with no press seen at all passes too.
        assert!(f.admit(gesture(WM_MBUTTONUP, 0), STALE));
    }

    #[test]
    fn stale_keys_drop_with_their_characters_and_release_but_modifiers_pass() {
        let mut f = StaleFilter::default();
        assert!(!f.admit(gesture(WM_KEYDOWN, 0x0D), STALE));
        // TranslateMessage posts its character fresh, in the same turn.
        assert!(!f.admit(gesture(WM_CHAR, 0x0D), FRESH));
        assert!(f.admit(gesture(0x0200, 0), FRESH));
        assert!(f.admit(gesture(WM_CHAR, 0x61), FRESH), "a later character is its own");
        assert!(!f.admit(gesture(WM_KEYUP, 0x0D), FRESH));
        for vk in [0x10, 0x11, 0x12, 0xA0, 0xA3] {
            assert!(f.admit(gesture(WM_KEYDOWN, vk), STALE), "modifier {vk:#x}");
            assert!(f.admit(gesture(WM_KEYUP, vk), STALE));
        }
        assert!(!f.admit(gesture(WM_CHAR, 0x62), STALE), "stale text on its own");
    }

    #[test]
    fn a_batch_taken_after_a_bridge_stall_loses_its_clicks_but_keeps_releases_and_metadata() {
        use scene::{Button, Input};
        let mut f = BatchFilter::default();
        let click = |down| Input::MouseButton { button: Button::Left, down, x: 10, y: 10 };
        // Pressed before the stall, released during it: the release goes.
        assert_eq!(f.filter(vec![click(true)], FRESH), [click(true)]);
        let stalled = vec![
            click(false),
            click(true), click(false), click(true), click(false),
            Input::MouseMove { x: 11, y: 11 },
            Input::Wheel { delta: 120, x: 1, y: 1 },
            Input::Key { vk: 0x0D, down: true, text: None },
            Input::Key { vk: 0x11, down: true, text: None },
            Input::Key { vk: 0, down: true, text: Some('a') },
            Input::Resized { width: 800, height: 600 },
            Input::Quit,
        ];
        assert_eq!(f.filter(stalled, 5000), [
            click(false),
            Input::MouseMove { x: 11, y: 11 },
            Input::Key { vk: 0x11, down: true, text: None },
            Input::Resized { width: 800, height: 600 },
            Input::Quit,
        ]);
        // Input arriving right after the stall (still in the socket) counts
        // as sent during it; the dropped Enter's release goes with it.
        let next = vec![Input::Key { vk: 0x0D, down: false, text: None }, click(true), click(false)];
        assert!(f.filter(next, FRESH).is_empty());
        for _ in 0..RESUME_GRACE_MS / FRESH {
            assert!(f.filter(vec![], FRESH).is_empty());
        }
        // Then input flows as usual.
        assert_eq!(f.filter(vec![click(true), click(false)], FRESH), [click(true), click(false)]);
    }

    #[test]
    fn a_fresh_press_replaces_a_dropped_one_and_wheel_and_world_follow_age() {
        let mut f = StaleFilter::default();
        let world = |down| if down { Gesture::Press(Source::World(0)) } else { Gesture::Release(Source::World(0)) };
        assert!(!f.admit(world(true), STALE));
        assert!(f.admit(world(true), FRESH));
        assert!(f.admit(world(false), FRESH), "the release belongs to the fresh press");
        assert!(!f.admit(gesture(WM_MOUSEWHEEL, 120 << 16), STALE));
        assert!(f.admit(gesture(WM_MOUSEWHEEL, 120 << 16), FRESH));
        assert!(f.admit(gesture(0x0401, 7), u32::MAX), "private and other messages always pass");
        // Double-click presses pair with ordinary releases.
        assert!(!f.admit(gesture(WM_LBUTTONDBLCLK, 1), STALE));
        assert!(!f.admit(gesture(WM_LBUTTONUP, 0), FRESH));
    }
}
