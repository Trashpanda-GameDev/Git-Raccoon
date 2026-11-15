//! Adding, editing and removing remotes, through the user's own `git`.

use crate::cli;
use crate::{Error, Result};

/// Whether `name` can name a remote.
pub fn is_valid_name(name: &str) -> bool {
    !name.trim().is_empty() && git2::Remote::is_valid_name(name)
}

fn check_name(name: &str) -> Result<()> {
    if is_valid_name(name) {
        Ok(())
    } else {
        Err(Error::Unsupported(format!(
            "{name:?} isn't a valid remote name"
        )))
    }
}

fn check_url(url: &str) -> Result<()> {
    if url.trim().is_empty() {
        Err(Error::Unsupported("a remote needs a URL".into()))
    } else {
        Ok(())
    }
}

pub fn add(path: &str, name: &str, url: &str) -> Result<()> {
    check_name(name)?;
    check_url(url)?;
    cli::run(path, &["remote", "add", name, url.trim()], None).map(drop)
}

/// Renames a remote; its remote-tracking branches and the branches that
/// track them follow.
pub fn rename(path: &str, old: &str, new: &str) -> Result<()> {
    check_name(new)?;
    cli::run(path, &["remote", "rename", old, new], None).map(drop)
}

pub fn set_url(path: &str, name: &str, url: &str) -> Result<()> {
    check_url(url)?;
    cli::run(path, &["remote", "set-url", name, url.trim()], None).map(drop)
}

/// Removes a remote, with its remote-tracking branches; local branches
/// that tracked it no longer have an upstream.
pub fn remove(path: &str, name: &str) -> Result<()> {
    cli::run(path, &["remote", "remove", name], None).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refs::load_refs;
    use crate::testutil::TempRepo;

    fn remotes(t: &TempRepo) -> Vec<(String, Option<String>, usize)> {
        load_refs(t.path())
            .unwrap()
            .remotes
            .into_iter()
            .map(|r| (r.name, r.url, r.branches.len()))
            .collect()
    }

    #[test]
    fn adds_renames_changes_and_removes_remotes() {
        let t = TempRepo::new();
        t.write("f.txt", "one\n");
        let head = t.commit_all("first");
        add(t.path(), "upstream", "https://example.com/a.git").unwrap();
        t.repo
            .reference("refs/remotes/upstream/main", head, true, "test")
            .unwrap();
        assert_eq!(
            remotes(&t),
            [(
                "upstream".into(),
                Some("https://example.com/a.git".into()),
                1
            )]
        );

        rename(t.path(), "upstream", "fork").unwrap();
        set_url(t.path(), "fork", " git@example.com:me/a.git ").unwrap();
        assert_eq!(
            remotes(&t),
            [("fork".into(), Some("git@example.com:me/a.git".into()), 1)],
            "branches follow the rename; the URL is trimmed"
        );

        remove(t.path(), "fork").unwrap();
        assert!(remotes(&t).is_empty());
    }

    #[test]
    fn refuses_bad_names_and_urls_and_duplicates() {
        let t = TempRepo::new();
        assert!(is_valid_name("origin") && is_valid_name("my-fork"));
        assert!(!is_valid_name("has space") && !is_valid_name(""));
        assert!(add(t.path(), "bad name", "https://example.com/a.git").is_err());
        assert!(add(t.path(), "origin", "  ").is_err());
        add(t.path(), "origin", "https://example.com/a.git").unwrap();
        let err = add(t.path(), "origin", "https://example.com/b.git").unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
    }
}
