//! A successful foreign-memory read is not proof that posted input ran.
//! Each potential native edit needs an acknowledgment posted after it in
//! the same game-window queue before its snapshot may clear pending state.

use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

#[derive(Default)]
pub(super) struct Delivery {
    epoch: u64,
    posted: u64,
    acknowledged: Arc<AtomicU64>,
}

#[derive(Clone)]
pub(super) struct Receipt {
    epoch: u64,
    acknowledged: Arc<AtomicU64>,
}

impl Receipt {
    pub(super) fn acknowledge(self) { self.acknowledged.fetch_max(self.epoch, Ordering::Release); }
}

impl Delivery {
    pub(super) fn edited(&mut self) { self.epoch = self.epoch.saturating_add(1); }
    pub(super) fn settled(&self) -> bool { self.acknowledged.load(Ordering::Acquire) >= self.epoch }
    pub(super) fn receipt(&self) -> Option<Receipt> {
        (self.posted < self.epoch).then(|| Receipt { epoch: self.epoch, acknowledged: self.acknowledged.clone() })
    }
    pub(super) fn posted(&mut self, receipt: &Receipt) { self.posted = self.posted.max(receipt.epoch); }
}

#[cfg(windows)]
pub(super) fn post(window: inproc::window::Window, receipt: Receipt) -> bool { live::post(window, receipt) }

#[cfg(windows)]
pub(super) fn post_accept(window: inproc::window::Window) -> bool { live::post_accept(window) }

#[cfg(windows)]
pub(crate) fn handle_message(message: inproc::message::Message) -> Option<isize> { live::handle_message(message) }

#[cfg(windows)]
mod live {
    use super::*;
    use std::{cell::RefCell, sync::{LazyLock, OnceLock}};
    use inproc::{message::Message, window::Window};

    /// Receipts waiting for their fence message: posted by the publisher,
    /// acknowledged on the game's thread, without a lock.
    static REQUESTS: LazyLock<civ3r_sync::Posted<Receipt>> = LazyLock::new(civ3r_sync::Posted::new);
    thread_local! {
        static INBOX: RefCell<civ3r_sync::Inbox<Receipt>> = RefCell::new(civ3r_sync::Inbox::default());
    }

    fn message_id() -> u32 {
        static ID: OnceLock<u32> = OnceLock::new();
        *ID.get_or_init(|| inproc::message::register("civ3-renderer/preferences-input-fence/v1"))
    }

    pub(super) fn post(window: Window, receipt: Receipt) -> bool {
        if !crate::world_input::ready(window) { return false; }
        let message = message_id();
        if message == 0 { return false; }
        let (token, _) = REQUESTS.push(receipt, None);
        if window.post_message(message, token) { return true; }
        REQUESTS.retract(token);
        false
    }

    fn accept_id() -> u32 {
        static ID: OnceLock<u32> = OnceLock::new();
        *ID.get_or_init(|| inproc::message::register("civ3-renderer/preferences-native-accept/v1"))
    }

    /// Queued after the forwarded input, so the game handles everything
    /// before it first, like a click that reaches its round button.
    pub(super) fn post_accept(window: Window) -> bool {
        let message = accept_id();
        message != 0 && crate::world_input::ready(window) && window.post_message(message, 0)
    }

    pub(super) fn handle_message(message: Message) -> Option<isize> {
        if message.id != 0 && message.id == accept_id() {
            // The game thread, between messages: where its own Button
            // notification would run the same callback.
            match civ3_sys::preferences::accept(civ3_sys::Build::PcGames) {
                Ok(true) => log::info!("preferences: native Accept (round button callback)"),
                Ok(false) => log::debug!("preferences: native Accept skipped, the form is no longer open"),
                Err(error) => log::error!("preferences: native Accept unavailable: {error}"),
            }
            return Some(0);
        }
        if message.id == 0 || message.id != message_id() { return None; }
        let receipts = INBOX.with_borrow_mut(|inbox| REQUESTS.take(message.wparam as u32, inbox));
        receipts.into_iter().for_each(Receipt::acknowledge);
        Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_delivery_acknowledgment_can_settle_the_latest_edit() {
        let mut delivery = Delivery::default();
        assert!(delivery.settled());
        delivery.edited();
        let first = delivery.receipt().unwrap();
        delivery.posted(&first);
        assert!(!delivery.settled(), "posting a fence is not its delivery");
        assert!(delivery.receipt().is_none());
        delivery.edited();
        first.acknowledge();
        assert!(!delivery.settled(), "an old acknowledgment cannot clear a newer queued edit");
        let second = delivery.receipt().unwrap();
        delivery.posted(&second);
        second.acknowledge();
        assert!(delivery.settled());
    }

    #[test]
    fn reordered_or_old_dialog_receipts_cannot_clear_new_input() {
        let mut delivery = Delivery::default();
        delivery.edited();
        let first = delivery.receipt().unwrap();
        delivery.edited();
        let second = delivery.receipt().unwrap();
        second.acknowledge();
        first.clone().acknowledge();
        assert!(delivery.settled(), "old receipts cannot move the acknowledged epoch backwards");
        delivery = Delivery::default();
        delivery.edited();
        first.acknowledge();
        assert!(!delivery.settled(), "a previous dialog's receipt owns a different acknowledgment cell");
    }
}

#[cfg(test)]
pub(super) fn acknowledge_current(delivery: &Delivery) { if let Some(receipt) = delivery.receipt() { receipt.acknowledge(); } }
