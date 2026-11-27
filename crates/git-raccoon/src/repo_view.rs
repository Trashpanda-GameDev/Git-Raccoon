//! The state of an open repository: its history, refs, selection and the
//! data loaded for whatever is selected.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use git_raccoon_core::commit::{self, Committed};
use git_raccoon_core::details::{self, CommitDetails, LineKind};
use git_raccoon_core::interactive::{self, Step};
use git_raccoon_core::merge::{self, MergeMode};
use git_raccoon_core::refs::{self, Refs};
use git_raccoon_core::repo::{GraphCommit, History};
use git_raccoon_core::rewrite::{self, ResetMode};
use git_raccoon_core::stage::{self, Pick};
use git_raccoon_core::sync::{self, Progress, PullMode};
use git_raccoon_core::watch::{Changes, RepoWatcher};
use git_raccoon_core::working::{self, Side, WorkingStatus};
use git_raccoon_core::{
    Error, branch, conflict, discard, file_history, modules, remote, stash, undo,
};

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
    pub extras: Extras,
}

/// Submodules, worktrees and whether Git LFS is used.
#[derive(Default)]
pub struct Extras {
    pub submodules: Vec<modules::Submodule>,
    pub worktrees: Vec<modules::Worktree>,
    /// `Some(installed)` if the repository uses Git LFS.
    pub lfs: Option<bool>,
}

pub fn load(path: &str) -> Result<Loaded, String> {
    git_raccoon_core::trust::check(path).map_err(|e| e.to_string())?;
    let history = History::load(path, 0).map_err(|e| e.to_string())?;
    let refs = refs::load_refs(path).map_err(|e| e.to_string())?;
    let head = undo::head_id(path).ok().flatten();
    let extras = Extras {
        submodules: modules::submodules(path).unwrap_or_default(),
        worktrees: modules::worktrees(path).unwrap_or_default(),
        lfs: modules::uses_lfs(path).then(modules::lfs_installed),
    };
    Ok(Loaded {
        history,
        refs,
        head,
        extras,
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

/// The changed lines (of the change's own hunks) among `picked`.
fn changed(
    diff: &RenderedDiff,
    picked: &std::collections::BTreeSet<(usize, usize)>,
) -> Vec<(usize, usize)> {
    picked
        .iter()
        .copied()
        .filter(|&(h, l)| {
            h < diff.changes
                && matches!(
                    diff.diff.hunks[h].lines[l].kind,
                    LineKind::Add | LineKind::Delete
                )
        })
        .collect()
}

/// Where a diff comes from: a commit, or one side of the working changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Source {
    Commit(String),
    Working(Side),
}

/// A diff: where from, the file, and whether it's the whole file.
pub type DiffKey = (Source, String, bool);

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
        /// Only the staged changes.
        staged: bool,
        /// Only these files (all if empty).
        paths: Vec<String>,
    },
    /// Add these paths to the .gitignore.
    Ignore(Vec<String>),
    /// Stash operations, by stash commit id.
    StashApply(String),
    StashPop(String),
    /// Pop, with its staged changes staged again (undoing a stash of them).
    StashPopStaged(String),
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
    /// Set the remote branch `branch` pulls from and pushes to
    /// ("origin/main"), or remove it.
    SetUpstream {
        branch: String,
        upstream: Option<String>,
    },
    DeleteBranch {
        name: String,
        force: bool,
    },
    /// Several branches (none, unless `force`, if any has unmerged work).
    DeleteBranches {
        names: Vec<String>,
        force: bool,
    },
    CreateTag {
        name: String,
        target: String,
        message: String,
    },
    DeleteTag(String),
    /// Delete a tag on these remotes, and here too when `local`.
    DeleteTagIn {
        name: String,
        local: bool,
        remotes: Vec<String>,
    },
    /// Delete a local branch and its copies on remotes (`remote_branches`,
    /// "origin/feature"): unless `force`, only if its commits are on some
    /// other branch.
    DeleteBranchEverywhere {
        name: String,
        remote_branches: Vec<String>,
        force: bool,
    },
    /// Put a tag deleted on a remote back there (undoing the delete).
    RestoreRemoteTag {
        remote: String,
        name: String,
        id: String,
    },
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
    /// Fetch every remote, as the options say.
    Fetch(sync::FetchOptions),
    /// Fetch one remote.
    FetchRemote(String, sync::FetchOptions),
    /// Pull from the upstream, or (`from`) that remote's branch of the
    /// same name; `stash` puts uncommitted changes aside and back.
    Pull {
        from: Option<String>,
        mode: PullMode,
        stash: bool,
    },
    /// Delete a branch on its remote ("origin/feature"), for everyone.
    DeleteRemoteBranch(String),
    /// Several of them.
    DeleteRemoteBranches(Vec<String>),
    /// Fetch a ref on a remote (a pull request's head) into a branch.
    FetchInto {
        remote: String,
        source: String,
        branch: String,
    },
    /// Put a deleted remote branch back at a commit (undoing the delete).
    RestoreRemoteBranch {
        remote_branch: String,
        id: String,
    },
    /// Make a branch pull from a ref on a remote that isn't a branch (a
    /// pull request's head).
    Follow {
        branch: String,
        remote: String,
        refname: String,
    },
    /// Push a branch (checked out or not) to its name on a remote, and
    /// track it there.
    PushBranch {
        remote: String,
        branch: String,
    },
    /// Push the checked-out branch; `force` only with lease.
    /// Push tags to a remote: these, or every one when `names` is empty.
    PushTags {
        remote: String,
        names: Vec<String>,
    },
    Push {
        force: bool,
        /// The remote and branch to push to and track, for a branch that
        /// has no upstream yet.
        to: Option<(String, String)>,
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
    /// Clone (if needed) and check out a submodule's recorded commit.
    UpdateSubmodule(String),
    /// Add the repository at `url` as a submodule in `folder` (empty: named
    /// after it).
    AddSubmodule {
        url: String,
        folder: String,
    },
    AddWorktree {
        folder: String,
        branch: String,
        new: bool,
    },
    RemoveWorktree {
        folder: String,
        force: bool,
    },
    /// Merge a branch (local, or remote like "origin/main") into HEAD.
    Merge {
        branch: String,
        mode: MergeMode,
    },
    /// Continue or abort the merge, rebase, cherry-pick or revert in progress.
    Continue,
    Abort,
    /// Stop finding the commit that broke something: back where it started.
    BisectStop,
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
    /// Bring a resolved file's conflict back.
    Unresolve(String),
    /// Forget how a file's conflict was resolved before, and bring the
    /// conflict back.
    Forget(String),
    /// Skip the commit being applied (a rebase, or a series of picks).
    Skip,
    /// Undo a mixed reset: back from `expected` to `to`, files untouched.
    UndoMixedReset {
        expected: String,
        to: String,
    },
    Rebase(String),
    /// Rewrite the commits after `base` following `steps` (oldest first).
    InteractiveRebase {
        base: String,
        steps: Vec<Step>,
    },
    CherryPick(String),
    Revert(String),
    Reset {
        id: String,
        mode: ResetMode,
    },
    /// Undo a merge, rebase, cherry-pick, revert or hard reset: back from
    /// `expected` to `to`, keeping local changes.
    ResetKeep {
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
    /// The Git command a person would type for this, to run it by hand
    /// (and see all Git says): `None` for what has no one command.
    pub fn command(&self) -> Option<String> {
        let q = |s: &str| {
            if s.is_empty() || s.contains([' ', '\'', '"', '$', '`', '\\']) {
                format!("'{}'", s.replace('\'', "'\\''"))
            } else {
                s.to_string()
            }
        };
        Some(match self {
            Op::Fetch(_) => "git fetch --all".into(),
            Op::FetchRemote(r, _) => format!("git fetch {}", q(r)),
            Op::Pull { from: None, .. } => "git pull".into(),
            Op::Pull { from: Some(r), .. } => format!("git pull {}", q(r)),
            Op::Push { force, to: None } => {
                if *force {
                    "git push --force-with-lease".into()
                } else {
                    "git push".into()
                }
            }
            Op::Push {
                force,
                to: Some((r, b)),
            } => format!(
                "git push -u {}{} HEAD:{}",
                if *force { "--force-with-lease " } else { "" },
                q(r),
                q(b)
            ),
            Op::PushBranch { remote, branch } => format!("git push -u {} {}", q(remote), q(branch)),
            Op::PushTags { remote, names } if names.is_empty() => {
                format!("git push {} --tags", q(remote))
            }
            Op::PushTags { remote, names } => format!(
                "git push {} {}",
                q(remote),
                names.iter().map(|n| q(n)).collect::<Vec<_>>().join(" ")
            ),
            Op::DeleteRemoteBranch(full) => {
                let (r, b) = full.split_once('/')?;
                format!("git push {} --delete {}", q(r), q(b))
            }
            Op::Merge { branch, .. } => format!("git merge {}", q(branch)),
            Op::Rebase(onto) => format!("git rebase {}", q(onto)),
            Op::CherryPick(id) => format!("git cherry-pick {}", q(id)),
            Op::Revert(id) => format!("git revert {}", q(id)),
            Op::Checkout(b) => format!("git switch {}", q(b)),
            Op::Continue => "git status".into(),
            Op::StashThen(op) => return op.command(),
            Op::Then(ops) => return ops.last()?.command(),
            _ => return None,
        })
    }

    /// A plain pull from the upstream: fast-forward only, with uncommitted
    /// changes put aside and back.
    pub fn pull_upstream() -> Op {
        Op::Pull {
            from: None,
            mode: PullMode::FastForward,
            stash: true,
        }
    }

    /// Whether it moves away from (or changes) the commit checked out, as
    /// finding the commit that broke something needs it left.
    fn gets_in_the_way_of_a_search(&self) -> bool {
        match self {
            Op::CreateBranch { checkout, .. } => *checkout,
            Op::Checkout(_)
            | Op::CheckoutRemote(_)
            | Op::CheckoutDetached(_)
            | Op::Pull { .. }
            | Op::Merge { .. }
            | Op::Rebase(_)
            | Op::InteractiveRebase { .. }
            | Op::CherryPick(_)
            | Op::Revert(_)
            | Op::Reset { .. }
            | Op::ResetKeep { .. }
            | Op::UndoCommit { .. }
            | Op::UndoMixedReset { .. } => true,
            Op::Then(ops) => ops.iter().any(Op::gets_in_the_way_of_a_search),
            Op::StashThen(op) => op.gets_in_the_way_of_a_search(),
            _ => false,
        }
    }

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
            Op::Stash {
                message,
                untracked,
                staged,
                paths,
            } => stash::save_paths(path, &message, untracked, staged, &paths),
            Op::Ignore(paths) => git_raccoon_core::ignore::add(path, &paths).map(drop),
            Op::StashApply(id) => stash::apply(path, &id),
            Op::StashPop(id) => stash::pop(path, &id),
            Op::StashPopStaged(id) => stash::pop_staged(path, &id),
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
            Op::SetUpstream { branch, upstream } => {
                branch::set_upstream(path, &branch, upstream.as_deref())
            }
            Op::Follow {
                branch,
                remote,
                refname,
            } => branch::follow(path, &branch, &remote, &refname),
            Op::DeleteBranch { name, force } => branch::delete(path, &name, force),
            Op::DeleteBranches { names, force } => branch::delete_many(path, &names, force),
            Op::CreateTag {
                name,
                target,
                message,
            } => branch::create_tag(path, &name, &target, &message),
            Op::DeleteTag(name) => branch::delete_tag(path, &name),
            Op::DeleteTagIn {
                name,
                local,
                remotes,
            } => {
                for remote in &remotes {
                    sync::delete_remote_tag(path, remote, &name, report, &ctl.cancel)?;
                }
                if local {
                    branch::delete_tag(path, &name)?;
                }
                Ok(())
            }
            Op::RestoreRemoteTag { remote, name, id } => {
                sync::restore_remote_tag(path, &remote, &name, &id, report, &ctl.cancel)
            }
            Op::DeleteBranchEverywhere {
                name,
                remote_branches,
                force,
            } => {
                if !force && !branch::kept_elsewhere(path, &name, &remote_branches)? {
                    return Err(Error::NotMerged(name));
                }
                // Here first: if that fails, nothing's gone from a remote.
                branch::delete(path, &name, true)?;
                remote_branches.iter().try_for_each(|rb| {
                    let (remote, b) = split_remote(rb)?;
                    sync::delete_remote_branch(path, remote, b, report, &ctl.cancel)
                })
            }
            Op::StashThen(op) => {
                let what = op.verb();
                stash::save(
                    path,
                    &format!("Stashed by Git Raccoon to {what}"),
                    true,
                    false,
                )?;
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
            Op::Fetch(options) => sync::fetch(path, &options, report, &ctl.cancel),
            Op::FetchRemote(name, options) => {
                sync::fetch_remote(path, &name, &options, report, &ctl.cancel)
            }
            Op::Pull { from, mode, stash } => {
                sync::pull(path, from.as_deref(), mode, stash, report, &ctl.cancel)
            }
            Op::Push { force, to: None } => sync::push(path, force, report, &ctl.cancel),
            Op::PushTags { remote, names } => {
                sync::push_tags(path, &remote, &names, report, &ctl.cancel)
            }
            Op::PushBranch { remote, branch } => {
                sync::push_branch(path, &remote, &branch, report, &ctl.cancel)
            }
            Op::Push {
                force,
                to: Some((remote, branch)),
            } => sync::push_to(path, &remote, &branch, force, report, &ctl.cancel),
            Op::DeleteRemoteBranch(name) => {
                let (remote, branch) = split_remote(&name)?;
                sync::delete_remote_branch(path, remote, branch, report, &ctl.cancel)
            }
            Op::DeleteRemoteBranches(names) => names.iter().try_for_each(|name| {
                let (remote, branch) = split_remote(name)?;
                sync::delete_remote_branch(path, remote, branch, report, &ctl.cancel)
            }),
            Op::FetchInto {
                remote,
                source,
                branch,
            } => sync::fetch_into(path, &remote, &source, &branch, report, &ctl.cancel),
            Op::RestoreRemoteBranch { remote_branch, id } => {
                let (remote, branch) = split_remote(&remote_branch)?;
                sync::restore_remote_branch(path, remote, branch, &id, report, &ctl.cancel)
            }
            Op::AddRemote { name, url } => remote::add(path, &name, &url),
            Op::RenameRemote { old, new } => remote::rename(path, &old, &new),
            Op::SetRemoteUrl { name, url } => remote::set_url(path, &name, &url),
            Op::RemoveRemote(name) => remote::remove(path, &name),
            Op::UpdateSubmodule(sub) => modules::update_submodule(path, &sub, report, &ctl.cancel),
            Op::AddSubmodule { url, folder } => {
                modules::add_submodule(path, &url, &folder, report, &ctl.cancel)
            }
            Op::AddWorktree {
                folder,
                branch,
                new,
            } => modules::add_worktree(path, &folder, &branch, new, "HEAD"),
            Op::RemoveWorktree { folder, force } => modules::remove_worktree(path, &folder, force),
            Op::Merge { branch, mode } => merge::merge(path, &branch, mode).map(drop),
            Op::Continue => merge::continue_op(path),
            Op::Abort => merge::abort(path),
            Op::BisectStop => git_raccoon_core::bisect::stop(path)
                .and_then(|()| git_raccoon_core::bisect::bring_back(path).map(drop)),
            Op::TakeSide { file, ours } => conflict::take_side(path, &file, ours),
            Op::WriteConflict {
                file,
                expected,
                text,
            } => conflict::write(path, &file, &expected, &text),
            Op::MarkResolved(file) => conflict::mark_resolved(path, &file),
            Op::Unresolve(file) => conflict::unresolve(path, &file),
            Op::Forget(file) => conflict::forget(path, &file),
            Op::Skip => merge::skip(path),
            Op::ResetKeep { expected, to } => undo::reset_keep(path, &expected, &to),
            Op::UndoMixedReset { expected, to } => undo::reset_mixed(path, &expected, &to),
            Op::Rebase(onto) => rewrite::rebase(path, &onto),
            Op::InteractiveRebase { base, steps } => interactive::run(path, &base, &steps),
            Op::CherryPick(id) => rewrite::cherry_pick(path, &id),
            Op::Revert(id) => rewrite::revert(path, &id),
            Op::Reset { id, mode } => rewrite::reset(path, &id, mode),
        }
    }

    pub(crate) fn verb(&self) -> &'static str {
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
            Op::Ignore(_) => "ignore",
            Op::StashApply(_) => "apply the stash",
            Op::StashPop(_) | Op::StashPopStaged(_) => "pop the stash",
            Op::StashDrop(_) => "delete the stash",
            Op::CreateBranch { checkout: true, .. } => "create and check out the branch",
            Op::CreateBranch { .. } => "create the branch",
            Op::Checkout(_) | Op::CheckoutRemote(_) => "check out the branch",
            Op::CheckoutDetached(_) => "check out the commit",
            Op::RenameBranch { .. } => "rename the branch",
            Op::SetUpstream { .. } | Op::Follow { .. } => "set the upstream",
            Op::DeleteBranch { .. } => "delete the branch",
            Op::DeleteBranches { .. } => "delete the branches",
            Op::CreateTag { .. } => "create the tag",
            Op::DeleteTag(_) | Op::DeleteTagIn { .. } => "delete the tag",
            Op::DeleteBranchEverywhere { .. } => "delete the branch",
            Op::RestoreRemoteTag { .. } => "undo",
            Op::StashThen(op) => op.verb(),
            Op::UndoCommit { .. }
            | Op::DeleteBranchAt { .. }
            | Op::RestoreBranch { .. }
            | Op::RestoreTag { .. }
            | Op::StashStore { .. } => "undo",
            // Named after its first step ("add the remote", then fetch).
            Op::Then(ops) => ops.first().map_or("undo", Op::verb),
            Op::Fetch(_) | Op::FetchRemote(..) => "fetch",
            Op::Pull { .. } => "pull",
            Op::Push { .. } | Op::PushBranch { .. } | Op::PushTags { .. } => "push",
            Op::DeleteRemoteBranch(_) => "delete the remote branch",
            Op::DeleteRemoteBranches(_) => "delete the remote branches",
            Op::FetchInto { .. } => "fetch",
            Op::RestoreRemoteBranch { .. } => "undo",
            Op::AddRemote { .. } => "add the remote",
            Op::RenameRemote { .. } | Op::SetRemoteUrl { .. } => "change the remote",
            Op::RemoveRemote(_) => "remove the remote",
            Op::UpdateSubmodule(_) => "update the submodule",
            Op::AddSubmodule { .. } => "add the submodule",
            Op::AddWorktree { .. } => "add the worktree",
            Op::RemoveWorktree { .. } => "remove the worktree",
            Op::Merge { .. } => "merge",
            Op::Continue => "continue",
            Op::Skip => "skip the commit",
            Op::Unresolve(_) | Op::Forget(_) => "bring the conflict back",
            Op::Abort => "abort",
            Op::BisectStop => "stop the search",
            Op::TakeSide { .. } | Op::WriteConflict { .. } | Op::MarkResolved(_) => {
                "resolve the conflict"
            }
            Op::ResetKeep { .. } | Op::UndoMixedReset { .. } => "undo",
            Op::Rebase(_) | Op::InteractiveRebase { .. } => "rebase",
            Op::CherryPick(_) => "cherry-pick",
            Op::Revert(_) => "revert",
            Op::Reset { .. } => "reset",
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
                | Op::Ignore(_)
                | Op::TakeSide { .. }
                | Op::WriteConflict { .. }
                | Op::MarkResolved(_)
                | Op::Unresolve(_)
                | Op::Forget(_)
        )
    }

    /// Whether local changes in its way can be stashed first.
    fn can_stash_first(&self) -> bool {
        match self {
            Op::Merge { .. } | Op::CherryPick(_) | Op::Revert(_) => true,
            Op::Pull { stash, .. } => !stash,
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
    /// The checked-out branch, or HEAD's commit if detached.
    checked_out: Option<Op>,
    upstream: Option<String>,
    tag: Option<String>,
    stash_message: Option<String>,
    branch_existed: bool,
    stash_top: Option<String>,
    /// Several branches' tips (and upstreams), for undoing a bulk delete.
    tips: Vec<(String, String, Option<String>)>,
    /// HEAD's commit, for operations that move it.
    head: Option<String>,
    /// A remote's address before it's changed.
    remote_url: Option<String>,
}

impl Before {
    fn capture(op: &Op, path: &str) -> Self {
        let mut b = Before::default();
        match op {
            Op::SetRemoteUrl { name, .. } => {
                b.remote_url = git_raccoon_core::git_output(path, &["remote", "get-url", name])
                    .ok()
                    .map(|u| u.trim().to_string())
                    .filter(|u| !u.is_empty());
            }
            Op::DeleteBranch { name, .. } | Op::DeleteBranchEverywhere { name, .. } => {
                b.tip = branch::tip(path, name).ok();
                b.upstream = undo::upstream_of(path, name).ok().flatten();
            }
            Op::DeleteTag(name) | Op::DeleteTagIn { name, .. } => {
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
            Op::DeleteRemoteBranch(name) => {
                b.tip = undo::ref_target(path, &format!("refs/remotes/{name}")).ok();
            }
            Op::DeleteBranches { names, .. } => {
                b.tips = names
                    .iter()
                    .filter_map(|n| {
                        let tip = branch::tip(path, n).ok()?;
                        Some((n.clone(), tip, undo::upstream_of(path, n).ok().flatten()))
                    })
                    .collect();
            }
            Op::DeleteRemoteBranches(names) => {
                b.tips = names
                    .iter()
                    .filter_map(|n| {
                        let tip = undo::ref_target(path, &format!("refs/remotes/{n}")).ok()?;
                        Some((n.clone(), tip, None))
                    })
                    .collect();
            }
            Op::CheckoutRemote(name) => {
                b.tip = branch::tip(path, local_name(name)).ok();
                b.branch_existed = b.tip.is_some();
                // Where to go back to: the reflog's last move may be the
                // fast-forward, not the switch.
                b.checked_out = refs::load_refs(path)
                    .ok()
                    .and_then(|r| r.local.into_iter().find(|l| l.is_head))
                    .map(|l| Op::Checkout(l.name))
                    .or_else(|| undo::head_id(path).ok().flatten().map(Op::CheckoutDetached));
            }
            Op::Stash { .. } => b.stash_top = stash::top(path).ok().flatten(),
            Op::StashThen(inner) => return Before::capture(inner, path),
            Op::Rebase(_)
            | Op::InteractiveRebase { .. }
            | Op::CherryPick(_)
            | Op::Revert(_)
            | Op::Reset { .. } => {
                b.head = undo::head_id(path).ok().flatten();
            }
            _ => {}
        }
        b
    }
}

/// The remote branch `op` checks out, if it does (stashing first or not).
fn remote_checkout(op: &Op) -> Option<&str> {
    match op {
        Op::CheckoutRemote(name) => Some(name),
        Op::StashThen(inner) => remote_checkout(inner),
        _ => None,
    }
}

/// ("origin", "feature") for the remote branch "origin/feature".
fn split_remote(name: &str) -> Result<(&str, &str), Error> {
    name.split_once('/')
        .ok_or_else(|| Error::Unsupported(format!("{name} isn't a remote branch")))
}

/// "feature" for the remote branch "origin/feature".
pub fn local_name(remote_branch: &str) -> &str {
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
        Op::SetRemoteUrl { name, .. } => (
            format!("Undo change {name}'s address"),
            vec![Op::SetRemoteUrl {
                name: name.clone(),
                url: before.remote_url.clone()?,
            }],
        ),
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
            let local = local_name(name);
            let mut ops = Vec::new();
            // Fast-forwarded: back to where it was (while it's checked out).
            if let (Some(was), Ok(now)) = (before.tip.clone(), branch::tip(path, local))
                && was != now
            {
                ops.push(Op::ResetKeep {
                    expected: now,
                    to: was,
                });
            }
            match before.checked_out {
                Some(Op::Checkout(b)) if b == local => {}
                Some(back) => ops.push(back),
                None => ops.push(checkout_back(path)?),
            }
            if !before.branch_existed {
                ops.push(delete_new(local)?);
            }
            if ops.is_empty() {
                return None;
            }
            (format!("Undo check out {name}"), ops)
        }
        Op::DeleteRemoteBranch(name) => (
            format!("Undo delete {name}"),
            vec![Op::RestoreRemoteBranch {
                remote_branch: name.clone(),
                id: before.tip?,
            }],
        ),
        Op::RenameBranch { old, new } => (
            format!("Undo rename {old} to {new}"),
            vec![Op::RenameBranch {
                old: new.clone(),
                new: old.clone(),
            }],
        ),
        Op::DeleteBranches { names, .. } => (
            format!("Undo delete {} branches", names.len()),
            before
                .tips
                .into_iter()
                .map(|(name, id, upstream)| Op::RestoreBranch { name, id, upstream })
                .collect(),
        ),
        Op::DeleteRemoteBranches(names) => (
            format!("Undo delete {} remote branches", names.len()),
            before
                .tips
                .into_iter()
                .map(|(remote_branch, id, _)| Op::RestoreRemoteBranch { remote_branch, id })
                .collect(),
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
        Op::DeleteBranchEverywhere {
            name,
            remote_branches,
            ..
        } => {
            let id = before.tip?;
            let mut ops: Vec<Op> = remote_branches
                .iter()
                .map(|rb| Op::RestoreRemoteBranch {
                    remote_branch: rb.clone(),
                    id: id.clone(),
                })
                .collect();
            ops.push(Op::RestoreBranch {
                name: name.clone(),
                id,
                upstream: before.upstream,
            });
            (format!("Undo delete {name}"), ops)
        }
        Op::DeleteTagIn {
            name,
            local,
            remotes,
        } => {
            let id = before.tag?;
            let mut ops = Vec::new();
            if *local {
                ops.push(Op::RestoreTag {
                    name: name.clone(),
                    id: id.clone(),
                });
            }
            ops.extend(remotes.iter().map(|remote| Op::RestoreRemoteTag {
                remote: remote.clone(),
                name: name.clone(),
                id: id.clone(),
            }));
            (format!("Undo delete tag {name}"), ops)
        }
        Op::Stash { staged, .. } => {
            let top = stash::top(path).ok()??;
            if before.stash_top.as_ref() == Some(&top) {
                return None; // nothing was stashed
            }
            // Staged changes come back staged.
            let pop = if *staged {
                Op::StashPopStaged(top)
            } else {
                Op::StashPop(top)
            };
            ("Undo stash".into(), vec![pop])
        }
        Op::StashDrop(id) => (
            "Undo delete stash".into(),
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
        Op::Rebase(_)
        | Op::InteractiveRebase { .. }
        | Op::CherryPick(_)
        | Op::Revert(_)
        | Op::Reset { .. } => {
            // Stopped on conflicts: abort is the way back, not undo.
            if merge::in_progress(path).ok()?.is_some() {
                return None;
            }
            let (from, now) = (before.head?, undo::head_id(path).ok()??);
            if from == now {
                return None; // nothing changed
            }
            let label = match op {
                Op::Rebase(onto) => format!("Undo rebase onto {onto}"),
                Op::InteractiveRebase { .. } => "Undo interactive rebase".to_string(),
                Op::CherryPick(id) => format!("Undo cherry-pick of {}", short(id)),
                Op::Revert(id) => format!("Undo revert of {}", short(id)),
                _ => "Undo reset".to_string(),
            };
            // A soft or mixed reset left the files alone: undoing it does too.
            let back = match op {
                Op::Reset {
                    mode: ResetMode::Soft,
                    ..
                } => Op::UndoCommit {
                    expected: now,
                    to: from,
                },
                Op::Reset {
                    mode: ResetMode::Mixed,
                    ..
                } => Op::UndoMixedReset {
                    expected: now,
                    to: from,
                },
                _ => Op::ResetKeep {
                    expected: now,
                    to: from,
                },
            };
            (label, vec![back])
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
                vec![Op::ResetKeep {
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

/// What to do with a commit in an interactive rebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanAction {
    Pick,
    Reword,
    Squash,
    Fixup,
    Drop,
}

impl PlanAction {
    pub const ALL: [PlanAction; 5] = [
        PlanAction::Pick,
        PlanAction::Reword,
        PlanAction::Squash,
        PlanAction::Fixup,
        PlanAction::Drop,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PlanAction::Pick => "Pick",
            PlanAction::Reword => "Reword",
            PlanAction::Squash => "Squash",
            PlanAction::Fixup => "Fixup",
            PlanAction::Drop => "Drop",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            PlanAction::Pick => "Keep the commit as it is",
            PlanAction::Reword => "Keep it, with a new message",
            PlanAction::Squash => "Combine it into the commit below, keeping both messages",
            PlanAction::Fixup => {
                "Combine it into the commit below, keeping only that one's message"
            }
            PlanAction::Drop => "Leave it out (its changes go too)",
        }
    }
}

pub struct PlanRow {
    pub id: String,
    pub summary: String,
    pub message: String,
    pub action: PlanAction,
    /// The new message, for Reword.
    pub new_message: String,
}

/// An interactive rebase being planned.
pub struct Planner {
    pub base: String,
    /// Describes the base commit ("a1b2c3d “Fix it”").
    pub base_label: String,
    /// Newest first, as in the graph.
    pub rows: Vec<PlanRow>,
    /// Some of these commits are already pushed.
    pub pushed: bool,
}

impl Planner {
    /// The plan as Git runs it: oldest first.
    pub fn steps(&self) -> Vec<Step> {
        self.rows
            .iter()
            .rev()
            .map(|r| Step {
                id: r.id.clone(),
                action: match r.action {
                    PlanAction::Pick => interactive::Action::Pick,
                    PlanAction::Reword => interactive::Action::Reword(r.new_message.clone()),
                    PlanAction::Squash => interactive::Action::Squash,
                    PlanAction::Fixup => interactive::Action::Fixup,
                    PlanAction::Drop => interactive::Action::Drop,
                },
            })
            .collect()
    }

    /// Whether anything would change.
    pub fn changes_anything(&self, original: &[String]) -> bool {
        let order: Vec<&str> = self.rows.iter().map(|r| r.id.as_str()).collect();
        order != original.iter().map(String::as_str).collect::<Vec<_>>()
            || self.rows.iter().any(|r| match r.action {
                PlanAction::Pick => false,
                PlanAction::Reword => r.new_message.trim() != r.message.trim(),
                _ => true,
            })
    }
}

/// How a click picks lines in the open diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Select {
    /// Just this line.
    Only,
    /// This line in or out, keeping the others (Ctrl+click).
    Toggle,
    /// Every line from the last one clicked to this one (Shift+click, drag).
    Range,
}

/// What the blame of the open diff is of: the version on its new side.
pub type BlameKey = (Source, String);

/// A conflicted file open for resolving.
pub struct ConflictFile {
    pub file: String,
    /// Its text as last read (or why it couldn't be read).
    pub text: Result<String, String>,
    /// That text in pieces, each conflict with its base where Git has it
    /// (`None`: no conflict markers).
    pub chunks: Option<Vec<conflict::Chunk>>,
    /// The text being edited by hand, in Edit mode.
    pub edit: Option<String>,
    /// The conflict in focus, and whether to scroll to it.
    pub current: usize,
    pub scroll: bool,
    /// Lines ticked to make a conflict's result: (conflict, column, line),
    /// columns being 0 base, 1 ours, 2 theirs.
    pub ticked: Ticks,
    /// Each conflict's result as it stands (from the ticks, or typed).
    pub results: std::collections::BTreeMap<usize, String>,
    /// Whether to show why the conflict in focus happened, and what was
    /// found out for each conflict so far.
    pub why_open: bool,
    pub why: std::collections::BTreeMap<usize, Result<git_raccoon_core::why::Why, String>>,
    /// Shown as panes (each side's whole file, and the output) rather
    /// than a card per conflict; and the share of the height the sides get.
    pub panes: bool,
    pub pane_split: f32,
}

/// Lines ticked to make conflicts' results, as (conflict, column, line),
/// in the order they were ticked: a result takes them in that order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ticks(Vec<(usize, u8, usize)>);

impl Ticks {
    /// Adds a line at the end (if it isn't ticked already).
    pub fn insert(&mut self, key: (usize, u8, usize)) {
        if !self.contains(&key) {
            self.0.push(key);
        }
    }

    pub fn remove(&mut self, key: &(usize, u8, usize)) {
        self.0.retain(|k| k != key);
    }

    pub fn contains(&self, key: &(usize, u8, usize)) -> bool {
        self.0.contains(key)
    }

    /// In the order ticked.
    pub fn iter(&self) -> impl Iterator<Item = &(usize, u8, usize)> {
        self.0.iter()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

/// A merge tool open on a conflicted file: waited for, then its result
/// brought back to check.
pub struct ToolOpen {
    pub name: String,
    pub file: String,
    pub pid: u32,
    /// The file as it was when the tool started.
    before: Option<Vec<u8>>,
    done: Task<Result<(), String>>,
}

/// What came of a merge tool, shown above the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolNote {
    pub file: String,
    pub text: String,
    /// The tool changed the file (its result is to be checked).
    pub changed: bool,
}

impl ConflictFile {
    /// Reads `file` and finds its conflicts (with their bases).
    pub fn load(path: &str, file: String) -> Self {
        let mut open = ConflictFile {
            file,
            text: Err(String::new()),
            chunks: None,
            edit: None,
            current: 0,
            scroll: false,
            ticked: Ticks::default(),
            results: std::collections::BTreeMap::new(),
            why_open: false,
            why: std::collections::BTreeMap::new(),
            panes: false,
            pane_split: 0.55,
        };
        open.reload(path);
        open
    }

    /// Reads the file again; what was ticked or typed goes if it changed.
    pub fn reload(&mut self, path: &str) {
        let text = conflict::read(path, &self.file).map_err(|e| e.to_string());
        if text == self.text {
            return;
        }
        self.chunks = text.as_ref().ok().and_then(|t| conflict::parse(t));
        if let Some(chunks) = &mut self.chunks {
            conflict::with_bases(path, &self.file, chunks);
        }
        self.text = text;
        self.ticked.clear();
        self.results.clear();
        self.why.clear();
        let left = self
            .chunks
            .as_ref()
            .map_or(0, |c| conflict::conflicts(c).count());
        self.current = self.current.min(left.saturating_sub(1));
    }
}

/// An agent's resolution of a conflicted file, waiting for the person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub file: String,
    /// The whole file, resolved.
    pub text: String,
    pub reasoning: String,
    /// The agent's name ("claude-code").
    pub agent: String,
}

/// A proposal that was used: the file's text before and after, to undo it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsedProposal {
    pub file: String,
    pub before: String,
    pub after: String,
    pub agent: String,
}

/// Whose commits are marked in the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mark {
    /// A pull/merge request's, by number.
    Request(u64),
    /// The commits behind one side of a conflict in a file.
    Conflict { file: String, side: String },
}

/// What this system calls its trash.
pub fn trash_name() -> &'static str {
    if cfg!(windows) {
        "Recycle Bin"
    } else {
        "Trash"
    }
}

/// Where to push `branch`, which has no upstream yet: `name` on `remote`,
/// which it then pushes to and pulls from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushTarget {
    pub branch: String,
    pub remote: String,
    pub name: String,
}

/// A question to answer before something happens.
/// One place a branch or tag can be deleted from, in the delete dialog.
#[derive(Debug, Clone)]
pub struct Place {
    /// "Local", or the remote's name.
    pub label: String,
    /// The remote ("origin"), or `None` for here.
    pub remote: Option<String>,
    pub on: bool,
}

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
        /// How many files are staged: if any, stashing just those is offered.
        staged_files: usize,
        /// Stash only the staged changes.
        staged: bool,
        /// Only these files (all if empty).
        paths: Vec<String>,
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
    /// Delete a branch or tag: ticked here and on which remotes.
    Delete {
        tag: bool,
        name: String,
        places: Vec<Place>,
    },
    /// Choose `branch`'s upstream ("origin/main"), or none.
    SetUpstream {
        branch: String,
        upstream: Option<String>,
    },
    /// Add a remote (`original` is `None`) or edit one.
    Remote {
        original: Option<String>,
        name: String,
        url: String,
    },
    /// Add the repository at `url` as a submodule, in `folder`.
    Submodule {
        url: String,
        folder: String,
    },
    /// Add a worktree in `folder` with `branch` (new from HEAD, or existing).
    Worktree {
        folder: String,
        branch: String,
        new: bool,
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
    /// Diffs show the whole file (from the settings).
    pub whole_file: bool,
    pub draft: CommitDraft,
    /// Working files picked together (Ctrl+click, Shift+click).
    pub file_selection: crate::ui::working::FileSelection,
    /// The forge it's on (GitHub, GitLab), with its requests and issues.
    pub forge: Option<crate::forge_view::ForgeView>,
    /// The web pages of its remotes on forges.
    pub web: crate::forge::Web,
    /// Which forge, and what's on it, should be found out (again).
    pub forge_stale: bool,
    /// A pull/merge request's commits, marked in the graph: its number,
    /// and the commits.
    pub highlight: Option<(Mark, std::collections::HashSet<String>)>,
    /// An agent's proposed resolution, and the last one used (to undo).
    pub proposal: Option<Proposal>,
    pub used_proposal: Option<UsedProposal>,
    /// A folder to open in a tab once the operation running is done (a
    /// worktree being made).
    pub open_when_done: Option<String>,
    /// A branch just pushed to the remote for the first time, to offer a
    /// pull/merge request for (once the push is done: then `true`).
    pub offer_request: Option<(String, bool)>,
    committing: Option<Task<Result<Option<Undoable>, String>>>,
    /// Who hears how the operation or commit running now went (an agent's):
    /// `Ok` or Git's error.
    report: Option<std::sync::mpsc::Sender<Result<(), String>>>,
    /// The commit running is an agent's: the commit box stays as it is.
    commit_for_agent: bool,
    /// How to undo the last action, if it can be.
    pub undo: Option<Undoable>,
    /// The conflicted file being resolved, if one is open.
    pub conflict: Option<ConflictFile>,
    /// A merge tool open on a conflicted file, and what came of the last.
    pub tool_open: Option<ToolOpen>,
    pub tool_note: Option<ToolNote>,
    /// The tool a rule gives the conflicted file open (file, tool).
    rule_tool: Option<(String, Option<String>)>,
    /// What rules' merge tools did in what's in progress.
    pub tool_results: Vec<git_raccoon_core::rules::DriverResult>,
    /// The built-in terminals for this repository (its tab's).
    pub terminals: crate::ui::terminal_panel::Terminals,
    /// The Git command an operation that just failed stands for.
    pub failed_command: Option<String>,
    /// While something's in progress: its conflicted files (how each
    /// conflicts), the files resolved since it started, and whether the
    /// history is shown in place of the conflicts' overview.
    pub conflict_files: Vec<conflict::ConflictedFile>,
    pub resolved_here: std::collections::BTreeSet<String>,
    pub history_over_conflicts: bool,
    /// The open file's history (beside its diff), and the file it's of:
    /// kept while an older name of it is open from the list.
    pub file_history: Keyed<String, Result<Arc<Vec<file_history::Change>>, String>>,
    pub history_file: Option<String>,
    /// Who last changed each line of the open diff's new side.
    pub blame: Keyed<BlameKey, Result<Arc<file_history::Blame>, String>>,
    /// Shown beside or in the diff (from the settings).
    pub show_history: bool,
    pub show_blame: bool,
    /// The diff the picked lines are in.
    picked_in: Option<DiffKey>,
    /// When the remotes were last fetched (or the repository opened), and
    /// a fetch running in the background.
    last_fetch: std::time::Instant,
    auto_fetch: Option<Task<Result<(), String>>>,
    /// Why the last background fetch failed, if it did.
    pub auto_fetch_error: Option<String>,
    /// Finding the commit that broke something.
    pub bisect: crate::bisect_view::Bisect,
    /// A fetch, pull or push that failed over SSH, being looked into.
    pub(crate) ssh_problem: Option<crate::ui::ssh_problem::SshProblem>,
    /// The guided SSH setup was asked for, for this remote.
    pub(crate) wants_ssh_setup: Option<String>,
    pub extras: Extras,
    /// A repository (a submodule or worktree) to open in a tab.
    pub open_request: Option<String>,
    /// An interactive rebase being planned (shown instead of the history),
    /// with the commits' original order.
    pub planner: Option<(Planner, Vec<String>)>,
    /// When the running refresh started (an older read can't drop a newer undo).
    refresh_started: std::time::Instant,
    /// Where a Shift-click range of picks starts.
    pick_anchor: Option<(usize, usize)>,
    /// An operation in progress.
    op: Option<Running>,
    /// A dialog waiting for an answer.
    pub dialog: Option<Dialog>,
    /// Asking where to push a branch that isn't on a remote yet.
    pub push_to: Option<PushTarget>,
    /// Tests only: where discarded files go instead of the Recycle Bin.
    pub trash_override: Option<std::path::PathBuf>,
    pub diff: Keyed<DiffKey, Result<Arc<RenderedDiff>, String>>,
    pub images: Keyed<DiffKey, Result<Arc<ImagePair>, String>>,
    pub search: Search,
    pub sidebar_filter: String,
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
            extras: loaded.extras,
            open_request: None,
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
            whole_file: false,
            pick_anchor: None,
            op: None,
            dialog: None,
            push_to: None,
            trash_override: None,
            draft: CommitDraft::default(),
            file_selection: crate::ui::working::FileSelection::default(),
            forge: None,
            web: crate::forge::Web::default(),
            forge_stale: true,
            highlight: None,
            proposal: None,
            used_proposal: None,
            open_when_done: None,
            offer_request: None,
            committing: None,
            report: None,
            commit_for_agent: false,
            undo: None,
            conflict: None,
            tool_open: None,
            tool_note: None,
            rule_tool: None,
            tool_results: Vec::new(),
            terminals: Default::default(),
            failed_command: None,
            conflict_files: Vec::new(),
            resolved_here: std::collections::BTreeSet::new(),
            history_over_conflicts: false,
            planner: None,
            file_history: Keyed::default(),
            history_file: None,
            blame: Keyed::default(),
            show_history: false,
            show_blame: false,
            picked_in: None,
            last_fetch: std::time::Instant::now(),
            auto_fetch: None,
            auto_fetch_error: None,
            bisect: crate::bisect_view::Bisect::default(),
            ssh_problem: None,
            wants_ssh_setup: None,
            refresh_started: std::time::Instant::now(),
            diff: Keyed::default(),
            images: Keyed::default(),
            search: Search::default(),
            sidebar_filter: String::new(),
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
            Some((Source::Working(self.active_side), file, self.whole_file))
        } else {
            Some((
                Source::Commit(self.selected.as_ref()?.id.clone()),
                file,
                self.whole_file,
            ))
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
        if self.bisect.searching() {
            self.ask_to_stop_the_search();
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

    /// Runs `op` for an agent, and says how it went on `report` once done.
    /// `false` (and nothing run) while something else is running.
    pub fn run_reporting(
        &mut self,
        ctx: &egui::Context,
        op: Op,
        report: std::sync::mpsc::Sender<Result<(), String>>,
    ) -> bool {
        if self.op.is_some() || self.committing.is_some() {
            return false;
        }
        self.report = Some(report);
        self.start(ctx, op, None);
        true
    }

    /// Commits (or amends) with `message` for an agent, leaving the commit
    /// box as it is, and says how it went on `report`. `false` (and nothing
    /// done) while something else is running.
    pub fn commit_reporting(
        &mut self,
        ctx: &egui::Context,
        message: String,
        amend: bool,
        report: std::sync::mpsc::Sender<Result<(), String>>,
    ) -> bool {
        if self.op.is_some() || self.committing.is_some() {
            return false;
        }
        let path = self.path().to_string();
        self.report = Some(report);
        self.commit_for_agent = true;
        self.committing = Some(Task::spawn(ctx, move || {
            let made = commit::commit(&path, &message, amend).map_err(|e| e.to_string())?;
            Ok(undo_commit(&path, &made, amend, &message))
        }));
        true
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

    /// Asks before discarding every uncommitted change, staged or not.
    pub fn confirm_discard_all(&mut self) {
        let Some((what, discard)) = self.discard_all() else {
            return;
        };
        self.dialog = Some(Dialog::Confirm {
            title: "Discard all changes?".into(),
            body: format!(
                "Every uncommitted change to {what} is thrown away: staged and unstaged changes, and new files. Each file's current version is moved to the {} first, so you can get it back from there.",
                trash_name()
            ),
            button: "Discard all".into(),
            danger: true,
            op: Op::Then(discard),
        });
    }

    /// The steps that discard every uncommitted change, and what they
    /// touch ("README.md", "3 files"); `None` if there are none.
    fn discard_all(&self) -> Option<(String, Vec<Op>)> {
        let status = self.status.as_ref()?;
        // Unstaging first turns staged changes (renames: both names) into
        // ones that discarding throws away.
        let mut files: Vec<String> = status
            .staged
            .iter()
            .chain(&status.unstaged)
            .flat_map(|f| std::iter::once(f.path.clone()).chain(f.old_path.clone()))
            .collect();
        files.sort();
        files.dedup();
        let what = match files.as_slice() {
            [] => return None,
            [one] => one.clone(),
            many => format!("{} files", many.len()),
        };
        Some((
            what,
            vec![
                Op::UnstageAll,
                Op::Discard(files, self.trash_override.clone()),
            ],
        ))
    }

    /// Asks before dropping a stash.
    pub fn confirm_drop(&mut self, id: String) {
        let name = self.stash(&id).map_or_else(
            || "This stash".to_string(),
            |s| format!("\u{201c}{}\u{201d}", s.message),
        );
        self.dialog = Some(Dialog::Confirm {
            title: "Delete this stash?".into(),
            body: format!(
                "{name} will be deleted. Its contents can still be recovered from its id ({}) until Git cleans up.",
                &id[..7.min(id.len())]
            ),
            button: "Delete stash".into(),
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

    /// Fetches every remote in the background once `minutes` have passed
    /// since the last fetch (0: never), unless something else is running.
    pub fn auto_fetch(&mut self, ctx: &egui::Context, minutes: u32, prune: bool) {
        if let Some(result) = self.auto_fetch.as_ref().and_then(Task::poll) {
            self.auto_fetch = None;
            self.last_fetch = std::time::Instant::now();
            // SSH failing: said quietly, with where to find out more.
            self.auto_fetch_error = result.err().map(|e| {
                if git_raccoon_core::ssh::is_ssh_failure(&e) {
                    "it couldn't connect over SSH. Fetch to see why, and the fix.".into()
                } else {
                    e
                }
            });
            self.refresh(ctx);
        }
        if minutes == 0 || self.auto_fetch.is_some() || self.op.is_some() {
            return;
        }
        if self.refs.remotes.is_empty() {
            return;
        }
        let due = std::time::Duration::from_secs(u64::from(minutes) * 60);
        let since = self.last_fetch.elapsed();
        if since < due {
            // Look again when it's due.
            ctx.request_repaint_after(due - since);
            return;
        }
        let path = self.path().to_string();
        self.auto_fetch = Some(Task::spawn(ctx, move || {
            sync::fetch_quietly(&path, prune).map_err(|e| e.to_string())
        }));
    }

    /// Makes the next background fetch due now (for tests).
    #[doc(hidden)]
    pub fn fetch_due_now(&mut self) {
        let long_ago = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(3600));
        self.last_fetch = long_ago.unwrap_or(self.last_fetch);
    }

    /// A line on background fetching, for the Fetch button's tip.
    pub fn auto_fetch_status(&self, minutes: u32) -> Option<String> {
        if minutes == 0 {
            return None;
        }
        if self.auto_fetch.is_some() {
            return Some("Fetching in the background…".into());
        }
        if let Some(e) = &self.auto_fetch_error {
            return Some(format!("The last background fetch didn't work: {e}"));
        }
        let ago = self.last_fetch.elapsed().as_secs() / 60;
        let left = u64::from(minutes).saturating_sub(ago);
        Some(match ago {
            0 => format!("Fetched under a minute ago; again in {left} min."),
            n => format!("Fetched {n} min ago; again in {left} min."),
        })
    }

    fn start(&mut self, ctx: &egui::Context, op: Op, message: Option<String>) {
        // While finding the commit that broke something, what would move
        // away from the commit being tested waits: stop the search first.
        if self.bisect.searching() && op.gets_in_the_way_of_a_search() {
            self.ask_to_stop_the_search();
            return;
        }
        if matches!(op, Op::Fetch(_) | Op::FetchRemote(..)) {
            // A fetch by hand counts too: the next one is a while off.
            self.last_fetch = std::time::Instant::now();
            self.auto_fetch_error = None;
        }
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
            Op::Fetch(_)
                | Op::FetchRemote(..)
                | Op::Pull { .. }
                | Op::Push { .. }
                | Op::PushBranch { .. }
                | Op::PushTags { .. }
                | Op::UpdateSubmodule(_)
                | Op::AddSubmodule { .. }
                | Op::DeleteRemoteBranch(_)
                | Op::DeleteRemoteBranches(_)
                | Op::RestoreRemoteBranch { .. }
                | Op::DeleteTagIn { .. }
                | Op::RestoreRemoteTag { .. }
                | Op::DeleteBranchEverywhere { .. }
                | Op::FetchInto { .. }
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

    /// Pushes the checked-out branch. One with no upstream yet asks where
    /// to first: the same name on "origin" (or the only remote) to start.
    pub fn push(&mut self, ctx: &egui::Context) {
        let new = self
            .refs
            .local
            .iter()
            .find(|b| b.is_head && b.upstream.is_none());
        let remotes = &self.refs.remotes;
        let remote = remotes
            .iter()
            .find(|r| r.name == "origin")
            .or_else(|| remotes.first());
        match (new, remote) {
            (Some(branch), Some(remote)) => {
                self.push_to = Some(PushTarget {
                    branch: branch.name.clone(),
                    remote: remote.name.clone(),
                    name: branch.name.clone(),
                });
            }
            // Pushing says why it can't (detached HEAD, no remote).
            _ => self.run(
                ctx,
                Op::Push {
                    force: false,
                    to: None,
                },
            ),
        }
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
    /// The stash dialog, titled with the commit summary written so far:
    /// stashing just the staged changes is offered (and chosen) when some
    /// are staged.
    pub fn stash_dialog(&self) -> Dialog {
        let staged_files = self.status.as_ref().map_or(0, |s| s.staged.len());
        Dialog::Stash {
            message: self.draft.summary.trim().to_string(),
            untracked: true,
            staged_files,
            staged: staged_files > 0,
            paths: Vec::new(),
        }
    }

    /// The stash dialog for just these files.
    pub fn stash_files_dialog(&self, paths: Vec<String>) -> Dialog {
        Dialog::Stash {
            message: String::new(),
            untracked: true,
            staged_files: 0,
            staged: false,
            paths,
        }
    }

    /// Asks before discarding all the changes to `files`, staged ones too.
    pub fn confirm_discard_staged(&mut self, files: Vec<String>) {
        let Some(status) = self.status.as_ref() else {
            return;
        };
        // Renamed files: the old name is restored too.
        let mut all: Vec<String> = files.clone();
        for f in status.staged.iter().filter(|f| files.contains(&f.path)) {
            all.extend(f.old_path.clone());
        }
        all.sort();
        all.dedup();
        let what = match files.as_slice() {
            [one] => one.clone(),
            many => format!("{} files", many.len()),
        };
        self.dialog = Some(Dialog::Confirm {
            title: format!("Discard changes to {what}?"),
            body: format!(
                "Their changes are thrown away, staged and unstaged. Each file's current version is moved to the {} first, so you can get it back from there.",
                trash_name()
            ),
            button: "Discard".into(),
            danger: true,
            op: Op::Then(
                files
                    .into_iter()
                    .map(Op::UnstageFile)
                    .chain([Op::Discard(all, self.trash_override.clone())])
                    .collect(),
            ),
        });
    }

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

    /// Asks which remote branch `branch` should pull from and push to:
    /// its upstream to start with, else the same name on a remote.
    pub fn set_upstream(&mut self, branch: String) {
        let current = self
            .refs
            .local
            .iter()
            .find(|b| b.name == branch)
            .and_then(|b| b.upstream.as_ref())
            .map(|u| u.name.clone());
        let upstream = current.or_else(|| {
            self.refs
                .remotes
                .iter()
                .flat_map(|r| &r.branches)
                .find(|b| b.name == branch)
                .map(|b| b.full_name.clone())
        });
        self.dialog = Some(Dialog::SetUpstream { branch, upstream });
    }

    /// Asks before deleting a branch. Git itself refuses if its work isn't
    /// merged anywhere; that refusal leads to a second, stronger question.
    /// Asks before deleting a branch on its remote.
    pub fn confirm_delete_remote_branch(&mut self, name: String) {
        if let Some((remote, branch)) = name.split_once('/') {
            let remote = remote.to_string();
            self.confirm_delete_branch_in(branch.to_string(), Some(&remote));
        }
    }

    /// Asks before deleting several branches.
    pub fn confirm_delete_branches(&mut self, names: Vec<String>) {
        self.dialog = Some(Dialog::Confirm {
            title: format!("Delete {} branches?", names.len()),
            body: format!(
                "{}\n\nOnly the branch names are removed; their commits stay. If any has commits that aren't on another branch, none are deleted until you say so.",
                names.join(", ")
            ),
            button: format!("Delete {} branches", names.len()),
            danger: true,
            op: Op::DeleteBranches {
                names,
                force: false,
            },
        });
    }

    /// Asks before deleting several branches on their remotes.
    pub fn confirm_delete_remote_branches(&mut self, names: Vec<String>) {
        self.dialog = Some(Dialog::Confirm {
            title: format!("Delete {} branches on the remote?", names.len()),
            body: format!(
                "{}\n\nThey're deleted on the remote, for everyone who uses it; your own branches and their commits here stay. Undo puts them back.",
                names.join(", ")
            ),
            button: "Delete on the remote".into(),
            danger: true,
            op: Op::DeleteRemoteBranches(names),
        });
    }

    pub fn confirm_delete_branch(&mut self, name: String) {
        self.confirm_delete_branch_in(name, None);
    }

    /// Shows the history or blame of `file` (as of commit `at`, or the
    /// working copy), loading what's needed in the background.
    /// Loads the open file's history and blame, when they're shown.
    fn load_file_past(&mut self, ctx: &egui::Context) {
        self.file_history.poll();
        self.blame.poll();
        let Some((source, file, _)) = self.diff_key() else {
            return;
        };
        if self.show_history {
            // Keep the list while one of the names in it is open.
            let listed = self
                .history_file
                .as_ref()
                .and_then(|h| self.file_history.get(h))
                .and_then(|r| r.as_ref().ok())
                .is_some_and(|list| list.iter().any(|c| c.path == file));
            if !listed && self.history_file.as_ref() != Some(&file) {
                self.history_file = Some(file.clone());
            }
            if let Some(of) = self.history_file.clone() {
                let path = self.path().to_string();
                self.file_history.want(ctx, &of, move |file| {
                    file_history::history(&path, &file)
                        .map(Arc::new)
                        .map_err(|e| e.to_string())
                });
            }
        }
        if self.show_blame {
            let path = self.path().to_string();
            self.blame
                .want(ctx, &(source, file), move |(source, file)| {
                    match source {
                        Source::Commit(id) => file_history::blame(&path, &file, Some(&id)),
                        Source::Working(Side::Unstaged) => file_history::blame(&path, &file, None),
                        Source::Working(Side::Staged) => file_history::blame_staged(&path, &file),
                    }
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
                });
        }
    }

    /// The open file's history, if it's shown.
    pub fn open_history(&self) -> Option<&Result<Arc<Vec<file_history::Change>>, String>> {
        self.file_history.get(self.history_file.as_ref()?)
    }

    /// The open diff's blame, if it's shown and loaded.
    pub fn open_blame(&self) -> Option<&Result<Arc<file_history::Blame>, String>> {
        let (source, file, _) = self.diff_key()?;
        self.blame.get(&(source, file))
    }

    /// The next conflicted file to resolve after `file` (or the first):
    /// files with conflicts in their text first, then the others.
    pub fn next_conflict(&self, after: Option<&str>) -> Option<String> {
        let mut files: Vec<&conflict::ConflictedFile> = self.conflict_files.iter().collect();
        files.sort_by_key(|f| (f.conflicts == 0, f.path.clone()));
        let at = after.and_then(|a| files.iter().position(|f| f.path == a));
        let start = at.map_or(0, |i| i + 1);
        files
            .iter()
            .cycle()
            .skip(start)
            .take(files.len())
            .find(|f| Some(f.path.as_str()) != after)
            .map(|f| f.path.clone())
    }

    /// Starts a merge tool on `file`'s conflict (its versions written out
    /// already), and waits for it in the background.
    pub fn open_in_tool(
        &mut self,
        ctx: &egui::Context,
        file: String,
        name: String,
        run: git_raccoon_core::tools::Run,
        versions: conflict::Versions,
    ) -> Result<(), String> {
        let before = std::fs::read(&versions.merged).ok();
        let mut command = run.command();
        command
            .current_dir(self.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped());
        let child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                versions.clean_up();
                return Err(format!(
                    "{name} didn't start ({}: {e})",
                    run.program.display()
                ));
            }
        };
        let pid = child.id();
        let who = name.clone();
        let done = Task::spawn(ctx, move || {
            let out = child.wait_with_output();
            // Whether or not it's still waited for here.
            versions.clean_up();
            match out {
                Ok(o) if o.status.success() => Ok(()),
                Ok(o) => {
                    let said = String::from_utf8_lossy(&o.stderr).trim().to_string();
                    let code = o
                        .status
                        .code()
                        .map_or("stopped".into(), |c| format!("exit {c}"));
                    Err(if said.is_empty() {
                        format!("{who} ended with {code}")
                    } else {
                        format!("{who} ended with {code}: {said}")
                    })
                }
                Err(e) => Err(format!("{who}: {e}")),
            }
        });
        self.tool_note = None;
        self.tool_open = Some(ToolOpen {
            name,
            file,
            pid,
            before,
            done,
        });
        Ok(())
    }

    /// The merge tool has closed: what it did to the file.
    fn tool_closed(&mut self, ctx: &egui::Context, open: ToolOpen, result: Result<(), String>) {
        let full = std::path::Path::new(self.path()).join(&open.file);
        let now = std::fs::read(&full).ok();
        let changed = now != open.before;
        let ToolOpen { name, file, .. } = open;
        let text = match (&result, changed) {
            (_, true) => {
                let markers = now
                    .as_deref()
                    .and_then(|b| std::str::from_utf8(b).ok())
                    .is_some_and(|t| conflict::parse(t).is_some());
                if markers {
                    format!(
                        "{name} saved {file}, with conflicts still in it: resolve the rest here."
                    )
                } else {
                    format!("{name} saved {file}. Check the result, then mark it resolved.")
                }
            }
            (Err(e), false) => format!("{e}. {file} wasn't changed."),
            (Ok(()), false) => format!("{name} closed without saving: {file} wasn't changed."),
        };
        self.tool_note = Some(ToolNote {
            file: file.clone(),
            text,
            changed,
        });
        if changed {
            let path = self.path().to_string();
            if let Some(c) = self.conflict.as_mut().filter(|c| c.file == file) {
                c.reload(&path);
            }
            self.refresh_status(ctx);
        }
    }

    /// The tool a rule for some files gives `file` (remembered per file,
    /// as it reads Git's attributes).
    pub fn file_rule_tool(&mut self, file: &str) -> Option<String> {
        if let Some((f, tool)) = &self.rule_tool
            && f == file
        {
            return tool.clone();
        }
        let tool = git_raccoon_core::rules::tool_for(self.path(), file);
        self.rule_tool = Some((file.to_string(), tool.clone()));
        tool
    }

    /// The line of the open diff to open the file at: the first picked
    /// line's, or the first change's (in the new version).
    pub fn line_looked_at(&self, key: &DiffKey) -> usize {
        let Some(Ok(d)) = self.diff.get(key) else {
            return 1;
        };
        let hunks = &d.diff.hunks;
        let picked = self
            .picked
            .iter()
            .find_map(|&(h, l)| hunks.get(h)?.lines.get(l)?.new_line);
        let first = || {
            hunks
                .iter()
                .flat_map(|h| &h.lines)
                .find(|l| l.kind != git_raccoon_core::details::LineKind::Context)
                .and_then(|l| l.new_line.or(l.old_line))
        };
        picked
            .or_else(first)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(1)
            .max(1)
    }

    /// Opens a conflicted file for resolving.
    pub fn open_conflict(&mut self, file: String) {
        self.active_file = None;
        self.conflict = Some(ConflictFile::load(self.path(), file));
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

    /// Writes `text` as the result of one conflict in the open file.
    pub fn write_conflict_result(&mut self, ctx: &egui::Context, index: usize, text: &str) {
        let Some(open) = &self.conflict else { return };
        // From the file's own text: the bases filled in for showing aren't
        // written into its other conflicts.
        let Ok(current) = &open.text else { return };
        let Some(chunks) = conflict::parse(current) else {
            return;
        };
        let op = Op::WriteConflict {
            file: open.file.clone(),
            expected: current.clone(),
            text: conflict::resolve_with(&chunks, index, text),
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

    /// What bringing `theirs` into the checked-out commit would do, as a
    /// sentence for a question's body, worked out without touching any
    /// files. `rebase` words it as places a rebase may stop. Empty when it
    /// can't tell, or there's nothing to bring in.
    pub fn conflict_forecast(&self, theirs: &str, rebase: bool) -> String {
        let Ok(p) = merge::predict(self.path(), "HEAD", theirs) else {
            return String::new();
        };
        if p.conflicts.is_empty() {
            return if p.changed == 0 {
                String::new()
            } else if rebase {
                "It applies cleanly, with no conflicts.".into()
            } else {
                "It merges cleanly, with no conflicts.".into()
            };
        }
        let n = p.conflicts.len();
        let files = if n == 1 { "file" } else { "files" };
        let mut names = p
            .conflicts
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if n > 3 {
            names.push_str(&format!(" and {} more", n - 3));
        }
        let lead = if rebase {
            format!("It may stop for conflicts in {n} {files}: {names}.")
        } else {
            format!("This will conflict in {n} {files}: {names}.")
        };
        let clean = p.changed.saturating_sub(n);
        let rest = match clean {
            0 => String::new(),
            1 => " Everything else (1 file) goes in cleanly.".into(),
            c => format!(" Everything else ({c} files) goes in cleanly."),
        };
        format!("{lead}{rest} (Checked without touching your files.)")
    }

    /// The branch a pull from `from` (else the upstream) brings in, as of
    /// the last fetch.
    fn pull_source(&self, from: Option<&str>) -> Option<String> {
        let upstream = self.head_upstream().map(|u| u.name.clone());
        match from {
            None => upstream,
            Some(remote) => {
                let branch = upstream
                    .as_deref()
                    .and_then(|u| u.split_once('/').map(|(_, b)| b.to_string()))
                    .or_else(|| self.head_branch().map(str::to_string))?;
                Some(format!("{remote}/{branch}"))
            }
        }
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
                body: with_forecast(
                    format!(
                        "Brings {source}'s commits into {head}. If {head} has nothing new of its own, it simply moves forward (fast-forward); otherwise a merge commit joins them. If both changed the same lines, you'll resolve the conflicts first."
                    ),
                    &self.conflict_forecast(&source, false),
                ),
                choices: vec![
                    ("Merge".into(), false, merge(MergeMode::Default)),
                    (
                        "Always create a merge commit".into(),
                        false,
                        merge(MergeMode::NoFastForward),
                    ),
                    (
                        format!("Rebase {head} onto {source}"),
                        false,
                        Op::Rebase(source.clone()),
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

    /// Opens the interactive rebase planner for the commits after `base`.
    pub fn plan_rebase(&mut self, base: String, base_label: String) {
        match interactive::commits(self.path(), &base) {
            Ok(commits) => {
                let pushed = commits
                    .first()
                    .is_some_and(|c| interactive::is_pushed(self.path(), &c.id));
                let rows: Vec<PlanRow> = commits
                    .into_iter()
                    .rev()
                    .map(|c| PlanRow {
                        new_message: c.message.clone(),
                        id: c.id,
                        summary: c.summary,
                        message: c.message,
                        action: PlanAction::Pick,
                    })
                    .collect();
                let original = rows.iter().map(|r| r.id.clone()).collect();
                self.conflict = None;
                self.active_file = None;
                self.planner = Some((
                    Planner {
                        base,
                        base_label,
                        rows,
                        pushed,
                    },
                    original,
                ));
            }
            Err(e) => self.refresh_error = Some(format!("Can't rebase from here: {e}")),
        }
    }

    /// Runs the planned interactive rebase.
    pub fn start_planned_rebase(&mut self, ctx: &egui::Context) {
        let Some((planner, _)) = self.planner.take() else {
            return;
        };
        let steps = planner.steps();
        self.run(
            ctx,
            Op::InteractiveRebase {
                base: planner.base,
                steps,
            },
        );
    }

    /// Asks before rebasing the checked-out branch onto `onto`.
    pub fn confirm_rebase(&mut self, onto: String) {
        let Some(head) = self.head_branch().map(str::to_string) else {
            self.refresh_error = Some("Could not rebase: no branch is checked out.".into());
            return;
        };
        let pushed = commit::head_is_pushed(self.path()).unwrap_or(false);
        let warning = if pushed {
            format!(
                " {head}'s commits are already pushed, so afterwards you'll need to force-push, and anyone who has them will need to fix up their copies."
            )
        } else {
            String::new()
        };
        self.dialog = Some(Dialog::Confirm {
            title: format!("Rebase {head} onto {onto}?"),
            body: with_forecast(
                format!(
                    "Replays {head}'s own commits on top of {onto}, for a straight history. The replayed commits are new commits.{warning} If a commit conflicts, you'll resolve it, then continue."
                ),
                &self.conflict_forecast(&onto, true),
            ),
            button: "Rebase".into(),
            danger: pushed,
            op: Op::Rebase(onto),
        });
    }

    /// Asks how to reset the checked-out branch to commit `id`.
    pub fn ask_reset(&mut self, id: String, label: String) {
        let head = self.head_branch().unwrap_or("HEAD").to_string();
        let reset = |mode| Op::Reset {
            id: id.clone(),
            mode,
        };
        self.dialog = Some(Dialog::Choice {
            title: format!("Reset {head} to {label}?"),
            body: format!(
                "Moves {head} to this commit; the commits after it leave the branch.\n\nSoft: their changes stay staged.\nMixed: their changes stay in your files, unstaged.\nHard: your files match this commit; their changes and any uncommitted changes are discarded."
            ),
            choices: vec![
                ("Soft".into(), false, reset(ResetMode::Soft)),
                ("Mixed".into(), false, reset(ResetMode::Mixed)),
                ("Hard".into(), true, reset(ResetMode::Hard)),
            ],
        });
    }

    /// Pulls from `from` (else the upstream) as `options` say; asks first
    /// when that discards uncommitted changes.
    pub fn pull(
        &mut self,
        ctx: &egui::Context,
        from: Option<String>,
        options: crate::settings::PullOptions,
    ) {
        use crate::settings::PullChanges;
        let from_name = from.clone();
        let op = Op::Pull {
            from,
            mode: options.strategy.mode(),
            stash: options.changes == PullChanges::Stash,
        };
        let discard = match options.changes {
            PullChanges::Discard => self.discard_all(),
            PullChanges::Stash | PullChanges::Leave => None,
        };
        let Some((what, mut steps)) = discard else {
            // Merging or rebasing pulls ask first when, as of the last
            // fetch, they would conflict.
            let rebase = match options.strategy.mode() {
                PullMode::FastForward => None,
                PullMode::Merge => Some(false),
                PullMode::Rebase => Some(true),
            };
            if let Some(rebase) = rebase
                && let Some(source) = self.pull_source(from_name.as_deref())
                && let Ok(p) = merge::predict(self.path(), "HEAD", &source)
                && !p.conflicts.is_empty()
            {
                self.dialog = Some(Dialog::Confirm {
                    title: format!("Pulling {source} will likely conflict"),
                    body: format!(
                        "{} You'll resolve the conflicts before the pull finishes, or abort it to go back. This is as of your last fetch; the pull fetches again first.",
                        self.conflict_forecast(&source, rebase)
                    ),
                    button: "Pull anyway".into(),
                    danger: false,
                    op,
                });
                return;
            }
            self.run(ctx, op);
            return;
        };
        steps.push(op);
        self.dialog = Some(Dialog::Confirm {
            title: "Discard your changes and pull?".into(),
            body: format!(
                "Pull is set to discard uncommitted changes: every change to {what} is thrown away before pulling: staged and unstaged changes, and new files. Each file's current version is moved to the {} first, so you can get it back from there. Choose another way in Pull's options to keep them.",
                trash_name()
            ),
            button: "Discard and pull".into(),
            danger: true,
            op: Op::Then(steps),
        });
    }

    /// Fetches from `remote` (else every remote) as `options` say; asks
    /// first when it forces, since that can lose commits.
    pub fn fetch(
        &mut self,
        ctx: &egui::Context,
        remote: Option<String>,
        options: sync::FetchOptions,
    ) {
        let from = remote.clone().unwrap_or_else(|| "every remote".into());
        let op = match remote {
            Some(r) => Op::FetchRemote(r, options),
            None => Op::Fetch(options),
        };
        if !options.force {
            self.run(ctx, op);
            return;
        }
        self.dialog = Some(Dialog::Confirm {
            title: "Fetch with Force?".into(),
            body: format!(
                "Fetching from {from}, refs that come in replace yours even where that drops commits they don't have. Turn Force off in Fetch's options to fetch without it."
            ),
            button: "Fetch with Force".into(),
            danger: true,
            op,
        });
    }

    /// Asks before taking one side's whole version of a conflicted file:
    /// the other side's changes in it, and what was picked or edited here,
    /// are replaced.
    pub fn confirm_take_side(&mut self, file: String, ours: bool) {
        // The sides by name: "main", "feature/login".
        let state = self.status.as_ref().and_then(|s| s.in_progress.as_ref());
        let (side, other) = match state {
            Some(s) if ours => (s.ours.name.clone(), s.theirs.name.clone()),
            Some(s) => (s.theirs.name.clone(), s.ours.name.clone()),
            None if ours => ("ours".into(), "theirs".into()),
            None => ("theirs".into(), "ours".into()),
        };
        self.dialog = Some(Dialog::Confirm {
            title: format!("Use {side}'s {file}?"),
            body: format!(
                "The whole file becomes {side}'s version: {other}'s changes in it, and anything picked or edited here, are replaced."
            ),
            button: format!("Use {side}'s"),
            danger: true,
            op: Op::TakeSide { file, ours },
        });
    }

    /// Says a search is going on, with Stop first.
    pub fn ask_to_stop_the_search(&mut self) {
        self.dialog = Some(Dialog::Confirm {
            title: "A search is going on".into(),
            body: "Finding the commit that broke something has a commit checked out to test: checking out, committing, merging or rebasing now would get in its way. Stop the search first (it goes back to where it started, with anything put aside brought back), then try again.".into(),
            button: "Stop the search".into(),
            danger: false,
            op: Op::BisectStop,
        });
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

    /// The full path of a submodule (given relative to this repository).
    pub fn submodule_dir(&self, sub: &str) -> String {
        std::path::Path::new(self.path())
            .join(sub)
            .to_string_lossy()
            .into_owned()
    }

    /// Opens the dialog to add a worktree, next to this repository.
    pub fn new_submodule(&mut self) {
        self.dialog = Some(Dialog::Submodule {
            url: String::new(),
            folder: String::new(),
        });
    }

    pub fn new_worktree(&mut self) {
        let root = std::path::Path::new(self.path().trim_end_matches(['/', '\\']));
        let name = root
            .file_name()
            .map_or_else(|| "repo".into(), |n| n.to_string_lossy().into_owned());
        let parent = root.parent().unwrap_or(root);
        self.dialog = Some(Dialog::Worktree {
            folder: parent
                .join(format!("{name}-worktree"))
                .to_string_lossy()
                .into_owned(),
            branch: String::new(),
            new: true,
        });
    }

    pub fn confirm_remove_worktree(&mut self, folder: String) {
        self.dialog = Some(Dialog::Confirm {
            title: "Remove this worktree?".into(),
            body: format!(
                "Deletes the folder {folder}. Its branch and commits stay in the repository."
            ),
            button: "Remove worktree".into(),
            danger: true,
            op: Op::RemoveWorktree {
                folder,
                force: false,
            },
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

    /// Asks before changing a remote's address, showing both.
    pub fn confirm_remote_url(&mut self, name: String, url: String, how: &str) {
        let old = self
            .refs
            .remotes
            .iter()
            .find(|r| r.name == name)
            .and_then(|r| r.url.clone())
            .unwrap_or_default();
        let why = if how == "Use port 443" {
            " GitHub also answers SSH on port 443, which firewalls that block SSH's usual port let through."
        } else {
            ""
        };
        self.dialog = Some(Dialog::Confirm {
            title: format!("{how} for {name}?"),
            body: format!(
                "Changes {name}'s address\nfrom {old}\nto {url}\n\nYou can undo it.{why}"
            ),
            button: how.to_string(),
            danger: false,
            op: Op::SetRemoteUrl { name, url },
        });
    }

    /// Tests signing in to a remote over SSH, saying who as, or why not.
    pub fn test_remote(&mut self, name: &str) {
        let Some(url) = self
            .refs
            .remotes
            .iter()
            .find(|r| r.name == name)
            .and_then(|r| r.url.clone())
        else {
            return;
        };
        self.ssh_problem = Some(crate::ui::ssh_problem::SshProblem::test(
            name.to_string(),
            url,
        ));
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

    /// Asks where to delete a tag: here, and on which remotes.
    pub fn confirm_delete_tag(&mut self, name: String) {
        let mut places = vec![Place {
            label: "Local".into(),
            remote: None,
            on: true,
        }];
        places.extend(self.refs.remotes.iter().map(|r| Place {
            label: r.name.clone(),
            remote: Some(r.name.clone()),
            on: false,
        }));
        self.dialog = Some(Dialog::Delete {
            tag: true,
            name,
            places,
        });
    }

    /// Asks where to delete branch `name`: here (if it's here), and on the
    /// remotes that have it. `remote` starts ticked instead of here, for a
    /// remote branch's own menu.
    fn confirm_delete_branch_in(&mut self, name: String, remote: Option<&str>) {
        let mut places = Vec::new();
        if self.refs.local.iter().any(|b| b.name == name) {
            places.push(Place {
                label: "Local".into(),
                remote: None,
                on: remote.is_none(),
            });
        }
        for r in &self.refs.remotes {
            if r.branches.iter().any(|b| b.name == name) {
                places.push(Place {
                    label: r.name.clone(),
                    remote: Some(r.name.clone()),
                    on: remote == Some(r.name.as_str()),
                });
            }
        }
        self.dialog = Some(Dialog::Delete {
            tag: false,
            name,
            places,
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

    /// The remote an operation went to over SSH, and its address: the
    /// one it names, else the upstream's, else origin.
    fn ssh_remote(&self, op: &Op) -> Option<(String, String)> {
        let named = match op {
            Op::Then(ops) => return ops.iter().rev().find_map(|o| self.ssh_remote(o)),
            Op::FetchRemote(r, _)
            | Op::PushBranch { remote: r, .. }
            | Op::PushTags { remote: r, .. }
            | Op::Push {
                to: Some((r, _)), ..
            }
            | Op::Pull { from: Some(r), .. }
            | Op::FetchInto { remote: r, .. } => Some(r.clone()),
            Op::Fetch(_) | Op::Pull { .. } | Op::Push { .. } => None,
            _ => return None,
        };
        let remote = named
            .or_else(|| {
                self.head_upstream()
                    .and_then(|u| u.name.split_once('/').map(|(r, _)| r.to_string()))
            })
            .unwrap_or_else(|| "origin".into());
        let url = self
            .refs
            .remotes
            .iter()
            .find(|r| r.name == remote)?
            .url
            .clone()?;
        git_raccoon_core::ssh::parse_url(&url)?;
        Some((remote, url))
    }

    /// When Git refused an operation for a reason the user can act on,
    /// offers the way on. Returns whether it did.
    fn offer_way_on(&mut self, op: &Op, error: &Error) -> bool {
        // A pull after discarding changes: only the pull refuses.
        if let Op::Then(ops) = op
            && let Some(pull @ Op::Pull { .. }) = ops.last()
        {
            return self.offer_way_on(pull, error);
        }
        let upstream = match op {
            Op::Push {
                to: Some((remote, branch)),
                ..
            } => format!("{remote}/{branch}"),
            // The same branch as the upstream's (else the local one) there.
            Op::Pull {
                from: Some(remote), ..
            } => {
                let branch = self
                    .head_upstream()
                    .and_then(|u| u.name.split_once('/').map(|(_, b)| b.to_string()))
                    .or_else(|| self.head_branch().map(str::to_string))
                    .unwrap_or_default();
                format!("{remote}/{branch}")
            }
            _ => self
                .head_upstream()
                .map_or_else(|| "the remote branch".to_string(), |u| u.name.clone()),
        };
        match (error, op) {
            (Error::Cancelled, _) => true,
            (
                Error::Dirty(_),
                Op::RemoveWorktree {
                    folder,
                    force: false,
                },
            ) => {
                self.dialog = Some(Dialog::Confirm {
                    title: "This worktree has uncommitted changes".into(),
                    body: format!(
                        "{folder} has changes that aren't committed anywhere. Removing it anyway deletes them; its branch and commits stay."
                    ),
                    button: "Remove anyway".into(),
                    danger: true,
                    op: Op::RemoveWorktree {
                        folder: folder.clone(),
                        force: true,
                    },
                });
                true
            }
            (Error::Rejected(_), Op::Push { force: false, to }) => {
                self.dialog = Some(Dialog::Choice {
                    title: "The remote has commits you don't have".into(),
                    body: format!(
                        "Someone pushed to {upstream} since you last pulled. Pull to combine their commits with yours, then push again.\n\nForce-pushing replaces the remote branch with yours, deleting their commits from it. It's only done if the remote is still as you last fetched it."
                    ),
                    choices: vec![
                        ("Pull first".into(), false, Op::pull_upstream()),
                        (
                            "Force push".into(),
                            true,
                            Op::Push {
                                force: true,
                                to: to.clone(),
                            },
                        ),
                    ],
                });
                true
            }
            (Error::Rejected(text), Op::Push { force: true, .. }) => {
                self.refresh_error = Some(format!(
                    "Could not force push: {upstream} has changed since you last fetched, so it wasn't overwritten. Fetch to see the new commits first.\n\n{text}"
                ));
                true
            }
            (Error::Diverged(_), op) if remote_checkout(op).is_some() => {
                let remote = remote_checkout(op).unwrap_or_default().to_string();
                let local = local_name(&remote).to_string();
                self.dialog = Some(Dialog::Choice {
                    title: format!("{local} and {remote} have both moved on"),
                    body: with_forecast(
                        format!(
                            "{local} is checked out, but it has commits {remote} doesn't, so it can't simply be moved up to it. Merge adds a merge commit joining them; rebase replays your commits on top of theirs, for a straight history."
                        ),
                        &self.conflict_forecast(&remote, false),
                    ),
                    choices: vec![
                        (
                            "Merge".into(),
                            false,
                            Op::Merge {
                                branch: remote.clone(),
                                mode: MergeMode::Default,
                            },
                        ),
                        ("Rebase".into(), false, Op::Rebase(remote)),
                    ],
                });
                true
            }
            (
                Error::Diverged(_),
                Op::Pull {
                    from,
                    mode: PullMode::FastForward,
                    stash,
                },
            ) => {
                let pull = |mode| Op::Pull {
                    from: from.clone(),
                    mode,
                    stash: *stash,
                };
                self.dialog = Some(Dialog::Choice {
                    title: format!("Your branch and {upstream} have both moved on"),
                    body: with_forecast(
                        "Each has commits the other doesn't. Merge adds a merge commit joining them; rebase replays your commits on top of theirs, for a straight history.".into(),
                        &self.conflict_forecast(&upstream, false),
                    ),
                    choices: vec![
                        ("Merge".into(), false, pull(PullMode::Merge)),
                        ("Rebase".into(), false, pull(PullMode::Rebase)),
                    ],
                });
                true
            }
            (Error::Dirty(_), op) if op.can_stash_first() => {
                let then = if op.is_checkout() {
                    "switch"
                } else {
                    match op {
                        Op::Then(ops) => ops.last().map_or("continue", Op::verb),
                        op => op.verb(),
                    }
                };
                let button = format!("Stash and {then}");
                self.dialog = Some(Dialog::Confirm {
                    title: "Your changes are in the way".into(),
                    body: format!(
                        "Some uncommitted changes would be overwritten. Stash them first (untracked files too), then {then}? Apply or pop the stash afterwards to get them back."
                    ),
                    button,
                    danger: false,
                    op: Op::StashThen(Box::new(op.clone())),
                });
                true
            }
            (
                Error::NotMerged(which),
                Op::DeleteBranches {
                    names,
                    force: false,
                },
            ) => {
                self.dialog = Some(Dialog::Confirm {
                    title: "Some have unmerged work".into(),
                    body: format!(
                        "{which}: their commits aren't on any other branch. Nothing was deleted. Deleting them anyway hides those commits from the graph (Undo brings them back)."
                    ),
                    button: format!("Delete all {} anyway", names.len()),
                    danger: true,
                    op: Op::DeleteBranches {
                        names: names.clone(),
                        force: true,
                    },
                });
                true
            }
            (
                Error::NotMerged(_),
                Op::DeleteBranchEverywhere {
                    name,
                    remote_branches,
                    force: false,
                },
            ) => {
                self.dialog = Some(Dialog::Confirm {
                    title: format!("{name} has work that's nowhere else"),
                    body: "Its commits aren't on any other branch or tag. Nothing was deleted. Deleting it everywhere anyway hides them from the graph (Undo brings them back).".into(),
                    button: "Delete anyway".into(),
                    danger: true,
                    op: Op::DeleteBranchEverywhere {
                        name: name.clone(),
                        remote_branches: remote_branches.clone(),
                        force: true,
                    },
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

    /// Picks lines of the open diff (to copy, or stage), as a click on
    /// line (`hunk`, `line`) does.
    pub fn select_line(&mut self, at: (usize, usize), how: Select) {
        self.picked_in = self.diff_key();
        match (how, self.pick_anchor) {
            (Select::Range, Some(anchor)) => {
                let Some(diff) = self.open_diff() else { return };
                // In the order they're shown (the whole file's unchanged
                // lines are numbered after the changes).
                let order: Vec<(usize, usize)> = diff
                    .unified
                    .iter()
                    .filter_map(|r| match r {
                        crate::highlight::Row::Line(h, l) => Some((*h, *l)),
                        crate::highlight::Row::Header(_) => None,
                    })
                    .collect();
                let (Some(a), Some(b)) = (
                    order.iter().position(|p| *p == anchor),
                    order.iter().position(|p| *p == at),
                ) else {
                    return;
                };
                self.picked = order[a.min(b)..=a.max(b)].iter().copied().collect();
            }
            (Select::Toggle, _) => {
                if !self.picked.remove(&at) {
                    self.picked.insert(at);
                }
                self.pick_anchor = Some(at);
            }
            _ => {
                self.picked.clear();
                self.picked.insert(at);
                self.pick_anchor = Some(at);
            }
        }
    }

    /// Picks every line of the open diff.
    pub fn select_all_lines(&mut self) {
        if let Some(diff) = self.open_diff() {
            self.picked_in = self.diff_key();
            self.picked = diff
                .unified
                .iter()
                .filter_map(|r| match r {
                    crate::highlight::Row::Line(h, l) => Some((*h, *l)),
                    crate::highlight::Row::Header(_) => None,
                })
                .collect();
        }
    }

    /// The picked lines' text, in the order they're shown.
    pub fn picked_text(&self) -> Option<String> {
        let diff = self.open_diff()?;
        let lines: Vec<&str> = diff
            .unified
            .iter()
            .filter_map(|r| match r {
                crate::highlight::Row::Line(h, l) if self.picked.contains(&(*h, *l)) => {
                    let line = &diff.diff.hunks[*h].lines[*l];
                    (line.kind != LineKind::NoEol).then_some(line.content.as_str())
                }
                _ => None,
            })
            .collect();
        (!lines.is_empty()).then(|| lines.join("\n"))
    }

    /// How many of the picked lines are changes (that staging would take).
    pub fn picked_changes(&self) -> usize {
        self.open_diff()
            .map_or(0, |d| changed(&d, &self.picked).len())
    }

    fn lines_op(&self, pick: Pick) -> Option<Op> {
        let diff = self.open_diff()?;
        Some(Op::Lines {
            side: self.active_side,
            file: self.active_file.clone()?,
            pick,
            headers: diff.diff.hunks[..diff.changes]
                .iter()
                .map(|h| h.header.clone())
                .collect(),
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
        let Some(diff) = self.open_diff() else { return };
        let lines = changed(&diff, &self.picked);
        if lines.is_empty() {
            return;
        }
        if let Some(op) = self.lines_op(Pick::Lines(lines)) {
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
        // Finding the commit that broke something: Git's state, and the
        // work going on.
        let searching = self.status.as_ref().is_some_and(|s| {
            s.in_progress
                .as_ref()
                .is_some_and(|p| p.what == merge::InProgress::Bisect)
        });
        let path = self.path().to_string();
        self.bisect.poll(ctx, &path, searching);
        if self.bisect.take_moved() {
            self.refresh(ctx);
        }
        self.load_file_past(ctx);
        if let Some(result) = self.tool_open.as_ref().and_then(|t| t.done.poll())
            && let Some(open) = self.tool_open.take()
        {
            self.tool_closed(ctx, open, result);
        }
        // Picked lines belong to the diff they were picked in.
        if !self.picked.is_empty() && self.picked_in != self.diff_key() {
            self.picked.clear();
            self.pick_anchor = None;
        }
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
            let for_agent = std::mem::take(&mut self.commit_for_agent);
            if let Some(report) = self.report.take() {
                let _ = report.send(result.as_ref().map(drop).map_err(Clone::clone));
            }
            match result {
                Ok(undo) => {
                    // An agent's commit leaves what's typed in the box.
                    if !for_agent {
                        self.draft = CommitDraft::default();
                    }
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
            if let Some(report) = self.report.take() {
                let _ = report.send(result.as_ref().map(drop).map_err(ToString::to_string));
            }
            self.picked.clear();
            self.pick_anchor = None;
            match result {
                Ok(Some(undo)) => self.undo = Some(undo),
                // Something changed that can't be undone: older undos would be out of order.
                // (A fetch only moves remote branches, so local undos still hold.)
                Ok(None)
                    if op.changes_refs() && !matches!(op, Op::Fetch(_) | Op::FetchRemote(..)) =>
                {
                    self.undo = None
                }
                Ok(None) => {}
                Err(e) => {
                    let said = e.to_string();
                    if git_raccoon_core::ssh::is_ssh_failure(&said)
                        && let Some((remote, url)) = self.ssh_remote(&op)
                    {
                        // Found out why, with the fix, instead of Git's words.
                        self.ssh_problem = Some(crate::ui::ssh_problem::SshProblem::new(
                            op.clone(),
                            remote,
                            url,
                            said,
                        ));
                    } else if !self.offer_way_on(&op, &e) {
                        self.refresh_error = Some(format!("Could not {}:\n{e}", op.verb()));
                        // To run by hand, in the terminal.
                        self.failed_command = op.command();
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
            self.diff.want(ctx, &key, move |(source, file, whole)| {
                let diff = match &source {
                    Source::Commit(id) => details::file_diff(&path, id, &file),
                    Source::Working(side) => working::working_diff(&path, *side, &file),
                }
                .map_err(|e| e.to_string())?;
                if !whole {
                    return Ok(Arc::new(RenderedDiff::new(diff)));
                }
                // The file before the change, for the lines around it.
                let before = match &source {
                    Source::Commit(id) => details::file_blobs(&path, id, &file),
                    Source::Working(side) => working::working_blobs(&path, *side, &file),
                }
                .ok()
                .and_then(|b| b.old)
                .and_then(|b| b.bytes)
                .unwrap_or_default();
                let runs = details::unchanged_around(&diff, &before);
                // Working diffs keep their hunk headers, for staging.
                let headers = matches!(source, Source::Working(_));
                Ok(Arc::new(RenderedDiff::whole(diff, runs, headers)))
            });
            if crate::images::is_image(&key.1) {
                let path = self.path().to_string();
                let ctx2 = ctx.clone();
                self.images.want(ctx, &key, move |(source, file, _)| {
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
        self.bisect.status_changed();
        self.rule_tool = None;
        self.tool_results = if status.as_ref().is_some_and(|s| s.in_progress.is_some()) {
            git_raccoon_core::rules::driver_results_now(self.path())
        } else {
            Vec::new()
        };
        let had_conflicts = self
            .status
            .as_ref()
            .is_some_and(|s| !s.conflicted.is_empty());
        let was_conflicted: Vec<String> = self
            .status
            .as_ref()
            .map(|s| s.conflicted.iter().map(|f| f.path.clone()).collect())
            .unwrap_or_default();
        self.status = status;
        let conflicted: Vec<String> = self
            .status
            .as_ref()
            .map(|s| s.conflicted.iter().map(|f| f.path.clone()).collect())
            .unwrap_or_default();
        let in_progress = self
            .status
            .as_ref()
            .is_some_and(|s| s.in_progress.is_some());
        // While something's in progress: how each file conflicts, and which
        // were resolved since (to review, or bring back).
        if in_progress {
            self.conflict_files = conflict::files(self.path()).unwrap_or_default();
            for file in was_conflicted.iter().filter(|f| !conflicted.contains(f)) {
                self.resolved_here.insert(file.clone());
            }
            for file in &conflicted {
                self.resolved_here.remove(file);
            }
        } else {
            self.conflict_files.clear();
            self.resolved_here.clear();
            self.history_over_conflicts = false;
        }
        // New conflicts (from a merge, pull or stash): the overview shows
        // them all (in place of the history).
        if !had_conflicts && !conflicted.is_empty() {
            self.select_working();
            self.history_over_conflicts = false;
        }
        // The open conflicted file: re-read it, or close it once resolved.
        if let Some(open) = &self.conflict {
            if conflicted.contains(&open.file) {
                if open.edit.is_none() {
                    let path = self.path().to_string();
                    if let Some(open) = &mut self.conflict {
                        open.reload(&path);
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
            self.blame.reload();
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
        self.close_forge_item();
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
        self.extras = loaded.extras;
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
        self.close_forge_item();
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

    /// Closes the pull/merge request or issue in the details pane.
    pub fn close_forge_item(&mut self) {
        if let Some(f) = &mut self.forge {
            f.open = None;
        }
    }

    pub fn deselect(&mut self) {
        self.conflict = None;
        self.close_forge_item();
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
        let next = current.saturating_add(step).clamp(0, last);
        self.select_index(usize::try_from(next).unwrap_or(0));
    }

    /// Follows the selected commit's branch: down to its first parent, or
    /// up to the newer commit that continues it (the one in the same lane,
    /// when a commit has several). With nothing selected, the newest.
    pub fn follow_branch(&mut self, down: bool) {
        let Some(row) = self.selected_row() else {
            return self.step(1);
        };
        self.rows.ensure(row..row + 1);
        let Some(commit) = self.rows.get(row) else {
            return;
        };
        let (id, lane) = (commit.id.clone(), commit.column);
        let target = if down {
            commit
                .parents
                .first()
                .and_then(|parent| self.history.locate(parent))
        } else {
            // Its children are above it: the nearest in its own lane, or
            // else the nearest.
            let mut nearest = None;
            let mut found = None;
            for r in (0..row).rev() {
                self.rows.ensure(r..r + 1);
                let Some(c) = self.rows.get(r) else { continue };
                if c.parents.first() == Some(&id) {
                    nearest.get_or_insert(r);
                    if c.column == lane {
                        found = Some(r);
                        break;
                    }
                }
            }
            found.or(nearest)
        };
        if let Some(target) = target {
            self.select_index(target);
        }
    }

    /// Selects the next (or previous) commit with a label: a branch, tag,
    /// remote branch or stash.
    pub fn step_label(&mut self, down: bool) {
        let from = self.selected_row();
        let found = if down {
            let start = from.map_or(0, |r| r + 1);
            (start..self.history.total()).find(|&r| self.labelled(r))
        } else {
            (0..from.unwrap_or(0)).rev().find(|&r| self.labelled(r))
        };
        if let Some(row) = found {
            self.select_index(row);
        }
    }

    /// Whether the commit at `row` has a label.
    fn labelled(&mut self, row: usize) -> bool {
        self.rows.ensure(row..row + 1);
        self.rows.get(row).is_some_and(|c| !c.refs.is_empty())
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

/// A question's body with a conflict forecast after it, if there is one.
fn with_forecast(body: String, forecast: &str) -> String {
    if forecast.is_empty() {
        body
    } else {
        format!("{body}\n\n{forecast}")
    }
}
