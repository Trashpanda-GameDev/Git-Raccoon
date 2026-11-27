//! Committing, through the user's own `git`, so their hooks, signing
//! settings and identity apply exactly as on the command line.

use git2::Repository;

use crate::{Error, Result};

/// Who the app's commits are by (the profile in use), and Git's own.
pub use crate::cli::{global_identity, set_identity, terminal_env};

/// What a successful commit produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Committed {
    /// The new commit's id.
    pub id: String,
}

/// Commits what is staged with `message` (or, with `amend`, replaces the
/// last commit, keeping its parent). Hook output and Git's own complaints
/// come back as [`Error::Command`] text.
pub fn commit(path: &str, message: &str, amend: bool) -> Result<Committed> {
    if message.trim().is_empty() {
        return Err(Error::Unsupported("a commit needs a message".into()));
    }
    let mut args = vec!["commit", "--file=-", "--cleanup=strip"];
    if amend {
        args.push("--amend");
    }
    crate::cli::run(path, &args, Some(message))?;
    let repo = Repository::discover(path)?;
    let id = repo.head()?.peel_to_commit()?.id().to_string();
    Ok(Committed { id })
}

/// The last commit's full message, for amending.
pub fn head_message(path: &str) -> Result<Option<String>> {
    let repo = Repository::discover(path)?;
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return Ok(None), // no commits yet
    };
    Ok(head
        .peel_to_commit()?
        .message()
        .map(|m| m.trim_end().to_string()))
}

/// Whether the last commit is already on the current branch's upstream, so
/// amending it would rewrite pushed history.
pub fn head_is_pushed(path: &str) -> Result<bool> {
    let repo = Repository::discover(path)?;
    let Ok(head) = repo.head() else {
        return Ok(false);
    };
    let (Some(target), true) = (head.target(), head.is_branch()) else {
        return Ok(false);
    };
    let Some(name) = head.shorthand() else {
        return Ok(false);
    };
    let Ok(branch) = repo.find_branch(name, git2::BranchType::Local) else {
        return Ok(false);
    };
    let Ok(upstream) = branch.upstream() else {
        return Ok(false);
    };
    let Some(up) = upstream.get().target() else {
        return Ok(false);
    };
    Ok(up == target || repo.graph_descendant_of(up, target)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;
    use std::path::Path;

    fn repo_with_identity() -> TempRepo {
        let t = TempRepo::new();
        let mut config = t.repo.config().unwrap();
        config.set_str("user.name", "Rocky Raccoon").unwrap();
        config.set_str("user.email", "rocky@example.com").unwrap();
        config.set_bool("commit.gpgsign", false).unwrap();
        t
    }

    fn stage(t: &TempRepo, file: &str) {
        let mut index = t.repo.index().unwrap();
        index.add_path(Path::new(file)).unwrap();
        index.write().unwrap();
    }

    #[test]
    fn commits_what_is_staged() {
        let t = repo_with_identity();
        t.write("a.txt", "1\n");
        t.commit_all("first");
        t.write("a.txt", "2\n");
        t.write("b.txt", "not staged\n");
        stage(&t, "a.txt");

        let done = commit(t.path(), "Change a\n\nWith a body.\n", false).unwrap();
        let repo = Repository::open(&t.dir).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.id().to_string(), done.id);
        assert_eq!(head.message(), Some("Change a\n\nWith a body.\n"));
        assert_eq!(head.author().name(), Some("Rocky Raccoon"));
        assert_eq!(head.parent_count(), 1);
        // The unstaged file is still unstaged.
        let status = crate::working::working_status(t.path()).unwrap().unwrap();
        assert!(status.staged.is_empty());
        assert_eq!(status.unstaged[0].path, "b.txt");
    }

    #[test]
    fn amends_the_last_commit() {
        let t = repo_with_identity();
        t.write("a.txt", "1\n");
        let first = t.commit_all("first");
        t.write("a.txt", "2\n");
        stage(&t, "a.txt");
        commit(t.path(), "Second, with a typo", false).unwrap();
        assert_eq!(
            head_message(t.path()).unwrap().as_deref(),
            Some("Second, with a typo")
        );

        commit(t.path(), "Second, fixed", true).unwrap();
        let repo = Repository::open(&t.dir).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message(), Some("Second, fixed\n"));
        assert_eq!(head.parent_id(0).unwrap(), first, "amend keeps the parent");
    }

    #[test]
    fn reports_hook_failures_and_empty_commits() {
        let t = repo_with_identity();
        t.write("a.txt", "1\n");
        t.commit_all("first");

        let err = commit(t.path(), "Nothing staged", false).unwrap_err();
        assert!(matches!(err, Error::Command(_)), "{err}");

        assert!(
            commit(t.path(), "  \n", false).is_err(),
            "an empty message is refused"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let hook = t.dir.join(".git/hooks/pre-commit");
            std::fs::write(
                &hook,
                "#!/bin/sh\necho 'lint failed: trailing whitespace' >&2\nexit 1\n",
            )
            .unwrap();
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
            t.write("a.txt", "2\n");
            stage(&t, "a.txt");
            let err = commit(t.path(), "Blocked by the hook", false).unwrap_err();
            assert!(err.to_string().contains("lint failed"), "{err}");
            assert_eq!(head_message(t.path()).unwrap().as_deref(), Some("first"));
        }
    }

    #[test]
    fn knows_whether_head_was_pushed() {
        let t = repo_with_identity();
        t.write("a.txt", "1\n");
        let first = t.commit_all("first");
        let branch = t.repo.head().unwrap().shorthand().unwrap().to_string();
        assert!(!head_is_pushed(t.path()).unwrap(), "no upstream");

        t.repo
            .reference(
                &format!("refs/remotes/origin/{branch}"),
                first,
                true,
                "test",
            )
            .unwrap();
        t.repo
            .remote("origin", "https://example.com/r.git")
            .unwrap();
        let mut config = t.repo.config().unwrap();
        config
            .set_str(&format!("branch.{branch}.remote"), "origin")
            .unwrap();
        config
            .set_str(
                &format!("branch.{branch}.merge"),
                &format!("refs/heads/{branch}"),
            )
            .unwrap();
        assert!(head_is_pushed(t.path()).unwrap());

        t.write("a.txt", "2\n");
        t.commit_all("local only");
        assert!(!head_is_pushed(t.path()).unwrap());
    }
}
