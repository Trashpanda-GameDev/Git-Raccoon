//! Submodules, worktrees and Git LFS: what a repository holds besides its
//! own history.

use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

use git2::{Repository, SubmoduleIgnore, SubmoduleStatus};

use crate::details::{FileDiff, LineKind};
use crate::sync::Progress;
use crate::{Error, Result, cli};

/// Where a submodule is at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmoduleState {
    UpToDate,
    /// Not cloned yet.
    Uninitialized,
    /// A different commit is checked out than the one recorded.
    OtherCommit,
    /// It has uncommitted changes of its own.
    Changed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Submodule {
    pub name: String,
    /// Relative to the repository root.
    pub path: String,
    pub url: Option<String>,
    pub state: SubmoduleState,
}

pub fn submodules(path: &str) -> Result<Vec<Submodule>> {
    let repo = Repository::discover(path)?;
    let mut out = Vec::new();
    for sm in repo.submodules()? {
        let name = sm.name().unwrap_or_default().to_string();
        let status = repo
            .submodule_status(&name, SubmoduleIgnore::None)
            .unwrap_or(SubmoduleStatus::empty());
        let state =
            if status.contains(SubmoduleStatus::WD_UNINITIALIZED) || sm.workdir_id().is_none() {
                SubmoduleState::Uninitialized
            } else if status.contains(SubmoduleStatus::WD_MODIFIED) {
                SubmoduleState::OtherCommit
            } else if status.intersects(
                SubmoduleStatus::WD_WD_MODIFIED
                    | SubmoduleStatus::WD_INDEX_MODIFIED
                    | SubmoduleStatus::WD_UNTRACKED,
            ) {
                SubmoduleState::Changed
            } else {
                SubmoduleState::UpToDate
            };
        out.push(Submodule {
            name,
            path: sm.path().to_string_lossy().into_owned(),
            url: sm.url().map(str::to_string),
            state,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Clones (if needed) and checks out the recorded commit of one submodule,
/// and of the submodules inside it.
pub fn update_submodule(
    path: &str,
    sub_path: &str,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    cli::run_with_progress(
        path,
        &[
            "submodule",
            "update",
            "--init",
            "--recursive",
            "--progress",
            "--",
            sub_path,
        ],
        &mut |l| on(Progress::parse(l)),
        cancel,
    )
    .map(drop)
}

/// Adds the repository at `url` as a submodule in `folder` (relative to the
/// repository; empty: a folder named after it), cloning it there. The new
/// submodule and `.gitmodules` are staged, ready to commit.
pub fn add_submodule(
    path: &str,
    url: &str,
    folder: &str,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<()> {
    // Git won't clone a submodule from a folder on this computer unless
    // allowed (against repositories that bring such submodules along);
    // here it's the one asked for.
    let local = (!url.contains("://") && !url.contains('@')) || url.starts_with("file://");
    let mut args = Vec::new();
    if local {
        args.extend(["-c", "protocol.file.allow=always"]);
    }
    args.extend(["submodule", "add", "--progress", "--", url]);
    if !folder.trim().is_empty() {
        args.push(folder.trim());
    }
    cli::run_with_progress(path, &args, &mut |l| on(Progress::parse(l)), cancel).map(drop)
}

/// A working tree of the repository.
#[derive(Debug, Clone, PartialEq)]
pub struct Worktree {
    pub path: String,
    /// The checked-out branch; `None` when detached (or bare).
    pub branch: Option<String>,
    pub head: Option<String>,
    /// The repository's main working tree (it can't be removed).
    pub main: bool,
    /// This repository is this worktree.
    pub current: bool,
    pub locked: bool,
    /// Its folder is gone; `git worktree prune` would forget it.
    pub missing: bool,
}

fn same_dir(a: &str, b: &str) -> bool {
    let canon = |p: &str| std::fs::canonicalize(p).ok();
    match (canon(a), canon(b)) {
        (Some(a), Some(b)) => a == b,
        _ => a.trim_end_matches(['/', '\\']) == b.trim_end_matches(['/', '\\']),
    }
}

pub fn worktrees(path: &str) -> Result<Vec<Worktree>> {
    let repo = Repository::discover(path)?;
    let here = repo
        .workdir()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let out = cli::run(path, &["worktree", "list", "--porcelain"], None)?;
    let mut list = Vec::new();
    for (i, block) in out.split("\n\n").enumerate() {
        let mut wt = Worktree {
            path: String::new(),
            branch: None,
            head: None,
            main: i == 0,
            current: false,
            locked: false,
            missing: false,
        };
        for line in block.lines() {
            let (key, value) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "worktree" => wt.path = value.to_string(),
                "HEAD" => wt.head = Some(value.to_string()),
                "branch" => wt.branch = Some(value.trim_start_matches("refs/heads/").to_string()),
                "locked" => wt.locked = true,
                "prunable" => wt.missing = true,
                _ => {}
            }
        }
        if wt.path.is_empty() {
            continue;
        }
        wt.current = !here.is_empty() && same_dir(&wt.path, &here);
        list.push(wt);
    }
    Ok(list)
}

/// Adds a worktree at `folder`: checking out `branch`, or creating it
/// there from `start` if `new` is set.
pub fn add_worktree(path: &str, folder: &str, branch: &str, new: bool, start: &str) -> Result<()> {
    if folder.trim().is_empty() || branch.trim().is_empty() {
        return Err(Error::Unsupported(
            "a worktree needs a folder and a branch".into(),
        ));
    }
    if new {
        cli::run(
            path,
            &["worktree", "add", "-b", branch, folder, start],
            None,
        )
        .map(drop)
    } else {
        cli::run(path, &["worktree", "add", folder, branch], None).map(drop)
    }
}

/// Removes a worktree. Without `force`, Git refuses if it has uncommitted
/// changes ([`Error::Dirty`]). Its branch stays.
pub fn remove_worktree(path: &str, folder: &str, force: bool) -> Result<()> {
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(folder);
    match cli::run(path, &args, None) {
        Err(Error::Command(text))
            if text.contains("modified or untracked files") || text.contains("use --force") =>
        {
            Err(Error::Dirty(text))
        }
        other => other.map(drop),
    }
}

/// Whether the repository stores files with Git LFS.
pub fn uses_lfs(path: &str) -> bool {
    let Ok(repo) = Repository::discover(path) else {
        return false;
    };
    if repo.path().join("lfs").is_dir() {
        return true;
    }
    repo.workdir()
        .and_then(|w| std::fs::read_to_string(w.join(".gitattributes")).ok())
        .is_some_and(|a| a.contains("filter=lfs"))
}

/// Whether git-lfs is installed (asked once).
pub fn lfs_installed() -> bool {
    static INSTALLED: OnceLock<bool> = OnceLock::new();
    *INSTALLED.get_or_init(|| {
        let dir = std::env::temp_dir();
        cli::run(&dir.to_string_lossy(), &["lfs", "version"], None).is_ok()
    })
}

/// A Git LFS pointer: what the repository stores instead of a large file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pointer {
    pub oid: String,
    pub size: u64,
}

/// Reads an LFS pointer file's text.
pub fn pointer(text: &str) -> Option<Pointer> {
    let mut lines = text.lines();
    if !lines
        .next()?
        .starts_with("version https://git-lfs.github.com/spec/")
    {
        return None;
    }
    let (mut oid, mut size) = (None, None);
    for line in lines {
        if let Some(v) = line.strip_prefix("oid ") {
            oid = Some(v.trim_start_matches("sha256:").to_string());
        } else if let Some(v) = line.strip_prefix("size ") {
            size = v.trim().parse().ok();
        }
    }
    Some(Pointer {
        oid: oid?,
        size: size?,
    })
}

/// If a diff is of LFS pointers, the pointers before and after.
pub fn pointer_change(diff: &FileDiff) -> Option<(Option<Pointer>, Option<Pointer>)> {
    let (mut old, mut new) = (String::new(), String::new());
    for hunk in &diff.hunks {
        for line in &hunk.lines {
            let text = format!("{}\n", line.content.trim_end_matches(['\n', '\r']));
            match line.kind {
                LineKind::Delete => old.push_str(&text),
                LineKind::Add => new.push_str(&text),
                LineKind::Context => {
                    old.push_str(&text);
                    new.push_str(&text);
                }
                _ => {}
            }
        }
    }
    let (before, after) = (pointer(&old), pointer(&new));
    (before.is_some() || after.is_some()).then_some((before, after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::tests::{git, identity};
    use crate::testutil::TempRepo;

    fn with_identity() -> TempRepo {
        let t = TempRepo::new();
        identity(&t);
        t.write("f.txt", "one\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "first"]);
        t
    }

    #[test]
    fn adds_a_submodule() {
        let lib = with_identity();
        let app = with_identity();
        let cancel = AtomicBool::new(false);
        add_submodule(app.path(), lib.path(), "vendor/lib", &mut |_| {}, &cancel).unwrap();
        let subs = submodules(app.path()).unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].path, "vendor/lib");
        assert_eq!(subs[0].state, SubmoduleState::UpToDate);
        assert!(
            std::path::Path::new(app.path())
                .join("vendor/lib/f.txt")
                .exists()
        );
        // Staged, with .gitmodules, ready to commit.
        let staged = cli::run(app.path(), &["diff", "--cached", "--name-only"], None).unwrap();
        assert!(
            staged.contains(".gitmodules") && staged.contains("vendor/lib"),
            "{staged}"
        );
        // A bad address says why.
        let err = add_submodule(app.path(), "/no/such/repo", "", &mut |_| {}, &cancel)
            .unwrap_err()
            .to_string();
        assert!(!err.is_empty());
    }

    #[test]
    fn lists_and_updates_submodules() {
        let lib = with_identity();
        let app = with_identity();
        git(
            &app,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                lib.path(),
                "vendor/lib",
            ],
        );
        git(&app, &["commit", "-q", "-m", "Add lib"]);
        let subs = submodules(app.path()).unwrap();
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].path, "vendor/lib");
        assert_eq!(subs[0].state, SubmoduleState::UpToDate);

        std::fs::write(app.dir.join("vendor/lib/f.txt"), "edited\n").unwrap();
        assert_eq!(
            submodules(app.path()).unwrap()[0].state,
            SubmoduleState::Changed
        );

        git(&app, &["-C", "vendor/lib", "checkout", "-q", "--", "f.txt"]);

        // Another commit checked out in it: updating puts the recorded one back.
        git(&lib, &["commit", "-q", "--allow-empty", "-m", "second"]);
        git(&app, &["-C", "vendor/lib", "fetch", "-q"]);
        git(&app, &["-C", "vendor/lib", "checkout", "-q", "FETCH_HEAD"]);
        assert_eq!(
            submodules(app.path()).unwrap()[0].state,
            SubmoduleState::OtherCommit
        );
        update_submodule(
            app.path(),
            "vendor/lib",
            &mut |_| {},
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(
            submodules(app.path()).unwrap()[0].state,
            SubmoduleState::UpToDate
        );

        // A fresh clone has it uninitialised until updated. (Cloning from a
        // local folder needs permission since Git 2.38; real ones are remote.)
        let copy = std::env::temp_dir().join(format!("git-raccoon-sub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&copy);
        let copy_path = copy.to_string_lossy().into_owned();
        cli::run(
            &std::env::temp_dir().to_string_lossy(),
            &["clone", "-q", app.path(), &copy_path],
            None,
        )
        .unwrap();
        assert_eq!(
            submodules(&copy_path).unwrap()[0].state,
            SubmoduleState::Uninitialized
        );
        cli::run(
            &copy_path,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
                "-q",
            ],
            None,
        )
        .unwrap();
        assert_eq!(
            submodules(&copy_path).unwrap()[0].state,
            SubmoduleState::UpToDate
        );
        let _ = std::fs::remove_dir_all(&copy);
    }

    #[test]
    fn adds_lists_and_removes_worktrees() {
        let t = with_identity();
        let folder = std::env::temp_dir().join(format!("git-raccoon-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let folder_s = folder.to_string_lossy().into_owned();
        add_worktree(t.path(), &folder_s, "side", true, "HEAD").unwrap();
        let list = worktrees(t.path()).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list[0].main && list[0].current);
        assert_eq!(list[1].branch.as_deref(), Some("side"));
        assert!(!list[1].current);
        // From inside the new one, it's the current one.
        assert!(worktrees(&folder_s).unwrap()[1].current);

        std::fs::write(folder.join("f.txt"), "dirty\n").unwrap();
        let err = remove_worktree(t.path(), &folder_s, false).unwrap_err();
        assert!(matches!(err, Error::Dirty(_)), "{err}");
        remove_worktree(t.path(), &folder_s, true).unwrap();
        assert_eq!(worktrees(t.path()).unwrap().len(), 1);
        assert!(!folder.exists());
    }

    #[test]
    fn reads_lfs_pointers() {
        let text = "version https://git-lfs.github.com/spec/v1\noid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393\nsize 12345\n";
        let p = pointer(text).unwrap();
        assert_eq!(p.size, 12345);
        assert!(p.oid.starts_with("4d7a2146"));
        assert!(pointer("just text\n").is_none());

        let t = with_identity();
        assert!(!uses_lfs(t.path()));
        t.write(
            ".gitattributes",
            "*.psd filter=lfs diff=lfs merge=lfs -text\n",
        );
        assert!(uses_lfs(t.path()));
    }
}
