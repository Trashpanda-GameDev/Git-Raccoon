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
/// it doesn't exist yet, and brings it up to the remote one.
///
/// An existing local branch behind the remote one is fast-forwarded (to
/// the remote branch as last fetched: nothing is downloaded). One with
/// commits the remote branch lacks is left as it is, still checked out,
/// with [`Error::Diverged`]: then it's for merging or rebasing.
pub fn checkout_remote(path: &str, remote_branch: &str) -> Result<()> {
    let repo = Repository::discover(path)?;
    let local = remote_branch
        .split_once('/')
        .map_or(remote_branch, |(_, rest)| rest);
    if repo.find_branch(local, git2::BranchType::Local).is_err() {
        return run_checkout(path, &["switch", "--track", remote_branch]);
    }
    checkout(path, local)?;
    let ours = repo.refname_to_id(&format!("refs/heads/{local}"))?;
    let theirs = repo.revparse_single(remote_branch)?.peel_to_commit()?.id();
    if ours == theirs || repo.graph_descendant_of(ours, theirs)? {
        return Ok(()); // up to date, or ahead
    }
    if repo.graph_descendant_of(theirs, ours)? {
        return run_checkout(path, &["merge", "--ff-only", "--quiet", remote_branch]);
    }
    Err(Error::Diverged(format!(
        "{local} and {remote_branch} have both moved on"
    )))
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

/// Sets the remote branch `branch` pulls from and pushes to ("origin/main"),
/// or (`None`) removes it.
pub fn set_upstream(path: &str, branch: &str, upstream: Option<&str>) -> Result<()> {
    let set = upstream.map(|up| format!("--set-upstream-to={up}"));
    let flag = set.as_deref().unwrap_or("--unset-upstream");
    cli::run(path, &["branch", flag, branch], None).map(drop)
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

/// The ones of `names` that Git wouldn't delete without forcing: their
/// commits aren't in their upstream (or, without one, in HEAD).
pub fn unmerged(path: &str, names: &[String]) -> Result<Vec<String>> {
    let repo = Repository::discover(path)?;
    let head = repo.head().ok().and_then(|h| h.target());
    let mut out = Vec::new();
    for name in names {
        let branch = repo.find_branch(name, git2::BranchType::Local)?;
        let Some(tip) = branch.get().target() else {
            continue;
        };
        let into = branch
            .upstream()
            .ok()
            .and_then(|u| u.get().target())
            .or(head);
        let merged = into.is_some_and(|into| {
            into == tip || repo.graph_descendant_of(into, tip).unwrap_or(false)
        });
        if !merged {
            out.push(name.clone());
        }
    }
    Ok(out)
}

/// Whether branch `name`'s commits are all on some other branch or tag,
/// local or remote, besides `except` (its remote copies): so deleting it
/// and them together leaves nothing out of the graph.
pub fn kept_elsewhere(path: &str, name: &str, except: &[String]) -> Result<bool> {
    let repo = Repository::discover(path)?;
    let tip = repo
        .find_branch(name, git2::BranchType::Local)?
        .get()
        .target()
        .ok_or_else(|| Error::Command(format!("{name} points nowhere")))?;
    let skip = |r: &str| {
        r == format!("refs/heads/{name}") || except.iter().any(|e| r == format!("refs/remotes/{e}"))
    };
    for reference in repo.references()?.flatten() {
        let Some(full) = reference.name() else {
            continue;
        };
        if skip(full)
            || full.ends_with("/HEAD")
            || !(full.starts_with("refs/heads/")
                || full.starts_with("refs/remotes/")
                || full.starts_with("refs/tags/"))
        {
            continue;
        }
        let Ok(commit) = reference.peel_to_commit() else {
            continue;
        };
        let at = commit.id();
        if at == tip || repo.graph_descendant_of(at, tip).unwrap_or(false) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Makes `branch` pull from `refname` (such as "refs/pull/12/head") on
/// `remote`: a ref that isn't a branch there, which Git can follow but
/// not set as an upstream the usual way.
pub fn follow(path: &str, branch: &str, remote: &str, refname: &str) -> Result<()> {
    cli::run(
        path,
        &["config", &format!("branch.{branch}.remote"), remote],
        None,
    )?;
    cli::run(
        path,
        &["config", &format!("branch.{branch}.merge"), refname],
        None,
    )
    .map(drop)
}

/// What a branch has for another (a pull/merge request from it, before
/// it's opened).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Comparison {
    /// Its commits the target doesn't have, oldest first: (id, message).
    pub commits: Vec<(String, String)>,
    /// How many of the target's commits it doesn't have.
    pub behind: usize,
    /// Merging it into the target would conflict.
    pub conflicts: bool,
}

/// Compares `source` with `target` (each a branch, local or remote, or any
/// revision).
pub fn compare(path: &str, source: &str, target: &str) -> Result<Comparison> {
    let repo = Repository::discover(path)?;
    let commit = |name: &str| -> Result<git2::Commit<'_>> {
        Ok(repo.revparse_single(name)?.peel_to_commit()?)
    };
    let (ours, theirs) = (commit(source)?, commit(target)?);
    let mut walk = repo.revwalk()?;
    walk.push(ours.id())?;
    walk.hide(theirs.id())?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)?;
    let mut commits = Vec::new();
    for id in walk.take(500) {
        let c = repo.find_commit(id?)?;
        commits.push((
            c.id().to_string(),
            c.message().unwrap_or_default().to_string(),
        ));
    }
    let (_, behind) = repo.graph_ahead_behind(ours.id(), theirs.id())?;
    let conflicts = behind > 0
        && !commits.is_empty()
        && repo
            .merge_commits(&theirs, &ours, None)
            .map(|index| index.has_conflicts())
            .unwrap_or(false);
    Ok(Comparison {
        commits,
        behind,
        conflicts,
    })
}

/// Deletes several local branches: none of them, unless `force`, if any
/// has unmerged work (the error names them).
pub fn delete_many(path: &str, names: &[String], force: bool) -> Result<()> {
    if !force {
        let unmerged = unmerged(path, names)?;
        if !unmerged.is_empty() {
            return Err(Error::NotMerged(unmerged.join(", ")));
        }
    }
    names.iter().try_for_each(|name| delete(path, name, force))
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

    #[test]
    fn deletes_several_branches_or_none() {
        let (t, first, second) = repo();
        let at = |name: &str, id: git2::Oid| {
            t.repo
                .branch(name, &t.repo.find_commit(id).unwrap(), false)
                .unwrap();
        };
        at("old", first);
        at("same", second);
        // An unmerged one: a commit only it has.
        at("side", first);
        t.commit(Some("refs/heads/side"), "only on side", &[first]);
        let names = |v: &[&str]| v.iter().map(ToString::to_string).collect::<Vec<_>>();

        let all = names(&["old", "same", "side"]);
        assert_eq!(unmerged(t.path(), &all).unwrap(), ["side"]);
        assert!(matches!(
            delete_many(t.path(), &all, false),
            Err(Error::NotMerged(n)) if n == "side"
        ));
        assert!(branches(&t).contains(&"old".to_string()), "none deleted");

        delete_many(t.path(), &names(&["old", "same"]), false).unwrap();
        delete_many(t.path(), &names(&["side"]), true).unwrap();
        let left = branches(&t);
        assert!(
            !left
                .iter()
                .any(|b| ["old", "same", "side"].contains(&b.as_str())),
            "{left:?}"
        );
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
    fn checking_out_a_remote_branch_brings_the_local_one_up_to_it() {
        let (t, first, second) = repo();
        t.repo
            .remote("origin", "https://example.com/r.git")
            .unwrap();
        let remote = |id| {
            t.repo
                .reference("refs/remotes/origin/feature", id, true, "test")
                .unwrap();
        };

        create(t.path(), "base", &second.to_string(), false).unwrap();

        // Behind: fast-forwarded to the remote branch.
        create(t.path(), "feature", &first.to_string(), false).unwrap();
        remote(second);
        checkout_remote(t.path(), "origin/feature").unwrap();
        assert_eq!(head(&t), (Some("feature".into()), second.to_string()));
        assert_eq!(
            std::fs::read_to_string(t.dir.join("f.txt")).unwrap(),
            "two\n"
        );

        // Ahead: left where it is.
        remote(first);
        checkout(t.path(), "base").unwrap();
        checkout_remote(t.path(), "origin/feature").unwrap();
        assert_eq!(head(&t), (Some("feature".into()), second.to_string()));

        // Both moved on: checked out, unchanged, and reported.
        t.write("g.txt", "mine\n");
        let mine = t.commit_all("mine");
        remote(first);
        checkout(t.path(), "base").unwrap();
        let theirs = {
            create(t.path(), "theirs", &first.to_string(), true).unwrap();
            t.write("h.txt", "theirs\n");
            t.commit_all("theirs")
        };
        remote(theirs);
        let err = checkout_remote(t.path(), "origin/feature").unwrap_err();
        assert!(matches!(err, Error::Diverged(_)), "{err}");
        assert_eq!(head(&t), (Some("feature".into()), mine.to_string()));
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

    #[test]
    fn compares_a_branch_with_its_target() {
        let (t, first, second) = repo();
        t.repo
            .branch("topic", &t.repo.find_commit(first).unwrap(), false)
            .unwrap();
        // topic changes the same line main did: they conflict.
        t.write("f.txt", "three\n");
        let mut index = t.repo.index().unwrap();
        index.add_path(std::path::Path::new("f.txt")).unwrap();
        let tree = t.repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = t.repo.signature().unwrap();
        let parent = t.repo.find_commit(first).unwrap();
        t.repo
            .commit(
                Some("refs/heads/topic"),
                &sig,
                &sig,
                "Topic work\n\nWhy.",
                &tree,
                &[&parent],
            )
            .unwrap();
        let c = compare(t.path(), "topic", &second.to_string()).unwrap();
        assert_eq!(c.commits.len(), 1);
        assert_eq!(c.commits[0].1, "Topic work\n\nWhy.");
        assert_eq!(c.behind, 1);
        assert!(c.conflicts);
        let same = compare(t.path(), &second.to_string(), &first.to_string()).unwrap();
        assert_eq!(
            (same.commits.len(), same.behind, same.conflicts),
            (1, 0, false)
        );
    }

    #[test]
    fn follows_a_ref_that_isnt_a_branch() {
        let (t, _, _) = repo();
        cli::run(t.path(), &["branch", "pr/12"], None).unwrap();
        follow(t.path(), "pr/12", "origin", "refs/pull/12/head").unwrap();
        let config = |key: &str| cli::run(t.path(), &["config", key], None).unwrap();
        assert_eq!(config("branch.pr/12.remote").trim(), "origin");
        assert_eq!(config("branch.pr/12.merge").trim(), "refs/pull/12/head");
    }
}
