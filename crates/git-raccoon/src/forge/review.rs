//! Everything about one pull/merge request: its files, commits,
//! conversation and threads, whether it can be merged, and the review
//! being written for it.

use serde::{Deserialize, Serialize};

/// Whether it's still open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Open,
    Merged,
    Closed,
}

/// A comment by someone.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Comment {
    /// The forge's id for it (GitHub's number, GitLab's note id).
    pub id: String,
    pub author: String,
    pub body: String,
    /// Seconds since the Unix epoch.
    pub created: i64,
}

/// A line in a diff: its number in the old file, the new one, or both
/// (an unchanged line).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Line {
    pub old: Option<u32>,
    pub new: Option<u32>,
}

impl Line {
    /// Whether `other` is the same line, as far as either side goes.
    pub fn same(&self, other: &Line) -> bool {
        match ((self.new, other.new), (self.old, other.old)) {
            ((Some(a), Some(b)), _) | (_, (Some(a), Some(b))) => a == b,
            _ => false,
        }
    }
}

/// Where a comment is: a file, a line in its diff, and maybe the first
/// line of a range ending there.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Anchor {
    pub path: String,
    /// Its name before the change, if it was renamed.
    pub old_path: Option<String>,
    pub line: Line,
    pub start: Option<Line>,
}

/// A comment thread on the code (or, on GitLab, a threaded comment on the
/// whole request: `at` is `None` then).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Thread {
    /// The forge's id for it (GitHub's node id, GitLab's discussion id).
    pub id: String,
    pub at: Option<Anchor>,
    pub resolved: bool,
    /// It can be resolved (GitLab only lets some be).
    pub resolvable: bool,
    /// The code moved on since: it's not on a line of the diff any more.
    pub outdated: bool,
    /// The code it's about, as the forge kept it (GitHub), if it did.
    pub hunk: String,
    /// What a reply goes to (GitHub: its first comment; GitLab: itself).
    pub reply: String,
    pub comments: Vec<Comment>,
}

impl Thread {
    pub fn created(&self) -> i64 {
        self.comments.first().map_or(0, |c| c.created)
    }
}

/// What a reviewer said overall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Commented,
    Approved,
    ChangesRequested,
}

/// Something in a request's conversation, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Comment(Comment),
    Review {
        author: String,
        verdict: Verdict,
        body: String,
        created: i64,
    },
    /// A thread, by its place in [`Overview::threads`].
    Thread(usize),
}

impl Event {
    fn created(&self, threads: &[Thread]) -> i64 {
        match self {
            Event::Comment(c) => c.created,
            Event::Review { created, .. } => *created,
            Event::Thread(i) => threads.get(*i).map_or(0, Thread::created),
        }
    }
}

/// What happened to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    Deleted,
    Modified,
    Renamed,
}

/// A changed file, with its patch (unified diff hunks, "@@ …" first).
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub path: String,
    pub old_path: Option<String>,
    pub change: Change,
    /// Empty when the forge left it out (binary, or too big).
    pub patch: String,
}

/// One of its commits.
#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    pub sha: String,
    pub summary: String,
    pub author: String,
    pub created: i64,
}

/// A check (CI job) and how it went.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub name: String,
    pub outcome: Outcome,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed,
    Running,
}

/// A way of merging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Merge,
    Squash,
    Rebase,
}

impl Method {
    pub fn name(self) -> &'static str {
        match self {
            Method::Merge => "Merge commit",
            Method::Squash => "Squash and merge",
            Method::Rebase => "Rebase and merge",
        }
    }

    pub fn explain(self) -> &'static str {
        match self {
            Method::Merge => "All its commits, joined to the target by a merge commit.",
            Method::Squash => "Its commits combined into one, on top of the target.",
            Method::Rebase => {
                "Its commits replayed one by one on top of the target, no merge commit."
            }
        }
    }
}

/// Whether, and how, it can be merged.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mergeable {
    /// What the repository allows (the usual one first).
    pub methods: Vec<Method>,
    /// Why it can't be merged yet, if it can't.
    pub blocked: Option<String>,
    /// Its branch can be deleted after (it's not from a fork).
    pub can_delete_branch: bool,
}

/// GitLab's diff refs, which a comment on a line needs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffRefs {
    pub base: String,
    pub start: String,
    pub head: String,
}

/// Everything shown in a request's view.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Overview {
    pub state: State,
    pub files: Vec<File>,
    pub commits: Vec<Commit>,
    pub threads: Vec<Thread>,
    /// The conversation, oldest first.
    pub events: Vec<Event>,
    pub checks: Vec<Check>,
    pub merge: Mergeable,
    pub refs: DiffRefs,
}

impl Overview {
    /// Puts the conversation in order.
    pub fn sort(&mut self) {
        let threads = std::mem::take(&mut self.threads);
        self.events.sort_by_key(|e| e.created(&threads));
        self.threads = threads;
    }

    /// The threads not resolved yet.
    pub fn unresolved(&self) -> usize {
        self.threads
            .iter()
            .filter(|t| t.resolvable && !t.resolved)
            .count()
    }

    /// A file's threads that are still on its diff.
    pub fn threads_on<'a>(&'a self, path: &'a str) -> impl Iterator<Item = (usize, &'a Thread)> {
        self.threads
            .iter()
            .enumerate()
            .filter(move |(_, t)| !t.outdated && t.at.as_ref().is_some_and(|a| a.path == path))
    }
}

/// A comment not sent yet: on a line or range, a reply to a thread, or on
/// the whole request.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Draft {
    pub at: Option<Anchor>,
    /// The thread it answers (its id).
    pub reply_to: Option<String>,
    pub body: String,
}

/// How a review ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    Comment,
    Approve,
    RequestChanges,
}

/// Merging, as asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merge {
    pub method: Method,
    /// The commit message: its first line is the title.
    pub message: String,
    pub delete_branch: bool,
}

/// A line of a patch, numbered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchLine {
    pub kind: char,
    pub line: Line,
    pub text: String,
}

/// Numbers a patch's lines (skipping "@@" headers, which come back with
/// kind '@').
pub fn number(patch: &str) -> Vec<PatchLine> {
    let mut out = Vec::new();
    let (mut old, mut new) = (0u32, 0u32);
    for text in patch.lines() {
        if let Some(rest) = text.strip_prefix("@@") {
            // "@@ -12,3 +14,4 @@ fn x"
            let mut parts = rest.split_whitespace();
            let start = |p: Option<&str>, sign: char| {
                p.and_then(|p| p.strip_prefix(sign))
                    .and_then(|p| p.split(',').next())
                    .and_then(|n| n.parse::<u32>().ok())
                    .unwrap_or(1)
            };
            old = start(parts.next(), '-');
            new = start(parts.next(), '+');
            out.push(PatchLine {
                kind: '@',
                line: Line::default(),
                text: text.to_string(),
            });
            continue;
        }
        if text.starts_with('\\') {
            continue; // "\ No newline at end of file"
        }
        let (kind, rest) = match text.chars().next() {
            Some('+') => ('+', &text[1..]),
            Some('-') => ('-', &text[1..]),
            Some(' ') => (' ', &text[1..]),
            _ => (' ', text),
        };
        let line = match kind {
            '+' => {
                new += 1;
                Line {
                    old: None,
                    new: Some(new - 1),
                }
            }
            '-' => {
                old += 1;
                Line {
                    old: Some(old - 1),
                    new: None,
                }
            }
            _ => {
                old += 1;
                new += 1;
                Line {
                    old: Some(old - 1),
                    new: Some(new - 1),
                }
            }
        };
        out.push(PatchLine {
            kind,
            line,
            text: rest.to_string(),
        });
    }
    out
}

/// The lines of a patch a thread is about (a few before its last line).
pub fn excerpt(patch: &str, at: &Anchor) -> Vec<PatchLine> {
    let lines = number(patch);
    let Some(end) = lines
        .iter()
        .position(|l| l.kind != '@' && l.line.same(&at.line))
    else {
        return Vec::new();
    };
    let start = at
        .start
        .and_then(|s| lines.iter().position(|l| l.kind != '@' && l.line.same(&s)))
        .filter(|s| *s <= end)
        .unwrap_or(end.saturating_sub(3));
    lines[start..=end]
        .iter()
        .filter(|l| l.kind != '@')
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_patch_lines() {
        let lines = number("@@ -3,3 +3,4 @@ fn a\n keep\n-old\n+new\n+more\n keep2\n\\ No newline");
        let numbers: Vec<_> = lines
            .iter()
            .map(|l| (l.kind, l.line.old, l.line.new))
            .collect();
        assert_eq!(
            numbers,
            [
                ('@', None, None),
                (' ', Some(3), Some(3)),
                ('-', Some(4), None),
                ('+', None, Some(4)),
                ('+', None, Some(5)),
                (' ', Some(5), Some(6)),
            ]
        );
        let at = Anchor {
            path: "a".into(),
            line: Line {
                old: None,
                new: Some(5),
            },
            start: Some(Line {
                old: Some(4),
                new: None,
            }),
            ..Anchor::default()
        };
        let texts: Vec<_> = excerpt("@@ -3,3 +3,4 @@\n keep\n-old\n+new\n+more\n", &at)
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(texts, ["old", "new", "more"]);
    }
}
