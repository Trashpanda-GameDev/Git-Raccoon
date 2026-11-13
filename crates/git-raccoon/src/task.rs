//! Running slow work (Git reads, decoding) off the UI thread.

use std::sync::mpsc::{self, Receiver};

use eframe::egui;

/// A result being computed on a background thread. The UI polls it each
/// frame; the thread asks for a repaint when it finishes.
pub struct Task<T> {
    rx: Receiver<T>,
}

impl<T: Send + 'static> Task<T> {
    pub fn spawn(ctx: &egui::Context, work: impl FnOnce() -> T + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // The receiver may be gone if the result is no longer wanted.
            let _ = tx.send(work());
            ctx.request_repaint();
        });
        Task { rx }
    }

    /// The result, once it is ready.
    pub fn poll(&self) -> Option<T> {
        self.rx.try_recv().ok()
    }
}

/// Data loaded for a key (a commit, a file), where the key changes as the
/// user clicks around. Results for keys no longer wanted are dropped; the
/// last result stays visible while the next one loads.
pub struct Keyed<K, T> {
    pending: Option<(K, Task<T>)>,
    loaded: Option<(K, T)>,
}

impl<K, T> Default for Keyed<K, T> {
    fn default() -> Self {
        Keyed {
            pending: None,
            loaded: None,
        }
    }
}

impl<K: PartialEq + Clone + Send + 'static, T: Send + 'static> Keyed<K, T> {
    /// Starts loading `key` unless it is loaded or loading already.
    pub fn want(
        &mut self,
        ctx: &egui::Context,
        key: &K,
        load: impl FnOnce(K) -> T + Send + 'static,
    ) {
        let have = self.loaded.as_ref().is_some_and(|(k, _)| k == key);
        let loading = self.pending.as_ref().is_some_and(|(k, _)| k == key);
        if !have && !loading {
            let k = key.clone();
            self.pending = Some((key.clone(), Task::spawn(ctx, move || load(k))));
        }
    }

    /// Moves a finished result into place. Call once per frame.
    pub fn poll(&mut self) {
        if let Some((key, task)) = &self.pending
            && let Some(value) = task.poll()
        {
            self.loaded = Some((key.clone(), value));
            self.pending = None;
        }
    }

    /// The result for `key`, if loaded.
    pub fn get(&self, key: &K) -> Option<&T> {
        self.loaded
            .as_ref()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// The most recent result, whatever its key.
    pub fn latest(&self) -> Option<&T> {
        self.loaded.as_ref().map(|(_, v)| v)
    }

    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }

    pub fn clear(&mut self) {
        self.pending = None;
        self.loaded = None;
    }
}
