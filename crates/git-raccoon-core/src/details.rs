//! Details of a single commit and the diffs of its files.
//!
//! A commit is compared with its first parent (or with nothing, for a root
//! commit), with rename detection, matching what `git show` lists.

use crate::Result;
use git2::{Commit, Delta, Diff, DiffFindOptions, Oid, Patch, Repository};

/// Stop sending diff lines past this many; the UI says the diff is cut short.
const MAX_DIFF_LINES: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Typechange,
}

#[derive(Debug)]
pub struct ChangedFile {
    pub path: String,
    /// Set when the file was renamed or copied.
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub additions: usize,
    pub deletions: usize,
    pub binary: bool,
}

#[derive(Debug)]
pub struct Person {
    pub name: String,
    pub email: String,
    pub time: i64,
}

#[derive(Debug)]
pub struct CommitDetails {
    pub id: String,
    pub parents: Vec<String>,
    pub summary: String,
    /// The message after the summary line, trimmed.
    pub body: String,
    pub author: Person,
    pub committer: Person,
    pub files: Vec<ChangedFile>,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Add,
    Delete,
    /// "\ No newline at end of file"
    NoEol,
}

#[derive(Debug)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    pub content: String,
}

#[derive(Debug)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug)]
pub struct FileDiff {
    pub path: String,
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub binary: bool,
    pub hunks: Vec<Hunk>,
    /// True when the diff had more than `MAX_DIFF_LINES` lines.
    pub truncated: bool,
}

fn find_commit<'r>(repo: &'r Repository, id: &str) -> Result<Commit<'r>> {
    Ok(repo.find_commit(Oid::from_str(id)?)?)
}

fn commit_diff<'r>(repo: &'r Repository, commit: &Commit) -> Result<Diff<'r>> {
    let new_tree = commit.tree()?;
    let old_tree = match commit.parent_count() {
        0 => None,
        _ => Some(commit.parent(0)?.tree()?),
    };
    let mut diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;
    diff.find_similar(Some(DiffFindOptions::new().renames(true)))?;
    Ok(diff)
}

fn status_of(delta: Delta) -> FileStatus {
    match delta {
        Delta::Added => FileStatus::Added,
        Delta::Deleted => FileStatus::Deleted,
        Delta::Renamed => FileStatus::Renamed,
        Delta::Copied => FileStatus::Copied,
        Delta::Typechange => FileStatus::Typechange,
        _ => FileStatus::Modified,
    }
}

/// The path shown for a file, plus its old path when renamed or copied.
fn paths_of(delta: &git2::DiffDelta) -> (String, Option<String>) {
    let lossy = |p: Option<&std::path::Path>| {
        p.map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let new = lossy(delta.new_file().path());
    let old = lossy(delta.old_file().path());
    match delta.status() {
        Delta::Renamed | Delta::Copied => (new, Some(old)),
        Delta::Deleted => (old, None),
        _ => (new, None),
    }
}

fn person(sig: git2::Signature) -> Person {
    Person {
        name: sig.name().unwrap_or_default().to_string(),
        email: sig.email().unwrap_or_default().to_string(),
        time: sig.when().seconds(),
    }
}

pub fn commit_details(path: &str, id: &str) -> Result<CommitDetails> {
    let repo = Repository::discover(path)?;
    let commit = find_commit(&repo, id)?;
    let diff = commit_diff(&repo, &commit)?;

    let mut files = Vec::with_capacity(diff.deltas().len());
    for (i, delta) in diff.deltas().enumerate() {
        let (path, old_path) = paths_of(&delta);
        let patch = Patch::from_diff(&diff, i)?;
        let binary = patch.as_ref().is_none_or(|p| p.delta().flags().is_binary());
        let (additions, deletions) = match &patch {
            Some(p) if !binary => {
                let (_, add, del) = p.line_stats()?;
                (add, del)
            }
            _ => (0, 0),
        };
        files.push(ChangedFile {
            path,
            old_path,
            status: status_of(delta.status()),
            additions,
            deletions,
            binary,
        });
    }

    let message = commit.message().unwrap_or_default();
    let body = message
        .split_once('\n')
        .map(|(_, rest)| rest.trim())
        .unwrap_or_default()
        .to_string();

    Ok(CommitDetails {
        id: commit.id().to_string(),
        parents: commit.parent_ids().map(|p| p.to_string()).collect(),
        summary: commit.summary().unwrap_or_default().to_string(),
        body,
        author: person(commit.author()),
        committer: person(commit.committer()),
        additions: files.iter().map(|f| f.additions).sum(),
        deletions: files.iter().map(|f| f.deletions).sum(),
        files,
    })
}

pub fn file_diff(path: &str, id: &str, file: &str) -> Result<FileDiff> {
    let repo = Repository::discover(path)?;
    let commit = find_commit(&repo, id)?;
    let diff = commit_diff(&repo, &commit)?;

    let index = find_delta(&diff, file)?;
    let delta = diff.get_delta(index).expect("index came from deltas()");
    let (path, old_path) = paths_of(&delta);
    let status = status_of(delta.status());

    let patch = Patch::from_diff(&diff, index)?;
    let binary = patch.as_ref().is_none_or(|p| p.delta().flags().is_binary());
    let mut hunks = Vec::new();
    let mut truncated = false;
    let mut total = 0;

    if let (Some(patch), false) = (patch, binary) {
        'hunks: for h in 0..patch.num_hunks() {
            let (hunk, count) = patch.hunk(h)?;
            let mut lines = Vec::with_capacity(count);
            for l in 0..count {
                if total == MAX_DIFF_LINES {
                    truncated = true;
                    hunks.push(Hunk {
                        header: header_of(&hunk),
                        lines,
                    });
                    break 'hunks;
                }
                total += 1;
                let line = patch.line_in_hunk(h, l)?;
                let kind = match line.origin() {
                    '+' => LineKind::Add,
                    '-' => LineKind::Delete,
                    '=' | '>' | '<' => LineKind::NoEol,
                    _ => LineKind::Context,
                };
                lines.push(if kind == LineKind::NoEol {
                    // A marker, not a line of the file: no line numbers.
                    DiffLine {
                        kind,
                        old_line: None,
                        new_line: None,
                        content: "No newline at end of file".into(),
                    }
                } else {
                    DiffLine {
                        kind,
                        old_line: line.old_lineno(),
                        new_line: line.new_lineno(),
                        content: String::from_utf8_lossy(line.content())
                            .trim_end_matches(['\n', '\r'])
                            .to_string(),
                    }
                });
            }
            hunks.push(Hunk {
                header: header_of(&hunk),
                lines,
            });
        }
    }

    Ok(FileDiff {
        path,
        old_path,
        status,
        binary,
        hunks,
        truncated,
    })
}

/// The index of the delta for `file` (its new path, or old path if deleted).
fn find_delta(diff: &Diff, file: &str) -> Result<usize> {
    diff.deltas()
        .position(|d| paths_of(&d).0 == file)
        .ok_or_else(|| crate::Error::NotChanged(file.to_string()))
}

/// Blobs larger than this are reported by size only.
const MAX_BLOB_BYTES: usize = 20 * 1024 * 1024;

/// One version of a file's contents, for previews such as images.
#[derive(Debug)]
pub struct BlobData {
    pub size: usize,
    /// The contents, or `None` when larger than the preview limit.
    pub bytes: Option<Vec<u8>>,
}

/// A changed file's contents before and after the commit. A side is `None`
/// when the file did not exist there (added or deleted).
#[derive(Debug)]
pub struct FileBlobs {
    pub old: Option<BlobData>,
    pub new: Option<BlobData>,
}

pub fn file_blobs(path: &str, id: &str, file: &str) -> Result<FileBlobs> {
    let repo = Repository::discover(path)?;
    let commit = find_commit(&repo, id)?;
    let diff = commit_diff(&repo, &commit)?;
    let delta = diff
        .get_delta(find_delta(&diff, file)?)
        .expect("index came from deltas()");
    let load = |side: git2::DiffFile| -> Result<Option<BlobData>> {
        if side.id().is_zero() {
            return Ok(None);
        }
        let blob = repo.find_blob(side.id())?;
        let size = blob.size();
        let bytes = (size <= MAX_BLOB_BYTES).then(|| blob.content().to_vec());
        Ok(Some(BlobData { size, bytes }))
    };
    Ok(FileBlobs {
        old: load(delta.old_file())?,
        new: load(delta.new_file())?,
    })
}

fn header_of(hunk: &git2::DiffHunk) -> String {
    String::from_utf8_lossy(hunk.header())
        .trim_end()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    #[test]
    fn root_commit_lists_every_file_as_added() {
        let t = TempRepo::new();
        t.write("a.txt", "one\ntwo\n");
        t.write("dir/b.txt", "x\n");
        let id = t.commit_all("Initial commit\n\nWith a body.\n");

        let d = commit_details(t.path(), &id.to_string()).unwrap();
        assert_eq!(d.summary, "Initial commit");
        assert_eq!(d.body, "With a body.");
        assert!(d.parents.is_empty());
        assert_eq!(d.author.name, "Raccoon");
        let files: Vec<_> = d
            .files
            .iter()
            .map(|f| (f.path.as_str(), f.status, f.additions))
            .collect();
        assert_eq!(
            files,
            [
                ("a.txt", FileStatus::Added, 2),
                ("dir/b.txt", FileStatus::Added, 1)
            ]
        );
        assert_eq!((d.additions, d.deletions), (3, 0));
    }

    #[test]
    fn lists_modified_deleted_renamed_and_binary_files() {
        let t = TempRepo::new();
        let long: String = (0..20).map(|i| format!("line {i}\n")).collect();
        t.write("keep.txt", "a\nb\nc\n");
        t.write("gone.txt", "bye\n");
        t.write("old_name.txt", &long);
        t.write("logo.bin", [0u8, 1, 2, 3, 0, 255]);
        t.commit_all("first");

        t.write("keep.txt", "a\nB\nc\nd\n");
        t.remove("gone.txt");
        t.remove("old_name.txt");
        t.write("new_name.txt", &long);
        t.write("logo.bin", [0u8, 9, 9, 9, 0, 255]);
        let id = t.commit_all("second");

        let d = commit_details(t.path(), &id.to_string()).unwrap();
        let find = |p: &str| {
            d.files
                .iter()
                .find(|f| f.path == p)
                .unwrap_or_else(|| panic!("{p} missing"))
        };

        let keep = find("keep.txt");
        assert_eq!(
            (keep.status, keep.additions, keep.deletions),
            (FileStatus::Modified, 2, 1)
        );
        assert_eq!(find("gone.txt").status, FileStatus::Deleted);
        let renamed = find("new_name.txt");
        assert_eq!(renamed.status, FileStatus::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("old_name.txt"));
        assert!(find("logo.bin").binary);
        assert_eq!(d.files.len(), 4);
    }

    #[test]
    fn file_diff_returns_numbered_lines() {
        let t = TempRepo::new();
        t.write("f.txt", "a\nb\nc\n");
        t.commit_all("first");
        t.write("f.txt", "a\nB\nc\nd");
        let id = t.commit_all("second");

        let d = file_diff(t.path(), &id.to_string(), "f.txt").unwrap();
        assert_eq!(d.status, FileStatus::Modified);
        assert!(!d.binary && !d.truncated);
        assert_eq!(d.hunks.len(), 1);
        assert!(
            d.hunks[0].header.starts_with("@@ -1,3 +1,4 @@"),
            "{}",
            d.hunks[0].header
        );

        let lines: Vec<_> = d.hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.old_line, l.new_line, l.content.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (LineKind::Context, Some(1), Some(1), "a"),
                (LineKind::Delete, Some(2), None, "b"),
                (LineKind::Add, None, Some(2), "B"),
                (LineKind::Context, Some(3), Some(3), "c"),
                (LineKind::Add, None, Some(4), "d"),
                (LineKind::NoEol, None, None, "No newline at end of file"),
            ]
        );
    }

    #[test]
    fn file_diff_finds_deleted_and_renamed_files() {
        let t = TempRepo::new();
        let long: String = (0..20).map(|i| format!("line {i}\n")).collect();
        t.write("gone.txt", "bye\n");
        t.write("old.txt", &long);
        t.commit_all("first");
        t.remove("gone.txt");
        t.remove("old.txt");
        t.write("new.txt", format!("{long}extra\n"));
        let id = t.commit_all("second").to_string();

        let gone = file_diff(t.path(), &id, "gone.txt").unwrap();
        assert_eq!(gone.status, FileStatus::Deleted);
        assert_eq!(gone.hunks[0].lines[0].kind, LineKind::Delete);

        let renamed = file_diff(t.path(), &id, "new.txt").unwrap();
        assert_eq!(renamed.status, FileStatus::Renamed);
        assert_eq!(renamed.old_path.as_deref(), Some("old.txt"));
        let added: Vec<_> = renamed
            .hunks
            .iter()
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == LineKind::Add)
            .collect();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].content, "extra");

        assert!(file_diff(t.path(), &id, "nope.txt").is_err());
    }

    #[test]
    fn merge_commit_is_compared_with_its_first_parent() {
        let t = TempRepo::new();
        t.write("base.txt", "base\n");
        let base = t.commit_all("base");
        t.write("main.txt", "main\n");
        let main = t.commit_all("main work");

        // Build the feature commit on top of base without touching main's tree.
        let mut index = t.repo.index().unwrap();
        index
            .read_tree(&t.repo.find_commit(base).unwrap().tree().unwrap())
            .unwrap();
        t.write("feature.txt", "feature\n");
        index.add_path(std::path::Path::new("feature.txt")).unwrap();
        index.write().unwrap();
        let feature = t.commit(None, "feature work", &[base]);

        // The merge result has all three files.
        index.add_path(std::path::Path::new("main.txt")).unwrap();
        index.write().unwrap();
        let merge = t.commit(None, "Merge feature", &[main, feature]);

        let d = commit_details(t.path(), &merge.to_string()).unwrap();
        assert_eq!(d.parents.len(), 2);
        let paths: Vec<_> = d.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["feature.txt"]);
    }

    #[test]
    fn file_blobs_returns_both_versions() {
        let t = TempRepo::new();
        t.write("logo.png", [0x89u8, b'P', b'N', b'G', 1]);
        t.write("gone.bin", [0u8, 1]);
        let first = t.commit_all("first").to_string();
        t.write("logo.png", [0x89u8, b'P', b'N', b'G', 2, 3]);
        t.remove("gone.bin");
        let second = t.commit_all("second").to_string();

        let added = file_blobs(t.path(), &first, "logo.png").unwrap();
        assert!(added.old.is_none());
        assert_eq!(added.new.as_ref().unwrap().size, 5);
        assert_eq!(
            added.new.unwrap().bytes.as_deref(),
            Some(&[0x89, b'P', b'N', b'G', 1][..])
        );

        let changed = file_blobs(t.path(), &second, "logo.png").unwrap();
        assert_eq!(changed.old.unwrap().size, 5);
        assert_eq!(changed.new.unwrap().size, 6);

        let deleted = file_blobs(t.path(), &second, "gone.bin").unwrap();
        assert_eq!(deleted.old.unwrap().size, 2);
        assert!(deleted.new.is_none());
    }
}
