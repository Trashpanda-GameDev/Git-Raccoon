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
    /// Something else (applying patches, bisecting): continued outside the app.
    Other,
}

impl InProgress {
    pub fn noun(&self) -> &'static str {
        match self {
            InProgress::Merge => "merge",
            InProgress::Rebase => "rebase",
            InProgress::CherryPick => "cherry-pick",
            InProgress::Revert => "revert",
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
            InProgress::Other => None,
        }
    }
}

/// What's in progress, with Git's description of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub what: InProgress,
    /// Such as "Merge branch 'feature'" (from Git's prepared message).
    pub detail: Option<String>,
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
        _ => InProgress::Other,
    };
    let detail = std::fs::read_to_string(repo.path().join("MERGE_MSG"))
        .ok()
        .and_then(|m| m.lines().next().map(str::to_string))
        .filter(|l| !l.trim().is_empty());
    Ok(Some(State { what, detail }))
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
