//! Progressive whole-history search running on a background thread.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use git_raccoon_core::repo::History;
use git_raccoon_core::search::{self, SearchField};

/// Stop collecting once this many commits match.
pub const MAX_MATCHES: usize = 10_000;
const DEBOUNCE: Duration = Duration::from_millis(250);

enum Update {
    Found {
        rows: Vec<usize>,
        next: Option<usize>,
    },
    Failed(String),
}

pub struct Search {
    /// Whether the search bar is showing (Ctrl+F opens it, Esc closes it).
    pub open: bool,
    pub query: String,
    pub field: SearchField,
    /// Matching rows found so far, in history order.
    pub rows: Vec<usize>,
    pub matches: HashSet<usize>,
    /// Rows searched so far.
    pub searched: usize,
    pub total: usize,
    pub running: bool,
    pub truncated: bool,
    pub error: Option<String>,
    /// Bumping it stops the running search thread.
    generation: Arc<AtomicU64>,
    rx: Option<Receiver<Update>>,
    /// When the query last changed; the search starts after a short pause.
    changed_at: Option<Instant>,
    history: Option<Arc<History>>,
    ctx: Option<eframe::egui::Context>,
}

impl Default for Search {
    fn default() -> Self {
        Search {
            open: false,
            query: String::new(),
            field: SearchField::Text,
            rows: Vec::new(),
            matches: HashSet::new(),
            searched: 0,
            total: 0,
            running: false,
            truncated: false,
            error: None,
            generation: Arc::new(AtomicU64::new(0)),
            rx: None,
            changed_at: None,
            history: None,
            ctx: None,
        }
    }
}

impl Search {
    pub fn is_active(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Call after editing `query` or `field`.
    pub fn changed(&mut self, ctx: &eframe::egui::Context, history: &Arc<History>) {
        self.stop();
        self.rows.clear();
        self.matches.clear();
        self.searched = 0;
        self.total = history.total();
        self.truncated = false;
        self.error = None;
        self.running = self.is_active();
        self.changed_at = Some(Instant::now());
        self.history = Some(history.clone());
        self.ctx = Some(ctx.clone());
        ctx.request_repaint_after(DEBOUNCE);
    }

    /// Re-runs the current query against a reloaded history.
    pub fn restart(&mut self, history: &Arc<History>) {
        if let Some(ctx) = self.ctx.clone() {
            self.changed(&ctx, history);
            self.changed_at = Some(Instant::now() - DEBOUNCE);
        }
    }

    /// Clears the search and hides the bar.
    pub fn close(&mut self) {
        self.clear();
        self.open = false;
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.stop();
        self.rows.clear();
        self.matches.clear();
        self.running = false;
        self.changed_at = None;
    }

    fn stop(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.rx = None;
    }

    /// Starts the search once the query has settled and collects results.
    pub fn poll(&mut self) {
        if let Some(at) = self.changed_at
            && at.elapsed() >= DEBOUNCE
        {
            self.changed_at = None;
            if self.is_active()
                && let (Some(history), Some(ctx)) = (self.history.clone(), self.ctx.clone())
            {
                self.start(history, ctx);
            }
        } else if let (Some(_), Some(ctx)) = (self.changed_at, &self.ctx) {
            ctx.request_repaint_after(Duration::from_millis(50));
        }

        let Some(rx) = &self.rx else { return };
        let mut finished = false;
        while let Ok(update) = rx.try_recv() {
            match update {
                Update::Found { rows, next } => {
                    self.matches.extend(rows.iter().copied());
                    self.rows.extend(rows);
                    self.searched = next.unwrap_or(self.total);
                    if self.rows.len() > MAX_MATCHES {
                        self.rows.truncate(MAX_MATCHES);
                        self.truncated = true;
                        finished = true;
                    }
                    if next.is_none() {
                        finished = true;
                    }
                }
                Update::Failed(e) => {
                    self.error = Some(e);
                    finished = true;
                }
            }
        }
        if finished {
            self.running = false;
            self.stop();
        }
    }

    fn start(&mut self, history: Arc<History>, ctx: eframe::egui::Context) {
        let (tx, rx) = mpsc::channel();
        let generation = self.generation.clone();
        let mine = generation.fetch_add(1, Ordering::SeqCst) + 1;
        let query = self.query.clone();
        let field = self.field;
        std::thread::spawn(move || {
            let mut start = 0;
            loop {
                let update = match search::search(
                    &history,
                    &query,
                    field,
                    start,
                    search::SLICE,
                    &generation,
                    mine,
                ) {
                    Ok(r) if r.cancelled => return,
                    Ok(r) => {
                        let next = r.next;
                        if tx.send(Update::Found { rows: r.rows, next }).is_err() {
                            return;
                        }
                        ctx.request_repaint();
                        match next {
                            Some(n) => {
                                start = n;
                                continue;
                            }
                            None => return,
                        }
                    }
                    Err(e) => Update::Failed(e.to_string()),
                };
                let _ = tx.send(update);
                ctx.request_repaint();
                return;
            }
        });
        self.rx = Some(rx);
    }

    /// Position of `row` among the matches, if it is one.
    pub fn position(&self, row: Option<usize>) -> Option<usize> {
        let row = row?;
        if !self.matches.contains(&row) {
            return None;
        }
        self.rows.binary_search(&row).ok()
    }
}
