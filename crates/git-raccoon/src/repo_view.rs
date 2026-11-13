//! The state of an open repository: its history, refs, selection and the
//! data loaded for whatever is selected.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use git_raccoon_core::details::{self, CommitDetails};
use git_raccoon_core::refs::{self, Refs};
use git_raccoon_core::repo::{GraphCommit, History};
use git_raccoon_core::watch::RepoWatcher;

use crate::highlight::RenderedDiff;
use crate::images::ImagePair;
use crate::search::Search;
use crate::task::{Keyed, Task};

/// Rows are read from the history in blocks of this many.
const BLOCK: usize = 256;
/// Forget cached rows past this many (they are cheap to read again).
const MAX_CACHED: usize = 20_000;

/// A freshly read repository.
pub struct Loaded {
    pub history: History,
    pub refs: Refs,
}

pub fn load(path: &str) -> Result<Loaded, String> {
    let history = History::load(path, 0).map_err(|e| e.to_string())?;
    let refs = refs::load_refs(path).map_err(|e| e.to_string())?;
    Ok(Loaded { history, refs })
}

/// Rows of the history read on demand and kept for reuse.
pub struct RowCache {
    history: Arc<History>,
    rows: HashMap<usize, GraphCommit>,
}

impl RowCache {
    fn new(history: Arc<History>) -> Self {
        RowCache {
            history,
            rows: HashMap::new(),
        }
    }

    /// Makes sure every row in `range` is read.
    pub fn ensure(&mut self, range: Range<usize>) {
        let end = range.end.min(self.history.total());
        if range.start >= end {
            return;
        }
        if self.rows.len() > MAX_CACHED {
            self.rows.clear();
        }
        for block in range.start / BLOCK..=(end - 1) / BLOCK {
            let start = block * BLOCK;
            if self.rows.contains_key(&start)
                && self.rows.contains_key(&(start + BLOCK - 1).min(end - 1))
            {
                continue;
            }
            if let Ok(page) = self.history.page(start, BLOCK) {
                for (i, commit) in page.commits.into_iter().enumerate() {
                    self.rows.insert(start + i, commit);
                }
            }
        }
    }

    pub fn get(&self, row: usize) -> Option<&GraphCommit> {
        self.rows.get(&row)
    }
}

pub struct Selection {
    pub id: String,
    /// Its row, if it is in the history (stashes aren't).
    pub row: Option<usize>,
}

pub type DiffKey = (String, String);

pub struct RepoView {
    pub history: Arc<History>,
    pub refs: Refs,
    pub rows: RowCache,
    pub selected: Option<Selection>,
    /// Row to bring into view on the next frame.
    pub scroll_to: Option<usize>,
    pub details: Keyed<String, Result<Arc<CommitDetails>, String>>,
    /// The file whose diff is open, if any.
    pub active_file: Option<String>,
    pub diff: Keyed<DiffKey, Result<Arc<RenderedDiff>, String>>,
    pub images: Keyed<DiffKey, Result<Arc<ImagePair>, String>>,
    pub search: Search,
    pub sidebar_filter: String,
    /// Sidebar sections and folders the user collapsed.
    pub collapsed: std::collections::HashSet<String>,
    _watcher: Option<RepoWatcher>,
    changed: Arc<AtomicBool>,
    refreshing: Option<Task<Result<Loaded, String>>>,
    pub refresh_error: Option<String>,
}

impl RepoView {
    pub fn new(ctx: &egui::Context, loaded: Loaded) -> Self {
        let history = Arc::new(loaded.history);
        let changed = Arc::new(AtomicBool::new(false));
        let watcher = {
            let changed = changed.clone();
            let ctx = ctx.clone();
            // Without a watcher the app still works; refresh is manual.
            RepoWatcher::new(history.path(), move || {
                changed.store(true, Ordering::Relaxed);
                ctx.request_repaint();
            })
            .ok()
        };
        RepoView {
            rows: RowCache::new(history.clone()),
            history,
            refs: loaded.refs,
            selected: None,
            scroll_to: None,
            details: Keyed::default(),
            active_file: None,
            diff: Keyed::default(),
            images: Keyed::default(),
            search: Search::default(),
            sidebar_filter: String::new(),
            collapsed: std::collections::HashSet::new(),
            _watcher: watcher,
            changed,
            refreshing: None,
            refresh_error: None,
        }
    }

    pub fn path(&self) -> &str {
        self.history.path()
    }

    /// Whether the selection's details (or the open diff) are still missing.
    pub fn is_loading_selection(&self) -> bool {
        let Some(sel) = &self.selected else {
            return false;
        };
        if self.details.get(&sel.id).is_none() {
            return true;
        }
        match &self.active_file {
            Some(file) => {
                let key = (sel.id.clone(), file.clone());
                self.diff.get(&key).is_none()
                    || (crate::images::is_image(file) && self.images.get(&key).is_none())
            }
            None => false,
        }
    }

    pub fn is_refreshing(&self) -> bool {
        self.refreshing.is_some()
    }

    /// Reloads the repository in the background, keeping the selection.
    pub fn refresh(&mut self, ctx: &egui::Context) {
        let path = self.path().to_string();
        self.refreshing = Some(Task::spawn(ctx, move || load(&path)));
    }

    /// Polls background work; call once per frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.details.poll();
        self.diff.poll();
        self.images.poll();
        self.search.poll();

        if self.changed.swap(false, Ordering::Relaxed) && self.refreshing.is_none() {
            self.refresh(ctx);
        }
        if let Some(task) = &self.refreshing
            && let Some(result) = task.poll()
        {
            self.refreshing = None;
            match result {
                Ok(loaded) => self.replace(loaded),
                Err(e) => self.refresh_error = Some(e),
            }
        }

        // Load whatever the selection needs.
        if let Some(sel) = &self.selected {
            let path = self.path().to_string();
            self.details.want(ctx, &sel.id, move |id| {
                details::commit_details(&path, &id)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            });
            if let Some(file) = &self.active_file {
                let key = (sel.id.clone(), file.clone());
                let path = self.path().to_string();
                self.diff.want(ctx, &key, move |(id, file)| {
                    details::file_diff(&path, &id, &file)
                        .map(|d| Arc::new(RenderedDiff::new(d)))
                        .map_err(|e| e.to_string())
                });
                if crate::images::is_image(file) {
                    let path = self.path().to_string();
                    let ctx2 = ctx.clone();
                    self.images.want(ctx, &key, move |(id, file)| {
                        details::file_blobs(&path, &id, &file)
                            .map(|b| Arc::new(ImagePair::decode(&ctx2, &file, b)))
                            .map_err(|e| e.to_string())
                    });
                }
            }
        }
    }

    /// Swaps in a reloaded history, keeping the selection and open diff.
    fn replace(&mut self, loaded: Loaded) {
        self.history = Arc::new(loaded.history);
        self.refs = loaded.refs;
        self.rows = RowCache::new(self.history.clone());
        self.refresh_error = None;
        if let Some(sel) = &mut self.selected {
            sel.row = self.history.locate(&sel.id);
        }
        self.search.restart(&self.history);
    }

    /// Selects a commit by id and scrolls to it.
    pub fn select(&mut self, id: &str) {
        let row = self.history.locate(id);
        self.select_row(id.to_string(), row);
    }

    pub fn select_row(&mut self, id: String, row: Option<usize>) {
        if self.selected.as_ref().is_some_and(|s| s.id != id) {
            self.active_file = None;
        }
        self.selected = Some(Selection { id, row });
        self.scroll_to = row;
    }

    /// Selects the commit at a history row.
    pub fn select_index(&mut self, row: usize) {
        self.rows.ensure(row..row + 1);
        if let Some(c) = self.rows.get(row) {
            let id = c.id.clone();
            self.select_row(id, Some(row));
        }
    }

    pub fn deselect(&mut self) {
        self.selected = None;
        self.active_file = None;
    }

    pub fn selected_row(&self) -> Option<usize> {
        self.selected.as_ref().and_then(|s| s.row)
    }

    /// Moves the selection by `step` rows (clamped).
    pub fn step(&mut self, step: isize) {
        let total = self.history.total();
        if total == 0 {
            return;
        }
        let current = self
            .selected_row()
            .map_or(-1, |r| isize::try_from(r).unwrap_or(isize::MAX));
        let last = isize::try_from(total - 1).unwrap_or(isize::MAX);
        let next = (current + step).clamp(0, last);
        self.select_index(usize::try_from(next).unwrap_or(0));
    }

    /// Rows matching the current search, in history order.
    pub fn search_rows(&self) -> &[usize] {
        &self.search.rows
    }

    /// Selects the next (or previous) search match after the selection.
    pub fn step_match(&mut self, forward: bool) {
        let rows = &self.search.rows;
        if rows.is_empty() {
            return;
        }
        let target = match (forward, self.selected_row()) {
            (true, Some(from)) => rows.iter().copied().find(|&r| r > from).unwrap_or(rows[0]),
            (true, None) => rows[0],
            (false, Some(from)) => rows
                .iter()
                .rev()
                .copied()
                .find(|&r| r < from)
                .unwrap_or(rows[rows.len() - 1]),
            (false, None) => rows[rows.len() - 1],
        };
        self.select_index(target);
    }
}
