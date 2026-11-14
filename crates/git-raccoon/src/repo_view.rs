//! The state of an open repository: its history, refs, selection and the
//! data loaded for whatever is selected.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use git_raccoon_core::details::{self, CommitDetails, LineKind};
use git_raccoon_core::refs::{self, Refs};
use git_raccoon_core::repo::{GraphCommit, History};
use git_raccoon_core::stage::{self, Pick};
use git_raccoon_core::watch::{Changes, RepoWatcher};
use git_raccoon_core::working::{self, Side, WorkingStatus};

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

/// Where a diff comes from: a commit, or one side of the working changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    Commit(String),
    Working(Side),
}

pub type DiffKey = (Source, String);

/// A change to the index the user asked for.
#[derive(Debug, Clone)]
pub enum Op {
    StageFile(String),
    UnstageFile(String),
    StageAll,
    UnstageAll,
    /// Stage (or, on the staged side, unstage) part of the open diff.
    Lines {
        side: Side,
        file: String,
        pick: Pick,
        headers: Vec<String>,
    },
}

impl Op {
    fn run(self, path: &str) -> Result<(), String> {
        match self {
            Op::StageFile(f) => stage::stage_file(path, &f),
            Op::UnstageFile(f) => stage::unstage_file(path, &f),
            Op::StageAll => stage::stage_all(path),
            Op::UnstageAll => stage::unstage_all(path),
            Op::Lines {
                side: Side::Unstaged,
                file,
                pick,
                headers,
            } => stage::stage_lines(path, &file, &pick, &headers),
            Op::Lines {
                side: Side::Staged,
                file,
                pick,
                headers,
            } => stage::unstage_lines(path, &file, &pick, &headers),
        }
        .map_err(|e| e.to_string())
    }

    fn verb(&self) -> &'static str {
        match self {
            Op::StageFile(_)
            | Op::StageAll
            | Op::Lines {
                side: Side::Unstaged,
                ..
            } => "stage",
            Op::UnstageFile(_)
            | Op::UnstageAll
            | Op::Lines {
                side: Side::Staged, ..
            } => "unstage",
        }
    }
}

/// How often to re-check the working tree when it can't be watched.
const STATUS_POLL: std::time::Duration = std::time::Duration::from_secs(3);

pub struct RepoView {
    pub history: Arc<History>,
    pub refs: Refs,
    pub rows: RowCache,
    pub selected: Option<Selection>,
    /// The working changes are selected (instead of a commit).
    pub working_selected: bool,
    /// Uncommitted changes; `None` until first loaded or for bare repos.
    pub status: Option<WorkingStatus>,
    status_task: Option<Task<Result<Option<WorkingStatus>, String>>>,
    /// Another status reload was requested while one was running.
    status_stale: bool,
    status_checked: std::time::Instant,
    /// Row to bring into view on the next frame.
    pub scroll_to: Option<usize>,
    pub details: Keyed<String, Result<Arc<CommitDetails>, String>>,
    /// The file whose diff is open, if any.
    pub active_file: Option<String>,
    /// For working changes: whether the open diff is the staged or unstaged one.
    pub active_side: Side,
    /// Lines picked in the open working diff, as (hunk, line).
    pub picked: std::collections::BTreeSet<(usize, usize)>,
    /// Where a Shift-click range of picks starts.
    pick_anchor: Option<(usize, usize)>,
    /// A staging operation in progress, and what it was.
    op: Option<(&'static str, Task<Result<(), String>>)>,
    pub diff: Keyed<DiffKey, Result<Arc<RenderedDiff>, String>>,
    pub images: Keyed<DiffKey, Result<Arc<ImagePair>, String>>,
    pub search: Search,
    pub sidebar_filter: String,
    /// Sidebar sections and folders the user collapsed.
    pub collapsed: std::collections::HashSet<String>,
    _watcher: Option<RepoWatcher>,
    /// Whether the working tree is watched; if not, the status is polled.
    watching_files: bool,
    history_changed: Arc<AtomicBool>,
    status_changed: Arc<AtomicBool>,
    refreshing: Option<Task<Result<Loaded, String>>>,
    pub refresh_error: Option<String>,
}

impl RepoView {
    pub fn new(ctx: &egui::Context, loaded: Loaded) -> Self {
        let history = Arc::new(loaded.history);
        let history_changed = Arc::new(AtomicBool::new(false));
        let status_changed = Arc::new(AtomicBool::new(false));
        let watcher = {
            let (h, s) = (history_changed.clone(), status_changed.clone());
            let ctx = ctx.clone();
            // Without a watcher the app still works; refresh is manual.
            RepoWatcher::new(history.path(), move |c: Changes| {
                if c.history {
                    h.store(true, Ordering::Relaxed);
                }
                if c.status {
                    s.store(true, Ordering::Relaxed);
                }
                ctx.request_repaint();
            })
            .ok()
        };
        let watching_files = watcher
            .as_ref()
            .is_some_and(RepoWatcher::watches_working_tree);
        // Load the status straight away.
        status_changed.store(true, Ordering::Relaxed);
        RepoView {
            rows: RowCache::new(history.clone()),
            history,
            refs: loaded.refs,
            selected: None,
            working_selected: false,
            status: None,
            status_task: None,
            status_stale: false,
            status_checked: std::time::Instant::now(),
            scroll_to: None,
            details: Keyed::default(),
            active_file: None,
            active_side: Side::Unstaged,
            picked: std::collections::BTreeSet::new(),
            pick_anchor: None,
            op: None,
            diff: Keyed::default(),
            images: Keyed::default(),
            search: Search::default(),
            sidebar_filter: String::new(),
            collapsed: std::collections::HashSet::new(),
            _watcher: watcher,
            watching_files,
            history_changed,
            status_changed,
            refreshing: None,
            refresh_error: None,
        }
    }

    pub fn path(&self) -> &str {
        self.history.path()
    }

    /// The open diff's key: the source (commit or working side) and file.
    pub fn diff_key(&self) -> Option<DiffKey> {
        let file = self.active_file.clone()?;
        if self.working_selected {
            Some((Source::Working(self.active_side), file))
        } else {
            Some((Source::Commit(self.selected.as_ref()?.id.clone()), file))
        }
    }

    /// Whether the selection's details (or the open diff) are still missing.
    pub fn is_loading_selection(&self) -> bool {
        if let Some(sel) = &self.selected
            && self.details.get(&sel.id).is_none()
        {
            return true;
        }
        match (self.diff_key(), &self.active_file) {
            (Some(key), Some(file)) => {
                self.diff.get(&key).is_none()
                    || (crate::images::is_image(file) && self.images.get(&key).is_none())
            }
            _ => false,
        }
    }

    pub fn is_loading_status(&self) -> bool {
        self.status_task.is_some() || self.status_changed.load(Ordering::Relaxed)
    }

    pub fn is_staging(&self) -> bool {
        self.op.is_some()
    }

    /// Starts a staging operation unless one is already running.
    pub fn run(&mut self, ctx: &egui::Context, op: Op) {
        if self.op.is_some() {
            return;
        }
        let verb = op.verb();
        let path = self.path().to_string();
        self.op = Some((verb, Task::spawn(ctx, move || op.run(&path))));
    }

    /// The open diff, if it has loaded.
    fn open_diff(&self) -> Option<Arc<RenderedDiff>> {
        let key = self.diff_key()?;
        self.diff.get(&key)?.as_ref().ok().cloned()
    }

    /// Picks or unpicks a changed line of the open working diff. With
    /// `extend`, every changed line from the last pick to this one is picked.
    pub fn toggle_pick(&mut self, hunk: usize, line: usize, extend: bool) {
        let Some(diff) = self.open_diff() else { return };
        match (extend, self.pick_anchor) {
            (true, Some(anchor)) => {
                let (from, to) = if anchor <= (hunk, line) {
                    (anchor, (hunk, line))
                } else {
                    ((hunk, line), anchor)
                };
                for (h, hk) in diff.diff.hunks.iter().enumerate() {
                    for (l, ln) in hk.lines.iter().enumerate() {
                        let changed = matches!(ln.kind, LineKind::Add | LineKind::Delete);
                        if changed && (h, l) >= from && (h, l) <= to {
                            self.picked.insert((h, l));
                        }
                    }
                }
            }
            _ => {
                if !self.picked.remove(&(hunk, line)) {
                    self.picked.insert((hunk, line));
                }
                self.pick_anchor = Some((hunk, line));
            }
        }
    }

    fn lines_op(&self, pick: Pick) -> Option<Op> {
        let diff = self.open_diff()?;
        Some(Op::Lines {
            side: self.active_side,
            file: self.active_file.clone()?,
            pick,
            headers: diff.diff.hunks.iter().map(|h| h.header.clone()).collect(),
        })
    }

    /// Stages (or unstages) a whole hunk of the open working diff.
    pub fn stage_hunk(&mut self, ctx: &egui::Context, hunk: usize) {
        if let Some(op) = self.lines_op(Pick::Hunk(hunk)) {
            self.run(ctx, op);
        }
    }

    /// Stages (or unstages) the picked lines of the open working diff.
    pub fn stage_picked(&mut self, ctx: &egui::Context) {
        if self.picked.is_empty() {
            return;
        }
        if let Some(op) = self.lines_op(Pick::Lines(self.picked.iter().copied().collect())) {
            self.run(ctx, op);
        }
    }

    /// Re-reads the working changes in the background.
    pub fn refresh_status(&mut self, ctx: &egui::Context) {
        if self.status_task.is_some() {
            self.status_stale = true;
            return;
        }
        let path = self.path().to_string();
        self.status_checked = std::time::Instant::now();
        self.status_task = Some(Task::spawn(ctx, move || {
            working::working_status(&path).map_err(|e| e.to_string())
        }));
    }

    pub fn is_refreshing(&self) -> bool {
        self.refreshing.is_some()
    }

    /// Reloads the repository in the background, keeping the selection.
    pub fn refresh(&mut self, ctx: &egui::Context) {
        let path = self.path().to_string();
        self.refreshing = Some(Task::spawn(ctx, move || load(&path)));
        self.refresh_status(ctx);
    }

    /// Polls background work; call once per frame.
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.details.poll();
        self.diff.poll();
        self.images.poll();
        self.search.poll();

        if self.history_changed.swap(false, Ordering::Relaxed) && self.refreshing.is_none() {
            self.refresh(ctx);
        }
        if self.status_changed.swap(false, Ordering::Relaxed) {
            self.refresh_status(ctx);
        }
        if !self.watching_files {
            if self.status_checked.elapsed() >= STATUS_POLL {
                self.refresh_status(ctx);
            }
            ctx.request_repaint_after(STATUS_POLL);
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
        if let Some((verb, task)) = &self.op
            && let Some(result) = task.poll()
        {
            let verb = *verb;
            self.op = None;
            self.picked.clear();
            self.pick_anchor = None;
            if let Err(e) = result {
                self.refresh_error = Some(format!("Could not {verb}: {e}"));
            }
            self.refresh_status(ctx);
        }
        if let Some(task) = &self.status_task
            && let Some(result) = task.poll()
        {
            self.status_task = None;
            match result {
                Ok(status) => self.set_status(status),
                Err(e) => self.refresh_error = Some(e),
            }
            if std::mem::take(&mut self.status_stale) {
                self.refresh_status(ctx);
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
        }
        if let Some(key) = self.diff_key() {
            // Working diffs change as files are edited: reload with each status.
            let path = self.path().to_string();
            self.diff.want(ctx, &key, move |(source, file)| {
                match source {
                    Source::Commit(id) => details::file_diff(&path, &id, &file),
                    Source::Working(side) => working::working_diff(&path, side, &file),
                }
                .map(|d| Arc::new(RenderedDiff::new(d)))
                .map_err(|e| e.to_string())
            });
            if crate::images::is_image(&key.1) {
                let path = self.path().to_string();
                let ctx2 = ctx.clone();
                self.images.want(ctx, &key, move |(source, file)| {
                    match source {
                        Source::Commit(id) => details::file_blobs(&path, &id, &file),
                        Source::Working(side) => working::working_blobs(&path, side, &file),
                    }
                    .map(|b| Arc::new(ImagePair::decode(&ctx2, &file, b)))
                    .map_err(|e| e.to_string())
                });
            }
        }
    }

    /// Takes in a newly read status. An open working diff is reloaded, and
    /// closed if its file is no longer changed.
    fn set_status(&mut self, status: Option<WorkingStatus>) {
        self.status = status;
        if !self.working_selected {
            return;
        }
        // The file may have been edited even if the list looks the same.
        if let Some(Source::Working(_)) = self.diff_key().map(|k| k.0) {
            self.diff.reload();
            self.images.reload();
        }
        let still_listed = |file: &str, side| {
            self.status
                .as_ref()
                .is_some_and(|s| s.files(side).iter().any(|f| f.path == file))
        };
        if let Some(file) = self.active_file.clone()
            && !still_listed(&file, self.active_side)
        {
            // Fully staged (or unstaged): follow the file to the other side.
            let other = match self.active_side {
                Side::Staged => Side::Unstaged,
                Side::Unstaged => Side::Staged,
            };
            if still_listed(&file, other) {
                self.active_side = other;
            } else {
                self.active_file = None;
            }
            self.picked.clear();
        }
        if self.status.as_ref().is_none_or(WorkingStatus::is_clean) {
            self.working_selected = false;
        }
    }

    /// Selects the working changes.
    pub fn select_working(&mut self) {
        if !self.working_selected {
            self.active_file = None;
        }
        self.selected = None;
        self.working_selected = true;
    }

    /// Opens a working file's staged or unstaged diff.
    pub fn open_working_file(&mut self, side: Side, file: String) {
        if self.active_side != side || self.active_file.as_deref() != Some(file.as_str()) {
            self.picked.clear();
        }
        self.active_side = side;
        self.active_file = Some(file);
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
        if self.working_selected || self.selected.as_ref().is_some_and(|s| s.id != id) {
            self.active_file = None;
        }
        self.working_selected = false;
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
        self.working_selected = false;
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
