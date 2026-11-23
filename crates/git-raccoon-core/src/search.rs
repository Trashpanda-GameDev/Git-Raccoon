//! Searching the whole history, not just the rows the UI has loaded.
//!
//! Searches run in short slices so the UI can show matches as they are
//! found and stop asking once a newer search replaces this one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use git2::{Commit, Repository};

use crate::Result;
use crate::repo::History;

/// How long one search call runs before returning partial results.
pub const SLICE: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchField {
    /// Commit message, author name or email, or SHA prefix.
    Text,
    /// Paths of the files a commit changed (compared with its first parent).
    Path,
}

/// What a search looks for: words typed as they are, and filters like
/// `author:ada` or `after:2026-01-01`. A commit has to match all of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Words with no filter, as typed (lowercase): looked for in the
    /// message, author and SHA, or in the paths when searching paths.
    pub text: Option<String>,
    pub text_is_path: bool,
    pub sha: Vec<String>,
    pub author: Vec<String>,
    pub committer: Vec<String>,
    pub message: Vec<String>,
    pub path: Vec<String>,
    /// Authored at or after this time (seconds since the Unix epoch).
    pub after: Option<i64>,
    /// Authored before this time.
    pub before: Option<i64>,
}

/// The filters, with what they look in, for the search box's list.
pub const FILTERS: &[(&str, &str)] = &[
    ("author:", "Who wrote it (name or email)"),
    ("committer:", "Who committed it (name or email)"),
    ("message:", "Words in its message"),
    ("sha:", "The start of its SHA"),
    ("path:", "A file it changed (part of the path)"),
    ("after:", "Written on or after a date"),
    ("before:", "Written before a date"),
    ("date:", "Written on a day, in a month or a year"),
];

impl Query {
    /// Reads `query` ("fix author:ada after:2026-01"), with dates in the
    /// local time zone, relative ones ("2w", "yesterday") counted from now.
    pub fn parse(query: &str, field: SearchField) -> std::result::Result<Query, String> {
        Query::parse_at(query, field, &chrono::Local::now())
    }

    pub fn parse_at<Tz: chrono::TimeZone>(
        query: &str,
        field: SearchField,
        now: &chrono::DateTime<Tz>,
    ) -> std::result::Result<Query, String> {
        let mut q = Query {
            text_is_path: field == SearchField::Path,
            ..Query::default()
        };
        let mut words = Vec::new();
        for word in split(query) {
            let Some((key, value)) = word.split_once(':').filter(|(k, _)| is_filter(k)) else {
                words.push(word);
                continue;
            };
            let value = value.trim();
            if value.is_empty() {
                continue; // Still being typed.
            }
            let lower = value.to_lowercase();
            match key.to_lowercase().as_str() {
                "author" => q.author.push(lower),
                "committer" => q.committer.push(lower),
                "message" | "msg" => q.message.push(lower),
                "sha" | "hash" => q.sha.push(lower),
                "path" | "file" => q.path.push(lower),
                "after" | "since" => {
                    let (from, _) = period(value, now)?;
                    q.after = Some(q.after.map_or(from, |a| a.max(from)));
                }
                "before" | "until" => {
                    let (from, _) = period(value, now)?;
                    q.before = Some(q.before.map_or(from, |b| b.min(from)));
                }
                _ => {
                    // "date": within that day, month or year.
                    let (from, to) = period(value, now)?;
                    q.after = Some(q.after.map_or(from, |a| a.max(from)));
                    q.before = Some(q.before.map_or(to, |b| b.min(to)));
                }
            }
        }
        let text = words.join(" ").trim().to_lowercase();
        q.text = (!text.is_empty()).then_some(text);
        Ok(q)
    }

    /// Nothing to look for (yet).
    pub fn is_empty(&self) -> bool {
        *self
            == Query {
                text_is_path: self.text_is_path,
                ..Query::default()
            }
    }

    fn needs_paths(&self) -> bool {
        !self.path.is_empty() || (self.text_is_path && self.text.is_some())
    }
}

impl Query {
    /// Whether a commit in a file's history matches: the filters as in the
    /// graph, and the words in its message, author, SHA or the names of
    /// the files it changed.
    pub fn matches_change(&self, c: &crate::file_history::Change) -> bool {
        let has = |s: &str, needle: &str| s.to_lowercase().contains(needle);
        let person =
            |name: &str, email: &str, needle: &str| has(name, needle) || has(email, needle);
        let message = |needle: &str| has(&c.summary, needle) || has(&c.body, needle);
        let sha = |prefix: &str| c.id.starts_with(prefix);
        let file = |needle: &str| c.files.iter().chain([&c.path]).any(|f| has(f, needle));
        if self.after.is_some_and(|a| c.time < a) || self.before.is_some_and(|b| c.time >= b) {
            return false;
        }
        let text = self.text.as_deref().is_none_or(|t| {
            let looks_like_sha = t.len() >= 4 && t.chars().all(|ch| ch.is_ascii_hexdigit());
            (looks_like_sha && sha(t)) || message(t) || person(&c.author, &c.email, t) || file(t)
        });
        text && self.sha.iter().all(|p| sha(p))
            && self.message.iter().all(|m| message(m))
            && self.author.iter().all(|a| person(&c.author, &c.email, a))
            && self
                .committer
                .iter()
                .all(|m| person(&c.committer, &c.committer_email, m))
            && self.path.iter().all(|p| file(p))
    }
}

fn is_filter(key: &str) -> bool {
    matches!(
        key.to_lowercase().as_str(),
        "author"
            | "committer"
            | "message"
            | "msg"
            | "sha"
            | "hash"
            | "path"
            | "file"
            | "after"
            | "since"
            | "before"
            | "until"
            | "date"
    )
}

/// Splits on spaces, keeping "quoted words" (or `author:"Ada L"`) whole,
/// without their quotes.
fn split(query: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for c in query.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The span of time a date names, as seconds since the Unix epoch, from
/// its start up to (not including) its end: "2026-11-23" a day, "2026-11"
/// a month, "2026" a year; "today", "yesterday"; or "3d", "2w", "6m",
/// "1y" ago (from that day to now).
fn period<Tz: chrono::TimeZone>(
    value: &str,
    now: &chrono::DateTime<Tz>,
) -> std::result::Result<(i64, i64), String> {
    use chrono::{Datelike, Days, Months, NaiveDate};
    let tz = now.timezone();
    let today = now.date_naive();
    let start = |d: NaiveDate| -> i64 {
        let midnight = d.and_hms_opt(0, 0, 0).unwrap_or_default();
        tz.from_local_datetime(&midnight)
            .earliest()
            .map_or_else(|| midnight.and_utc().timestamp(), |t| t.timestamp())
    };
    let unknown = || {
        format!(
            "Can't read the date \u{201c}{value}\u{201d}: try 2026-11-23, 2026-11, 2026, yesterday or 2w"
        )
    };
    let v = value.to_lowercase();
    let day = |d: NaiveDate| (start(d), start(d + Days::new(1)));
    match v.as_str() {
        "today" => return Ok(day(today)),
        "yesterday" => return Ok(day(today - Days::new(1))),
        _ => {}
    }
    // "3d", "2w", "6m", "1y": from that many ago until now.
    if let Some(unit) = v.chars().last().filter(|c| "dwmy".contains(*c))
        && let Ok(n) = v[..v.len() - 1].parse::<u32>()
    {
        let from = match unit {
            'd' => today.checked_sub_days(Days::new(n.into())),
            'w' => today.checked_sub_days(Days::new(u64::from(n) * 7)),
            'm' => today.checked_sub_months(Months::new(n)),
            _ => today.checked_sub_months(Months::new(n.saturating_mul(12))),
        }
        .ok_or_else(unknown)?;
        return Ok((start(from), i64::MAX));
    }
    let parts: Vec<&str> = v.split('-').collect();
    let number = |s: &str| s.parse::<u32>().map_err(|_| unknown());
    match parts.as_slice() {
        [y, m, d] => {
            let d = NaiveDate::from_ymd_opt(
                i32::try_from(number(y)?).map_err(|_| unknown())?,
                number(m)?,
                number(d)?,
            )
            .ok_or_else(unknown)?;
            Ok(day(d))
        }
        [y, m] => {
            let first = NaiveDate::from_ymd_opt(
                i32::try_from(number(y)?).map_err(|_| unknown())?,
                number(m)?,
                1,
            )
            .ok_or_else(unknown)?;
            let next = first + Months::new(1);
            Ok((start(first), start(next)))
        }
        [y] if y.len() == 4 => {
            let first =
                NaiveDate::from_ymd_opt(i32::try_from(number(y)?).map_err(|_| unknown())?, 1, 1)
                    .ok_or_else(unknown)?;
            Ok((
                start(first),
                start(first.with_year(first.year() + 1).ok_or_else(unknown)?),
            ))
        }
        _ => Err(unknown()),
    }
}

/// Matches among rows `start .. next` of the history.
#[derive(Debug)]
pub struct SearchResult {
    /// Rows of the matching commits, in history order.
    pub rows: Vec<usize>,
    /// Where to continue, or `None` once the whole history is searched.
    pub next: Option<usize>,
    /// A newer search started, so this one stopped early.
    pub cancelled: bool,
}

fn person_matches(sig: &git2::Signature<'_>, needle: &str) -> bool {
    let contains = |s: Option<&str>| s.is_some_and(|s| s.to_lowercase().contains(needle));
    contains(sig.name()) || contains(sig.email())
}

fn message_matches(commit: &Commit, needle: &str) -> bool {
    commit
        .message()
        .is_some_and(|m| m.to_lowercase().contains(needle))
}

fn sha_matches(commit: &Commit, prefix: &str) -> bool {
    commit.id().to_string().starts_with(prefix)
}

fn text_matches(commit: &Commit, needle: &str) -> bool {
    let sha_prefix = needle.len() >= 4 && needle.chars().all(|c| c.is_ascii_hexdigit());
    (sha_prefix && sha_matches(commit, needle))
        || message_matches(commit, needle)
        || person_matches(&commit.author(), needle)
}

/// The paths a commit changed (compared with its first parent), lowercase.
fn changed_paths(repo: &Repository, commit: &Commit) -> Result<Vec<String>> {
    let new_tree = commit.tree()?;
    let old_tree = match commit.parent_count() {
        0 => None,
        _ => Some(commit.parent(0)?.tree()?),
    };
    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)?;
    Ok(diff
        .deltas()
        .flat_map(|d| [d.new_file().path(), d.old_file().path()])
        .flatten()
        .map(|p| p.to_string_lossy().to_lowercase())
        .collect())
}

fn matches(repo: &Repository, commit: &Commit, q: &Query) -> Result<bool> {
    // The cheap checks first; the changed files only when they're asked for.
    let time = commit.author().when().seconds();
    if q.after.is_some_and(|a| time < a) || q.before.is_some_and(|b| time >= b) {
        return Ok(false);
    }
    if !q.sha.iter().all(|s| sha_matches(commit, s))
        || !q.message.iter().all(|m| message_matches(commit, m))
        || !q.author.iter().all(|a| person_matches(&commit.author(), a))
        || !q
            .committer
            .iter()
            .all(|c| person_matches(&commit.committer(), c))
    {
        return Ok(false);
    }
    if let Some(text) = &q.text
        && !q.text_is_path
        && !text_matches(commit, text)
    {
        return Ok(false);
    }
    if q.needs_paths() {
        let paths = changed_paths(repo, commit)?;
        let has = |needle: &str| paths.iter().any(|p| p.contains(needle));
        let text = q.text.as_deref().filter(|_| q.text_is_path);
        if !q.path.iter().map(String::as_str).chain(text).all(has) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Finds commits matching `query`, starting at row `start` and returning
/// after about `budget` with where to continue. Stops early when
/// `generation` no longer holds `mine` (a newer search started).
pub fn search(
    history: &History,
    query: &Query,
    start: usize,
    budget: Duration,
    generation: &AtomicU64,
    mine: u64,
) -> Result<SearchResult> {
    let mut result = SearchResult {
        rows: Vec::new(),
        next: None,
        cancelled: false,
    };
    if query.is_empty() {
        return Ok(result);
    }
    let deadline = Instant::now() + budget;
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
        if matches(&repo, &commit, query)? {
            result.rows.push(row);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    fn query(q: &str, field: SearchField) -> Query {
        Query::parse(q, field).unwrap()
    }

    fn run(h: &History, q: &str, field: SearchField) -> Vec<usize> {
        let generation = AtomicU64::new(1);
        let r = search(h, &query(q, field), 0, SLICE, &generation, 1).unwrap();
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
            &query("commit 1", SearchField::Text),
            0,
            Duration::ZERO,
            &generation,
            1,
        )
        .unwrap();
        assert_eq!(first.next, Some(64));
        let q = query("commit 1", SearchField::Text);
        let rest = search(&h, &q, 64, SLICE, &generation, 1).unwrap();
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
        let q = query("one", SearchField::Text);
        let r = search(&h, &q, 0, SLICE, &generation, 1).unwrap();
        assert!(r.cancelled);
        assert_eq!(r.rows.len(), 64, "only the rows before the first check");
    }

    /// A commit by `author`, committed by `committer`, at `time`.
    fn commit_by(
        t: &TempRepo,
        file: &str,
        message: &str,
        who: (&str, &str),
        time: i64,
    ) -> git2::Oid {
        t.write(file, message);
        let mut index = t.repo.index().unwrap();
        index.add_path(std::path::Path::new(file)).unwrap();
        index.write().unwrap();
        let tree = t.repo.find_tree(index.write_tree().unwrap()).unwrap();
        let when = git2::Time::new(time, 0);
        let author = git2::Signature::new(
            who.0,
            &format!("{}@example.com", who.0.to_lowercase()),
            &when,
        )
        .unwrap();
        let committer = git2::Signature::new(
            who.1,
            &format!("{}@example.com", who.1.to_lowercase()),
            &when,
        )
        .unwrap();
        let parents: Vec<git2::Commit<'_>> = t
            .repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .into_iter()
            .collect();
        let parents: Vec<&git2::Commit<'_>> = parents.iter().collect();
        t.repo
            .commit(Some("HEAD"), &author, &committer, message, &tree, &parents)
            .unwrap()
    }

    #[test]
    fn filters_by_who_when_what_and_where() {
        let t = TempRepo::new();
        // 2026-01-15, 2026-03-02 and 2026-11-20, at noon UTC.
        let first = commit_by(
            &t,
            "src/a.rs",
            "Add the loader",
            ("Ada", "Ada"),
            1_768_478_400,
        );
        commit_by(
            &t,
            "docs/guide.md",
            "Write the guide",
            ("Bo", "Ada"),
            1_772_452_800,
        );
        commit_by(
            &t,
            "src/b.rs",
            "Fix the loader",
            ("Ada", "Cy"),
            1_795_176_000,
        );
        let h = History::load(t.path(), 1).unwrap();
        // Rows: 0 = Fix (Nov), 1 = guide (Mar), 2 = Add (Jan).
        let text = |q: &str| run(&h, q, SearchField::Text);

        assert_eq!(text("author:ada"), [0, 2]);
        assert_eq!(text("committer:ada"), [1, 2]);
        assert_eq!(text("author:ada committer:cy"), [0]);
        assert_eq!(text("loader author:ada"), [0, 2], "words and a filter");
        assert_eq!(text("message:fix"), [0]);
        assert_eq!(text(&format!("sha:{}", &first.to_string()[..6])), [2]);
        assert_eq!(text("path:src/"), [0, 2]);
        assert_eq!(text("path:src/ message:add"), [2]);
        assert_eq!(text("after:2026-03-01"), [0, 1]);
        assert_eq!(text("before:2026-03-01"), [2]);
        assert_eq!(text("after:2026-02 before:2026-04"), [1]);
        assert_eq!(text("date:2026-11"), [0]);
        assert_eq!(text("date:2026-01-15"), [2]);
        assert_eq!(text("date:2026"), [0, 1, 2]);
        assert_eq!(text(r#"author:"ad" "the guide""#), Vec::<usize>::new());
        assert_eq!(text(r#""the guide""#), [1], "a quoted phrase");
        // A filter still being typed is left out.
        assert_eq!(text("author:"), Vec::<usize>::new());
        assert_eq!(text("author: loader"), [0, 2]);
        // Searching paths: the words are a path; filters still apply.
        assert_eq!(run(&h, "src author:ada", SearchField::Path), [0, 2]);
        assert_eq!(run(&h, "guide", SearchField::Path), [1]);
    }

    #[test]
    fn reads_dates() {
        use chrono::TimeZone;
        let now = chrono::Utc
            .with_ymd_and_hms(2026, 11, 23, 15, 0, 0)
            .unwrap();
        let q = |s: &str| Query::parse_at(s, SearchField::Text, &now);
        let day = 86_400;
        let nov_23 = now.timestamp() - 15 * 3600;
        assert_eq!(q("date:today").unwrap().after, Some(nov_23));
        assert_eq!(q("date:today").unwrap().before, Some(nov_23 + day));
        assert_eq!(q("after:yesterday").unwrap().after, Some(nov_23 - day));
        assert_eq!(q("after:3d").unwrap().after, Some(nov_23 - 3 * day));
        assert_eq!(q("after:2w").unwrap().after, Some(nov_23 - 14 * day));
        assert_eq!(q("since:1m").unwrap().after, Some(nov_23 - 31 * day));
        assert_eq!(q("after:1y").unwrap().after, Some(nov_23 - 365 * day));
        assert_eq!(q("after:3d").unwrap().before, None);
        // The later of two "after"s, the earlier of two "before"s.
        let both = q("after:2026-01 after:2026-03 before:2026-10 until:2026-06").unwrap();
        assert!(both.after.unwrap() > q("after:2026-02").unwrap().after.unwrap());
        assert_eq!(both.before, q("before:2026-06").unwrap().before);
        for bad in ["after:soon", "before:2026-13", "date:2026-02-30", "date:26"] {
            let e = q(bad).unwrap_err();
            assert!(e.starts_with("Can't read the date"), "{bad}: {e}");
        }
        assert!(q("").unwrap().is_empty());
        assert!(q("author:").unwrap().is_empty());
        assert!(!q("fix").unwrap().is_empty());
        // Words that only look like filters are words.
        assert_eq!(q("re: fix").unwrap().text.as_deref(), Some("re: fix"));
    }

    #[test]
    fn searches_a_files_history() {
        let t = TempRepo::new();
        commit_by(
            &t,
            "src/a.rs",
            "Add the loader",
            ("Ada", "Ada"),
            1_768_478_400,
        );
        t.write("docs/notes.md", "n");
        let mut index = t.repo.index().unwrap();
        index
            .add_path(std::path::Path::new("docs/notes.md"))
            .unwrap();
        index.write().unwrap();
        commit_by(
            &t,
            "src/a.rs",
            "Speed up the loader\n\nCaches the index.",
            ("Bo", "Cy"),
            1_772_452_800,
        );
        commit_by(&t, "src/a.rs", "Tidy", ("Ada", "Ada"), 1_795_176_000);
        let all = crate::file_history::history(t.path(), "src/a.rs").unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[1].committer, "Cy");
        assert_eq!(all[1].body, "Caches the index.");
        assert_eq!(all[1].files, ["docs/notes.md", "src/a.rs"]);
        let found = |q: &str| -> Vec<String> {
            let q = Query::parse(q, SearchField::Text).unwrap();
            all.iter()
                .filter(|c| q.matches_change(c))
                .map(|c| c.summary.clone())
                .collect()
        };
        assert_eq!(found("author:ada"), ["Tidy", "Add the loader"]);
        assert_eq!(found("committer:cy"), ["Speed up the loader"]);
        assert_eq!(found("caches"), ["Speed up the loader"], "the body");
        assert_eq!(found("message:index"), ["Speed up the loader"]);
        assert_eq!(found("path:notes"), ["Speed up the loader"], "another file");
        assert_eq!(found("notes.md"), ["Speed up the loader"]);
        assert_eq!(
            found(&format!("sha:{}", &all[2].id[..7])),
            ["Add the loader"]
        );
        assert_eq!(found("before:2026-03-01"), ["Add the loader"]);
        assert_eq!(found("date:2026-11 author:ada"), ["Tidy"]);
        assert_eq!(found("loader after:2026-02"), ["Speed up the loader"]);
    }
}
