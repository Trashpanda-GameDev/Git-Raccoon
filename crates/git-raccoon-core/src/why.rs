//! Why a conflict happened: the commits on each side that changed its
//! lines since the two sides parted, and a plain hint when it's clear what
//! each side did.

use std::path::Path;

use git2::{BlameOptions, Oid, Repository};

use crate::Result;
use crate::conflict::{Chunk, Conflict, conflicts};
use crate::merge::{InProgress, State};

/// A commit that changed a conflict's lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub id: String,
    pub summary: String,
    /// The rest of its message, if any.
    pub body: String,
    pub author: String,
    /// When it was made (seconds since the epoch).
    pub time: i64,
    /// The pull/merge request it came in with, when its message says so
    /// ("… (#38)", "See merge request !38").
    pub request: Option<u64>,
}

/// Each side's commits for one conflict, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Why {
    pub ours: Vec<Change>,
    pub theirs: Vec<Change>,
    /// Whether "theirs" is the one commit being applied (a rebase, a
    /// cherry-pick or a revert), rather than a branch's commits.
    pub applying: bool,
    /// A short hint, when it's clear what happened.
    pub hint: Option<String>,
}

/// At most this many commits per side.
const MOST: usize = 10;

/// Why conflict `index` of `file` (whose text is `chunks`) happened.
pub fn why(path: &str, file: &str, state: &State, chunks: &[Chunk], index: usize) -> Result<Why> {
    let repo = Repository::discover(path)?;
    let id = |s: &crate::merge::Side| s.commit.as_deref().and_then(|c| Oid::from_str(c).ok());
    let ours = id(&state.ours).or_else(|| repo.head().ok()?.target());
    let theirs = id(&state.theirs);
    let applying = matches!(
        state.what,
        InProgress::Rebase | InProgress::CherryPick | InProgress::Revert
    );
    let all: Vec<&Conflict> = conflicts(chunks).collect();
    let Some(conflict) = all.get(index).copied() else {
        return Ok(Why::default());
    };
    let base = match (ours, theirs) {
        (Some(o), Some(t)) => repo.merge_base(o, t).ok(),
        _ => None,
    };
    let side = |tip: Option<Oid>, text: fn(&Conflict) -> &str| -> Vec<Change> {
        let Some(tip) = tip else { return Vec::new() };
        let lines = locate(&repo, tip, file, &all, index, text);
        let mut ids = lines
            .and_then(|range| blamed(&repo, tip, base, file, range))
            .unwrap_or_default();
        if ids.is_empty() {
            ids = touching(&repo, tip, base, file);
        }
        ids.into_iter()
            .take(MOST)
            .filter_map(|id| change(&repo, id))
            .collect()
    };
    let ours_changes = side(ours, |c| &c.ours);
    let theirs_changes = if applying {
        theirs.and_then(|t| change(&repo, t)).into_iter().collect()
    } else {
        side(theirs, |c| &c.theirs)
    };
    Ok(Why {
        hint: hint(conflict, &state.ours.name, &state.theirs.name),
        ours: ours_changes,
        theirs: theirs_changes,
        applying,
    })
}

/// The 1-based lines conflict `index`'s side (`text`) takes up in `file` at
/// commit `tip`: found by looking for each conflict's lines in turn.
fn locate(
    repo: &Repository,
    tip: Oid,
    file: &str,
    all: &[&Conflict],
    index: usize,
    text: fn(&Conflict) -> &str,
) -> Option<(usize, usize)> {
    let blob = repo
        .find_commit(tip)
        .ok()?
        .tree()
        .ok()?
        .get_path(Path::new(file))
        .ok()?
        .to_object(repo)
        .ok()?
        .peel_to_blob()
        .ok()?;
    let content = std::str::from_utf8(blob.content()).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let mut from = 0;
    let mut found = None;
    for c in all.iter().take(index + 1) {
        let want: Vec<&str> = text(c).lines().collect();
        if want.is_empty() {
            found = None;
            continue;
        }
        let at = (from..=lines.len().saturating_sub(want.len()))
            .find(|&i| lines.get(i..i + want.len()) == Some(&want[..]));
        match at {
            Some(i) => {
                found = Some((i + 1, i + want.len()));
                from = i + want.len();
            }
            None => found = None,
        }
    }
    found
}

/// The commits since `base` that last changed lines `range` of `file` at
/// `tip`, newest first.
fn blamed(
    repo: &Repository,
    tip: Oid,
    base: Option<Oid>,
    file: &str,
    (first, last): (usize, usize),
) -> Option<Vec<Oid>> {
    let mut options = BlameOptions::new();
    options.newest_commit(tip).min_line(first).max_line(last);
    if let Some(base) = base {
        options.oldest_commit(base);
    }
    let blame = repo.blame_file(Path::new(file), Some(&mut options)).ok()?;
    let mut ids: Vec<Oid> = Vec::new();
    for hunk in blame.iter() {
        let id = hunk.final_commit_id();
        if hunk.is_boundary() || Some(id) == base || ids.contains(&id) {
            continue;
        }
        ids.push(id);
    }
    // Newest first.
    ids.sort_by_key(|id| {
        std::cmp::Reverse(repo.find_commit(*id).map_or(0, |c| c.time().seconds()))
    });
    Some(ids)
}

/// The commits since `base` up to `tip` that changed `file`, newest first:
/// for lines a side removed, which blame can't point at.
fn touching(repo: &Repository, tip: Oid, base: Option<Oid>, file: &str) -> Vec<Oid> {
    let Ok(mut walk) = repo.revwalk() else {
        return Vec::new();
    };
    if walk.push(tip).is_err() {
        return Vec::new();
    }
    if let Some(base) = base {
        let _ = walk.hide(base);
    }
    let path = Path::new(file);
    let entry = |c: &git2::Commit| {
        c.tree()
            .ok()
            .and_then(|t| t.get_path(path).ok())
            .map(|e| e.id())
    };
    walk.flatten()
        .filter(|id| {
            let Ok(c) = repo.find_commit(*id) else {
                return false;
            };
            let here = entry(&c);
            match c.parents().next() {
                Some(p) => entry(&p) != here,
                None => here.is_some(),
            }
        })
        .take(MOST)
        .collect()
}

fn change(repo: &Repository, id: Oid) -> Option<Change> {
    let c = repo.find_commit(id).ok()?;
    let message = c.message().unwrap_or("").trim().to_string();
    let (summary, body) = match message.split_once('\n') {
        Some((s, b)) => (s.trim().to_string(), b.trim().to_string()),
        None => (message.clone(), String::new()),
    };
    Some(Change {
        id: id.to_string(),
        request: request_number(&message),
        summary,
        body,
        author: c.author().name().unwrap_or("").to_string(),
        time: c.time().seconds(),
    })
}

/// The request a commit message says it came in with: GitHub's "(#38)"
/// on squashed commits or "Merge pull request #38", GitLab's "See merge
/// request group/project!38".
fn request_number(message: &str) -> Option<u64> {
    let digits = |s: &str| -> Option<u64> {
        let n: String = s.chars().take_while(char::is_ascii_digit).collect();
        n.parse().ok()
    };
    let first = message.lines().next().unwrap_or("");
    if let Some(at) = first.rfind("(#") {
        return digits(&first[at + 2..]);
    }
    if let Some(rest) = first.strip_prefix("Merge pull request #") {
        return digits(rest);
    }
    let gitlab = message
        .lines()
        .find_map(|l| l.trim().strip_prefix("See merge request "))?;
    digits(&gitlab[gitlab.rfind('!')? + 1..])
}

/// A plain hint, when it's clear from the three versions what happened.
fn hint(c: &Conflict, ours: &str, theirs: &str) -> Option<String> {
    let base = c.base.as_deref()?;
    let (o, t) = (c.ours.as_str(), c.theirs.as_str());
    if o == base {
        return Some(format!(
            "Only {theirs} changed these lines. You likely want {theirs}'s version."
        ));
    }
    if t == base {
        return Some(format!(
            "Only {ours} changed these lines. You likely want {ours}'s version."
        ));
    }
    if o.trim().is_empty() {
        return Some(format!(
            "{ours} removed these lines, and {theirs} changed them. Keep {theirs}'s change only if it's still needed."
        ));
    }
    if t.trim().is_empty() {
        return Some(format!(
            "{theirs} removed these lines, and {ours} changed them. Keep {ours}'s change only if it's still needed."
        ));
    }
    let squash = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    if squash(o) == squash(base) {
        return Some(format!(
            "{ours} only changed spacing here. You likely want {theirs}'s version."
        ));
    }
    if squash(t) == squash(base) {
        return Some(format!(
            "{theirs} only changed spacing here. You likely want {ours}'s version."
        ));
    }
    // A rename on one side, of a name the other side still uses.
    if let Some((old, new)) = renamed(base, o).filter(|(old, _)| t.contains(old.as_str())) {
        return Some(format!(
            "{ours} renamed `{old}` to `{new}` here. You likely want {theirs}'s change, using `{new}`."
        ));
    }
    if let Some((old, new)) = renamed(base, t).filter(|(old, _)| o.contains(old.as_str())) {
        return Some(format!(
            "{theirs} renamed `{old}` to `{new}` here. You likely want {ours}'s change, using `{new}`."
        ));
    }
    let added = |side: &str| side.starts_with(base) || side.ends_with(base);
    match (added(o), added(t)) {
        (true, true) => Some(format!(
            "Both only added lines here. You likely want both: {ours}'s and {theirs}'s."
        )),
        (true, false) => Some(format!(
            "{ours} only added lines here, next to {theirs}'s change. You likely want both."
        )),
        (false, true) => Some(format!(
            "{theirs} only added lines here, next to {ours}'s change. You likely want both."
        )),
        (false, false) => None,
    }
}

/// When `after` is `before` with one name swapped for another everywhere,
/// the two names.
fn renamed(before: &str, after: &str) -> Option<(String, String)> {
    let words = |s: &str| -> Vec<String> {
        let mut out = Vec::new();
        let mut word = String::new();
        for ch in s.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                word.push(ch);
            } else {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
                if !ch.is_whitespace() {
                    out.push(ch.to_string());
                }
            }
        }
        if !word.is_empty() {
            out.push(word);
        }
        out
    };
    let (b, a) = (words(before), words(after));
    // Something around the name stays, to tell a rename from a rewrite.
    if b.len() != a.len() || b.len() < 3 {
        return None;
    }
    let mut pair: Option<(&String, &String)> = None;
    for (x, y) in b.iter().zip(&a) {
        if x == y {
            continue;
        }
        let name = |w: &str| {
            w.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
        };
        if !name(x) || !name(y) {
            return None;
        }
        match pair {
            None => pair = Some((x, y)),
            Some(p) if p == (x, y) => {}
            Some(_) => return None,
        }
    }
    pair.map(|(x, y)| (x.clone(), y.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict::{parse, read, with_bases};
    use crate::merge::tests::git;
    use crate::merge::{MergeMode, in_progress, merge};
    use crate::testutil::TempRepo;

    fn conflict(base: Option<&str>, ours: &str, theirs: &str) -> Conflict {
        Conflict {
            ours_label: "HEAD".into(),
            ours: ours.into(),
            base: base.map(str::to_string),
            theirs_label: "x".into(),
            theirs: theirs.into(),
        }
    }

    #[test]
    fn hints_at_what_happened() {
        let h = |b, o, t| hint(&conflict(Some(b), o, t), "main", "login");
        assert_eq!(
            h(
                "let s = Store::new();\n",
                "let s = SessionStore::new();\n",
                "let s = Store::new();\nlast_user();\n"
            )
            .as_deref(),
            Some(
                "main renamed `Store` to `SessionStore` here. You likely want login's change, using `SessionStore`."
            )
        );
        assert!(
            h("a\n", "a\nb\n", "c\n")
                .unwrap()
                .starts_with("main only added lines")
        );
        assert!(
            h("a\n", "", "c\n")
                .unwrap()
                .starts_with("main removed these lines")
        );
        assert!(
            h("a  b\n", "a b\n", "c\n")
                .unwrap()
                .starts_with("main only changed spacing")
        );
        assert_eq!(h("a\n", "b\n", "c\n"), None);
        assert_eq!(hint(&conflict(None, "b\n", "c\n"), "main", "login"), None);
    }

    #[test]
    fn reads_request_numbers_from_messages() {
        assert_eq!(request_number("Session store refactor (#38)"), Some(38));
        assert_eq!(
            request_number("Merge pull request #7 from a/b\n\nx"),
            Some(7)
        );
        assert_eq!(
            request_number("Refactor\n\nSee merge request group/app!12"),
            Some(12)
        );
        assert_eq!(request_number("Plain"), None);
    }

    #[test]
    fn finds_the_commits_behind_each_side() {
        let t = TempRepo::new();
        git(&t, &["config", "user.name", "Ada"]);
        git(&t, &["config", "user.email", "ada@example.com"]);
        t.write("f.txt", "one\nlet s = Store::new();\nthree\n");
        t.write("other.txt", "x\n");
        git(&t, &["add", "."]);
        git(&t, &["commit", "-q", "-m", "base"]);
        let main = t.repo.head().unwrap().shorthand().unwrap().to_string();
        git(&t, &["switch", "-q", "-c", "login"]);
        t.write("f.txt", "one\nlet s = Store::new();\nlast_user();\nthree\n");
        git(&t, &["commit", "-q", "-am", "Remember the last user"]);
        git(&t, &["switch", "-q", &main]);
        t.write("f.txt", "one\nlet s = SessionStore::new();\nthree\n");
        git(
            &t,
            &[
                "commit",
                "-q",
                "-am",
                "Rename session store (#38)\n\nIt takes the whole config.",
            ],
        );
        // Not about these lines.
        t.write("other.txt", "y\n");
        git(&t, &["commit", "-q", "-am", "Unrelated"]);
        merge(t.path(), "login", MergeMode::Default).unwrap();
        let state = in_progress(t.path()).unwrap().unwrap();
        let mut chunks = parse(&read(t.path(), "f.txt").unwrap()).unwrap();
        with_bases(t.path(), "f.txt", &mut chunks);
        let w = why(t.path(), "f.txt", &state, &chunks, 0).unwrap();
        let summaries = |c: &[Change]| c.iter().map(|c| c.summary.clone()).collect::<Vec<_>>();
        assert_eq!(summaries(&w.ours), ["Rename session store (#38)"]);
        assert_eq!(w.ours[0].request, Some(38));
        assert_eq!(w.ours[0].body, "It takes the whole config.");
        assert_eq!(w.ours[0].author, "Ada");
        assert_eq!(summaries(&w.theirs), ["Remember the last user"]);
        assert!(!w.applying);
        assert!(
            w.hint
                .unwrap()
                .contains("renamed `Store` to `SessionStore`")
        );
    }
}
