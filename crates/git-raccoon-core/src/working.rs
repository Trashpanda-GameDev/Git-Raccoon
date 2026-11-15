//! Uncommitted changes: what is staged (HEAD → index) and what is not
//! (index → working tree, including untracked files), like `git status`.
//!
//! Both lists come from the same diffs used for each file's diff, so the
//! list and the diff always agree.

use std::path::Path;

use git2::{Diff, DiffFindOptions, DiffOptions, Repository};

use crate::Result;
use crate::details::{self, BlobData, FileBlobs, FileDiff, FileStatus, MAX_BLOB_BYTES};

/// Which half of the working changes a file is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// In the index, ready to commit.
    Staged,
    /// In the working tree only.
    Unstaged,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkingFile {
    pub path: String,
    /// Set for renames.
    pub old_path: Option<String>,
    pub status: FileStatus,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkingStatus {
    pub staged: Vec<WorkingFile>,
    pub unstaged: Vec<WorkingFile>,
    /// Files with unresolved conflicts (listed here only, not as staged or
    /// unstaged).
    pub conflicted: Vec<WorkingFile>,
    /// A merge, rebase, cherry-pick or revert waiting to be finished.
    pub in_progress: Option<crate::merge::State>,
}

impl WorkingStatus {
    /// No changes, no conflicts and nothing in progress.
    pub fn is_clean(&self) -> bool {
        self.staged.is_empty()
            && self.unstaged.is_empty()
            && self.conflicted.is_empty()
            && self.in_progress.is_none()
    }

    pub fn files(&self, side: Side) -> &[WorkingFile] {
        match side {
            Side::Staged => &self.staged,
            Side::Unstaged => &self.unstaged,
        }
    }
}

fn side_diff<'r>(repo: &'r Repository, side: Side) -> Result<Diff<'r>> {
    let mut opts = DiffOptions::new();
    match side {
        Side::Staged => {
            // An unborn branch has no HEAD tree: everything staged is new.
            let head = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            let mut diff = repo.diff_tree_to_index(head.as_ref(), None, Some(&mut opts))?;
            diff.find_similar(Some(DiffFindOptions::new().renames(true)))?;
            Ok(diff)
        }
        Side::Unstaged => {
            opts.include_untracked(true)
                .recurse_untracked_dirs(true)
                .show_untracked_content(true);
            Ok(repo.diff_index_to_workdir(None, Some(&mut opts))?)
        }
    }
}

fn files_of(diff: &Diff) -> Vec<WorkingFile> {
    diff.deltas()
        .map(|d| {
            let (path, old_path) = details::paths_of(&d);
            WorkingFile {
                path,
                old_path,
                status: details::status_of(d.status()),
            }
        })
        .collect()
}

/// The repository's uncommitted changes, or `None` for a bare repository
/// (which has no working tree).
pub fn working_status(path: &str) -> Result<Option<WorkingStatus>> {
    let repo = Repository::discover(path)?;
    if repo.is_bare() {
        return Ok(None);
    }
    let staged = files_of(&side_diff(&repo, Side::Staged)?);
    let unstaged = files_of(&side_diff(&repo, Side::Unstaged)?);
    let is_conflict = |f: &WorkingFile| f.status == FileStatus::Conflicted;
    let mut conflicted: Vec<WorkingFile> = unstaged
        .iter()
        .filter(|f| is_conflict(f))
        .cloned()
        .collect();
    for f in staged.iter().filter(|f| is_conflict(f)) {
        if !conflicted.iter().any(|c| c.path == f.path) {
            conflicted.push(f.clone());
        }
    }
    Ok(Some(WorkingStatus {
        staged: staged.into_iter().filter(|f| !is_conflict(f)).collect(),
        unstaged: unstaged.into_iter().filter(|f| !is_conflict(f)).collect(),
        conflicted,
        in_progress: crate::merge::in_progress(path)?,
    }))
}

/// The diff of one file's staged or unstaged change.
pub fn working_diff(path: &str, side: Side, file: &str) -> Result<FileDiff> {
    let repo = Repository::discover(path)?;
    let diff = side_diff(&repo, side)?;
    details::patch_lines(&diff, details::find_delta(&diff, file)?)
}

/// The contents of a changed file before and after its staged or unstaged
/// change (for image previews). Unstaged "after" contents come from disk.
pub fn working_blobs(path: &str, side: Side, file: &str) -> Result<FileBlobs> {
    let repo = Repository::discover(path)?;
    let diff = side_diff(&repo, side)?;
    let delta = diff
        .get_delta(details::find_delta(&diff, file)?)
        .expect("index came from deltas()");
    let old = details::stored_blob(&repo, &delta.old_file())?;
    let new = match side {
        Side::Staged => details::stored_blob(&repo, &delta.new_file())?,
        Side::Unstaged => match (repo.workdir(), delta.new_file().path()) {
            (Some(root), Some(rel)) => read_file(&root.join(rel)),
            _ => None,
        },
    };
    Ok(FileBlobs { old, new })
}

fn read_file(path: &Path) -> Option<BlobData> {
    let size = usize::try_from(std::fs::metadata(path).ok()?.len()).unwrap_or(usize::MAX);
    let bytes = if size <= MAX_BLOB_BYTES {
        Some(std::fs::read(path).ok()?)
    } else {
        None
    };
    Some(BlobData { size, bytes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    fn stage(t: &TempRepo, file: &str) {
        let mut index = t.repo.index().unwrap();
        if t.dir.join(file).exists() {
            index.add_path(Path::new(file)).unwrap();
        } else {
            index.remove_path(Path::new(file)).unwrap();
        }
        index.write().unwrap();
    }

    fn summary(files: &[WorkingFile]) -> Vec<(&str, FileStatus)> {
        files.iter().map(|f| (f.path.as_str(), f.status)).collect()
    }

    #[test]
    fn a_clean_repo_has_no_changes() {
        let t = TempRepo::new();
        t.write("a.txt", "1\n");
        t.commit_all("first");
        assert!(working_status(t.path()).unwrap().unwrap().is_clean());
    }

    #[test]
    fn splits_staged_unstaged_and_untracked_changes() {
        let t = TempRepo::new();
        t.write("edited.txt", "one\n");
        t.write("staged.txt", "one\n");
        t.write("both.txt", "one\n");
        t.write("gone.txt", "bye\n");
        t.commit_all("first");

        t.write("edited.txt", "two\n");
        t.write("staged.txt", "two\n");
        stage(&t, "staged.txt");
        t.write("both.txt", "two\n");
        stage(&t, "both.txt");
        t.write("both.txt", "three\n");
        t.remove("gone.txt");
        t.write("new/dir/file.txt", "hi\n");

        let s = working_status(t.path()).unwrap().unwrap();
        assert_eq!(
            summary(&s.staged),
            [
                ("both.txt", FileStatus::Modified),
                ("staged.txt", FileStatus::Modified)
            ]
        );
        assert_eq!(
            summary(&s.unstaged),
            [
                ("both.txt", FileStatus::Modified),
                ("edited.txt", FileStatus::Modified),
                ("gone.txt", FileStatus::Deleted),
                ("new/dir/file.txt", FileStatus::Untracked),
            ]
        );
    }

    #[test]
    fn diffs_each_side_separately() {
        let t = TempRepo::new();
        t.write("both.txt", "one\n");
        t.commit_all("first");
        t.write("both.txt", "two\n");
        stage(&t, "both.txt");
        t.write("both.txt", "three\n");

        let content = |side| {
            let d = working_diff(t.path(), side, "both.txt").unwrap();
            d.hunks[0]
                .lines
                .iter()
                .map(|l| (l.kind, l.content.clone()))
                .collect::<Vec<_>>()
        };
        use crate::details::LineKind::{Add, Delete};
        assert_eq!(
            content(Side::Staged),
            [(Delete, "one".into()), (Add, "two".into())]
        );
        assert_eq!(
            content(Side::Unstaged),
            [(Delete, "two".into()), (Add, "three".into())]
        );
        assert!(working_diff(t.path(), Side::Staged, "nope.txt").is_err());
    }

    #[test]
    fn untracked_files_diff_as_all_added() {
        let t = TempRepo::new();
        t.write("a.txt", "1\n");
        t.commit_all("first");
        t.write("notes.md", "line one\nline two\n");
        let d = working_diff(t.path(), Side::Unstaged, "notes.md").unwrap();
        assert_eq!(d.status, FileStatus::Untracked);
        assert_eq!(d.hunks[0].lines.len(), 2);
    }

    #[test]
    fn detects_staged_renames() {
        let t = TempRepo::new();
        let long: String = (0..20).map(|i| format!("line {i}\n")).collect();
        t.write("old.txt", &long);
        t.commit_all("first");
        std::fs::rename(t.dir.join("old.txt"), t.dir.join("new.txt")).unwrap();
        stage(&t, "old.txt");
        stage(&t, "new.txt");
        let s = working_status(t.path()).unwrap().unwrap();
        assert_eq!(s.staged.len(), 1);
        assert_eq!(s.staged[0].status, FileStatus::Renamed);
        assert_eq!(s.staged[0].old_path.as_deref(), Some("old.txt"));
        assert!(s.unstaged.is_empty());
    }

    #[test]
    fn works_before_the_first_commit() {
        let t = TempRepo::new();
        t.write("a.txt", "1\n");
        stage(&t, "a.txt");
        t.write("b.txt", "2\n");
        let s = working_status(t.path()).unwrap().unwrap();
        assert_eq!(summary(&s.staged), [("a.txt", FileStatus::Added)]);
        assert_eq!(summary(&s.unstaged), [("b.txt", FileStatus::Untracked)]);
    }

    #[test]
    fn blobs_come_from_git_and_from_disk() {
        let t = TempRepo::new();
        t.write("img.png", [1u8, 2, 3]);
        t.commit_all("first");
        t.write("img.png", [1u8, 2, 3, 4]);
        stage(&t, "img.png");
        t.write("img.png", [9u8, 9, 9, 9, 9]);

        let staged = working_blobs(t.path(), Side::Staged, "img.png").unwrap();
        assert_eq!(staged.old.unwrap().bytes.unwrap(), [1, 2, 3]);
        assert_eq!(staged.new.unwrap().bytes.unwrap(), [1, 2, 3, 4]);
        let unstaged = working_blobs(t.path(), Side::Unstaged, "img.png").unwrap();
        assert_eq!(unstaged.old.unwrap().bytes.unwrap(), [1, 2, 3, 4]);
        assert_eq!(unstaged.new.unwrap().bytes.unwrap(), [9, 9, 9, 9, 9]);
    }
}
