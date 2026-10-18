//! Alt-mode edits of the game's native sliders. Private window messages
//! put every write and native notification on the game's UI thread.

use std::collections::HashMap;
use civ3_sys::sliders::Slider;
use scene::SliderAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Request { id: u64, action: SliderAction, value: i32 }

#[derive(Default)]
struct Requests {
    next: u32,
    requests: HashMap<u32, Request>,
    preview: Option<u32>,
}

impl Requests {
    fn barrier(&mut self) { self.preview = None; }

    fn push(&mut self, request: Request) -> (u32, bool) {
        if request.action == SliderAction::Preview
            && let Some(token) = self.preview
            && let Some(previous) = self.requests.get_mut(&token)
            && previous.id == request.id
        {
            *previous = request;
            return (token, false);
        }
        self.barrier();
        loop {
            self.next = self.next.wrapping_add(1).max(1);
            if !self.requests.contains_key(&self.next) { break; }
        }
        let token = self.next;
        self.requests.insert(token, request);
        if request.action == SliderAction::Preview { self.preview = Some(token); }
        (token, true)
    }

    fn take(&mut self, token: u32) -> Option<Request> {
        if self.preview == Some(token) { self.preview = None; }
        self.requests.remove(&token)
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
            SliderAction::Begin | SliderAction::Cancel => unreachable!(),
        }
    }
}

#[cfg(windows)]
pub use live::{barrier, handle_message, install, post};

#[cfg(windows)]
mod live {
    use std::sync::{Mutex, OnceLock};
    use std::sync::atomic::{AtomicBool, Ordering};
    use civ3_sys::{Build, Civ3};
    use inproc::message::Message;
    use inproc::window::Window;
    use super::*;

    struct Live { game: Civ3, message: u32 }
    static LIVE: OnceLock<Live> = OnceLock::new();
    static STATE: Mutex<Option<State>> = Mutex::new(None);
    static DISPATCHING: AtomicBool = AtomicBool::new(false);

    #[derive(Default)]
    struct State {
        requests: Requests,
        transaction: Transaction,
    }

    pub fn install(game: Civ3) -> Result<(), String> {
        if game.build != Build::PcGames { return Err(format!("native sliders are not decoded for {:?}", game.build)); }
        civ3_sys::sliders::validate().map_err(|e| e.to_string())?;
        let message = inproc::message::register("civ3-renderer/native-slider/v1");
        if message == 0 { return Err("registering native slider message failed".into()); }
        LIVE.set(Live { game, message }).map_err(|_| "native sliders installed twice".to_owned())?;
        *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(State::default());
        Ok(())
    }

    pub fn post(window: Window, id: u64, action: SliderAction, value: i32) {
        let Some(live) = LIVE.get() else { return };
        if id > u64::from(u32::MAX) || !crate::world_input::ready(window) { return; }
        crate::world_input::barrier();
        let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let Some(state) = state.as_mut() else { return };
        let (token, post) = state.requests.push(Request { id, action, value });
        if post && !window.post_message(live.message, token) { state.requests.take(token); }
    }

    /// Keep preview coalescing on its own side of keys, buttons and other
    /// input already posted to the game's queue.
    pub fn barrier() {
        if let Some(state) = STATE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { state.requests.barrier(); }
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

    /// Called first by the shared game-window interceptor. No state lock
    /// survives a native notification, which may dispatch modal messages.
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
        let (mut transaction, request) = {
            let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
            let state = state.as_mut()?;
            let request = if private {
                let token = message.wparam as u32;
                state.requests.take(token)
            } else { state.requests.barrier(); None };
            (std::mem::take(&mut state.transaction), request)
        };
        let mut driver = Native { game: live.game };
        if let Some(request) = request { transaction.apply(request, &mut driver); }
        else if matches!(message.id, 0x0008 | 0x0010 | 0x0082 | 0x0100 | 0x0104 | 0x0201 | 0x0204 | 0x0207) {
            // Restore before a foreign click/key can close or replace the
            // form. Alt key-up and mouse-up do not cancel an edit.
            transaction.cancel(&mut driver);
        } else { transaction.validate(&mut driver); }
        if let Some(state) = STATE.lock().unwrap_or_else(|e| e.into_inner()).as_mut() { state.transaction = transaction; }
        private.then_some(0)
    }
}

#[cfg(test)]
mod tests;
