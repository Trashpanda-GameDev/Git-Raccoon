//! Noticing when the computer's network changes (a VPN connecting, Wi-Fi
//! coming back), from the system's own notifications, so what failed to
//! load can be tried again then.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

/// How long the network has to stay as it is before trying again: a VPN
/// takes a moment to set up its routes and names after its address.
const SETTLE: Duration = Duration::from_secs(2);

/// Watches the network interfaces and their addresses.
pub struct Watch {
    /// When it last changed, until that's taken.
    changed: Arc<Mutex<Option<Instant>>>,
    _handle: Option<netwatcher::WatchHandle>,
}

impl Watch {
    /// Starts watching; without the system's notifications, it never says
    /// the network changed (what failed still tries again on a timer).
    pub fn start(ctx: &egui::Context) -> Watch {
        let changed: Arc<Mutex<Option<Instant>>> = Arc::default();
        let (mark, ctx) = (Arc::clone(&changed), ctx.clone());
        let handle = netwatcher::watch_interfaces_with_callback(move |update| {
            if update.is_initial {
                return;
            }
            if let Ok(mut c) = mark.lock() {
                *c = Some(Instant::now());
            }
            ctx.request_repaint_after(SETTLE);
        })
        .ok();
        Watch {
            changed,
            _handle: handle,
        }
    }

    /// Notes a change, as if the system had said (tests).
    pub fn changed_now(&self) {
        if let Ok(mut c) = self.changed.lock() {
            *c = Some(Instant::now() - SETTLE);
        }
    }

    /// Whether the network changed and has settled since: once per change.
    pub fn settled_change(&self, ctx: &egui::Context) -> bool {
        let Ok(mut c) = self.changed.lock() else {
            return false;
        };
        match *c {
            Some(at) if at.elapsed() >= SETTLE => {
                *c = None;
                true
            }
            Some(at) => {
                ctx.request_repaint_after(SETTLE.saturating_sub(at.elapsed()));
                false
            }
            None => false,
        }
    }
}
