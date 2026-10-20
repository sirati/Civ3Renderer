//! Alt-mode edits of the game's native sliders. Private window messages
//! put every write and native notification on the game's UI thread.

use civ3_sys::sliders::Slider;
use scene::SliderAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Request { id: u64, action: SliderAction, value: i32 }

/// Requests waiting for their private message, handed to the game's thread
/// without a lock (`civ3r_sync::Posted`): consecutive previews of one
/// slider share a message, the latest value winning.
#[derive(Default)]
struct Requests(civ3r_sync::Posted<Request>);

impl Requests {
    fn barrier(&self) { self.0.barrier(); }

    fn push(&self, request: Request) -> (u32, bool) {
        let key = (request.action == SliderAction::Preview).then(|| (request.id as u32) ^ ((request.id >> 32) as u32).rotate_left(16));
        self.0.push(request, key)
    }

    /// The message for `token` was not posted.
    #[cfg_attr(not(windows), allow(dead_code))]
    fn retract(&self, token: u32) { self.0.retract(token); }

    /// On the game's thread: the requests up to `token`'s message.
    fn take(&self, token: u32, inbox: &mut civ3r_sync::Inbox<Request>) -> Vec<Request> {
        self.0.take(token, inbox)
    }
}

trait Driver {
    fn read(&self, id: u64, accessible: bool) -> Option<Slider>;
    fn set(&mut self, original: Slider, exact: i32);
}

#[derive(Default)]
struct Transaction { original: Option<Slider> }

impl Transaction {
    fn cancel(&mut self, driver: &mut impl Driver) {
        if let Some(original) = self.original.take()
            && driver.read(u64::from(original.identity.address), false).is_some_and(|now| now.same_control(original))
        {
            driver.set(original, original.exact);
        }
    }

    fn validate(&mut self, driver: &mut impl Driver) {
        if self.original.is_some_and(|original| !driver.read(u64::from(original.identity.address), true).is_some_and(|now| now.same_control(original))) {
            self.cancel(driver);
        }
    }

    fn apply(&mut self, request: Request, driver: &mut impl Driver) {
        if request.action == SliderAction::Begin {
            self.cancel(driver);
            // No native mouse-down, click, rounding or write at entry.
            self.original = driver.read(request.id, true);
            return;
        }
        if request.action == SliderAction::Step {
            self.step(request, driver);
            return;
        }
        let Some(original) = self.original.filter(|old| u64::from(old.identity.address) == request.id) else { return };
        if request.action == SliderAction::Cancel {
            self.cancel(driver);
            return;
        }
        self.validate(driver);
        if self.original.is_none() { return; }
        match request.action {
            SliderAction::Preview => driver.set(original, original.exact_at(request.value)),
            SliderAction::Commit => self.original = None,
            SliderAction::Begin | SliderAction::Cancel | SliderAction::Step => unreachable!(),
        }
    }

    /// An arrow key on a selected slider: `request.value` arrow-button
    /// steps from its current value. Inside an open edit of the same
    /// control Cancel still restores the entry value; an edit of another
    /// control is restored first.
    fn step(&mut self, request: Request, driver: &mut impl Driver) {
        if self.original.is_some_and(|old| u64::from(old.identity.address) != request.id) {
            self.cancel(driver);
        }
        self.validate(driver);
        let Some(current) = driver.read(request.id, true) else { return };
        if self.original.is_some_and(|old| !old.same_control(current)) { return; }
        let exact = current.stepped(request.value);
        if exact != current.exact { driver.set(current, exact); }
    }
}

#[cfg(windows)]
pub use live::{barrier, handle_message, install, post};

#[cfg(windows)]
mod live {
    use std::cell::{Cell, RefCell};
    use std::sync::{LazyLock, OnceLock};
    use std::sync::atomic::{AtomicBool, Ordering};
    use civ3_sys::{Build, Civ3};
    use inproc::message::Message;
    use inproc::window::Window;
    use super::*;

    struct Live { game: Civ3, message: u32 }
    static LIVE: OnceLock<Live> = OnceLock::new();
    /// Filled by the publisher, taken by the game's thread (no lock).
    static REQUESTS: LazyLock<Requests> = LazyLock::new(Requests::default);
    static DISPATCHING: AtomicBool = AtomicBool::new(false);
    thread_local! {
        /// Received requests whose messages are still queued.
        static INBOX: RefCell<civ3r_sync::Inbox<Request>> = RefCell::new(civ3r_sync::Inbox::default());
        /// The edit in progress, owned by the game's thread.
        static TRANSACTION: Cell<Transaction> = const { Cell::new(Transaction { original: None }) };
    }

    pub fn install(game: Civ3) -> Result<(), String> {
        if game.build != Build::PcGames { return Err(format!("native sliders are not decoded for {:?}", game.build)); }
        civ3_sys::sliders::validate().map_err(|e| e.to_string())?;
        let message = inproc::message::register("civ3-renderer/native-slider/v1");
        if message == 0 { return Err("registering native slider message failed".into()); }
        LIVE.set(Live { game, message }).map_err(|_| "native sliders installed twice".to_owned())?;
        Ok(())
    }

    pub fn post(window: Window, id: u64, action: SliderAction, value: i32) {
        let Some(live) = LIVE.get() else { return };
        if id > u64::from(u32::MAX) || !crate::world_input::ready(window) { return; }
        crate::world_input::barrier();
        let (token, post) = REQUESTS.push(Request { id, action, value });
        if post && !window.post_message(live.message, token) { REQUESTS.retract(token); }
    }

    /// Keep preview coalescing on its own side of keys, buttons and other
    /// input already posted to the game's queue.
    pub fn barrier() {
        REQUESTS.barrier();
    }

    struct Native { game: Civ3 }
    impl Driver for Native {
        fn read(&self, id: u64, accessible: bool) -> Option<Slider> {
            let address = u32::try_from(id).ok()?;
            if accessible {
                let sites = civ3_sys::controls::sites(self.game.build)?;
                if !civ3_sys::controls::controls(&civ3_sys::controls::Live, &sites).iter().any(|c| c.id == id && matches!(c.kind, civ3_sys::controls::ControlKind::Slider { .. })) {
                    return None;
                }
            }
            Slider::read(&civ3_sys::controls::Live, address)
        }
        fn set(&mut self, original: Slider, exact: i32) {
            if !civ3_sys::sliders::set_exact(original, exact) { log::debug!("native slider edit no longer matches its control"); }
        }
    }

    /// Called first by the shared game-window interceptor. The transaction
    /// is taken out while native notifications (which may dispatch modal
    /// messages) run.
    pub fn handle_message(message: Message) -> Option<isize> {
        let live = LIVE.get()?;
        let private = message.id == live.message;
        if private {
            // A semantic UI edit has no WM_MOUSEMOVE to restore native
            // client lookup after the pointer leaves a renderer map view.
            crate::world_input::leave_world();
        }
        if DISPATCHING.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
            if private { message.window.post_message(live.message, message.wparam as u32); return Some(0); }
            return None;
        }
        struct Dispatch;
        impl Drop for Dispatch { fn drop(&mut self) { DISPATCHING.store(false, Ordering::Release); } }
        let _dispatch = Dispatch;
        let requests = if private {
            let token = message.wparam as u32;
            INBOX.with_borrow_mut(|inbox| REQUESTS.take(token, inbox))
        } else { REQUESTS.barrier(); Vec::new() };
        let mut transaction = TRANSACTION.take();
        let mut driver = Native { game: live.game };
        if !requests.is_empty() { requests.into_iter().for_each(|request| transaction.apply(request, &mut driver)); }
        else if matches!(message.id, 0x0008 | 0x0010 | 0x0082 | 0x0100 | 0x0104 | 0x0201 | 0x0204 | 0x0207) {
            // Restore before a foreign click/key can close or replace the
            // form. Alt key-up and mouse-up do not cancel an edit.
            transaction.cancel(&mut driver);
        } else { transaction.validate(&mut driver); }
        TRANSACTION.set(transaction);
        private.then_some(0)
    }
}

#[cfg(test)]
mod tests;
