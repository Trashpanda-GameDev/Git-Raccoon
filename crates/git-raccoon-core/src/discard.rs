//! Discarding unstaged changes, without losing them for good: each file's
//! current version goes to the Recycle Bin (the system trash) first, then
//! the staged version (or, for untracked files, nothing) takes its place.
//!
//! Staged changes are not touched; unstage them first.
//!
//! A submodule isn't a file: discarding it puts it back on the commit this
//! repository records for it (its own commits stay in it), and only when it
//! has no work of its own, which is left for it to handle.

use std::path::Path;

use git2::build::CheckoutBuilder;
use git2::{Repository, SubmoduleIgnore, SubmoduleStatus};

use crate::details::FileStatus;
use crate::{Error, Result};
use crate::{cli, working};

/// Discards the unstaged changes to `files`, moving their current versions
/// to the system trash. Returns the files whose versions were moved there.
pub fn discard(path: &str, files: &[String]) -> Result<Vec<String>> {
    discard_with(path, files, |p| {
        trash::delete(p).map_err(|e| {
            Error::Command(format!(
                "could not move {} to the trash, so nothing was discarded for it: {e}",
                p.display()
            ))
        })
    })
}

/// [`discard`], but moving the discarded versions into `folder` instead of
/// the system trash (used by tests that drive the app).
pub fn discard_into(path: &str, files: &[String], folder: &Path) -> Result<Vec<String>> {
    std::fs::create_dir_all(folder).map_err(|e| Error::Command(e.to_string()))?;
    discard_with(path, files, |p| {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut target = folder.join(&name);
        let mut n = 1;
        while target.exists() {
            target = folder.join(format!("{name}.{n}"));
            n += 1;
        }
        std::fs::rename(p, &target).map_err(|e| Error::Command(e.to_string()))
    })
}

/// [`discard`], with `keep` doing the "move to the trash" (tests use a
/// folder instead). `keep` must move the file away; if it fails, that file
/// and the ones after it are left as they are.
pub fn discard_with(
    path: &str,
    files: &[String],
    mut keep: impl FnMut(&Path) -> Result<()>,
) -> Result<Vec<String>> {
    let repo = Repository::discover(path)?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| Error::Unsupported("a bare repository has no working tree".into()))?
        .to_path_buf();
    let status = working::working_status(path)?.unwrap_or_default();
    let mut kept = Vec::new();
    for file in files {
        let Some(change) = status.unstaged.iter().find(|f| &f.path == file) else {
            return Err(Error::Unsupported(format!(
                "{file} has no unstaged changes"
            )));
        };
        if change.status == FileStatus::Conflicted {
            return Err(Error::Unsupported(format!(
                "{file} has a merge conflict; resolve it first"
            )));
        }
        if let Some(name) = submodule_at(&repo, file) {
            let status = repo
                .submodule_status(&name, SubmoduleIgnore::None)
                .unwrap_or(SubmoduleStatus::empty());
            if status.intersects(
                SubmoduleStatus::WD_WD_MODIFIED
                    | SubmoduleStatus::WD_INDEX_MODIFIED
                    | SubmoduleStatus::WD_UNTRACKED,
            ) {
                return Err(Error::Unsupported(format!(
                    "{file} is a submodule with changes of its own; open it to commit or discard them"
                )));
            }
            // Back on the commit recorded here (as staged).
            cli::run(
                path,
                &["submodule", "update", "--checkout", "--", file],
                None,
            )?;
            continue;
        }
        let full = workdir.join(file);
        if full.symlink_metadata().is_ok() {
            keep(&full)?;
            kept.push(file.clone());
        }
        if change.status != FileStatus::Untracked {
            // Put the staged version back.
            repo.checkout_index(None, Some(CheckoutBuilder::new().force().path(file)))?;
        }
    }
    Ok(kept)
}

/// The name of the submodule at `file` (a path in the repository), if it
/// is one.
fn submodule_at(repo: &Repository, file: &str) -> Option<String> {
    let path = Path::new(file);
    repo.submodules()
        .ok()?
        .into_iter()
        .find(|s| s.path() == path)
        .and_then(|s| s.name().map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;
    use std::path::PathBuf;

    /// A stand-in for the Recycle Bin: moves files into a folder.
    struct Bin {
        dir: PathBuf,
    }

    impl Bin {
        fn new(t: &TempRepo) -> Self {
            let dir = t.dir.with_extension("bin");
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Bin { dir }
        }

        fn keep(&self) -> impl FnMut(&Path) -> Result<()> + '_ {
            |p: &Path| {
                let name = p.file_name().unwrap();
                std::fs::rename(p, self.dir.join(name)).map_err(|e| Error::Command(e.to_string()))
            }
        }

        fn read(&self, name: &str) -> String {
            std::fs::read_to_string(self.dir.join(name)).unwrap()
        }
    }

    impl Drop for Bin {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn read(t: &TempRepo, file: &str) -> Option<String> {
        std::fs::read_to_string(t.dir.join(file)).ok()
    }

    #[test]
    fn restores_files_and_keeps_the_discarded_versions() {
        let t = TempRepo::new();
        t.write("edited.txt", "original\n");
        t.write("gone.txt", "still wanted\n");
        t.commit_all("first");
        t.write("edited.txt", "my work in progress\n");
        t.remove("gone.txt");
        t.write("new/scratch.txt", "scratch\n");
        let bin = Bin::new(&t);

        let files: Vec<String> = ["edited.txt", "gone.txt", "new/scratch.txt"]
            .map(String::from)
            .to_vec();
        let kept = discard_with(t.path(), &files, bin.keep()).unwrap();

        assert_eq!(
            kept,
            ["edited.txt", "new/scratch.txt"],
            "a deleted file has nothing to keep"
        );
        assert_eq!(read(&t, "edited.txt").as_deref(), Some("original\n"));
        assert_eq!(read(&t, "gone.txt").as_deref(), Some("still wanted\n"));
        assert_eq!(read(&t, "new/scratch.txt"), None);
        assert_eq!(bin.read("edited.txt"), "my work in progress\n");
        assert_eq!(bin.read("scratch.txt"), "scratch\n");
        assert!(
            working::working_status(t.path())
                .unwrap()
                .unwrap()
                .is_clean()
        );
    }

    #[test]
    fn puts_a_submodule_back_on_its_commit_and_leaves_its_work() {
        use crate::merge::tests::{git, identity};
        let repo = || {
            let t = TempRepo::new();
            identity(&t);
            t.write("f.txt", "one\n");
            git(&t, &["add", "."]);
            git(&t, &["commit", "-q", "-m", "first"]);
            t
        };
        let (lib, app) = (repo(), repo());
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
        let sub = app.dir.join("vendor/lib");
        let head = |what: &str| {
            cli::run(&sub.to_string_lossy(), &["rev-parse", what], None)
                .unwrap()
                .trim()
                .to_string()
        };
        let recorded = head("HEAD");
        // A commit in the submodule: it's on another commit than recorded.
        std::fs::write(sub.join("f.txt"), "two\n").unwrap();
        cli::run(
            &sub.to_string_lossy(),
            &["commit", "-q", "-am", "two"],
            None,
        )
        .unwrap();
        let newer = head("HEAD");
        let bin = Bin::new(&app);
        let files = vec!["vendor/lib".to_string()];

        let kept = discard_with(app.path(), &files, bin.keep()).unwrap();
        assert!(kept.is_empty(), "nothing went to the trash");
        assert_eq!(head("HEAD"), recorded, "back on the recorded commit");
        // (Checked out with the platform's line endings.)
        let text = std::fs::read_to_string(sub.join("f.txt")).unwrap();
        assert_eq!(text.replace("\r\n", "\n"), "one\n");
        // Its own commit is still there.
        assert_eq!(head(&format!("{newer}^{{commit}}")), newer);
        assert!(
            working::working_status(app.path())
                .unwrap()
                .unwrap()
                .is_clean()
        );

        // Work of its own: left alone, and said why.
        std::fs::write(sub.join("f.txt"), "unsaved work\n").unwrap();
        let err = discard_with(app.path(), &files, bin.keep())
            .unwrap_err()
            .to_string();
        assert!(err.contains("changes of its own"), "{err}");
        assert_eq!(
            std::fs::read_to_string(sub.join("f.txt")).unwrap(),
            "unsaved work\n"
        );
    }

    #[test]
    fn keeps_staged_changes() {
        let t = TempRepo::new();
        t.write("f.txt", "one\n");
        t.commit_all("first");
        t.write("f.txt", "two\n");
        let mut index = t.repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        index.write().unwrap();
        t.write("f.txt", "three\n");
        let bin = Bin::new(&t);

        discard_with(t.path(), &["f.txt".into()], bin.keep()).unwrap();
        assert_eq!(
            read(&t, "f.txt").as_deref(),
            Some("two\n"),
            "back to the staged version"
        );
        let s = working::working_status(t.path()).unwrap().unwrap();
        assert_eq!(s.staged.len(), 1);
        assert!(s.unstaged.is_empty());
    }

    #[test]
    fn changes_nothing_when_the_file_cannot_be_kept() {
        let t = TempRepo::new();
        t.write("f.txt", "one\n");
        t.commit_all("first");
        t.write("f.txt", "precious\n");
        let err = discard_with(t.path(), &["f.txt".into()], |_| {
            Err(Error::Command("no trash here".into()))
        })
        .unwrap_err();
        assert!(err.to_string().contains("no trash here"));
        assert_eq!(read(&t, "f.txt").as_deref(), Some("precious\n"));
    }

    #[test]
    fn refuses_files_without_unstaged_changes() {
        let t = TempRepo::new();
        t.write("f.txt", "one\n");
        t.commit_all("first");
        let bin = Bin::new(&t);
        assert!(discard_with(t.path(), &["f.txt".into()], bin.keep()).is_err());
    }
}
