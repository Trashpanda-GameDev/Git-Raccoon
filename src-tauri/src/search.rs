//! Searching the whole history, not just the rows the UI has loaded.
//!
//! Searches run in short slices so the UI can show matches as they are
//! found and stop asking once a newer search replaces this one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use git2::{Commit, Repository};
use serde::{Deserialize, Serialize};

use crate::repo::History;

/// How long one search call runs before returning partial results.
pub const SLICE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SearchField {
    /// Commit message, author name or email, or SHA prefix.
    Text,
    /// Paths of the files a commit changed (compared with its first parent).
    Path,
}

/// Matches among rows `start .. next` of the history.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// Rows of the matching commits, in history order.
    pub rows: Vec<usize>,
    /// Where to continue, or `None` once the whole history is searched.
    pub next: Option<usize>,
    /// A newer search started, so this one stopped early.
    pub cancelled: bool,
}

fn text_matches(commit: &Commit, needle: &str, sha_prefix: bool) -> bool {
    if sha_prefix && commit.id().to_string().starts_with(needle) {
        return true;
    }
    let contains = |s: Option<&str>| s.is_some_and(|s| s.to_lowercase().contains(needle));
    let author = commit.author();
    contains(commit.message()) || contains(author.name()) || contains(author.email())
}

fn path_matches(repo: &Repository, commit: &Commit, needle: &str) -> Result<bool, git2::Error> {
    let new_tree = commit.tree()?;
    let old_tree = match commit.parent_count() {
        0 => None,
        _ => Some(commit.parent(0)?.tree()?),
    };
    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;
    Ok(diff.deltas().any(|d| {
        [d.new_file().path(), d.old_file().path()]
            .into_iter()
            .flatten()
            .any(|p| p.to_string_lossy().to_lowercase().contains(needle))
    }))
}

/// Finds commits matching `query` (case-insensitively), starting at row
/// `start` and returning after about `budget` with where to continue. Stops
/// early when `generation` no longer holds `mine` (a newer search started).
pub fn search(
    history: &History,
    query: &str,
    field: SearchField,
    start: usize,
    budget: Duration,
    generation: &AtomicU64,
    mine: u64,
) -> Result<SearchResult, git2::Error> {
    let needle = query.trim().to_lowercase();
    let mut result = SearchResult {
        rows: Vec::new(),
        next: None,
        cancelled: false,
    };
    if needle.is_empty() {
        return Ok(result);
    }
    let deadline = Instant::now() + budget;
    let sha_prefix = needle.len() >= 4 && needle.chars().all(|c| c.is_ascii_hexdigit());
    let repo = history.open_repo()?;

    for (row, id) in history.ids().iter().enumerate().skip(start) {
        if row > start && row % 64 == 0 {
            if generation.load(Ordering::Relaxed) != mine {
                result.cancelled = true;
                break;
            }
            if Instant::now() >= deadline {
                result.next = Some(row);
                break;
            }
        }
        let commit = repo.find_commit(*id)?;
        let hit = match field {
            SearchField::Text => text_matches(&commit, &needle, sha_prefix),
            SearchField::Path => path_matches(&repo, &commit, &needle)?,
        };
        if hit {
            result.rows.push(row);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    fn run(h: &History, q: &str, field: SearchField) -> Vec<usize> {
        let generation = AtomicU64::new(1);
        let r = search(h, q, field, 0, SLICE, &generation, 1).unwrap();
        assert_eq!(r.next, None);
        r.rows
    }

    #[test]
    fn finds_commits_by_message_author_sha_and_path() {
        let t = TempRepo::new();
        t.write("src/main.rs", "fn main() {}");
        let first = t.commit_all("Add entry point");
        t.write("README.md", "hi");
        t.commit_all("Write the README\n\nMentions the Raccoon mascot.");
        t.write("src/main.rs", "fn main() { run() }");
        t.commit_all("Call run");
        let h = History::load(t.path(), 1).unwrap();
        // Rows: 0 = "Call run", 1 = README, 2 = entry point.

        assert_eq!(run(&h, "readme", SearchField::Text), [1]);
        assert_eq!(
            run(&h, "MASCOT", SearchField::Text),
            [1],
            "body text, any case"
        );
        assert_eq!(
            run(&h, "raccoon@example", SearchField::Text),
            [0, 1, 2],
            "author email"
        );
        assert_eq!(run(&h, &first.to_string()[..7], SearchField::Text), [2]);
        assert_eq!(run(&h, "main.rs", SearchField::Path), [0, 2]);
        assert_eq!(run(&h, "SRC/", SearchField::Path), [0, 2]);
        assert!(run(&h, "nothing like this", SearchField::Text).is_empty());
        assert!(run(&h, "   ", SearchField::Text).is_empty());
    }

    #[test]
    fn continues_from_where_a_slice_ended() {
        let t = TempRepo::new();
        let mut parent = Vec::new();
        for i in 0..200 {
            parent = vec![t.commit(Some("HEAD"), &format!("commit {i}"), &parent)];
        }
        let h = History::load(t.path(), 1).unwrap();
        let generation = AtomicU64::new(1);
        // A zero budget returns at the first check (row 64) with partial results.
        let first = search(
            &h,
            "commit 1",
            SearchField::Text,
            0,
            Duration::ZERO,
            &generation,
            1,
        )
        .unwrap();
        assert_eq!(first.next, Some(64));
        let rest = search(&h, "commit 1", SearchField::Text, 64, SLICE, &generation, 1).unwrap();
        assert_eq!(rest.next, None);
        let all: Vec<usize> = first.rows.into_iter().chain(rest.rows).collect();
        // "commit 1", "commit 10".."commit 19", "commit 100".."commit 199".
        assert_eq!(all.len(), 111);
        assert!(all.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn stops_when_a_newer_search_starts() {
        let t = TempRepo::new();
        for i in 0..100 {
            t.write("a", i.to_string());
            t.commit_all("one");
        }
        let h = History::load(t.path(), 1).unwrap();
        let generation = AtomicU64::new(2); // a newer search than ours (1)
        let r = search(&h, "one", SearchField::Text, 0, SLICE, &generation, 1).unwrap();
        assert!(r.cancelled);
        assert_eq!(r.rows.len(), 64, "only the rows before the first check");
    }
}
