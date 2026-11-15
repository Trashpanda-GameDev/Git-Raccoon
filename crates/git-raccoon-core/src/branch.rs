//! Branches and tags, through the user's own `git`, so its safety checks
//! (unmerged branches, local changes in the way of a checkout) apply.
//!
//! When Git refuses for one of those reasons, the error says so
//! ([`Error::Dirty`], [`Error::NotMerged`]), so the UI can offer a way on.

use git2::{Branch, Reference, Repository};

use crate::cli;
use crate::{Error, Result};

/// Whether `name` is a valid branch name.
pub fn is_valid_branch_name(name: &str) -> bool {
    !name.trim().is_empty() && Branch::name_is_valid(name).unwrap_or(false)
}

/// Whether `name` is a valid tag name.
pub fn is_valid_tag_name(name: &str) -> bool {
    !name.trim().is_empty() && Reference::is_valid_name(&format!("refs/tags/{name}"))
}

/// Runs git, turning its "your changes are in the way" refusal into
/// [`Error::Dirty`].
fn run_checkout(path: &str, args: &[&str]) -> Result<()> {
    match cli::run(path, args, None) {
        Ok(_) => Ok(()),
        Err(Error::Command(text))
            if text.contains("would be overwritten")
                || text.contains("commit your changes or stash them") =>
        {
            Err(Error::Dirty(text))
        }
        Err(e) => Err(e),
    }
}

/// Creates `name` at commit `start`, and checks it out if asked.
pub fn create(path: &str, name: &str, start: &str, checkout: bool) -> Result<()> {
    if !is_valid_branch_name(name) {
        return Err(Error::Unsupported(format!(
            "{name:?} isn't a valid branch name"
        )));
    }
    if checkout {
        run_checkout(path, &["switch", "--create", name, start])
    } else {
        cli::run(path, &["branch", name, start], None).map(drop)
    }
}

/// Checks out a local branch.
pub fn checkout(path: &str, name: &str) -> Result<()> {
    run_checkout(path, &["switch", name])
}

/// Checks out a remote branch such as "origin/feature": switches to the
/// local branch of the same name, creating it to track the remote one if
/// it doesn't exist yet.
pub fn checkout_remote(path: &str, remote_branch: &str) -> Result<()> {
    let repo = Repository::discover(path)?;
    let local = remote_branch
        .split_once('/')
        .map_or(remote_branch, |(_, rest)| rest);
    if repo.find_branch(local, git2::BranchType::Local).is_ok() {
        return checkout(path, local);
    }
    run_checkout(path, &["switch", "--track", remote_branch])
}

/// Checks out a commit without a branch ("detached HEAD").
pub fn checkout_detached(path: &str, id: &str) -> Result<()> {
    run_checkout(path, &["switch", "--detach", id])
}

pub fn rename(path: &str, old: &str, new: &str) -> Result<()> {
    if !is_valid_branch_name(new) {
        return Err(Error::Unsupported(format!(
            "{new:?} isn't a valid branch name"
        )));
    }
    cli::run(path, &["branch", "--move", old, new], None).map(drop)
}

/// The commit a local branch points at.
pub fn tip(path: &str, name: &str) -> Result<String> {
    let repo = Repository::discover(path)?;
    let branch = repo.find_branch(name, git2::BranchType::Local)?;
    Ok(branch
        .get()
        .target()
        .ok_or_else(|| Error::Unsupported(format!("{name} has no commit")))?
        .to_string())
}

/// Deletes a local branch. Without `force`, Git refuses if the branch has
/// commits not merged anywhere else ([`Error::NotMerged`]).
pub fn delete(path: &str, name: &str, force: bool) -> Result<()> {
    let flag = if force { "-D" } else { "-d" };
    match cli::run(path, &["branch", flag, name], None) {
        Ok(_) => Ok(()),
        Err(Error::Command(text)) if text.contains("not fully merged") => {
            Err(Error::NotMerged(text))
        }
        Err(e) => Err(e),
    }
}

/// Creates a tag at commit `target`: annotated if a message is given.
pub fn create_tag(path: &str, name: &str, target: &str, message: &str) -> Result<()> {
    if !is_valid_tag_name(name) {
        return Err(Error::Unsupported(format!(
            "{name:?} isn't a valid tag name"
        )));
    }
    let message = message.trim();
    if message.is_empty() {
        cli::run(path, &["tag", name, target], None).map(drop)
    } else {
        cli::run(
            path,
            &["tag", "--annotate", name, "--file=-", target],
            Some(message),
        )
        .map(drop)
    }
}

pub fn delete_tag(path: &str, name: &str) -> Result<()> {
    cli::run(path, &["tag", "--delete", name], None).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    fn repo() -> (TempRepo, git2::Oid, git2::Oid) {
        let t = TempRepo::new();
        let mut config = t.repo.config().unwrap();
        config.set_str("user.name", "Rocky Raccoon").unwrap();
        config.set_str("user.email", "rocky@example.com").unwrap();
        t.write("f.txt", "one\n");
        let first = t.commit_all("first");
        t.write("f.txt", "two\n");
        let second = t.commit_all("second");
        (t, first, second)
    }

    fn head(t: &TempRepo) -> (Option<String>, String) {
        let repo = Repository::open(&t.dir).unwrap();
        let h = repo.head().unwrap();
        let name = h.is_branch().then(|| h.shorthand().unwrap().to_string());
        (name, h.target().unwrap().to_string())
    }

    fn branches(t: &TempRepo) -> Vec<String> {
        crate::refs::load_refs(t.path())
            .unwrap()
            .local
            .into_iter()
            .map(|b| b.name)
            .collect()
    }

    fn main_name(t: &TempRepo) -> String {
        head(t).0.unwrap()
    }

    #[test]
    fn validates_names() {
        assert!(is_valid_branch_name("feature/new-thing"));
        assert!(!is_valid_branch_name("has space"));
        assert!(!is_valid_branch_name("double..dot"));
        assert!(!is_valid_branch_name(""));
        assert!(is_valid_tag_name("v1.2.3"));
        assert!(!is_valid_tag_name("bad~tag"));
    }

    #[test]
    fn creates_checks_out_renames_and_deletes() {
        let (t, first, second) = repo();
        let main = main_name(&t);
        create(t.path(), "old-work", &first.to_string(), false).unwrap();
        assert_eq!(
            head(&t),
            (Some(main.clone()), second.to_string()),
            "not checked out"
        );

        create(t.path(), "feature", &first.to_string(), true).unwrap();
        assert_eq!(head(&t), (Some("feature".into()), first.to_string()));
        assert_eq!(
            std::fs::read_to_string(t.dir.join("f.txt")).unwrap(),
            "one\n"
        );

        rename(t.path(), "feature", "feature/renamed").unwrap();
        assert_eq!(head(&t).0.as_deref(), Some("feature/renamed"));

        checkout(t.path(), &main).unwrap();
        assert_eq!(head(&t), (Some(main.clone()), second.to_string()));
        delete(t.path(), "feature/renamed", false).unwrap();
        assert!(!branches(&t).contains(&"feature/renamed".to_string()));

        assert!(create(t.path(), "bad name", &first.to_string(), false).is_err());
    }

    #[test]
    fn refuses_to_delete_unmerged_work_without_force() {
        let (t, _, _) = repo();
        let main = main_name(&t);
        create(t.path(), "experiment", "HEAD", true).unwrap();
        t.write("f.txt", "experimental\n");
        t.commit_all("only on experiment");
        let tip_id = tip(t.path(), "experiment").unwrap();
        checkout(t.path(), &main).unwrap();

        let err = delete(t.path(), "experiment", false).unwrap_err();
        assert!(matches!(err, Error::NotMerged(_)), "{err}");
        assert!(branches(&t).contains(&"experiment".to_string()));

        delete(t.path(), "experiment", true).unwrap();
        assert!(!branches(&t).contains(&"experiment".to_string()));
        // The commit is still there, and the branch can be recreated from its tip.
        create(t.path(), "experiment", &tip_id, false).unwrap();
        assert_eq!(tip(t.path(), "experiment").unwrap(), tip_id);
    }

    #[test]
    fn reports_local_changes_in_the_way() {
        let (t, first, _) = repo();
        create(t.path(), "old", &first.to_string(), false).unwrap();
        t.write("f.txt", "uncommitted\n");
        let err = checkout(t.path(), "old").unwrap_err();
        assert!(matches!(err, Error::Dirty(_)), "{err}");
        assert_eq!(
            std::fs::read_to_string(t.dir.join("f.txt")).unwrap(),
            "uncommitted\n",
            "nothing was overwritten"
        );
    }

    #[test]
    fn checks_out_remote_branches_and_commits() {
        let (t, first, second) = repo();
        t.repo
            .reference("refs/remotes/origin/feature", first, true, "test")
            .unwrap();
        t.repo
            .remote("origin", "https://example.com/r.git")
            .unwrap();

        checkout_remote(t.path(), "origin/feature").unwrap();
        assert_eq!(head(&t), (Some("feature".into()), first.to_string()));
        let refs = crate::refs::load_refs(t.path()).unwrap();
        let feature = refs.local.iter().find(|b| b.name == "feature").unwrap();
        assert_eq!(
            feature.upstream.as_ref().unwrap().name,
            "origin/feature",
            "it tracks the remote branch"
        );

        checkout_detached(t.path(), &second.to_string()).unwrap();
        assert_eq!(head(&t), (None, second.to_string()));
    }

    #[test]
    fn creates_and_deletes_tags() {
        let (t, first, _) = repo();
        create_tag(t.path(), "v1", &first.to_string(), "").unwrap();
        create_tag(t.path(), "v2", "HEAD", "Second release").unwrap();
        let tags = crate::refs::load_refs(t.path()).unwrap().tags;
        let summary: Vec<(String, bool)> =
            tags.iter().map(|t| (t.name.clone(), t.annotated)).collect();
        assert!(summary.contains(&("v1".into(), false)));
        assert!(summary.contains(&("v2".into(), true)));
        delete_tag(t.path(), "v1").unwrap();
        assert_eq!(crate::refs::load_refs(t.path()).unwrap().tags.len(), 1);
        assert!(create_tag(t.path(), "bad~tag", "HEAD", "").is_err());
    }
}
