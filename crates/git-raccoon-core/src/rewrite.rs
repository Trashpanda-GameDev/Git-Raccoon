//! Rebase, cherry-pick, revert and reset, through the user's own `git`.
//! The first three can stop on conflicts, leaving the operation in
//! progress (see [`crate::merge`]) rather than failing.

use git2::Repository;

use crate::merge::in_progress;
use crate::{Error, Result, cli};

/// Runs a command that may stop on conflicts: stopping isn't an error.
fn may_stop(path: &str, args: &[&str]) -> Result<()> {
    match cli::run(path, args, None) {
        Ok(_) => Ok(()),
        Err(Error::Command(text)) => {
            if in_progress(path)?.is_some() {
                Ok(())
            } else if text.contains("would be overwritten") {
                Err(Error::Dirty(text))
            } else {
                Err(Error::Command(text))
            }
        }
        Err(e) => Err(e),
    }
}

/// Merge commits need a parent to compare against: the first, the branch
/// they were merged into.
fn mainline(path: &str, id: &str) -> Result<bool> {
    let repo = Repository::discover(path)?;
    let commit = repo.revparse_single(id)?.peel_to_commit()?;
    Ok(commit.parent_count() > 1)
}

/// Replays the checked-out branch's own commits on top of `onto`.
/// Uncommitted changes are put aside and restored afterwards.
pub fn rebase(path: &str, onto: &str) -> Result<()> {
    may_stop(path, &["rebase", "--autostash", onto])
}

/// Applies the changes of commit `id` on top of the checked-out branch.
pub fn cherry_pick(path: &str, id: &str) -> Result<()> {
    if mainline(path, id)? {
        may_stop(path, &["cherry-pick", "-m", "1", id])
    } else {
        may_stop(path, &["cherry-pick", id])
    }
}

/// Adds a commit that undoes the changes of commit `id`.
pub fn revert(path: &str, id: &str) -> Result<()> {
    if mainline(path, id)? {
        may_stop(path, &["revert", "--no-edit", "-m", "1", id])
    } else {
        may_stop(path, &["revert", "--no-edit", id])
    }
}

/// What a reset does besides moving the branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    /// Keeps the index and files: the undone commits' changes stay staged.
    Soft,
    /// Keeps the files, unstaged.
    Mixed,
    /// Makes the files match too, discarding uncommitted changes.
    Hard,
}

/// Moves the checked-out branch to commit `id`.
pub fn reset(path: &str, id: &str, mode: ResetMode) -> Result<()> {
    let flag = match mode {
        ResetMode::Soft => "--soft",
        ResetMode::Mixed => "--mixed",
        ResetMode::Hard => "--hard",
    };
    cli::run(path, &["reset", "--quiet", flag, id], None).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::tests::{conflicting, git, identity};
    use crate::merge::{InProgress, abort};
    use crate::testutil::TempRepo;
    use crate::undo::head_id;

    fn log(t: &TempRepo) -> Vec<String> {
        let out = cli::run(t.path(), &["log", "--format=%s"], None).unwrap();
        out.lines().map(str::to_string).collect()
    }

    /// main: base, m1; feature (from base): f1, f2 touching other files.
    fn branches() -> (TempRepo, String) {
        let t = TempRepo::new();
        identity(&t);
        t.write("base.txt", "base\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "base"]);
        let main = t.repo.head().unwrap().shorthand().unwrap().to_string();
        git(&t, &["switch", "-q", "-c", "feature"]);
        for n in ["f1", "f2"] {
            t.write(&format!("{n}.txt"), "x\n");
            git(&t, &["add", "."]);
            git(&t, &["commit", "-q", "-m", n]);
        }
        git(&t, &["switch", "-q", &main]);
        t.write("m1.txt", "x\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "m1"]);
        (t, main)
    }

    #[test]
    fn rebases_keeping_local_changes() {
        let (t, main) = branches();
        git(&t, &["switch", "-q", "feature"]);
        t.write("base.txt", "uncommitted\n");
        rebase(t.path(), &main).unwrap();
        assert_eq!(
            log(&t),
            ["f2", "f1", "m1", "base"],
            "replayed on top of main"
        );
        assert_eq!(
            std::fs::read_to_string(t.dir.join("base.txt")).unwrap(),
            "uncommitted\n"
        );
    }

    #[test]
    fn rebase_stops_on_conflicts() {
        let (t, main) = conflicting();
        git(&t, &["switch", "-q", "feature"]);
        rebase(t.path(), &main).unwrap();
        assert_eq!(
            in_progress(t.path()).unwrap().unwrap().what,
            InProgress::Rebase
        );
        abort(t.path()).unwrap();
        assert_eq!(in_progress(t.path()).unwrap(), None);
    }

    #[test]
    fn cherry_picks_and_reverts() {
        let (t, _) = branches();
        let f1 = cli::run(t.path(), &["rev-parse", "feature~1"], None).unwrap();
        cherry_pick(t.path(), f1.trim()).unwrap();
        assert_eq!(log(&t)[0], "f1");
        assert!(t.dir.join("f1.txt").exists());

        let head = head_id(t.path()).unwrap().unwrap();
        revert(t.path(), &head).unwrap();
        assert_eq!(log(&t)[0], "Revert \"f1\"");
        assert!(!t.dir.join("f1.txt").exists());
    }

    #[test]
    fn cherry_pick_reports_conflicts_and_changes_in_the_way() {
        let (t, _) = conflicting();
        let theirs = cli::run(t.path(), &["rev-parse", "feature"], None).unwrap();
        t.write("f.txt", "uncommitted\n");
        let err = cherry_pick(t.path(), theirs.trim()).unwrap_err();
        assert!(matches!(err, Error::Dirty(_)), "{err}");
        git(&t, &["checkout", "--", "f.txt"]);
        cherry_pick(t.path(), theirs.trim()).unwrap();
        assert_eq!(
            in_progress(t.path()).unwrap().unwrap().what,
            InProgress::CherryPick
        );
    }

    #[test]
    fn resets_three_ways() {
        let (t, _) = branches();
        let back = cli::run(t.path(), &["rev-parse", "HEAD~1"], None).unwrap();
        let back = back.trim();
        let status = |t: &TempRepo| crate::working::working_status(t.path()).unwrap().unwrap();

        reset(t.path(), back, ResetMode::Soft).unwrap();
        assert_eq!(status(&t).staged.len(), 1, "m1.txt staged");
        git(&t, &["reset", "-q", "--hard", "ORIG_HEAD"]);

        reset(t.path(), back, ResetMode::Mixed).unwrap();
        let s = status(&t);
        assert!(
            s.staged.is_empty() && s.unstaged.len() == 1,
            "m1.txt untracked"
        );
        git(&t, &["reset", "-q", "--hard", "ORIG_HEAD"]);

        // Undoing a mixed reset puts the branch and index back, files untouched.
        let tip = head_id(t.path()).unwrap().unwrap();
        reset(t.path(), back, ResetMode::Mixed).unwrap();
        crate::undo::reset_mixed(t.path(), back, &tip).unwrap();
        assert!(status(&t).is_clean(), "exactly as before");

        reset(t.path(), back, ResetMode::Hard).unwrap();
        assert!(status(&t).is_clean());
        assert!(!t.dir.join("m1.txt").exists());
    }
}
