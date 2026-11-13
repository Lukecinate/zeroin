//! Bounded hand-off of raw input events from the input thread to the UI.
//!
//! Raw Input is registered with `RIDEV_INPUTSINK`, so reports keep arriving
//! while ZeroIn is in the background, and eframe runs no frames for an idle or
//! minimized window. The queue therefore drops events while the UI does not
//! want them (`set_accept(false)`) and caps what can pile up between two
//! frames, so memory stays bounded however long the app sits behind a game.

// Used by the Windows raw-input thread; other platforms only run the tests.
#![cfg_attr(not(windows), allow(dead_code))]

use super::InputEvent;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::Arc;

/// About one second of reports at 8000 Hz plus keyboard events.
pub const DEFAULT_CAPACITY: usize = 16_384;

struct Shared {
    accept: AtomicBool,
    dropped: AtomicU64,
}

/// Input-thread side.
pub struct EventSender {
    tx: SyncSender<InputEvent>,
    shared: Arc<Shared>,
}

/// UI side.
pub struct EventReceiver {
    rx: Receiver<InputEvent>,
    shared: Arc<Shared>,
}

/// A bounded queue that starts out accepting events.
pub fn event_queue(capacity: usize) -> (EventSender, EventReceiver) {
    let (tx, rx) = sync_channel(capacity.max(1));
    let shared = Arc::new(Shared { accept: AtomicBool::new(true), dropped: AtomicU64::new(0) });
    (EventSender { tx, shared: shared.clone() }, EventReceiver { rx, shared })
}

impl EventSender {
    /// Queue an event without ever blocking the input thread. Returns false
    /// when the event was dropped (not wanted, queue full, or UI gone).
    pub fn push(&self, ev: InputEvent) -> bool {
        if !self.shared.accept.load(Ordering::Relaxed) {
            return false;
        }
        match self.tx.try_send(ev) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                self.shared.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
}

impl EventReceiver {
    /// Tell the input thread whether events are wanted (the window has focus).
    pub fn set_accept(&self, on: bool) {
        self.shared.accept.store(on, Ordering::Relaxed);
    }

    /// Move everything queued into `out`.
    pub fn drain(&self, out: &mut Vec<InputEvent>) {
        out.extend(self.rx.try_iter());
    }

    /// Events dropped because the queue was full.
    #[cfg(test)]
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::InputKind;

    fn ev(t: u64) -> InputEvent {
        InputEvent { t_us: t, device: 1, kind: InputKind::Motion { dx: 1, dy: 0 } }
    }

    #[test]
    fn queue_never_grows_past_capacity() {
        let (tx, rx) = event_queue(100);
        // An hour at 8 kHz would be 28.8 M events; a fraction proves the cap.
        let sent = (0..200_000u64).filter(|&t| tx.push(ev(t))).count();
        assert_eq!(sent, 100);
        assert_eq!(rx.dropped(), 199_900);
        let mut out = Vec::new();
        rx.drain(&mut out);
        assert_eq!(out.len(), 100);
        // Oldest kept, newest dropped; after draining there is room again.
        assert_eq!(out[0].t_us, 0);
        assert!(tx.push(ev(1)));
    }

    #[test]
    fn nothing_is_queued_while_not_accepting() {
        let (tx, rx) = event_queue(DEFAULT_CAPACITY);
        rx.set_accept(false);
        for t in 0..10_000 {
            assert!(!tx.push(ev(t)));
        }
        let mut out = Vec::new();
        rx.drain(&mut out);
        assert!(out.is_empty());
        assert_eq!(rx.dropped(), 0, "unwanted events are not counted as overflow");
        rx.set_accept(true);
        assert!(tx.push(ev(1)));
        rx.drain(&mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn sender_from_another_thread_never_blocks() {
        let (tx, rx) = event_queue(64);
        let h = std::thread::spawn(move || (0..100_000u64).filter(|&t| tx.push(ev(t))).count());
        let sent = h.join().unwrap();
        assert!(sent <= 64);
        let mut out = Vec::new();
        rx.drain(&mut out);
        assert_eq!(out.len(), sent);
    }

    #[test]
    fn push_after_receiver_dropped_is_harmless() {
        let (tx, rx) = event_queue(4);
        drop(rx);
        assert!(!tx.push(ev(0)));
    }
}
