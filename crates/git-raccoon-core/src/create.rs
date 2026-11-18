//! New repositories: cloning one, or starting one, through the user's own
//! `git` (so their templates, default branch name and credentials apply).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::sync::Progress;
use crate::{Error, Result, cli};

/// The folder name a clone of `url` gets by default: the last part of the
/// path, without ".git" ("https://host/owner/repo.git" gives "repo").
pub fn folder_name(url: &str) -> String {
    let url = url.trim().trim_end_matches(['/', '\\']);
    let last = url.rsplit(['/', '\\', ':']).next().unwrap_or(url);
    last.strip_suffix(".git").unwrap_or(last).to_string()
}

/// Whether a clone can go into `dest`: it doesn't exist, or is an empty folder.
pub fn can_clone_into(dest: &Path) -> bool {
    match std::fs::read_dir(dest) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => !dest.exists(),
    }
}

/// How much of a repository a clone takes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CloneOptions {
    /// Only the latest commit's history (`--depth 1`).
    pub shallow: bool,
    /// Check out only these folders (and the files at the top); `Some` of
    /// none: the top-level files only.
    pub sparse: Option<Vec<String>>,
}

/// Clones `url` into `dest` as `options` say, reporting progress; returns
/// `dest`. If it fails or is cancelled, a folder the clone created is
/// removed again.
pub fn clone(
    url: &str,
    dest: &Path,
    options: &CloneOptions,
    on: &mut dyn FnMut(Progress),
    cancel: &AtomicBool,
) -> Result<PathBuf> {
    let url = url.trim();
    if url.is_empty() {
        return Err(Error::Unsupported("there's no URL to clone".into()));
    }
    if !can_clone_into(dest) {
        return Err(Error::Unsupported(format!(
            "{} already exists and isn't empty",
            dest.display()
        )));
    }
    let (Some(parent), Some(name)) = (dest.parent(), dest.file_name()) else {
        return Err(Error::Unsupported(format!(
            "can't clone into {}",
            dest.display()
        )));
    };
    std::fs::create_dir_all(parent)
        .map_err(|e| Error::Unsupported(format!("can't create {}: {e}", parent.display())))?;
    let existed = dest.exists();
    let parent_str = parent.to_string_lossy();
    let name = name.to_string_lossy();
    let mut args = vec!["clone", "--progress"];
    if options.shallow {
        args.extend(["--depth", "1"]);
    }
    if options.sparse.is_some() {
        args.push("--sparse");
    }
    args.extend([url, &name]);
    let result =
        cli::run_with_progress(&parent_str, &args, &mut |l| on(Progress::parse(l)), cancel)
            .and_then(|_| match &options.sparse {
                Some(folders) if !folders.is_empty() => {
                    let mut set = vec!["sparse-checkout", "set"];
                    set.extend(folders.iter().map(String::as_str));
                    cli::run(&dest.to_string_lossy(), &set, None).map(drop)
                }
                _ => Ok(()),
            });
    match result {
        Ok(_) => Ok(dest.to_path_buf()),
        Err(e) => {
            if !existed {
                let _ = std::fs::remove_dir_all(dest);
            }
            Err(e)
        }
    }
}

/// Starts a repository in `dir` (created if needed), its first branch
/// named `branch` (else as the user's Git names it). A folder that is
/// already a repository is left as it is.
pub fn init(dir: &Path, branch: Option<&str>) -> Result<()> {
    std::fs::create_dir_all(dir)
        .map_err(|e| Error::Unsupported(format!("can't create {}: {e}", dir.display())))?;
    let path = dir.to_string_lossy();
    if git2::Repository::open(dir).is_ok() {
        return Ok(());
    }
    let mut args = vec!["init"];
    if let Some(b) = branch {
        args.extend(["-b", b]);
    }
    cli::run(&path, &args, None).map(drop)
}

/// What a new repository starts with.
#[derive(Debug, Default)]
pub struct Starter {
    /// Its first branch's name (else as it already is).
    pub branch: Option<String>,
    /// Files for its first commit: (name, contents). None: no commit.
    pub files: Vec<(String, String)>,
    /// Set Git LFS up in it.
    pub lfs: bool,
}

/// Sets up a repository just started or cloned empty: its first branch's
/// name, Git LFS, and a first commit of `starter.files`. Returns the branch
/// it committed on, if it did. A repository with commits keeps its branch.
pub fn start(dir: &Path, starter: &Starter) -> Result<Option<String>> {
    let path = dir.to_string_lossy();
    // No commits yet, on whichever branch (libgit2's `is_empty` only counts
    // its default name).
    let unborn = matches!(
        git2::Repository::open(dir)?.head(),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch
    );
    if unborn && let Some(b) = &starter.branch {
        cli::run(
            &path,
            &["symbolic-ref", "HEAD", &format!("refs/heads/{b}")],
            None,
        )?;
    }
    if starter.lfs {
        cli::run(&path, &["lfs", "install", "--local"], None)?;
    }
    if starter.files.is_empty() {
        return Ok(None);
    }
    for (name, contents) in &starter.files {
        std::fs::write(dir.join(name), contents)
            .map_err(|e| Error::Unsupported(format!("can't write {name}: {e}")))?;
    }
    let mut add = vec!["add", "--"];
    add.extend(starter.files.iter().map(|(name, _)| name.as_str()));
    cli::run(&path, &add, None)?;
    cli::run(&path, &["commit", "-q", "-m", "Initial commit"], None)?;
    let branch = cli::run(&path, &["symbolic-ref", "--short", "HEAD"], None)?;
    Ok(Some(branch.trim().to_string()))
}

/// The user's name in their Git settings, if they have one (for a licence's
/// copyright line).
pub fn user_name() -> Option<String> {
    let dir = std::env::temp_dir();
    cli::run(
        &dir.to_string_lossy(),
        &["config", "--global", "user.name"],
        None,
    )
    .ok()
    .map(|n| n.trim().to_string())
    .filter(|n| !n.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    #[test]
    fn names_the_folder_after_the_url() {
        assert_eq!(folder_name("https://github.com/owner/repo.git"), "repo");
        assert_eq!(folder_name("git@github.com:owner/repo.git"), "repo");
        assert_eq!(folder_name("https://example.com/repo/"), "repo");
        assert_eq!(folder_name("C:\\work\\repo.git"), "repo");
        assert_eq!(folder_name("/srv/git/project"), "project");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "git-raccoon-create-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn clones_and_refuses_folders_in_use() {
        let source = TempRepo::new();
        source.write("f.txt", "one\n");
        let head = source.commit_all("first");
        let base = scratch("clone");
        let dest = base.join("copy");
        let none = AtomicBool::new(false);

        let done = clone(
            source.path(),
            &dest,
            &CloneOptions::default(),
            &mut |_| {},
            &none,
        )
        .unwrap();
        assert_eq!(done, dest);
        let copy = git2::Repository::open(&dest).unwrap();
        assert_eq!(copy.head().unwrap().target().unwrap(), head);
        assert!(copy.find_remote("origin").is_ok());

        let err = clone(
            source.path(),
            &dest,
            &CloneOptions::default(),
            &mut |_| {},
            &none,
        )
        .unwrap_err();
        assert!(err.to_string().contains("isn't empty"), "{err}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleans_up_after_a_failed_or_cancelled_clone() {
        let base = scratch("fail");
        let dest = base.join("copy");
        let none = AtomicBool::new(false);
        let missing = base.join("no-such-repo");
        assert!(
            clone(
                &missing.to_string_lossy(),
                &dest,
                &CloneOptions::default(),
                &mut |_| {},
                &none
            )
            .is_err()
        );
        assert!(!dest.exists(), "the half-made folder is gone");

        // An empty folder the user chose is kept.
        std::fs::create_dir_all(&dest).unwrap();
        assert!(
            clone(
                &missing.to_string_lossy(),
                &dest,
                &CloneOptions::default(),
                &mut |_| {},
                &none
            )
            .is_err()
        );
        assert!(dest.exists());

        let cancelled = AtomicBool::new(true);
        let other = base.join("other");
        let err = clone(
            "https://example.com/r.git",
            &other,
            &CloneOptions::default(),
            &mut |_| {},
            &cancelled,
        )
        .unwrap_err();
        assert!(matches!(err, Error::Cancelled));
        assert!(!other.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn clones_shallow_and_sparse() {
        let source = TempRepo::new();
        source.write(
            "root.txt", "top
",
        );
        source.write(
            "a/one.txt",
            "a
",
        );
        source.write(
            "b/two.txt",
            "b
",
        );
        source.commit_all("first");
        source.write(
            "root.txt",
            "top again
",
        );
        source.commit_all("second");
        // Local clones ignore --depth unless the source is a file:// URL.
        let url = format!("file://{}", source.path().replace('\\', "/"));
        let base = scratch("partial");
        let none = AtomicBool::new(false);

        let shallow = base.join("shallow");
        let options = CloneOptions {
            shallow: true,
            sparse: None,
        };
        clone(&url, &shallow, &options, &mut |_| {}, &none).unwrap();
        let repo = git2::Repository::open(&shallow).unwrap();
        assert!(repo.is_shallow());

        let sparse = base.join("sparse");
        let options = CloneOptions {
            shallow: false,
            sparse: Some(vec!["a".into()]),
        };
        clone(&url, &sparse, &options, &mut |_| {}, &none).unwrap();
        assert!(sparse.join("root.txt").exists(), "the top-level files");
        assert!(sparse.join("a/one.txt").exists(), "the folder asked for");
        assert!(!sparse.join("b").exists(), "not the others");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn starts_new_repositories() {
        let base = scratch("init");
        let dir = base.join("fresh");
        init(&dir, None).unwrap();
        let repo = git2::Repository::open(&dir).unwrap();
        assert!(repo.is_empty().unwrap());
        // Again: left as it is.
        init(&dir, None).unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn starts_with_a_branch_and_first_files() {
        let base = scratch("start");
        let dir = base.join("fresh");
        init(&dir, Some("trunk")).unwrap();
        // An identity for the commit, whatever the machine has.
        for (k, v) in [("user.name", "Rocky"), ("user.email", "r@example.com")] {
            cli::run(&dir.to_string_lossy(), &["config", k, v], None).unwrap();
        }
        let starter = Starter {
            branch: Some("develop".into()),
            files: vec![
                (
                    ".gitignore".into(),
                    "target/
"
                    .into(),
                ),
                (
                    "LICENSE".into(),
                    "MIT
"
                    .into(),
                ),
            ],
            lfs: false,
        };
        assert_eq!(start(&dir, &starter).unwrap().as_deref(), Some("develop"));
        let repo = git2::Repository::open(&dir).unwrap();
        let head = repo.head().unwrap();
        assert_eq!(head.shorthand(), Some("develop"));
        let tree = head.peel_to_commit().unwrap().tree().unwrap();
        assert!(tree.get_name(".gitignore").is_some() && tree.get_name("LICENSE").is_some());
        // Nothing to commit: nothing committed.
        let other = base.join("plain");
        init(&other, Some("main")).unwrap();
        assert_eq!(start(&other, &Starter::default()).unwrap(), None);
        assert_eq!(
            git2::Repository::open(&other)
                .unwrap()
                .find_reference("HEAD")
                .unwrap()
                .symbolic_target(),
            Some("refs/heads/main")
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
