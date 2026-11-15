//! The state of an open repository: its history, refs, selection and the
//! data loaded for whatever is selected.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use git_raccoon_core::commit::{self, Committed};
use git_raccoon_core::details::{self, CommitDetails, LineKind};
use git_raccoon_core::merge::{self, MergeMode};
use git_raccoon_core::refs::{self, Refs};
use git_raccoon_core::repo::{GraphCommit, History};
use git_raccoon_core::stage::{self, Pick};
use git_raccoon_core::sync::{self, Progress, PullMode};
use git_raccoon_core::watch::{Changes, RepoWatcher};
use git_raccoon_core::working::{self, Side, WorkingStatus};
use git_raccoon_core::{Error, branch, conflict, discard, remote, stash, undo};

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
    /// HEAD's commit when it was read.
    pub head: Option<String>,
}

pub fn load(path: &str) -> Result<Loaded, String> {
    let history = History::load(path, 0).map_err(|e| e.to_string())?;
    let refs = refs::load_refs(path).map_err(|e| e.to_string())?;
    let head = undo::head_id(path).ok().flatten();
    Ok(Loaded {
        history,
        refs,
        head,
    })
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

/// The commit message being written.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommitDraft {
    pub summary: String,
    pub body: String,
    /// Replace the last commit instead of adding one.
    pub amend: bool,
    /// The last commit is already on the upstream (amending rewrites it).
    pub head_pushed: bool,
}

impl CommitDraft {
    pub fn message(&self) -> String {
        let body = self.body.trim();
        if body.is_empty() {
            self.summary.trim().to_string()
        } else {
            format!("{}\n\n{body}", self.summary.trim())
        }
    }
}

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
    /// Discard unstaged changes; current versions go to the Recycle Bin
    /// (or to the folder given, in tests).
    Discard(Vec<String>, Option<std::path::PathBuf>),
    Stash {
        message: String,
        untracked: bool,
    },
    /// Stash operations, by stash commit id.
    StashApply(String),
    StashPop(String),
    StashDrop(String),
    CreateBranch {
        name: String,
        start: String,
        checkout: bool,
    },
    Checkout(String),
    /// Check out a remote branch ("origin/x") through a tracking branch.
    CheckoutRemote(String),
    CheckoutDetached(String),
    RenameBranch {
        old: String,
        new: String,
    },
    DeleteBranch {
        name: String,
        force: bool,
    },
    CreateTag {
        name: String,
        target: String,
        message: String,
    },
    DeleteTag(String),
    /// Stash everything (untracked files too), then run the operation:
    /// for a checkout that local changes were in the way of.
    StashThen(Box<Op>),
    // What undoing uses:
    /// Move the branch back from commit `expected` to `to`, keeping changes staged.
    UndoCommit {
        expected: String,
        to: String,
    },
    /// Delete a branch only if it still points at `expected`.
    DeleteBranchAt {
        name: String,
        expected: String,
    },
    RestoreBranch {
        name: String,
        id: String,
        upstream: Option<String>,
    },
    RestoreTag {
        name: String,
        id: String,
    },
    StashStore {
        id: String,
        message: String,
    },
    /// Several operations in order, stopping at the first failure.
    Then(Vec<Op>),
    Fetch,
    /// Fetch one remote.
    FetchRemote(String),
    Pull(PullMode),
    /// Push the checked-out branch; `force` only with lease.
    Push {
        force: bool,
    },
    AddRemote {
        name: String,
        url: String,
    },
    RenameRemote {
        old: String,
        new: String,
    },
    SetRemoteUrl {
        name: String,
        url: String,
    },
    RemoveRemote(String),
    /// Merge a branch (local, or remote like "origin/main") into HEAD.
    Merge {
        branch: String,
        mode: MergeMode,
    },
    /// Continue or abort the merge, rebase, cherry-pick or revert in progress.
    Continue,
    Abort,
    /// Resolve a conflicted file with one side's whole version.
    TakeSide {
        file: String,
        ours: bool,
    },
    /// Write a conflicted file's new text (if it still reads `expected`).
    WriteConflict {
        file: String,
        expected: String,
        text: String,
    },
    MarkResolved(String),
    /// Undo a merge: back from `expected` to `to`, keeping local changes.
    UndoMerge {
        expected: String,
        to: String,
    },
}

/// Shared with a running fetch, pull or push: its progress, and the switch
/// that cancels it.
pub struct Control {
    pub progress: std::sync::Mutex<Option<Progress>>,
    pub cancel: AtomicBool,
    ctx: egui::Context,
}

impl Control {
    fn report(&self, p: Progress) {
        if let Ok(mut slot) = self.progress.lock() {
            *slot = Some(p);
        }
        self.ctx.request_repaint();
    }
}

impl Op {
    fn run(self, path: &str, ctl: &Control) -> Result<(), Error> {
        let report = &mut |p| ctl.report(p);
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
            Op::Discard(files, None) => discard::discard(path, &files).map(drop),
            Op::Discard(files, Some(folder)) => {
                discard::discard_into(path, &files, &folder).map(drop)
            }
            Op::Stash { message, untracked } => stash::save(path, &message, untracked),
            Op::StashApply(id) => stash::apply(path, &id),
            Op::StashPop(id) => stash::pop(path, &id),
            Op::StashDrop(id) => stash::drop_stash(path, &id),
            Op::CreateBranch {
                name,
                start,
                checkout,
            } => branch::create(path, &name, &start, checkout),
            Op::Checkout(name) => branch::checkout(path, &name),
            Op::CheckoutRemote(name) => branch::checkout_remote(path, &name),
            Op::CheckoutDetached(id) => branch::checkout_detached(path, &id),
            Op::RenameBranch { old, new } => branch::rename(path, &old, &new),
            Op::DeleteBranch { name, force } => branch::delete(path, &name, force),
            Op::CreateTag {
                name,
                target,
                message,
            } => branch::create_tag(path, &name, &target, &message),
            Op::DeleteTag(name) => branch::delete_tag(path, &name),
            Op::StashThen(op) => {
                let what = op.verb();
                stash::save(path, &format!("Stashed by Git Raccoon to {what}"), true)?;
                op.run(path, ctl)
            }
            Op::UndoCommit { expected, to } => undo::reset_soft(path, &expected, &to),
            Op::DeleteBranchAt { name, expected } => undo::delete_branch_at(path, &name, &expected),
            Op::RestoreBranch { name, id, upstream } => {
                undo::restore_ref(path, &format!("refs/heads/{name}"), &id)?;
                if let Some(up) = upstream {
                    undo::set_upstream(path, &name, &up);
                }
                Ok(())
            }
            Op::RestoreTag { name, id } => {
                undo::restore_ref(path, &format!("refs/tags/{name}"), &id)
            }
            Op::StashStore { id, message } => stash::store(path, &id, &message),
            Op::Then(ops) => ops.into_iter().try_for_each(|op| op.run(path, ctl)),
            Op::Fetch => sync::fetch(path, report, &ctl.cancel),
            Op::FetchRemote(name) => sync::fetch_remote(path, &name, report, &ctl.cancel),
            Op::Pull(mode) => sync::pull(path, mode, report, &ctl.cancel),
            Op::Push { force } => sync::push(path, force, report, &ctl.cancel),
            Op::AddRemote { name, url } => remote::add(path, &name, &url),
            Op::RenameRemote { old, new } => remote::rename(path, &old, &new),
            Op::SetRemoteUrl { name, url } => remote::set_url(path, &name, &url),
            Op::RemoveRemote(name) => remote::remove(path, &name),
            Op::Merge { branch, mode } => merge::merge(path, &branch, mode).map(drop),
            Op::Continue => merge::continue_op(path),
            Op::Abort => merge::abort(path),
            Op::TakeSide { file, ours } => conflict::take_side(path, &file, ours),
            Op::WriteConflict {
                file,
                expected,
                text,
            } => conflict::write(path, &file, &expected, &text),
            Op::MarkResolved(file) => conflict::mark_resolved(path, &file),
            Op::UndoMerge { expected, to } => undo::reset_keep(path, &expected, &to),
        }
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
            Op::Discard(..) => "discard",
            Op::Stash { .. } => "stash",
            Op::StashApply(_) => "apply the stash",
            Op::StashPop(_) => "pop the stash",
            Op::StashDrop(_) => "drop the stash",
            Op::CreateBranch { checkout: true, .. } => "create and check out the branch",
            Op::CreateBranch { .. } => "create the branch",
            Op::Checkout(_) | Op::CheckoutRemote(_) => "check out the branch",
            Op::CheckoutDetached(_) => "check out the commit",
            Op::RenameBranch { .. } => "rename the branch",
            Op::DeleteBranch { .. } => "delete the branch",
            Op::CreateTag { .. } => "create the tag",
            Op::DeleteTag(_) => "delete the tag",
            Op::StashThen(op) => op.verb(),
            Op::UndoCommit { .. }
            | Op::DeleteBranchAt { .. }
            | Op::RestoreBranch { .. }
            | Op::RestoreTag { .. }
            | Op::StashStore { .. } => "undo",
            // Named after its first step ("add the remote", then fetch).
            Op::Then(ops) => ops.first().map_or("undo", Op::verb),
            Op::Fetch | Op::FetchRemote(_) => "fetch",
            Op::Pull(_) => "pull",
            Op::Push { .. } => "push",
            Op::AddRemote { .. } => "add the remote",
            Op::RenameRemote { .. } | Op::SetRemoteUrl { .. } => "change the remote",
            Op::RemoveRemote(_) => "remove the remote",
            Op::Merge { .. } => "merge",
            Op::Continue => "continue",
            Op::Abort => "abort",
            Op::TakeSide { .. } | Op::WriteConflict { .. } | Op::MarkResolved(_) => {
                "resolve the conflict"
            }
            Op::UndoMerge { .. } => "undo",
        }
    }

    /// Whether it changes refs, so the history and sidebar reload too.
    fn changes_refs(&self) -> bool {
        !matches!(
            self,
            Op::StageFile(_)
                | Op::UnstageFile(_)
                | Op::StageAll
                | Op::UnstageAll
                | Op::Lines { .. }
                | Op::Discard(..)
                | Op::TakeSide { .. }
                | Op::WriteConflict { .. }
                | Op::MarkResolved(_)
        )
    }

    /// Whether local changes in its way can be stashed first.
    fn can_stash_first(&self) -> bool {
        match self {
            Op::Merge { .. } => true,
            Op::Then(ops) => ops.first().is_some_and(Op::can_stash_first),
            op => op.is_checkout(),
        }
    }

    /// Whether it checks something out (and so can be blocked by local changes).
    fn is_checkout(&self) -> bool {
        matches!(
            self,
            Op::CreateBranch { checkout: true, .. }
                | Op::Checkout(_)
                | Op::CheckoutRemote(_)
                | Op::CheckoutDetached(_)
        )
    }
}

/// How to undo the last action.
#[derive(Debug, Clone)]
pub struct Undoable {
    /// What the button says ("Undo commit “Fix it”").
    pub label: String,
    pub op: Op,
    /// HEAD right after the action; if it moves elsewhere, the undo is dropped.
    head: Option<String>,
    created: std::time::Instant,
    /// For an undone commit: its message, to put back in the commit box.
    message: Option<String>,
}

impl Undoable {
    fn new(path: &str, label: String, op: Op) -> Self {
        Undoable {
            label,
            op,
            head: undo::head_id(path).ok().flatten(),
            created: std::time::Instant::now(),
            message: None,
        }
    }
}

/// What an operation will destroy, read just before it runs, so it can be
/// put back.
#[derive(Default)]
struct Before {
    tip: Option<String>,
    upstream: Option<String>,
    tag: Option<String>,
    stash_message: Option<String>,
    branch_existed: bool,
    stash_top: Option<String>,
}

impl Before {
    fn capture(op: &Op, path: &str) -> Self {
        let mut b = Before::default();
        match op {
            Op::DeleteBranch { name, .. } => {
                b.tip = branch::tip(path, name).ok();
                b.upstream = undo::upstream_of(path, name).ok().flatten();
            }
            Op::DeleteTag(name) => {
                b.tag = undo::ref_target(path, &format!("refs/tags/{name}")).ok()
            }
            Op::StashDrop(id) => {
                b.stash_message = refs::load_refs(path).ok().and_then(|r| {
                    r.stashes
                        .into_iter()
                        .find(|s| &s.target == id)
                        .map(|s| s.message)
                });
            }
            Op::CheckoutRemote(name) => {
                b.branch_existed = branch::tip(path, local_name(name)).is_ok();
            }
            Op::Stash { .. } => b.stash_top = stash::top(path).ok().flatten(),
            Op::StashThen(inner) => return Before::capture(inner, path),
            _ => {}
        }
        b
    }
}

/// "feature" for the remote branch "origin/feature".
fn local_name(remote_branch: &str) -> &str {
    remote_branch
        .split_once('/')
        .map_or(remote_branch, |(_, rest)| rest)
}

fn short(id: &str) -> &str {
    &id[..7.min(id.len())]
}

/// Switching back to where the last checkout came from, per the reflog.
fn checkout_back(path: &str) -> Option<Op> {
    let last = undo::last_head_move(path).ok()??;
    let from = last.checked_out_from()?;
    if branch::tip(path, from).is_ok() {
        Some(Op::Checkout(from.to_string()))
    } else {
        Some(Op::CheckoutDetached(last.from?))
    }
}

/// How to undo `op`, which just succeeded; `None` if it can't be undone.
fn undo_for(op: &Op, before: Before, path: &str) -> Option<Undoable> {
    let delete_new = |name: &str| {
        Some(Op::DeleteBranchAt {
            name: name.to_string(),
            expected: branch::tip(path, name).ok()?,
        })
    };
    let (label, ops): (String, Vec<Op>) = match op {
        Op::CreateBranch { name, checkout, .. } => {
            let mut ops = Vec::new();
            if *checkout {
                ops.push(checkout_back(path)?);
            }
            ops.push(delete_new(name)?);
            (format!("Undo create branch {name}"), ops)
        }
        Op::Checkout(name) => (format!("Undo check out {name}"), vec![checkout_back(path)?]),
        Op::CheckoutDetached(id) => (
            format!("Undo check out {}", short(id)),
            vec![checkout_back(path)?],
        ),
        Op::CheckoutRemote(name) => {
            let mut ops = vec![checkout_back(path)?];
            if !before.branch_existed {
                ops.push(delete_new(local_name(name))?);
            }
            (format!("Undo check out {name}"), ops)
        }
        Op::RenameBranch { old, new } => (
            format!("Undo rename {old} to {new}"),
            vec![Op::RenameBranch {
                old: new.clone(),
                new: old.clone(),
            }],
        ),
        Op::DeleteBranch { name, .. } => (
            format!("Undo delete branch {name}"),
            vec![Op::RestoreBranch {
                name: name.clone(),
                id: before.tip?,
                upstream: before.upstream,
            }],
        ),
        Op::CreateTag { name, .. } => (
            format!("Undo create tag {name}"),
            vec![Op::DeleteTag(name.clone())],
        ),
        Op::DeleteTag(name) => (
            format!("Undo delete tag {name}"),
            vec![Op::RestoreTag {
                name: name.clone(),
                id: before.tag?,
            }],
        ),
        Op::Stash { .. } => {
            let top = stash::top(path).ok()??;
            if before.stash_top.as_ref() == Some(&top) {
                return None; // nothing was stashed
            }
            ("Undo stash".into(), vec![Op::StashPop(top)])
        }
        Op::StashDrop(id) => (
            "Undo drop stash".into(),
            vec![Op::StashStore {
                id: id.clone(),
                message: before.stash_message?,
            }],
        ),
        Op::StashThen(inner) => {
            let top = stash::top(path).ok()??;
            let mut u = undo_for(inner, before, path)?;
            let mut ops = match u.op {
                Op::Then(ops) => ops,
                op => vec![op],
            };
            ops.push(Op::StashPop(top));
            u.op = Op::Then(ops);
            return Some(u);
        }
        Op::Merge { .. } | Op::Continue => {
            // Finished (no conflicts left in progress): undo from the reflog.
            if merge::in_progress(path).ok()?.is_some() {
                return None;
            }
            let last = undo::last_head_move(path).ok()??;
            if !last.is_merge() {
                return None;
            }
            let label = match op {
                Op::Merge { branch, .. } => format!("Undo merge of {branch}"),
                _ => "Undo merge".to_string(),
            };
            (
                label,
                vec![Op::UndoMerge {
                    expected: last.to.clone(),
                    to: last.from?,
                }],
            )
        }
        _ => return None,
    };
    let op = if ops.len() == 1 {
        ops.into_iter().next()?
    } else {
        Op::Then(ops)
    };
    Some(Undoable::new(path, label, op))
}

/// How to undo the commit that was just made, from the reflog.
fn undo_commit(path: &str, made: &Committed, amend: bool, message: &str) -> Option<Undoable> {
    let last = undo::last_head_move(path).ok()??;
    if !last.is_commit(amend) || last.to != made.id {
        return None;
    }
    let summary: String = message
        .lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(40)
        .collect();
    let label = if amend {
        format!("Undo amend \u{201c}{summary}\u{201d}")
    } else {
        format!("Undo commit \u{201c}{summary}\u{201d}")
    };
    let mut u = Undoable::new(
        path,
        label,
        Op::UndoCommit {
            expected: made.id.clone(),
            to: last.from?,
        },
    );
    u.message = (!amend).then(|| message.to_string());
    Some(u)
}

/// An operation running in the background.
struct Running {
    /// What was asked, to offer a way on if Git refuses.
    op: Op,
    /// For an undo of a commit: the message to put back in the commit box.
    message: Option<String>,
    control: Arc<Control>,
    task: Task<Result<Option<Undoable>, Error>>,
}

/// A conflicted file open for resolving.
pub struct ConflictFile {
    pub file: String,
    /// Its text as last read (or why it couldn't be read).
    pub text: Result<String, String>,
    /// The text being edited by hand, in Edit mode.
    pub edit: Option<String>,
}

/// What this system calls its trash.
pub fn trash_name() -> &'static str {
    if cfg!(windows) {
        "Recycle Bin"
    } else {
        "Trash"
    }
}

/// A question to answer before something happens.
pub enum Dialog {
    /// "Are you sure?" before an operation. `danger` colours the button red.
    Confirm {
        title: String,
        body: String,
        button: String,
        danger: bool,
        op: Op,
    },
    /// Stash the working changes, with an optional message.
    Stash {
        message: String,
        untracked: bool,
    },
    /// Name a new branch starting at `start` (a commit id or "HEAD").
    NewBranch {
        start: String,
        /// How to describe the start ("main", "a1b2c3d Fix the thing").
        start_label: String,
        name: String,
        checkout: bool,
    },
    RenameBranch {
        old: String,
        name: String,
    },
    /// Add a remote (`original` is `None`) or edit one.
    Remote {
        original: Option<String>,
        name: String,
        url: String,
    },
    /// A question with several answers (and Cancel); `bool` marks a risky one.
    Choice {
        title: String,
        body: String,
        choices: Vec<(String, bool, Op)>,
    },
    /// Name a new tag at `target`; a message makes it annotated.
    NewTag {
        target: String,
        target_label: String,
        name: String,
        message: String,
    },
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
    pub draft: CommitDraft,
    committing: Option<Task<Result<Option<Undoable>, String>>>,
    /// How to undo the last action, if it can be.
    pub undo: Option<Undoable>,
    /// The conflicted file being resolved, if one is open.
    pub conflict: Option<ConflictFile>,
    /// When the running refresh started (an older read can't drop a newer undo).
    refresh_started: std::time::Instant,
    /// Where a Shift-click range of picks starts.
    pick_anchor: Option<(usize, usize)>,
    /// An operation in progress.
    op: Option<Running>,
    /// A dialog waiting for an answer.
    pub dialog: Option<Dialog>,
    /// Tests only: where discarded files go instead of the Recycle Bin.
    pub trash_override: Option<std::path::PathBuf>,
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
    /// A background failure to show (already worded for the user).
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
            dialog: None,
            trash_override: None,
            draft: CommitDraft::default(),
            committing: None,
            undo: None,
            conflict: None,
            refresh_started: std::time::Instant::now(),
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

    pub fn is_committing(&self) -> bool {
        self.committing.is_some()
    }

    /// Commits (or amends) with the draft message, through `git`.
    pub fn commit(&mut self, ctx: &egui::Context) {
        if self.committing.is_some() || self.draft.summary.trim().is_empty() {
            return;
        }
        let (path, message, amend) = (
            self.path().to_string(),
            self.draft.message(),
            self.draft.amend,
        );
        self.committing = Some(Task::spawn(ctx, move || {
            let made = commit::commit(&path, &message, amend).map_err(|e| e.to_string())?;
            Ok(undo_commit(&path, &made, amend, &message))
        }));
    }

    /// Switches amending on or off. Turning it on fills in the last commit's
    /// message (if nothing was typed yet); turning it off clears that again.
    pub fn set_amend(&mut self, amend: bool) {
        if amend == self.draft.amend {
            return;
        }
        let path = self.path().to_string();
        let last = commit::head_message(&path)
            .ok()
            .flatten()
            .unwrap_or_default();
        let (summary, body) = last
            .split_once('\n')
            .map_or((last.as_str(), ""), |(s, b)| (s, b.trim()));
        if amend && self.draft.summary.trim().is_empty() && self.draft.body.trim().is_empty() {
            self.draft.summary = summary.to_string();
            self.draft.body = body.to_string();
        } else if !amend && self.draft.summary == summary && self.draft.body.trim() == body {
            self.draft.summary.clear();
            self.draft.body.clear();
        }
        self.draft.amend = amend;
        self.draft.head_pushed = amend && commit::head_is_pushed(&path).unwrap_or(false);
    }

    /// Whether any operation (or a commit) is running.
    pub fn op_running(&self) -> bool {
        self.op.is_some() || self.committing.is_some()
    }

    pub fn is_staging(&self) -> bool {
        self.op.is_some()
    }

    /// The stash with this commit id, if the id is a stash.
    pub fn stash(&self, id: &str) -> Option<&git_raccoon_core::refs::Stash> {
        self.refs.stashes.iter().find(|s| s.target == id)
    }

    /// Asks before discarding unstaged changes to `files`.
    pub fn confirm_discard(&mut self, files: Vec<String>) {
        let what = match files.as_slice() {
            [one] => one.clone(),
            many => format!("{} files", many.len()),
        };
        self.dialog = Some(Dialog::Confirm {
            title: format!("Discard changes to {what}?"),
            body: format!(
                "Unstaged changes are thrown away; staged changes stay. Each file's current version is moved to the {} first, so you can get it back from there.",
                trash_name()
            ),
            button: "Discard".into(),
            danger: true,
            op: Op::Discard(files, self.trash_override.clone()),
        });
    }

    /// Asks before dropping a stash.
    pub fn confirm_drop(&mut self, id: String) {
        let name = self.stash(&id).map_or_else(
            || "This stash".to_string(),
            |s| format!("\u{201c}{}\u{201d}", s.message),
        );
        self.dialog = Some(Dialog::Confirm {
            title: "Drop this stash?".into(),
            body: format!(
                "{name} will be deleted. Its contents can still be recovered from its id ({}) until Git cleans up.",
                &id[..7.min(id.len())]
            ),
            button: "Drop stash".into(),
            danger: true,
            op: Op::StashDrop(id),
        });
    }

    /// Starts an operation unless one is already running.
    pub fn run(&mut self, ctx: &egui::Context, op: Op) {
        if self.op.is_some() {
            return;
        }
        self.start(ctx, op, None);
    }

    fn start(&mut self, ctx: &egui::Context, op: Op, message: Option<String>) {
        let path = self.path().to_string();
        let task_op = op.clone();
        let control = Arc::new(Control {
            progress: std::sync::Mutex::new(None),
            cancel: AtomicBool::new(false),
            ctx: ctx.clone(),
        });
        let ctl = control.clone();
        self.op = Some(Running {
            op,
            message,
            control,
            task: Task::spawn(ctx, move || {
                let before = Before::capture(&task_op, &path);
                task_op.clone().run(&path, &ctl)?;
                Ok(undo_for(&task_op, before, &path))
            }),
        });
    }

    /// A running fetch, pull or push: what it is and how far it has got.
    pub fn sync_progress(&self) -> Option<(&'static str, Option<Progress>)> {
        let running = self.op.as_ref()?;
        if !matches!(
            running.op,
            Op::Fetch | Op::FetchRemote(_) | Op::Pull(_) | Op::Push { .. }
        ) {
            return None;
        }
        let progress = running.control.progress.lock().ok().and_then(|p| p.clone());
        Some((running.op.verb(), progress))
    }

    /// Stops a running fetch, pull or push.
    pub fn cancel_sync(&self) {
        if let Some(running) = &self.op {
            running.control.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// The checked-out branch's upstream, with ahead/behind counts.
    pub fn head_upstream(&self) -> Option<&git_raccoon_core::refs::Upstream> {
        self.refs
            .local
            .iter()
            .find(|b| b.is_head)?
            .upstream
            .as_ref()
    }

    /// Undoes the last action, if it can be undone.
    pub fn undo(&mut self, ctx: &egui::Context) {
        if self.op.is_some() || self.committing.is_some() {
            return;
        }
        if let Some(u) = self.undo.take() {
            self.start(ctx, u.op, u.message);
        }
    }

    /// The checked-out branch, if any.
    pub fn head_branch(&self) -> Option<&str> {
        self.refs
            .local
            .iter()
            .find(|b| b.is_head)
            .map(|b| b.name.as_str())
    }

    /// Checks out a local branch (nothing to do if it is already checked out).
    pub fn checkout(&mut self, ctx: &egui::Context, name: String) {
        if self.head_branch() != Some(name.as_str()) {
            self.run(ctx, Op::Checkout(name));
        }
    }

    /// Opens the new-branch dialog for a start point.
    pub fn new_branch(&mut self, start: String, start_label: String) {
        self.dialog = Some(Dialog::NewBranch {
            start,
            start_label,
            name: String::new(),
            checkout: true,
        });
    }

    pub fn new_tag(&mut self, target: String, target_label: String) {
        self.dialog = Some(Dialog::NewTag {
            target,
            target_label,
            name: String::new(),
            message: String::new(),
        });
    }

    pub fn rename_branch(&mut self, old: String) {
        self.dialog = Some(Dialog::RenameBranch {
            name: old.clone(),
            old,
        });
    }

    /// Asks before deleting a branch. Git itself refuses if its work isn't
    /// merged anywhere; that refusal leads to a second, stronger question.
    pub fn confirm_delete_branch(&mut self, name: String) {
        self.dialog = Some(Dialog::Confirm {
            title: format!("Delete branch {name}?"),
            body: "Only the branch name is removed; its commits stay. Git won't delete it if it has commits that aren't on any other branch.".into(),
            button: "Delete branch".into(),
            danger: true,
            op: Op::DeleteBranch { name, force: false },
        });
    }

    /// Opens a conflicted file for resolving.
    pub fn open_conflict(&mut self, file: String) {
        self.active_file = None;
        let text = conflict::read(self.path(), &file).map_err(|e| e.to_string());
        self.conflict = Some(ConflictFile {
            file,
            text,
            edit: None,
        });
    }

    /// Picks a side for one conflict in the open file.
    pub fn pick_conflict(&mut self, ctx: &egui::Context, index: usize, pick: conflict::Pick) {
        let Some(open) = &self.conflict else { return };
        let Ok(text) = &open.text else { return };
        let Some(chunks) = conflict::parse(text) else {
            return;
        };
        let op = Op::WriteConflict {
            file: open.file.clone(),
            expected: text.clone(),
            text: conflict::resolve(&chunks, index, pick),
        };
        self.run(ctx, op);
    }

    /// Saves the hand-edited text of the open conflicted file.
    pub fn save_conflict_edit(&mut self, ctx: &egui::Context) {
        let Some(open) = &mut self.conflict else {
            return;
        };
        let (Ok(text), Some(edit)) = (&open.text, open.edit.take()) else {
            return;
        };
        let op = Op::WriteConflict {
            file: open.file.clone(),
            expected: text.clone(),
            text: edit,
        };
        self.run(ctx, op);
    }

    /// Opens the merge question for `source` into `target` (the checked-out
    /// branch if `None`).
    pub fn ask_merge(&mut self, source: String, target: Option<String>) {
        let Some(head) = self.head_branch().map(str::to_string) else {
            self.refresh_error = Some(
                "Could not merge: no branch is checked out (detached HEAD). Check out the branch to merge into first.".into(),
            );
            return;
        };
        let target = target.unwrap_or_else(|| head.clone());
        if source == target {
            return;
        }
        let merge = |mode| Op::Merge {
            branch: source.clone(),
            mode,
        };
        if target == head {
            self.dialog = Some(Dialog::Choice {
                title: format!("Merge {source} into {head}?"),
                body: format!(
                    "Brings {source}'s commits into {head}. If {head} has nothing new of its own, it simply moves forward (fast-forward); otherwise a merge commit joins them. If both changed the same lines, you'll resolve the conflicts first."
                ),
                choices: vec![
                    ("Merge".into(), false, merge(MergeMode::Default)),
                    (
                        "Always create a merge commit".into(),
                        false,
                        merge(MergeMode::NoFastForward),
                    ),
                ],
            });
        } else {
            self.dialog = Some(Dialog::Choice {
                title: format!("Merge {source} into {target}?"),
                body: format!(
                    "{target} isn't checked out. Check it out first, then merge {source} into it."
                ),
                choices: vec![(
                    format!("Check out {target} and merge"),
                    false,
                    Op::Then(vec![
                        Op::Checkout(target.clone()),
                        merge(MergeMode::Default),
                    ]),
                )],
            });
        }
    }

    pub fn confirm_abort(&mut self) {
        let what = self
            .status
            .as_ref()
            .and_then(|s| s.in_progress.as_ref())
            .map_or("operation", |s| s.what.noun());
        self.dialog = Some(Dialog::Confirm {
            title: format!("Abort the {what}?"),
            body: format!(
                "Goes back to how things were before the {what} started. Conflict resolutions made so far are lost."
            ),
            button: "Yes, abort".into(),
            danger: true,
            op: Op::Abort,
        });
    }

    /// Opens the dialog to add a remote, or edit `name`.
    pub fn edit_remote(&mut self, name: Option<String>) {
        let url = name
            .as_ref()
            .and_then(|n| self.refs.remotes.iter().find(|r| &r.name == n))
            .and_then(|r| r.url.clone())
            .unwrap_or_default();
        self.dialog = Some(Dialog::Remote {
            name: name.clone().unwrap_or_default(),
            original: name,
            url,
        });
    }

    pub fn confirm_remove_remote(&mut self, name: String) {
        let tracking = self
            .refs
            .local
            .iter()
            .filter(|b| {
                b.upstream
                    .as_ref()
                    .is_some_and(|u| u.name.starts_with(&format!("{name}/")))
            })
            .count();
        let also = match tracking {
            0 => String::new(),
            1 => " One local branch tracks it and will no longer have an upstream.".into(),
            n => format!(" {n} local branches track it and will no longer have an upstream."),
        };
        self.dialog = Some(Dialog::Confirm {
            title: format!("Remove remote {name}?"),
            body: format!(
                "Its remote branches disappear from the graph. Nothing changes on the remote itself, and your local branches and commits stay.{also}"
            ),
            button: "Remove remote".into(),
            danger: true,
            op: Op::RemoveRemote(name),
        });
    }

    pub fn confirm_delete_tag(&mut self, name: String) {
        self.dialog = Some(Dialog::Confirm {
            title: format!("Delete tag {name}?"),
            body: "Only the tag is removed; the commit stays. If the tag was pushed, it stays on the remote until deleted there too.".into(),
            button: "Delete tag".into(),
            danger: true,
            op: Op::DeleteTag(name),
        });
    }

    /// Asks before checking out a commit without a branch.
    pub fn confirm_checkout_detached(&mut self, id: String, label: String) {
        self.dialog = Some(Dialog::Confirm {
            title: format!("Check out {label}?"),
            body: "You'll be on this commit without a branch (\u{201c}detached HEAD\u{201d}). You can look around and build, but create a branch before committing, or new commits are easy to lose.".into(),
            button: "Check out commit".into(),
            danger: false,
            op: Op::CheckoutDetached(id),
        });
    }

    /// When Git refused an operation for a reason the user can act on,
    /// offers the way on. Returns whether it did.
    fn offer_way_on(&mut self, op: &Op, error: &Error) -> bool {
        let upstream = self
            .head_upstream()
            .map_or_else(|| "the remote branch".to_string(), |u| u.name.clone());
        match (error, op) {
            (Error::Cancelled, _) => true,
            (Error::Rejected(_), Op::Push { force: false }) => {
                self.dialog = Some(Dialog::Choice {
                    title: "The remote has commits you don't have".into(),
                    body: format!(
                        "Someone pushed to {upstream} since you last pulled. Pull to combine their commits with yours, then push again.\n\nForce-pushing replaces the remote branch with yours, deleting their commits from it. It's only done if the remote is still as you last fetched it."
                    ),
                    choices: vec![
                        ("Pull first".into(), false, Op::Pull(PullMode::FastForward)),
                        ("Force push".into(), true, Op::Push { force: true }),
                    ],
                });
                true
            }
            (Error::Rejected(text), Op::Push { force: true }) => {
                self.refresh_error = Some(format!(
                    "Could not force push: {upstream} has changed since you last fetched, so it wasn't overwritten. Fetch to see the new commits first.\n\n{text}"
                ));
                true
            }
            (Error::Diverged(_), Op::Pull(PullMode::FastForward)) => {
                self.dialog = Some(Dialog::Choice {
                    title: format!("Your branch and {upstream} have both moved on"),
                    body: "Each has commits the other doesn't. Merge adds a merge commit joining them; rebase replays your commits on top of theirs, for a straight history.".into(),
                    choices: vec![
                        ("Merge".into(), false, Op::Pull(PullMode::Merge)),
                        ("Rebase".into(), false, Op::Pull(PullMode::Rebase)),
                    ],
                });
                true
            }
            (Error::Dirty(_), op) if op.can_stash_first() => {
                let (then, button) = if op.is_checkout() {
                    ("switch", "Stash and switch")
                } else {
                    ("merge", "Stash and merge")
                };
                self.dialog = Some(Dialog::Confirm {
                    title: "Your changes are in the way".into(),
                    body: format!(
                        "Some uncommitted changes would be overwritten. Stash them first (untracked files too), then {then}? Apply or pop the stash afterwards to get them back."
                    ),
                    button: button.into(),
                    danger: false,
                    op: Op::StashThen(Box::new(op.clone())),
                });
                true
            }
            (Error::NotMerged(_), Op::DeleteBranch { name, force: false }) => {
                let tip = branch::tip(self.path(), name).unwrap_or_default();
                self.dialog = Some(Dialog::Confirm {
                    title: format!("{name} has unmerged work"),
                    body: format!(
                        "Its commits aren't on any other branch. Deleting it anyway hides them from the graph; they can be recovered from the tip commit's id ({}) until Git cleans up.",
                        &tip[..7.min(tip.len())]
                    ),
                    button: "Delete anyway".into(),
                    danger: true,
                    op: Op::DeleteBranch {
                        name: name.clone(),
                        force: true,
                    },
                });
                true
            }
            _ => false,
        }
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
        self.refresh_started = std::time::Instant::now();
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
                Err(e) => self.refresh_error = Some(format!("Could not refresh: {e}")),
            }
        }
        if let Some(task) = &self.committing
            && let Some(result) = task.poll()
        {
            self.committing = None;
            match result {
                Ok(undo) => {
                    self.draft = CommitDraft::default();
                    self.undo = undo;
                    self.refresh(ctx);
                }
                Err(e) => self.refresh_error = Some(format!("Could not commit:\n{e}")),
            }
        }
        if let Some(running) = &self.op
            && let Some(result) = running.task.poll()
        {
            let (op, message) = (running.op.clone(), running.message.clone());
            self.op = None;
            self.picked.clear();
            self.pick_anchor = None;
            match result {
                Ok(Some(undo)) => self.undo = Some(undo),
                // Something changed that can't be undone: older undos would be out of order.
                // (A fetch only moves remote branches, so local undos still hold.)
                Ok(None) if op.changes_refs() && !matches!(op, Op::Fetch | Op::FetchRemote(_)) => {
                    self.undo = None
                }
                Ok(None) => {}
                Err(e) => {
                    if !self.offer_way_on(&op, &e) {
                        self.refresh_error = Some(format!("Could not {}:\n{e}", op.verb()));
                    }
                }
            }
            // An undone commit's message goes back in the commit box.
            if let Some(m) = message
                && self.draft.summary.trim().is_empty()
                && self.draft.body.trim().is_empty()
            {
                let (summary, body) = m.split_once('\n').unwrap_or((&m, ""));
                self.draft.summary = summary.trim().to_string();
                self.draft.body = body.trim().to_string();
            }
            if op.changes_refs() {
                self.refresh(ctx);
            } else {
                self.refresh_status(ctx);
            }
        }
        if let Some(task) = &self.status_task
            && let Some(result) = task.poll()
        {
            self.status_task = None;
            match result {
                Ok(status) => self.set_status(status),
                Err(e) => self.refresh_error = Some(format!("Could not refresh: {e}")),
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
        let had_conflicts = self
            .status
            .as_ref()
            .is_some_and(|s| !s.conflicted.is_empty());
        self.status = status;
        let conflicted: Vec<String> = self
            .status
            .as_ref()
            .map(|s| s.conflicted.iter().map(|f| f.path.clone()).collect())
            .unwrap_or_default();
        // New conflicts (from a merge, pull or stash): show the first.
        if !had_conflicts && let Some(first) = conflicted.first() {
            self.select_working();
            self.open_conflict(first.clone());
        }
        // The open conflicted file: re-read it, or close it once resolved.
        if let Some(open) = &self.conflict {
            if conflicted.contains(&open.file) {
                if open.edit.is_none() {
                    let file = open.file.clone();
                    let text = conflict::read(self.path(), &file).map_err(|e| e.to_string());
                    if let Some(open) = &mut self.conflict {
                        open.text = text;
                    }
                }
            } else {
                self.conflict = None;
            }
        }
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
        self.conflict = None;
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
        // HEAD moved elsewhere (outside the app, say): the undo no longer applies.
        if self
            .undo
            .as_ref()
            .is_some_and(|u| u.created < self.refresh_started && u.head != loaded.head)
        {
            self.undo = None;
        }
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
        self.conflict = None;
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
        self.conflict = None;
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
