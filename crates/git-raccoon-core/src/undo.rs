//! What undoing needs from Git: the reflog's record of where HEAD was, and
//! careful ways to put refs back.
//!
//! Every function that moves something back first checks that it is still
//! where the action left it, so an undo never throws away work done since
//! (it fails with [`Error::Stale`] instead).

use git2::{BranchType, Oid, Repository};

use crate::cli;
use crate::{Error, Result};

/// The newest entry of HEAD's reflog: HEAD moved from `from` to `to`.
#[derive(Debug, Clone, PartialEq)]
pub struct HeadMove {
    /// Where HEAD was; `None` before the first commit.
    pub from: Option<String>,
    pub to: String,
    /// Git's description, such as "commit: Fix it" or
    /// "checkout: moving from main to feature".
    pub message: String,
}

impl HeadMove {
    /// For a checkout: the branch (or commit id, if detached) HEAD was on.
    pub fn checked_out_from(&self) -> Option<&str> {
        let rest = self.message.strip_prefix("checkout: moving from ")?;
        rest.rsplit_once(" to ").map(|(from, _)| from)
    }

    /// Whether this records a merge: completed directly ("merge feature: ...")
    /// or committed after resolving conflicts ("commit (merge): ...").
    pub fn is_merge(&self) -> bool {
        self.message.starts_with("merge ") || self.message.starts_with("commit (merge):")
    }

    /// Whether this records a new commit ("commit:", "commit (initial):"
    /// or, with `amend`, "commit (amend):").
    pub fn is_commit(&self, amend: bool) -> bool {
        if amend {
            self.message.starts_with("commit (amend):")
        } else {
            self.message.starts_with("commit:") || self.message.starts_with("commit (initial):")
        }
    }
}

/// The last thing that moved HEAD, from the reflog.
pub fn last_head_move(path: &str) -> Result<Option<HeadMove>> {
    let repo = Repository::discover(path)?;
    let log = repo.reflog("HEAD")?;
    Ok(log.get(0).map(|e| HeadMove {
        from: (!e.id_old().is_zero()).then(|| e.id_old().to_string()),
        to: e.id_new().to_string(),
        message: e.message().unwrap_or_default().to_string(),
    }))
}

/// HEAD's commit id, if there is one.
pub fn head_id(path: &str) -> Result<Option<String>> {
    let repo = Repository::discover(path)?;
    Ok(repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .map(|id| id.to_string()))
}

fn short(id: &str) -> &str {
    &id[..7.min(id.len())]
}

/// Moves the current branch back from commit `expected` to `to`, keeping
/// the index and working tree: the undone commit's changes end up staged.
pub fn reset_soft(path: &str, expected: &str, to: &str) -> Result<()> {
    if head_id(path)?.as_deref() != Some(expected) {
        return Err(Error::Stale(format!(
            "HEAD is no longer at {}, so the commit wasn't undone",
            short(expected)
        )));
    }
    cli::run(path, &["reset", "--soft", to], None).map(drop)
}

/// Moves the current branch back from `expected` to `to` and makes the
/// index match it, leaving files alone (undoing a mixed reset).
pub fn reset_mixed(path: &str, expected: &str, to: &str) -> Result<()> {
    if head_id(path)?.as_deref() != Some(expected) {
        return Err(Error::Stale(format!(
            "HEAD is no longer at {}, so the reset wasn't undone",
            short(expected)
        )));
    }
    cli::run(path, &["reset", "--quiet", "--mixed", to], None).map(drop)
}

/// Moves the current branch back from `expected` to `to` (undoing a merge),
/// keeping uncommitted changes; Git refuses if they'd be overwritten.
pub fn reset_keep(path: &str, expected: &str, to: &str) -> Result<()> {
    if head_id(path)?.as_deref() != Some(expected) {
        return Err(Error::Stale(format!(
            "HEAD is no longer at {}, so the merge wasn't undone",
            short(expected)
        )));
    }
    cli::run(path, &["reset", "--keep", to], None).map(drop)
}

/// Deletes a branch, but only if it still points at `expected`, so commits
/// made on it since are never lost.
pub fn delete_branch_at(path: &str, name: &str, expected: &str) -> Result<()> {
    let tip = crate::branch::tip(path, name)?;
    if tip != expected {
        return Err(Error::Stale(format!(
            "{name} has moved on since (it was at {}), so it wasn't deleted",
            short(expected)
        )));
    }
    crate::branch::delete(path, name, true)
}

/// A local branch's upstream, such as "origin/main".
pub fn upstream_of(path: &str, name: &str) -> Result<Option<String>> {
    let repo = Repository::discover(path)?;
    let branch = repo.find_branch(name, BranchType::Local)?;
    let upstream = branch.upstream().ok();
    Ok(upstream.and_then(|u| u.name().ok().flatten().map(str::to_string)))
}

/// What a full ref name ("refs/tags/v1") points at.
pub fn ref_target(path: &str, name: &str) -> Result<String> {
    let repo = Repository::discover(path)?;
    let r = repo.find_reference(name)?;
    r.target()
        .map(|id| id.to_string())
        .ok_or_else(|| Error::Unsupported(format!("{name} is a symbolic ref")))
}

/// Puts back a deleted ref (a branch or tag) exactly as it was. Refuses if
/// something by that name exists again.
pub fn restore_ref(path: &str, name: &str, id: &str) -> Result<()> {
    let repo = Repository::discover(path)?;
    let oid = Oid::from_str(id)?;
    if repo.find_reference(name).is_ok() {
        return Err(Error::Stale(format!("{name} exists again")));
    }
    repo.reference(name, oid, false, "Git Raccoon: undo")?;
    Ok(())
}

/// Restores a branch's upstream; ignored if the remote branch is gone.
pub fn set_upstream(path: &str, name: &str, upstream: &str) {
    if let Ok(repo) = Repository::discover(path)
        && let Ok(mut branch) = repo.find_branch(name, BranchType::Local)
    {
        let _ = branch.set_upstream(Some(upstream));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;
    use crate::{branch, commit, refs, stash};

    fn repo() -> TempRepo {
        let t = TempRepo::new();
        let mut config = t.repo.config().unwrap();
        config.set_str("user.name", "Rocky Raccoon").unwrap();
        config.set_str("user.email", "rocky@example.com").unwrap();
        config.set_bool("commit.gpgsign", false).unwrap();
        t.write("f.txt", "one\n");
        cli::run(t.path(), &["add", "f.txt"], None).unwrap();
        commit::commit(t.path(), "first", false).unwrap();
        t
    }

    #[test]
    fn undoes_a_commit_keeping_its_changes_staged() {
        let t = repo();
        let before = head_id(t.path()).unwrap().unwrap();
        t.write("f.txt", "two\n");
        cli::run(t.path(), &["add", "f.txt"], None).unwrap();
        let done = commit::commit(t.path(), "second", false).unwrap();

        let last = last_head_move(t.path()).unwrap().unwrap();
        assert!(last.is_commit(false), "{last:?}");
        assert_eq!(last.from.as_deref(), Some(before.as_str()));
        assert_eq!(last.to, done.id);

        reset_soft(t.path(), &done.id, &before).unwrap();
        assert_eq!(head_id(t.path()).unwrap().as_deref(), Some(before.as_str()));
        let status = crate::working::working_status(t.path()).unwrap().unwrap();
        assert_eq!(status.staged.len(), 1, "the change is staged again");
        assert_eq!(
            std::fs::read_to_string(t.dir.join("f.txt")).unwrap(),
            "two\n"
        );

        // A second undo of the same commit is refused: HEAD moved.
        let err = reset_soft(t.path(), &done.id, &before).unwrap_err();
        assert!(matches!(err, Error::Stale(_)), "{err}");
    }

    #[test]
    fn recognises_amends_and_checkouts() {
        let t = repo();
        t.write("f.txt", "two\n");
        cli::run(t.path(), &["add", "f.txt"], None).unwrap();
        commit::commit(t.path(), "first, amended", true).unwrap();
        let last = last_head_move(t.path()).unwrap().unwrap();
        assert!(last.is_commit(true) && !last.is_commit(false), "{last:?}");

        let main = refs::load_refs(t.path()).unwrap().local[0].name.clone();
        branch::create(t.path(), "feature", "HEAD", true).unwrap();
        let last = last_head_move(t.path()).unwrap().unwrap();
        assert_eq!(last.checked_out_from(), Some(main.as_str()), "{last:?}");
        branch::checkout_detached(t.path(), "HEAD").unwrap();
        branch::checkout(t.path(), "feature").unwrap();
        let last = last_head_move(t.path()).unwrap().unwrap();
        assert_eq!(
            last.checked_out_from(),
            last.from.as_deref(),
            "detached: an id"
        );
    }

    #[test]
    fn undoes_a_merge_keeping_local_changes() {
        let (t, _) = crate::merge::tests::conflicting();
        // Resolve f.txt's conflict by hand, then finish the merge.
        let before = head_id(t.path()).unwrap().unwrap();
        crate::merge::merge(t.path(), "feature", crate::merge::MergeMode::Default).unwrap();
        crate::conflict::take_side(t.path(), "f.txt", true).unwrap();
        crate::merge::continue_op(t.path()).unwrap();
        let last = last_head_move(t.path()).unwrap().unwrap();
        assert!(last.is_merge(), "{last:?}");
        t.write("local.txt", "untracked work\n");

        reset_keep(t.path(), &last.to, &before).unwrap();
        assert_eq!(head_id(t.path()).unwrap().as_deref(), Some(before.as_str()));
        assert!(!t.dir.join("g.txt").exists(), "the merged-in file is gone");
        assert!(t.dir.join("local.txt").exists(), "local work stays");
    }

    #[test]
    fn deletes_a_new_branch_only_if_untouched() {
        let t = repo();
        let head = head_id(t.path()).unwrap().unwrap();
        branch::create(t.path(), "feature", "HEAD", true).unwrap();
        t.write("f.txt", "work\n");
        cli::run(t.path(), &["add", "f.txt"], None).unwrap();
        commit::commit(t.path(), "work on feature", false).unwrap();
        let main = refs::load_refs(t.path())
            .unwrap()
            .local
            .into_iter()
            .find(|b| b.name != "feature")
            .unwrap()
            .name;
        branch::checkout(t.path(), &main).unwrap();

        let err = delete_branch_at(t.path(), "feature", &head).unwrap_err();
        assert!(matches!(err, Error::Stale(_)), "{err}");
        let tip = branch::tip(t.path(), "feature").unwrap();
        delete_branch_at(t.path(), "feature", &tip).unwrap();
    }

    #[test]
    fn restores_deleted_branches_tags_and_upstreams() {
        let t = repo();
        let head = head_id(t.path()).unwrap().unwrap();
        t.repo
            .reference(
                "refs/remotes/origin/feature",
                Oid::from_str(&head).unwrap(),
                true,
                "test",
            )
            .unwrap();
        t.repo
            .remote("origin", "https://example.com/r.git")
            .unwrap();
        branch::create(t.path(), "feature", "HEAD", false).unwrap();
        set_upstream(t.path(), "feature", "origin/feature");
        assert_eq!(
            upstream_of(t.path(), "feature").unwrap().as_deref(),
            Some("origin/feature")
        );
        branch::create_tag(t.path(), "v1", "HEAD", "Annotated").unwrap();
        let tag = ref_target(t.path(), "refs/tags/v1").unwrap();
        assert_ne!(tag, head, "an annotated tag points at a tag object");

        branch::delete(t.path(), "feature", false).unwrap();
        branch::delete_tag(t.path(), "v1").unwrap();
        restore_ref(t.path(), "refs/heads/feature", &head).unwrap();
        set_upstream(t.path(), "feature", "origin/feature");
        restore_ref(t.path(), "refs/tags/v1", &tag).unwrap();

        let r = refs::load_refs(t.path()).unwrap();
        let feature = r.local.iter().find(|b| b.name == "feature").unwrap();
        assert_eq!(feature.upstream.as_ref().unwrap().name, "origin/feature");
        assert!(r.tags.iter().any(|t| t.name == "v1" && t.annotated));
        assert!(
            restore_ref(t.path(), "refs/tags/v1", &tag).is_err(),
            "exists again"
        );
    }

    #[test]
    fn stores_a_dropped_stash_back() {
        let t = repo();
        t.write("f.txt", "stashed\n");
        stash::save(t.path(), "keep me", false).unwrap();
        let s = refs::load_refs(t.path()).unwrap().stashes.remove(0);
        stash::drop_stash(t.path(), &s.target).unwrap();
        assert!(refs::load_refs(t.path()).unwrap().stashes.is_empty());
        stash::store(t.path(), &s.target, &s.message).unwrap();
        let back = refs::load_refs(t.path()).unwrap().stashes;
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].target, s.target);
        assert_eq!(back[0].message, s.message);
    }
}
