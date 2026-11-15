//! A file's history (following renames) and blame, through the user's own
//! `git`, so their settings (such as `blame.ignoreRevsFile`) apply.

use crate::{Result, cli};

/// A commit that changed the file.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub id: String,
    pub summary: String,
    pub author: String,
    /// Seconds since the Unix epoch.
    pub time: i64,
    /// The file's path in that commit (it may have been renamed since).
    pub path: String,
}

/// At most this many commits are listed.
pub const MAX_HISTORY: usize = 5000;

/// The commits that changed `file`, newest first, following renames.
pub fn history(path: &str, file: &str) -> Result<Vec<Change>> {
    let max = format!("--max-count={MAX_HISTORY}");
    let out = cli::run(
        path,
        &[
            "log",
            "--follow",
            &max,
            "--format=%x1e%H%x1f%an%x1f%at%x1f%s",
            "--name-only",
            "--",
            file,
        ],
        None,
    )?;
    let mut changes = Vec::new();
    for record in out.split('\x1e').map(str::trim).filter(|r| !r.is_empty()) {
        let mut lines = record.lines();
        let header = lines.next().unwrap_or_default();
        let fields: Vec<&str> = header.split('\x1f').collect();
        let [id, author, time, summary] = fields[..] else {
            continue;
        };
        // The path it had then: the last name listed (after a rename, the new one).
        let at = lines
            .map(str::trim)
            .rfind(|l| !l.is_empty())
            .unwrap_or(file);
        changes.push(Change {
            id: id.to_string(),
            summary: summary.to_string(),
            author: author.to_string(),
            time: time.parse().unwrap_or(0),
            path: at.to_string(),
        });
    }
    Ok(changes)
}

/// One line of a blamed file.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    /// The commit that last changed it; `None` for uncommitted lines.
    pub id: Option<String>,
    /// Index into [`Blame::commits`].
    pub commit: usize,
    /// Whether this line starts a run from the same commit.
    pub starts_run: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlameCommit {
    pub id: Option<String>,
    pub author: String,
    pub time: i64,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Blame {
    pub lines: Vec<Line>,
    pub commits: Vec<BlameCommit>,
}

/// Who last changed each line of `file`: as of commit `at`, or as it is on
/// disk now if `None`.
pub fn blame(path: &str, file: &str, at: Option<&str>) -> Result<Blame> {
    let mut args = vec!["blame", "--porcelain"];
    if let Some(at) = at {
        args.push(at);
    }
    args.extend(["--", file]);
    let out = cli::run(path, &args, None)?;
    Ok(parse_porcelain(&out))
}

fn parse_porcelain(out: &str) -> Blame {
    let mut blame = Blame::default();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut current: Option<usize> = None;
    let mut pending: Option<BlameCommit> = None;
    let mut pending_sha = String::new();
    let mut previous: Option<usize> = None;
    for line in out.split('\n') {
        if let Some(text) = line.strip_prefix('\t') {
            // The line itself; its commit's details came just before.
            if let Some(c) = pending.take() {
                blame.commits.push(c);
                index.insert(pending_sha.clone(), blame.commits.len() - 1);
                current = Some(blame.commits.len() - 1);
            }
            let commit = current.unwrap_or(0);
            blame.lines.push(Line {
                text: text.to_string(),
                id: blame.commits.get(commit).and_then(|c| c.id.clone()),
                commit,
                starts_run: previous != Some(commit),
            });
            previous = Some(commit);
            continue;
        }
        let mut parts = line.split(' ');
        let first = parts.next().unwrap_or_default();
        if first.len() == 40 && first.chars().all(|c| c.is_ascii_hexdigit()) {
            if let Some(&i) = index.get(first) {
                current = Some(i);
                pending = None;
            } else {
                let uncommitted = first.chars().all(|c| c == '0');
                pending_sha = first.to_string();
                pending = Some(BlameCommit {
                    id: (!uncommitted).then(|| first.to_string()),
                    author: String::new(),
                    time: 0,
                    summary: String::new(),
                });
            }
            continue;
        }
        if let Some(c) = &mut pending {
            let rest = line.split_once(' ').map_or("", |(_, r)| r);
            match first {
                "author" => c.author = rest.to_string(),
                "author-time" => c.time = rest.parse().unwrap_or(0),
                "summary" => c.summary = rest.to_string(),
                _ => {}
            }
        }
    }
    blame
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::tests::{git, identity};
    use crate::testutil::TempRepo;

    fn repo() -> TempRepo {
        let t = TempRepo::new();
        identity(&t);
        t.write("a.txt", "one\ntwo\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "Add a"]);
        t.write("other.txt", "x\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "Unrelated"]);
        t.write("a.txt", "one\nTWO\nthree\n");
        git(&t, &["commit", "-q", "-am", "Change a"]);
        git(&t, &["mv", "a.txt", "b.txt"]);
        git(&t, &["commit", "-q", "-m", "Rename a to b"]);
        t
    }

    #[test]
    fn lists_changes_following_renames() {
        let t = repo();
        let h = history(t.path(), "b.txt").unwrap();
        let summaries: Vec<&str> = h.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(summaries, ["Rename a to b", "Change a", "Add a"]);
        assert_eq!(h[0].path, "b.txt");
        assert_eq!(h[1].path, "a.txt", "the name it had then");
        assert_eq!(h[2].author, "Rocky Raccoon");
        assert!(h[2].time > 0);
    }

    #[test]
    fn blames_lines_at_a_commit_and_on_disk() {
        let t = repo();
        let b = blame(t.path(), "b.txt", None).unwrap();
        let who: Vec<(&str, &str)> = b
            .lines
            .iter()
            .map(|l| (l.text.as_str(), b.commits[l.commit].summary.as_str()))
            .collect();
        assert_eq!(
            who,
            [("one", "Add a"), ("TWO", "Change a"), ("three", "Change a")]
        );
        assert!(b.lines[1].starts_run && !b.lines[2].starts_run);
        assert_eq!(b.commits.len(), 2, "each commit once");

        // An uncommitted edit is nobody's yet.
        t.write("b.txt", "one\nTWO\nthree\nfour\n");
        let b = blame(t.path(), "b.txt", None).unwrap();
        assert_eq!(b.lines[3].id, None);

        // As of the first commit.
        let first = history(t.path(), "b.txt").unwrap().pop().unwrap();
        let b = blame(t.path(), "a.txt", Some(&first.id)).unwrap();
        assert_eq!(b.lines.len(), 2);
        assert!(
            b.lines
                .iter()
                .all(|l| l.id.as_deref() == Some(first.id.as_str()))
        );
    }
}
