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

/// Clones `url` into `dest`, reporting progress; returns `dest`. If it
/// fails or is cancelled, a folder the clone created is removed again.
pub fn clone(
    url: &str,
    dest: &Path,
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
    let result = cli::run_with_progress(
        &parent_str,
        &["clone", "--progress", url, &name],
        &mut |l| on(Progress::parse(l)),
        cancel,
    );
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

/// Starts a repository in `dir` (created if needed). A folder that is
/// already a repository is left as it is.
pub fn init(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .map_err(|e| Error::Unsupported(format!("can't create {}: {e}", dir.display())))?;
    let path = dir.to_string_lossy();
    if git2::Repository::open(dir).is_ok() {
        return Ok(());
    }
    cli::run(&path, &["init"], None).map(drop)
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

        let done = clone(source.path(), &dest, &mut |_| {}, &none).unwrap();
        assert_eq!(done, dest);
        let copy = git2::Repository::open(&dest).unwrap();
        assert_eq!(copy.head().unwrap().target().unwrap(), head);
        assert!(copy.find_remote("origin").is_ok());

        let err = clone(source.path(), &dest, &mut |_| {}, &none).unwrap_err();
        assert!(err.to_string().contains("isn't empty"), "{err}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleans_up_after_a_failed_or_cancelled_clone() {
        let base = scratch("fail");
        let dest = base.join("copy");
        let none = AtomicBool::new(false);
        let missing = base.join("no-such-repo");
        assert!(clone(&missing.to_string_lossy(), &dest, &mut |_| {}, &none).is_err());
        assert!(!dest.exists(), "the half-made folder is gone");

        // An empty folder the user chose is kept.
        std::fs::create_dir_all(&dest).unwrap();
        assert!(clone(&missing.to_string_lossy(), &dest, &mut |_| {}, &none).is_err());
        assert!(dest.exists());

        let cancelled = AtomicBool::new(true);
        let other = base.join("other");
        let err = clone("https://example.com/r.git", &other, &mut |_| {}, &cancelled).unwrap_err();
        assert!(matches!(err, Error::Cancelled));
        assert!(!other.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn starts_new_repositories() {
        let base = scratch("init");
        let dir = base.join("fresh");
        init(&dir).unwrap();
        let repo = git2::Repository::open(&dir).unwrap();
        assert!(repo.is_empty().unwrap());
        // Again: left as it is.
        init(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }
}
