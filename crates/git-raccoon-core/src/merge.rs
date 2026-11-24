//! Merging, and the operations Git can leave half-done (merge, rebase,
//! cherry-pick, revert) with their continue and abort, through the user's
//! own `git`.

use git2::{Repository, RepositoryState};

use crate::{Error, Result, cli};

/// An operation Git has started and is waiting to finish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InProgress {
    Merge,
    Rebase,
    CherryPick,
    Revert,
    /// Finding the commit that broke something (Git's bisect): its own bar.
    Bisect,
    /// Something else (applying patches): continued outside the app.
    Other,
}

impl InProgress {
    pub fn noun(&self) -> &'static str {
        match self {
            InProgress::Merge => "merge",
            InProgress::Rebase => "rebase",
            InProgress::CherryPick => "cherry-pick",
            InProgress::Revert => "revert",
            InProgress::Bisect => "search for the commit that broke it",
            InProgress::Other => "Git operation",
        }
    }

    /// The `git` subcommand that continues and aborts it.
    fn command(&self) -> Option<&'static str> {
        match self {
            InProgress::Merge => Some("merge"),
            InProgress::Rebase => Some("rebase"),
            InProgress::CherryPick => Some("cherry-pick"),
            InProgress::Revert => Some("revert"),
            InProgress::Bisect | InProgress::Other => None,
        }
    }
}

/// What's in progress, with Git's description of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub what: InProgress,
    /// Such as "Merge branch 'feature'" (from Git's prepared message).
    pub detail: Option<String>,
    /// The two sides of its conflicts, named: what the files had ("ours"
    /// to Git) and what's coming in ("theirs"). In a rebase that's what's
    /// being rebased onto, and the commit being applied.
    pub ours: Side,
    pub theirs: Side,
    /// A rebase's (or a series' of picks) progress: the step, of how many.
    pub step: Option<(usize, usize)>,
}

/// One side of a conflict: a branch's name (or a commit's short id), and
/// the commit it is, with its message's first line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Side {
    pub name: String,
    pub commit: Option<String>,
    pub summary: Option<String>,
}

impl State {
    /// Whether the commit being applied can be skipped (a rebase, or a
    /// series of picks or reverts).
    pub fn can_skip(&self) -> bool {
        matches!(
            self.what,
            InProgress::Rebase | InProgress::CherryPick | InProgress::Revert
        )
    }
}

/// The operation in progress, if any.
pub fn in_progress(path: &str) -> Result<Option<State>> {
    let repo = Repository::discover(path)?;
    let what = match repo.state() {
        RepositoryState::Clean => return Ok(None),
        RepositoryState::Merge => InProgress::Merge,
        RepositoryState::Rebase
        | RepositoryState::RebaseInteractive
        | RepositoryState::RebaseMerge => InProgress::Rebase,
        RepositoryState::CherryPick | RepositoryState::CherryPickSequence => InProgress::CherryPick,
        RepositoryState::Revert | RepositoryState::RevertSequence => InProgress::Revert,
        RepositoryState::Bisect => InProgress::Bisect,
        _ => InProgress::Other,
    };
    let detail = std::fs::read_to_string(repo.path().join("MERGE_MSG"))
        .ok()
        .and_then(|m| m.lines().next().map(str::to_string))
        .filter(|l| !l.trim().is_empty());
    let (ours, theirs, step) = sides(&repo, &what, detail.as_deref());
    Ok(Some(State {
        what,
        detail,
        ours,
        theirs,
        step,
    }))
}

/// A file in the repository's Git folder, trimmed.
fn git_file(repo: &Repository, name: &str) -> Option<String> {
    std::fs::read_to_string(repo.path().join(name))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A commit as a side: `name` if given, otherwise a branch at it (local
/// first), otherwise its short id; with its message's first line.
fn side_at(repo: &Repository, id: Option<&str>, name: Option<String>) -> Side {
    let commit = id
        .and_then(|id| git2::Oid::from_str(id.trim()).ok())
        .and_then(|oid| repo.find_commit(oid).ok());
    let at = |kind| {
        let oid = commit.as_ref()?.id();
        repo.branches(Some(kind))
            .ok()?
            .flatten()
            .find_map(|(b, _)| {
                (b.get().target() == Some(oid))
                    .then(|| b.name().ok().flatten().map(str::to_string))
                    .flatten()
            })
    };
    let name = name
        .or_else(|| at(git2::BranchType::Local))
        .or_else(|| at(git2::BranchType::Remote))
        .or_else(|| commit.as_ref().map(|c| c.id().to_string()[..7].to_string()))
        .unwrap_or_else(|| "HEAD".into());
    Side {
        name,
        commit: commit.as_ref().map(|c| c.id().to_string()),
        summary: commit
            .as_ref()
            .and_then(|c| c.summary().map(str::to_string)),
    }
}

/// The branch checked out, as a side.
fn checked_out(repo: &Repository) -> Side {
    let head = repo.head().ok();
    let id = head
        .as_ref()
        .and_then(|h| h.target())
        .map(|o| o.to_string());
    let name = head
        .as_ref()
        .filter(|h| h.is_branch())
        .and_then(|h| h.shorthand().map(str::to_string));
    side_at(repo, id.as_deref(), name)
}

/// The branch a merge brings in, from Git's message: "Merge branch 'x'",
/// "Merge remote-tracking branch 'origin/x'".
fn merged_name(detail: Option<&str>) -> Option<String> {
    let d = detail?;
    let rest = d
        .strip_prefix("Merge branch '")
        .or_else(|| d.strip_prefix("Merge remote-tracking branch '"))?;
    rest.split('\'').next().map(str::to_string)
}

/// Names the sides of what's in progress, and how far along a rebase is.
fn sides(
    repo: &Repository,
    what: &InProgress,
    detail: Option<&str>,
) -> (Side, Side, Option<(usize, usize)>) {
    match what {
        InProgress::Merge => (
            checked_out(repo),
            side_at(
                repo,
                git_file(repo, "MERGE_HEAD")
                    .as_deref()
                    .and_then(|s| s.lines().next()),
                merged_name(detail),
            ),
            None,
        ),
        InProgress::Rebase => {
            // `rebase-merge` (the usual) or `rebase-apply` (older, `--apply`).
            let (dir, at, of) = if repo.path().join("rebase-merge").is_dir() {
                ("rebase-merge", "msgnum", "end")
            } else {
                ("rebase-apply", "next", "last")
            };
            let file = |name: &str| git_file(repo, &format!("{dir}/{name}"));
            let number = |name: &str| file(name).and_then(|s| s.parse().ok());
            let step = number(at).zip(number(of));
            let branch = file("head-name").map(|h| {
                h.strip_prefix("refs/heads/")
                    .map_or(h.clone(), str::to_string)
            });
            let onto = side_at(repo, file("onto").as_deref(), None);
            let applying = git_file(repo, "REBASE_HEAD").or_else(|| file("stopped-sha"));
            (onto, side_at(repo, applying.as_deref(), branch), step)
        }
        InProgress::CherryPick => (
            checked_out(repo),
            side_at(repo, git_file(repo, "CHERRY_PICK_HEAD").as_deref(), None),
            None,
        ),
        InProgress::Revert => {
            let mut theirs = side_at(repo, git_file(repo, "REVERT_HEAD").as_deref(), None);
            theirs.name = format!("reverting {}", theirs.name);
            (checked_out(repo), theirs, None)
        }
        InProgress::Bisect | InProgress::Other => (checked_out(repo), Side::default(), None),
    }
}

/// What bringing one commit into another would do, worked out in memory
/// (no files touched): the files that would conflict, and how many files
/// the incoming side changes in all.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Prediction {
    pub conflicts: Vec<String>,
    pub changed: usize,
}

/// Predicts merging `theirs` into `ours` (branches, tags or commits). A
/// rebase applies commits one by one, so for one this says which files it
/// may stop on.
pub fn predict(path: &str, ours: &str, theirs: &str) -> Result<Prediction> {
    let repo = Repository::discover(path)?;
    let commit = |name: &str| -> Result<git2::Commit<'_>> {
        Ok(repo.revparse_single(name)?.peel_to_commit()?)
    };
    let (ours, theirs) = (commit(ours)?, commit(theirs)?);
    let base = repo.merge_base(ours.id(), theirs.id()).ok();
    // Already in, or a fast-forward: nothing can conflict.
    if base == Some(theirs.id()) || base == Some(ours.id()) {
        return Ok(Prediction::default());
    }
    let changed = match base {
        Some(base) => repo
            .diff_tree_to_tree(
                Some(&repo.find_commit(base)?.tree()?),
                Some(&theirs.tree()?),
                None,
            )?
            .deltas()
            .len(),
        None => 0,
    };
    let index = repo.merge_commits(&ours, &theirs, None)?;
    let mut conflicts: Vec<String> = index
        .conflicts()?
        .flatten()
        .filter_map(|c| {
            let e = c.our.or(c.their).or(c.ancestor)?;
            Some(String::from_utf8_lossy(&e.path).into_owned())
        })
        .collect();
    conflicts.sort();
    conflicts.dedup();
    Ok(Prediction { conflicts, changed })
}

/// How a merge may combine histories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeMode {
    /// Fast-forward when possible, otherwise make a merge commit.
    Default,
    /// Always make a merge commit.
    NoFastForward,
}

/// How a merge ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merged {
    /// Done (or there was nothing to merge).
    Done,
    /// Stopped with conflicts to resolve; the merge is in progress.
    Conflicts,
}

/// Merges `branch` (local or remote, such as "origin/main") into the
/// checked-out branch.
pub fn merge(path: &str, branch: &str, mode: MergeMode) -> Result<Merged> {
    let mut args = vec!["merge", "--no-edit"];
    if mode == MergeMode::NoFastForward {
        args.push("--no-ff");
    }
    args.push(branch);
    match cli::run(path, &args, None) {
        Ok(_) => Ok(Merged::Done),
        Err(Error::Command(text)) => {
            if in_progress(path)?.is_some_and(|s| s.what == InProgress::Merge) {
                Ok(Merged::Conflicts)
            } else if text.contains("would be overwritten") {
                Err(Error::Dirty(text))
            } else {
                Err(Error::Command(text))
            }
        }
        Err(e) => Err(e),
    }
}

/// Continues the operation in progress, once its conflicts are resolved,
/// with Git's prepared message. Stopping on new conflicts (in a rebase,
/// say) isn't an error: it's still in progress.
pub fn continue_op(path: &str) -> Result<()> {
    let state = in_progress(path)?.ok_or_else(|| Error::Stale("nothing in progress".into()))?;
    let result = match state.what {
        InProgress::Merge => cli::run(path, &["commit", "--no-edit"], None),
        other => {
            let cmd = other
                .command()
                .ok_or_else(|| Error::Unsupported("continue this from the command line".into()))?;
            cli::run(path, &[cmd, "--continue"], None)
        }
    };
    match result {
        Ok(_) => Ok(()),
        // Stopped again at the next conflict: nothing went wrong.
        Err(_) if has_conflicts(path)? => Ok(()),
        Err(e) => Err(e),
    }
}

/// Skips the commit being applied (in a rebase, or a series of picks or
/// reverts) and goes on with the next. Stopping on new conflicts isn't an
/// error: it's still in progress.
pub fn skip(path: &str) -> Result<()> {
    let state = in_progress(path)?.ok_or_else(|| Error::Stale("nothing in progress".into()))?;
    if !state.can_skip() {
        return Err(Error::Unsupported(format!(
            "a {} has no commit to skip",
            state.what.noun()
        )));
    }
    let cmd = state.what.command().unwrap_or("rebase");
    match cli::run(path, &[cmd, "--skip"], None) {
        Ok(_) => Ok(()),
        Err(_) if has_conflicts(path)? => Ok(()),
        Err(e) => Err(e),
    }
}

/// Abandons the operation in progress, returning to how things were.
pub fn abort(path: &str) -> Result<()> {
    let state = in_progress(path)?.ok_or_else(|| Error::Stale("nothing in progress".into()))?;
    let cmd = state
        .what
        .command()
        .ok_or_else(|| Error::Unsupported("abort this from the command line".into()))?;
    cli::run(path, &[cmd, "--abort"], None).map(drop)
}

/// Whether any files still have unresolved conflicts.
pub fn has_conflicts(path: &str) -> Result<bool> {
    let repo = Repository::discover(path)?;
    Ok(repo.index()?.has_conflicts())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    pub(crate) fn identity(t: &TempRepo) {
        let mut config = t.repo.config().unwrap();
        config.set_str("user.name", "Rocky Raccoon").unwrap();
        config.set_str("user.email", "rocky@example.com").unwrap();
        config.set_bool("commit.gpgsign", false).unwrap();
    }

    pub(crate) fn git(t: &TempRepo, args: &[&str]) {
        cli::run(t.path(), args, None).unwrap();
    }

    /// main and feature both change line 2 of f.txt; feature also adds g.txt.
    pub(crate) fn conflicting() -> (TempRepo, String) {
        let t = TempRepo::new();
        identity(&t);
        t.write("f.txt", "one\ntwo\nthree\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "base"]);
        let main = t.repo.head().unwrap().shorthand().unwrap().to_string();
        git(&t, &["switch", "-q", "-c", "feature"]);
        t.write("f.txt", "one\nTWO (feature)\nthree\n");
        t.write("g.txt", "new\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "feature work"]);
        git(&t, &["switch", "-q", &main]);
        t.write("f.txt", "one\ntwo (main)\nthree\n");
        git(&t, &["commit", "-q", "-am", "main work"]);
        (t, main)
    }

    #[test]
    fn predicts_conflicts_without_touching_anything() {
        let (t, main) = conflicting();
        let before = std::fs::read_to_string(t.dir.join("f.txt")).unwrap();
        let p = predict(t.path(), &main, "feature").unwrap();
        assert_eq!(p.conflicts, ["f.txt"]);
        assert_eq!(p.changed, 2, "f.txt and g.txt");
        assert_eq!(
            std::fs::read_to_string(t.dir.join("f.txt")).unwrap(),
            before
        );
        assert!(in_progress(t.path()).unwrap().is_none());
        // A clean one, and one already in.
        git(&t, &["switch", "-q", "-c", "other", &format!("{main}~1")]);
        t.write("x.txt", "x\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "x"]);
        assert!(
            predict(t.path(), &main, "other")
                .unwrap()
                .conflicts
                .is_empty()
        );
        assert_eq!(
            predict(t.path(), &main, &format!("{main}~1")).unwrap(),
            Prediction::default()
        );
    }

    #[test]
    fn names_the_sides_of_a_merge_and_a_rebase() {
        let (t, main) = conflicting();
        merge(t.path(), "feature", MergeMode::Default).unwrap();
        let s = in_progress(t.path()).unwrap().unwrap();
        assert_eq!(
            (s.ours.name.as_str(), s.theirs.name.as_str()),
            (main.as_str(), "feature")
        );
        assert_eq!(s.theirs.summary.as_deref(), Some("feature work"));
        assert!(!s.can_skip() && s.step.is_none());
        git(&t, &["merge", "--abort"]);

        // Rebasing feature (two commits) onto main: the first conflicts.
        git(&t, &["switch", "-q", "feature"]);
        t.write("h.txt", "more\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "more feature work"]);
        assert!(cli::run(t.path(), &["rebase", &main], None).is_err());
        let s = in_progress(t.path()).unwrap().unwrap();
        assert_eq!(s.what, InProgress::Rebase);
        assert_eq!(s.ours.name, main, "what it's rebased onto");
        assert_eq!(s.theirs.name, "feature", "the branch being rebased");
        assert_eq!(s.theirs.summary.as_deref(), Some("feature work"));
        assert_eq!(s.step, Some((1, 2)));
        assert!(s.can_skip());
        // Skipping that commit: the next one applies cleanly, and it's done.
        skip(t.path()).unwrap();
        assert!(in_progress(t.path()).unwrap().is_none());
        assert!(skip(t.path()).is_err(), "nothing to skip now");
    }

    #[test]
    fn merges_cleanly_or_fast_forwards() {
        let t = TempRepo::new();
        identity(&t);
        t.write("f.txt", "one\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "base"]);
        let main = t.repo.head().unwrap().shorthand().unwrap().to_string();
        git(&t, &["switch", "-q", "-c", "feature"]);
        t.write("g.txt", "new\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "feature"]);
        let feature = t.repo.head().unwrap().target().unwrap();
        git(&t, &["switch", "-q", &main]);

        assert_eq!(
            merge(t.path(), "feature", MergeMode::Default).unwrap(),
            Merged::Done
        );
        let head = Repository::open(&t.dir)
            .unwrap()
            .head()
            .unwrap()
            .target()
            .unwrap();
        assert_eq!(head, feature, "fast-forwarded");

        git(&t, &["reset", "-q", "--hard", "HEAD~1"]);
        merge(t.path(), "feature", MergeMode::NoFastForward).unwrap();
        let repo = Repository::open(&t.dir).unwrap();
        assert_eq!(
            repo.head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .parent_count(),
            2
        );
        assert_eq!(in_progress(t.path()).unwrap(), None);
    }

    #[test]
    fn stops_on_conflicts_and_can_abort_or_continue() {
        let (t, _) = conflicting();
        let before = t.repo.head().unwrap().target().unwrap();
        assert_eq!(
            merge(t.path(), "feature", MergeMode::Default).unwrap(),
            Merged::Conflicts
        );
        let state = in_progress(t.path()).unwrap().unwrap();
        assert_eq!(state.what, InProgress::Merge);
        assert_eq!(state.detail.as_deref(), Some("Merge branch 'feature'"));
        assert!(has_conflicts(t.path()).unwrap());
        assert!(
            continue_op(t.path()).is_ok(),
            "still conflicted: stays in progress"
        );
        assert!(in_progress(t.path()).unwrap().is_some());

        abort(t.path()).unwrap();
        assert_eq!(in_progress(t.path()).unwrap(), None);
        assert_eq!(
            Repository::open(&t.dir)
                .unwrap()
                .head()
                .unwrap()
                .target()
                .unwrap(),
            before
        );

        merge(t.path(), "feature", MergeMode::Default).unwrap();
        t.write("f.txt", "one\nboth\nthree\n");
        git(&t, &["add", "f.txt"]);
        continue_op(t.path()).unwrap();
        assert_eq!(in_progress(t.path()).unwrap(), None);
        let repo = Repository::open(&t.dir).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.parent_count(), 2);
        assert_eq!(head.summary(), Some("Merge branch 'feature'"));
    }

    #[test]
    fn reports_local_changes_in_the_way() {
        let (t, _) = conflicting();
        t.write("f.txt", "uncommitted\n");
        let err = merge(t.path(), "feature", MergeMode::Default).unwrap_err();
        assert!(matches!(err, Error::Dirty(_)), "{err}");
        assert_eq!(in_progress(t.path()).unwrap(), None);
    }
}
