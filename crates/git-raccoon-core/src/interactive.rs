//! Interactive rebase: reorder, reword, squash and drop the commits after
//! a base commit, by handing a prepared plan to the user's own
//! `git rebase -i`.

use std::path::PathBuf;

use git2::{Oid, Repository, Sort};

use crate::merge::in_progress;
use crate::{Error, Result, cli};

/// A commit that can be rewritten.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub id: String,
    pub summary: String,
    pub message: String,
}

/// What to do with a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Pick,
    /// Keep it with this new message.
    Reword(String),
    /// Combine it into the commit before it, keeping both messages.
    Squash,
    /// Combine it into the commit before it, keeping only that one's message.
    Fixup,
    Drop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub action: Action,
}

/// The commits after `base` up to HEAD, oldest first. Refuses if `base`
/// isn't behind HEAD, or a merge is among them.
pub fn commits(path: &str, base: &str) -> Result<Vec<Planned>> {
    let repo = Repository::discover(path)?;
    let head = repo.head()?;
    if !head.is_branch() {
        return Err(Error::Unsupported(
            "no branch is checked out (detached HEAD)".into(),
        ));
    }
    let head = head.peel_to_commit()?.id();
    let base = repo.revparse_single(base)?.peel_to_commit()?.id();
    if base == head {
        return Err(Error::Unsupported(
            "there are no commits after this one".into(),
        ));
    }
    if !repo.graph_descendant_of(head, base)? {
        return Err(Error::Unsupported(
            "this commit isn't on the checked-out branch".into(),
        ));
    }
    let mut walk = repo.revwalk()?;
    walk.push(head)?;
    walk.hide(base)?;
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::REVERSE)?;
    let mut out = Vec::new();
    for id in walk {
        let commit = repo.find_commit(id?)?;
        if commit.parent_count() > 1 {
            return Err(Error::Unsupported(
                "a merge is among these commits; interactive rebase can't rewrite merges here"
                    .into(),
            ));
        }
        out.push(Planned {
            id: commit.id().to_string(),
            summary: commit.summary().unwrap_or_default().to_string(),
            message: commit.message().unwrap_or_default().trim_end().to_string(),
        });
    }
    Ok(out)
}

/// Whether the checked-out branch's upstream already has commit `id` (so
/// rewriting it means force-pushing).
pub fn is_pushed(path: &str, id: &str) -> bool {
    let check = || -> Result<bool> {
        let repo = Repository::discover(path)?;
        let head = repo.head()?;
        let name = head.shorthand().unwrap_or_default();
        let branch = repo.find_branch(name, git2::BranchType::Local)?;
        let Some(up) = branch.upstream()?.get().target() else {
            return Ok(false);
        };
        let id = Oid::from_str(id)?;
        Ok(up == id || repo.graph_descendant_of(up, id)?)
    };
    check().unwrap_or(false)
}

/// Why a plan (oldest first) can't run, if it can't.
pub fn check(steps: &[Step]) -> Option<&'static str> {
    let first = steps.iter().find(|s| s.action != Action::Drop)?;
    if matches!(first.action, Action::Squash | Action::Fixup) {
        return Some(
            "The first (oldest) commit kept can't be squashed: there's nothing before it to combine with.",
        );
    }
    let empty_reword = steps
        .iter()
        .any(|s| matches!(&s.action, Action::Reword(m) if m.trim().is_empty()));
    if empty_reword {
        return Some("A reworded commit needs a message.");
    }
    None
}

/// Where this run's files go: the plan, and new messages.
fn scratch() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "git-raccoon-rebase-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    std::fs::create_dir_all(&dir)
        .map_err(|e| Error::Unsupported(format!("can't prepare the rebase: {e}")))?;
    Ok(dir)
}

/// A path for git's shell, which wants forward slashes, single-quoted.
fn quoted(path: &std::path::Path) -> String {
    format!(
        "'{}'",
        path.to_string_lossy()
            .replace('\\', "/")
            .replace('\'', "'\\''")
    )
}

/// Rewrites the commits after `base` following `steps` (oldest first, in
/// the new order). Uncommitted changes are put aside and restored. Stopping
/// on a conflict isn't an error: the rebase is then in progress.
pub fn run(path: &str, base: &str, steps: &[Step]) -> Result<()> {
    if let Some(problem) = check(steps) {
        return Err(Error::Unsupported(problem.into()));
    }
    // Every commit of the range must be in the plan, once.
    let mut planned: Vec<String> = commits(path, base)?.into_iter().map(|c| c.id).collect();
    let mut given: Vec<String> = steps.iter().map(|s| s.id.clone()).collect();
    planned.sort();
    given.sort();
    if planned != given {
        return Err(Error::Stale(
            "the branch changed since the plan was made".into(),
        ));
    }

    let dir = scratch()?;
    let mut todo = String::new();
    for (i, step) in steps.iter().enumerate() {
        let id = Oid::from_str(&step.id)?.to_string();
        match &step.action {
            Action::Pick => todo.push_str(&format!("pick {id}\n")),
            Action::Squash => todo.push_str(&format!("squash {id}\n")),
            Action::Fixup => todo.push_str(&format!("fixup {id}\n")),
            Action::Drop => todo.push_str(&format!("drop {id}\n")),
            Action::Reword(message) => {
                let file = dir.join(format!("message-{i}.txt"));
                std::fs::write(&file, message)
                    .map_err(|e| Error::Unsupported(format!("can't prepare the rebase: {e}")))?;
                todo.push_str(&format!("pick {id}\n"));
                todo.push_str(&format!(
                    "exec git commit --amend --quiet --cleanup=strip -F {}\n",
                    quoted(&file)
                ));
            }
        }
    }
    let plan = dir.join("todo");
    std::fs::write(&plan, todo)
        .map_err(|e| Error::Unsupported(format!("can't prepare the rebase: {e}")))?;
    let editor = format!("cp {}", quoted(&plan));
    let result = cli::run_env(
        path,
        &["rebase", "-i", "--autostash", base],
        None,
        &[("GIT_SEQUENCE_EDITOR", &editor)],
    );
    match result {
        Ok(_) => Ok(()),
        Err(_) if in_progress(path)?.is_some() => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::tests::{git, identity};
    use crate::testutil::TempRepo;

    fn log(t: &TempRepo) -> Vec<String> {
        let out = cli::run(t.path(), &["log", "--format=%B%x00"], None).unwrap();
        out.split('\0')
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .collect()
    }

    /// base, then a, b, c, d each adding a file.
    fn repo() -> (TempRepo, String, Vec<Planned>) {
        let t = TempRepo::new();
        identity(&t);
        t.write("base.txt", "base\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "base"]);
        let base = cli::run(t.path(), &["rev-parse", "HEAD"], None)
            .unwrap()
            .trim()
            .to_string();
        for n in ["a", "b", "c", "d"] {
            t.write(&format!("{n}.txt"), format!("{n}\n"));
            git(&t, &["add", "."]);
            git(&t, &["commit", "-q", "-m", n]);
        }
        let planned = commits(t.path(), &base).unwrap();
        (t, base, planned)
    }

    fn step(p: &Planned, action: Action) -> Step {
        Step {
            id: p.id.clone(),
            action,
        }
    }

    #[test]
    fn lists_the_commits_oldest_first() {
        let (t, base, planned) = repo();
        let summaries: Vec<&str> = planned.iter().map(|p| p.summary.as_str()).collect();
        assert_eq!(summaries, ["a", "b", "c", "d"]);
        let head = cli::run(t.path(), &["rev-parse", "HEAD"], None).unwrap();
        assert!(
            commits(t.path(), head.trim()).is_err(),
            "nothing after HEAD"
        );
        let _ = base;
    }

    #[test]
    fn reorders_rewords_squashes_and_drops() {
        let (t, base, p) = repo();
        let t_dirty = "uncommitted\n";
        t.write("base.txt", t_dirty);
        // New order (oldest first): b, a (reworded), d squashed into it, c dropped.
        let steps = vec![
            step(&p[1], Action::Pick),
            step(&p[0], Action::Reword("A, reworded\n\nwith a body".into())),
            step(&p[3], Action::Squash),
            step(&p[2], Action::Drop),
        ];
        run(t.path(), &base, &steps).unwrap();
        assert_eq!(in_progress(t.path()).unwrap(), None);
        let log = log(&t);
        assert_eq!(log.len(), 3, "{log:?}");
        assert!(log[0].starts_with("A, reworded\n\nwith a body"), "{log:?}");
        assert!(log[0].contains('d'), "squash keeps both messages: {log:?}");
        assert_eq!(log[1], "b");
        assert_eq!(log[2], "base");
        assert!(t.dir.join("d.txt").exists() && !t.dir.join("c.txt").exists());
        assert_eq!(
            std::fs::read_to_string(t.dir.join("base.txt")).unwrap(),
            t_dirty
        );
    }

    #[test]
    fn fixup_keeps_only_the_first_message() {
        let (t, base, p) = repo();
        let steps = vec![
            step(&p[0], Action::Pick),
            step(&p[1], Action::Fixup),
            step(&p[2], Action::Pick),
            step(&p[3], Action::Pick),
        ];
        run(t.path(), &base, &steps).unwrap();
        assert_eq!(log(&t), ["d", "c", "a", "base"]);
    }

    #[test]
    fn refuses_bad_plans() {
        let (t, base, p) = repo();
        let squash_first: Vec<Step> = p
            .iter()
            .enumerate()
            .map(|(i, c)| step(c, if i == 0 { Action::Squash } else { Action::Pick }))
            .collect();
        assert!(run(t.path(), &base, &squash_first).is_err());
        let empty_message = vec![
            step(&p[0], Action::Reword("  ".into())),
            step(&p[1], Action::Pick),
            step(&p[2], Action::Pick),
            step(&p[3], Action::Pick),
        ];
        assert!(check(&empty_message).is_some());
        let missing = vec![step(&p[0], Action::Pick)];
        let err = run(t.path(), &base, &missing).unwrap_err();
        assert!(matches!(err, Error::Stale(_)), "{err}");
        assert_eq!(log(&t).len(), 5, "nothing changed");
    }

    #[test]
    fn stops_on_conflicts() {
        let (t, base, p) = repo();
        // Make c and d both edit the same line, then swap them.
        git(&t, &["reset", "-q", "--hard", &p[1].id]);
        for (n, text) in [("c", "one\n"), ("d", "two\n")] {
            t.write("shared.txt", text);
            git(&t, &["add", "."]);
            git(&t, &["commit", "-q", "-m", n]);
        }
        let p = commits(t.path(), &base).unwrap();
        let steps = vec![
            step(&p[0], Action::Pick),
            step(&p[1], Action::Pick),
            step(&p[3], Action::Pick),
            step(&p[2], Action::Pick),
        ];
        run(t.path(), &base, &steps).unwrap();
        assert!(
            in_progress(t.path()).unwrap().is_some(),
            "stopped on the conflict"
        );
        crate::merge::abort(t.path()).unwrap();
    }

    #[test]
    fn refuses_ranges_with_merges() {
        let t = TempRepo::new();
        identity(&t);
        t.write("f.txt", "1\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "base"]);
        let base = cli::run(t.path(), &["rev-parse", "HEAD"], None).unwrap();
        git(&t, &["switch", "-q", "-c", "side"]);
        t.write("g.txt", "1\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "side"]);
        git(&t, &["switch", "-q", "-"]);
        git(&t, &["merge", "-q", "--no-ff", "--no-edit", "side"]);
        let err = commits(t.path(), base.trim()).unwrap_err();
        assert!(err.to_string().contains("merge"), "{err}");
    }
}
