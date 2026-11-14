//! Staging and unstaging: whole files, hunks or single lines.
//!
//! Only the index changes; the working tree is never touched, so nothing
//! here can lose work.
//!
//! Partial staging rebuilds the file's index version from two exact copies
//! of the file: staging applies the chosen changes of the unstaged diff
//! (index → working tree) to the index version; unstaging applies every
//! staged change except the chosen ones (HEAD → index) to the HEAD version.
//! Lines are copied byte for byte, so line endings and encodings survive.

use std::path::Path;

use git2::{IndexEntry, IndexTime, Repository};

use crate::details::{FileDiff, LineKind};
use crate::working::{self, Side};
use crate::{Error, Result};

/// Which lines of a file's diff to act on, by hunk and line index.
#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    Hunk(usize),
    Lines(Vec<(usize, usize)>),
}

impl Pick {
    fn contains(&self, hunk: usize, line: usize) -> bool {
        match self {
            Pick::Hunk(h) => *h == hunk,
            Pick::Lines(lines) => lines.contains(&(hunk, line)),
        }
    }
}

fn open(path: &str) -> Result<Repository> {
    let repo = Repository::discover(path)?;
    if repo.is_bare() {
        return Err(Error::Unsupported(
            "a bare repository has no working tree".into(),
        ));
    }
    Ok(repo)
}

/// Stages a whole file (like `git add`, or `git rm --cached` for a deleted one).
pub fn stage_file(path: &str, file: &str) -> Result<()> {
    let repo = open(path)?;
    let mut index = repo.index()?;
    let workdir = repo.workdir().expect("not bare");
    if workdir.join(file).symlink_metadata().is_ok() {
        index.add_path(Path::new(file))?;
    } else {
        index.remove_path(Path::new(file))?;
    }
    index.write()?;
    Ok(())
}

/// Stages every unstaged change, including untracked files (like `git add -A`).
pub fn stage_all(path: &str) -> Result<()> {
    let repo = open(path)?;
    let mut index = repo.index()?;
    index.add_all(["*"], git2::IndexAddOption::DEFAULT, None)?;
    index.update_all(["*"], None)?;
    index.write()?;
    Ok(())
}

/// Unstages a whole file (like `git restore --staged`). For a staged rename,
/// both the new and the old path are restored.
pub fn unstage_file(path: &str, file: &str) -> Result<()> {
    let repo = open(path)?;
    let mut paths = vec![file.to_string()];
    if let Some(status) = working::working_status(path)?
        && let Some(f) = status.staged.iter().find(|f| f.path == file)
        && let Some(old) = &f.old_path
    {
        paths.push(old.clone());
    }
    unstage_paths(&repo, &paths)
}

/// Unstages everything (like `git reset`).
pub fn unstage_all(path: &str) -> Result<()> {
    let repo = open(path)?;
    let paths: Vec<String> = working::working_status(path)?
        .map(|s| {
            s.staged
                .iter()
                .flat_map(|f| std::iter::once(f.path.clone()).chain(f.old_path.clone()))
                .collect()
        })
        .unwrap_or_default();
    unstage_paths(&repo, &paths)
}

fn unstage_paths(repo: &Repository, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    match repo.head().ok().and_then(|h| h.peel_to_commit().ok()) {
        Some(head) => repo.reset_default(Some(head.as_object()), paths)?,
        // Before the first commit, unstaging means dropping from the index.
        None => {
            let mut index = repo.index()?;
            for p in paths {
                index.remove_path(Path::new(p))?;
            }
            index.write()?;
        }
    }
    Ok(())
}

/// Stages the picked lines of a file's unstaged diff. `expected` holds the
/// hunk headers the caller saw, so a file that changed since is refused
/// rather than staged wrongly.
pub fn stage_lines(path: &str, file: &str, pick: &Pick, expected: &[String]) -> Result<()> {
    let repo = open(path)?;
    let diff = checked_diff(path, Side::Unstaged, file, expected)?;
    let old = index_content(&repo, file)?;
    let new = std::fs::read(repo.workdir().expect("not bare").join(file)).ok();
    let content = rebuild(
        old.as_deref().unwrap_or_default(),
        new.as_deref().unwrap_or_default(),
        &diff,
        |h, l| pick.contains(h, l),
    );
    // Staging every line of a deleted file stages the deletion itself.
    if new.is_none() && everything(&diff, pick) {
        return remove_from_index(&repo, file);
    }
    write_index_content(&repo, file, &content)
}

/// Unstages the picked lines of a file's staged diff.
pub fn unstage_lines(path: &str, file: &str, pick: &Pick, expected: &[String]) -> Result<()> {
    let repo = open(path)?;
    let diff = checked_diff(path, Side::Staged, file, expected)?;
    let head = head_content(&repo, file)?;
    let staged = index_content(&repo, file)?;
    // Keep every staged change except the picked ones.
    let content = rebuild(
        head.as_deref().unwrap_or_default(),
        staged.as_deref().unwrap_or_default(),
        &diff,
        |h, l| !pick.contains(h, l),
    );
    // Unstaging all of a newly added file takes it out of the index again.
    if head.is_none() && everything(&diff, pick) {
        return remove_from_index(&repo, file);
    }
    write_index_content(&repo, file, &content)
}

fn checked_diff(path: &str, side: Side, file: &str, expected: &[String]) -> Result<FileDiff> {
    let diff = working::working_diff(path, side, file)?;
    if diff.binary {
        return Err(Error::Unsupported(
            "binary files can only be staged whole".into(),
        ));
    }
    if diff.truncated {
        return Err(Error::Unsupported(
            "this diff is too large to stage in parts".into(),
        ));
    }
    let headers: Vec<&str> = diff.hunks.iter().map(|h| h.header.as_str()).collect();
    if headers != expected {
        return Err(Error::Stale(file.to_string()));
    }
    Ok(diff)
}

/// Whether the pick covers every changed line of the diff.
fn everything(diff: &FileDiff, pick: &Pick) -> bool {
    diff.hunks.iter().enumerate().all(|(h, hunk)| {
        hunk.lines.iter().enumerate().all(|(l, line)| {
            matches!(line.kind, LineKind::Context | LineKind::NoEol) || pick.contains(h, l)
        })
    })
}

/// Splits into lines, each keeping its own ending.
fn lines(content: &[u8]) -> Vec<&[u8]> {
    content.split_inclusive(|b| *b == b'\n').collect()
}

/// Builds a file from `old`, applying the changes of `diff` (which turns
/// `old` into `new`) that `include` selects. Context and unchosen deletions
/// come from `old`; chosen additions come from `new`.
fn rebuild(
    old: &[u8],
    new: &[u8],
    diff: &FileDiff,
    include: impl Fn(usize, usize) -> bool,
) -> Vec<u8> {
    let old_lines = lines(old);
    let new_lines = lines(new);
    let ending: &[u8] =
        if old.windows(2).any(|w| w == b"\r\n") || new.windows(2).any(|w| w == b"\r\n") {
            b"\r\n"
        } else {
            b"\n"
        };
    let mut out: Vec<u8> = Vec::with_capacity(old.len().max(new.len()));
    let mut push = |out: &mut Vec<u8>, line: &[u8]| {
        // A line without an ending (the old last line) gets one when more follows.
        if !out.is_empty() && !out.ends_with(b"\n") {
            out.extend_from_slice(ending);
        }
        out.extend_from_slice(line);
    };
    let mut next_old = 1usize; // 1-based, like the diff's line numbers
    let copy_until = |out: &mut Vec<u8>,
                      next_old: &mut usize,
                      until: usize,
                      push: &mut dyn FnMut(&mut Vec<u8>, &[u8])| {
        while *next_old < until {
            if let Some(line) = old_lines.get(*next_old - 1) {
                push(out, line);
            }
            *next_old += 1;
        }
    };

    for (h, hunk) in diff.hunks.iter().enumerate() {
        for (l, line) in hunk.lines.iter().enumerate() {
            match line.kind {
                LineKind::Context | LineKind::Delete => {
                    let Some(n) = line.old_line.map(|n| n as usize) else {
                        continue;
                    };
                    copy_until(&mut out, &mut next_old, n, &mut push);
                    let keep = line.kind == LineKind::Context || !include(h, l);
                    if keep && let Some(text) = old_lines.get(n - 1) {
                        push(&mut out, text);
                    }
                    next_old = n + 1;
                }
                LineKind::Add => {
                    if include(h, l)
                        && let Some(text) =
                            line.new_line.and_then(|n| new_lines.get(n as usize - 1))
                    {
                        push(&mut out, text);
                    }
                }
                LineKind::NoEol => {}
            }
        }
    }
    copy_until(&mut out, &mut next_old, old_lines.len() + 1, &mut push);
    out
}

fn index_content(repo: &Repository, file: &str) -> Result<Option<Vec<u8>>> {
    let mut index = repo.index()?;
    // Pick up writes made through other handles (or by git itself).
    index.read(false)?;
    match index.get_path(Path::new(file), 0) {
        Some(entry) => Ok(Some(repo.find_blob(entry.id)?.content().to_vec())),
        None => Ok(None),
    }
}

fn head_content(repo: &Repository, file: &str) -> Result<Option<Vec<u8>>> {
    let Some(tree) = repo.head().ok().and_then(|h| h.peel_to_tree().ok()) else {
        return Ok(None);
    };
    match tree.get_path(Path::new(file)) {
        Ok(entry) => Ok(Some(repo.find_blob(entry.id())?.content().to_vec())),
        Err(_) => Ok(None),
    }
}

fn remove_from_index(repo: &Repository, file: &str) -> Result<()> {
    let mut index = repo.index()?;
    index.remove_path(Path::new(file))?;
    index.write()?;
    Ok(())
}

/// Writes `content` as the file's index version, keeping its mode.
fn write_index_content(repo: &Repository, file: &str, content: &[u8]) -> Result<()> {
    let mut index = repo.index()?;
    let entry = match index.get_path(Path::new(file), 0) {
        Some(mut e) => {
            e.file_size = u32::try_from(content.len()).unwrap_or(u32::MAX);
            e
        }
        None => new_entry(repo, file, content.len()),
    };
    index.add_frombuffer(&entry, content)?;
    index.write()?;
    Ok(())
}

/// An index entry for a file that isn't in the index yet.
fn new_entry(repo: &Repository, file: &str, size: usize) -> IndexEntry {
    let executable = repo
        .workdir()
        .and_then(|w| std::fs::metadata(w.join(file)).ok())
        .is_some_and(|m| is_executable(&m));
    IndexEntry {
        ctime: IndexTime::new(0, 0),
        mtime: IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode: if executable { 0o100_755 } else { 0o100_644 },
        uid: 0,
        gid: 0,
        file_size: u32::try_from(size).unwrap_or(u32::MAX),
        id: git2::Oid::zero(),
        flags: 0,
        flags_extended: 0,
        path: file.as_bytes().to_vec(),
    }
}

#[cfg(unix)]
fn is_executable(m: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    m.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::details::FileStatus;
    use crate::testutil::TempRepo;
    use crate::working::working_status;

    fn headers(t: &TempRepo, side: Side, file: &str) -> Vec<String> {
        working::working_diff(t.path(), side, file)
            .unwrap()
            .hunks
            .iter()
            .map(|h| h.header.clone())
            .collect()
    }

    fn staged_text(t: &TempRepo, file: &str) -> String {
        String::from_utf8(index_content(&t.repo, file).unwrap().unwrap()).unwrap()
    }

    fn names(files: &[crate::working::WorkingFile]) -> Vec<&str> {
        files.iter().map(|f| f.path.as_str()).collect()
    }

    /// Ten lines, so hunks far apart stay separate.
    fn numbered(change: impl Fn(usize) -> Option<String>) -> String {
        (1..=20)
            .map(|i| change(i).unwrap_or_else(|| format!("line {i}\n")))
            .collect()
    }

    #[test]
    fn stages_and_unstages_whole_files() {
        let t = TempRepo::new();
        t.write("a.txt", "1\n");
        t.write("gone.txt", "x\n");
        t.commit_all("first");
        t.write("a.txt", "2\n");
        t.write("new.txt", "n\n");
        t.remove("gone.txt");

        for f in ["a.txt", "new.txt", "gone.txt"] {
            stage_file(t.path(), f).unwrap();
        }
        let s = working_status(t.path()).unwrap().unwrap();
        assert_eq!(names(&s.staged), ["a.txt", "gone.txt", "new.txt"]);
        assert!(s.unstaged.is_empty());

        unstage_file(t.path(), "new.txt").unwrap();
        unstage_file(t.path(), "gone.txt").unwrap();
        let s = working_status(t.path()).unwrap().unwrap();
        assert_eq!(names(&s.staged), ["a.txt"]);
        assert_eq!(names(&s.unstaged), ["gone.txt", "new.txt"]);
        // The working tree is untouched throughout.
        assert_eq!(std::fs::read_to_string(t.dir.join("a.txt")).unwrap(), "2\n");
        assert!(!t.dir.join("gone.txt").exists());

        stage_all(t.path()).unwrap();
        assert!(
            working_status(t.path())
                .unwrap()
                .unwrap()
                .unstaged
                .is_empty()
        );
        unstage_all(t.path()).unwrap();
        assert!(working_status(t.path()).unwrap().unwrap().staged.is_empty());
    }

    #[test]
    fn unstaging_a_rename_restores_both_paths() {
        let t = TempRepo::new();
        let long: String = (0..20).map(|i| format!("line {i}\n")).collect();
        t.write("old.txt", &long);
        t.commit_all("first");
        std::fs::rename(t.dir.join("old.txt"), t.dir.join("new.txt")).unwrap();
        stage_all(t.path()).unwrap();
        assert_eq!(
            working_status(t.path()).unwrap().unwrap().staged[0].status,
            FileStatus::Renamed
        );

        unstage_file(t.path(), "new.txt").unwrap();
        let s = working_status(t.path()).unwrap().unwrap();
        assert!(s.staged.is_empty(), "{:?}", s.staged);
        assert_eq!(names(&s.unstaged), ["new.txt", "old.txt"]);
    }

    #[test]
    fn stages_one_hunk_of_two() {
        let t = TempRepo::new();
        t.write("f.txt", numbered(|_| None));
        t.commit_all("first");
        let edited = numbered(|i| match i {
            2 => Some("line 2 edited\n".into()),
            18 => Some("line 18 edited\n".into()),
            _ => None,
        });
        t.write("f.txt", &edited);
        let seen = headers(&t, Side::Unstaged, "f.txt");
        assert_eq!(seen.len(), 2);

        stage_lines(t.path(), "f.txt", &Pick::Hunk(1), &seen).unwrap();
        assert_eq!(
            staged_text(&t, "f.txt"),
            numbered(|i| (i == 18).then(|| "line 18 edited\n".into()))
        );
        // The other hunk is still unstaged; the file on disk is unchanged.
        assert_eq!(headers(&t, Side::Unstaged, "f.txt").len(), 1);
        assert_eq!(
            std::fs::read_to_string(t.dir.join("f.txt")).unwrap(),
            edited
        );

        // And back again.
        let staged = headers(&t, Side::Staged, "f.txt");
        unstage_lines(t.path(), "f.txt", &Pick::Hunk(0), &staged).unwrap();
        assert_eq!(staged_text(&t, "f.txt"), numbered(|_| None));
    }

    #[test]
    fn stages_single_lines() {
        let t = TempRepo::new();
        t.write("f.txt", "keep\nold a\nold b\nend\n");
        t.commit_all("first");
        t.write("f.txt", "keep\nnew a\nnew b\nend\n");
        let diff = working::working_diff(t.path(), Side::Unstaged, "f.txt").unwrap();
        let lines: Vec<(LineKind, &str)> = diff.hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.content.as_str()))
            .collect();
        let at = |kind, text| (0, lines.iter().position(|l| *l == (kind, text)).unwrap());

        // Replace "old a" with "new a" only. As in `git add -p`, the unpicked
        // deletion ("old b") stays as an unchanged line where it was, and the
        // picked addition follows it.
        let pick = Pick::Lines(vec![
            at(LineKind::Delete, "old a"),
            at(LineKind::Add, "new a"),
        ]);
        stage_lines(
            t.path(),
            "f.txt",
            &pick,
            &headers(&t, Side::Unstaged, "f.txt"),
        )
        .unwrap();
        assert_eq!(staged_text(&t, "f.txt"), "keep\nold b\nnew a\nend\n");

        // Staging the rest arrives at the working tree's version exactly.
        let rest = headers(&t, Side::Unstaged, "f.txt");
        stage_lines(t.path(), "f.txt", &Pick::Hunk(0), &rest).unwrap();
        assert_eq!(staged_text(&t, "f.txt"), "keep\nnew a\nnew b\nend\n");
        assert!(
            working_status(t.path())
                .unwrap()
                .unwrap()
                .unstaged
                .is_empty()
        );

        // A lone addition, without the changed line next to it.
        t.write("g.txt", "x\ny\n");
        t.commit_all("second");
        t.write("g.txt", "x\nY\nz\n");
        let diff = working::working_diff(t.path(), Side::Unstaged, "g.txt").unwrap();
        let z = diff.hunks[0]
            .lines
            .iter()
            .position(|l| l.content == "z")
            .unwrap();
        stage_lines(
            t.path(),
            "g.txt",
            &Pick::Lines(vec![(0, z)]),
            &headers(&t, Side::Unstaged, "g.txt"),
        )
        .unwrap();
        assert_eq!(staged_text(&t, "g.txt"), "x\ny\nz\n");
    }

    #[test]
    fn unstages_single_lines() {
        let t = TempRepo::new();
        t.write("f.txt", "a\nb\nc\n");
        t.commit_all("first");
        t.write("f.txt", "a\nB\nc\nd\n");
        stage_file(t.path(), "f.txt").unwrap();
        let diff = working::working_diff(t.path(), Side::Staged, "f.txt").unwrap();
        let d = diff.hunks[0]
            .lines
            .iter()
            .position(|l| l.content == "d")
            .unwrap();

        unstage_lines(
            t.path(),
            "f.txt",
            &Pick::Lines(vec![(0, d)]),
            &headers(&t, Side::Staged, "f.txt"),
        )
        .unwrap();
        assert_eq!(staged_text(&t, "f.txt"), "a\nB\nc\n");
        assert_eq!(
            std::fs::read_to_string(t.dir.join("f.txt")).unwrap(),
            "a\nB\nc\nd\n"
        );
    }

    #[test]
    fn keeps_crlf_endings_and_missing_final_newlines() {
        let t = TempRepo::new();
        // CRLF endings and the missing final newline are copied exactly.
        t.write("w.txt", "one\r\ntwo\r\nthree");
        t.commit_all("first");
        t.write("w.txt", "ONE\r\ntwo\r\nthree");
        stage_lines(
            t.path(),
            "w.txt",
            &Pick::Hunk(0),
            &headers(&t, Side::Unstaged, "w.txt"),
        )
        .unwrap();
        assert_eq!(staged_text(&t, "w.txt"), "ONE\r\ntwo\r\nthree");

        // A line added after a last line without a newline gets one in between.
        t.write("n.txt", "a");
        t.commit_all("second");
        t.write("n.txt", "a\nb");
        let diff = working::working_diff(t.path(), Side::Unstaged, "n.txt").unwrap();
        let b = diff.hunks[0]
            .lines
            .iter()
            .position(|l| l.content == "b")
            .unwrap();
        stage_lines(
            t.path(),
            "n.txt",
            &Pick::Lines(vec![(0, b)]),
            &headers(&t, Side::Unstaged, "n.txt"),
        )
        .unwrap();
        assert_eq!(staged_text(&t, "n.txt"), "a\nb");
    }

    #[test]
    fn partially_stages_a_new_file_and_unstages_it_entirely() {
        let t = TempRepo::new();
        t.write("a.txt", "1\n");
        t.commit_all("first");
        t.write("new.txt", "first\nsecond\n");
        let diff = working::working_diff(t.path(), Side::Unstaged, "new.txt").unwrap();
        let first = diff.hunks[0]
            .lines
            .iter()
            .position(|l| l.content == "first")
            .unwrap();
        stage_lines(
            t.path(),
            "new.txt",
            &Pick::Lines(vec![(0, first)]),
            &headers(&t, Side::Unstaged, "new.txt"),
        )
        .unwrap();
        assert_eq!(staged_text(&t, "new.txt"), "first\n");

        unstage_lines(
            t.path(),
            "new.txt",
            &Pick::Hunk(0),
            &headers(&t, Side::Staged, "new.txt"),
        )
        .unwrap();
        let s = working_status(t.path()).unwrap().unwrap();
        assert!(
            s.staged.is_empty(),
            "unstaging all of a new file drops it from the index"
        );
        assert_eq!(s.unstaged[0].status, FileStatus::Untracked);
    }

    #[test]
    fn staging_all_of_a_deleted_file_stages_the_deletion() {
        let t = TempRepo::new();
        t.write("gone.txt", "x\ny\n");
        t.commit_all("first");
        t.remove("gone.txt");
        stage_lines(
            t.path(),
            "gone.txt",
            &Pick::Hunk(0),
            &headers(&t, Side::Unstaged, "gone.txt"),
        )
        .unwrap();
        let s = working_status(t.path()).unwrap().unwrap();
        assert_eq!(s.staged[0].status, FileStatus::Deleted);
    }

    #[test]
    fn refuses_when_the_file_changed_since_it_was_shown() {
        let t = TempRepo::new();
        t.write("f.txt", numbered(|_| None));
        t.commit_all("first");
        t.write("f.txt", numbered(|i| (i == 2).then(|| "x\n".into())));
        let seen = headers(&t, Side::Unstaged, "f.txt");
        t.write("f.txt", numbered(|i| (i == 18).then(|| "y\n".into())));
        let err = stage_lines(t.path(), "f.txt", &Pick::Hunk(0), &seen).unwrap_err();
        assert!(matches!(err, Error::Stale(_)), "{err}");
        assert_eq!(
            staged_text(&t, "f.txt"),
            numbered(|_| None),
            "nothing was staged"
        );
    }
}
