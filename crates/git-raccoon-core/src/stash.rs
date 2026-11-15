//! Stashes, through the user's own `git stash`, which handles untracked
//! files, conflicts and messages exactly as on the command line.
//!
//! Stashes are named by their commit id rather than their position, since
//! positions shift whenever a stash is added or removed.

use git2::{Oid, Repository};

use crate::cli;
use crate::{Error, Result};

/// Stashes all uncommitted changes (and untracked files, if asked).
pub fn save(path: &str, message: &str, include_untracked: bool) -> Result<()> {
    let mut args = vec!["stash", "push"];
    if include_untracked {
        args.push("--include-untracked");
    }
    let message = message.trim();
    if !message.is_empty() {
        args.extend(["--message", message]);
    }
    cli::run(path, &args, None).map(drop)
}

/// "stash@{n}" for the stash whose commit is `id`.
fn reference(path: &str, id: &str) -> Result<String> {
    let mut repo = Repository::discover(path)?;
    let wanted = Oid::from_str(id)?;
    let mut found = None;
    repo.stash_foreach(|index, _, oid| {
        if *oid == wanted {
            found = Some(index);
        }
        found.is_none()
    })?;
    found
        .map(|n| format!("stash@{{{n}}}"))
        .ok_or_else(|| Error::Stale(format!("stash {}", &id[..7.min(id.len())])))
}

/// Applies a stash, keeping it.
pub fn apply(path: &str, id: &str) -> Result<()> {
    cli::run(path, &["stash", "apply", &reference(path, id)?], None).map(drop)
}

/// Applies a stash and removes it. If applying conflicts, Git keeps the stash.
pub fn pop(path: &str, id: &str) -> Result<()> {
    cli::run(path, &["stash", "pop", &reference(path, id)?], None).map(drop)
}

/// Deletes a stash. Its commit stays recoverable by id until Git prunes it.
pub fn drop_stash(path: &str, id: &str) -> Result<()> {
    cli::run(path, &["stash", "drop", &reference(path, id)?], None).map(drop)
}

/// Puts a dropped stash back on the stash list by its commit id.
pub fn store(path: &str, id: &str, message: &str) -> Result<()> {
    cli::run(path, &["stash", "store", "--message", message, id], None).map(drop)
}

/// The newest stash's commit id.
pub fn top(path: &str) -> Result<Option<String>> {
    let repo = Repository::discover(path)?;
    Ok(repo
        .find_reference("refs/stash")
        .ok()
        .and_then(|r| r.target())
        .map(|id| id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refs::load_refs;
    use crate::testutil::TempRepo;
    use crate::working::working_status;

    fn repo() -> TempRepo {
        let t = TempRepo::new();
        let mut config = t.repo.config().unwrap();
        config.set_str("user.name", "Rocky Raccoon").unwrap();
        config.set_str("user.email", "rocky@example.com").unwrap();
        t.write("f.txt", "one\n");
        t.commit_all("first");
        t
    }

    fn stash_ids(t: &TempRepo) -> Vec<(String, String)> {
        load_refs(t.path())
            .unwrap()
            .stashes
            .into_iter()
            .map(|s| (s.target, s.message))
            .collect()
    }

    fn read(t: &TempRepo, file: &str) -> Option<String> {
        std::fs::read_to_string(t.dir.join(file)).ok()
    }

    #[test]
    fn saves_applies_pops_and_drops() {
        let t = repo();
        t.write("f.txt", "two\n");
        t.write("new.txt", "untracked\n");
        save(t.path(), "half done", true).unwrap();
        assert!(
            working_status(t.path()).unwrap().unwrap().is_clean(),
            "untracked files went too"
        );
        let stashes = stash_ids(&t);
        assert_eq!(stashes.len(), 1);
        assert!(stashes[0].1.contains("half done"));
        let id = stashes[0].0.clone();

        apply(t.path(), &id).unwrap();
        assert_eq!(read(&t, "f.txt").as_deref(), Some("two\n"));
        assert_eq!(read(&t, "new.txt").as_deref(), Some("untracked\n"));
        assert_eq!(stash_ids(&t).len(), 1, "apply keeps the stash");

        // Put the changes away again, then pop the older stash by id.
        save(t.path(), "second", true).unwrap();
        assert_eq!(stash_ids(&t).len(), 2);
        pop(t.path(), &id).unwrap();
        let left = stash_ids(&t);
        assert_eq!(left.len(), 1);
        assert!(left[0].1.contains("second"), "the other stash is untouched");

        drop_stash(t.path(), &left[0].0).unwrap();
        assert!(stash_ids(&t).is_empty());
        assert!(matches!(
            drop_stash(t.path(), &left[0].0),
            Err(Error::Stale(_))
        ));
    }

    #[test]
    fn a_conflicting_pop_keeps_the_stash() {
        let t = repo();
        t.write("f.txt", "stashed version\n");
        save(t.path(), "", false).unwrap();
        let id = stash_ids(&t)[0].0.clone();
        t.write("f.txt", "conflicting edit\n");
        t.commit_all("meanwhile");

        t.write("f.txt", "uncommitted edit\n");
        let err = pop(t.path(), &id).unwrap_err();
        assert!(matches!(err, Error::Command(_)), "{err}");
        assert_eq!(stash_ids(&t).len(), 1, "the stash is kept");
        assert_eq!(
            read(&t, "f.txt").as_deref(),
            Some("uncommitted edit\n"),
            "local work is untouched"
        );
    }
}
