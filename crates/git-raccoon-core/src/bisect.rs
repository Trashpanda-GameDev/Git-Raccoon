//! Finding the commit that broke something (Git's bisect), through the
//! user's own `git bisect`: a search started in a terminal and one started
//! in the app are the same search.
//!
//! In the app's words a commit is "broken" or "works" (Git's "bad" and
//! "good"), or other words the person chose (Git's `--term-new` and
//! `--term-old`).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use git2::Repository;

use crate::{Error, Result, cli};

/// The two words for a commit: the new behaviour (Git's "bad": broken)
/// and the old one (Git's "good": works).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terms {
    /// Git's word for the new behaviour ("bad", or the person's own).
    pub new: String,
    /// Git's word for the old behaviour ("good", or the person's own).
    pub old: String,
}

impl Default for Terms {
    fn default() -> Self {
        Terms {
            new: "bad".into(),
            old: "good".into(),
        }
    }
}

impl Terms {
    /// Git's own words, shown as "broken" and "works".
    pub fn is_default(&self) -> bool {
        (self.new == "bad" && self.old == "good") || (self.new == "new" && self.old == "old")
    }
}

/// An answer for a commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The new behaviour (broken).
    New,
    /// The old behaviour (works).
    Old,
    /// It can't be tested.
    Skip,
}

/// One answer given, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub id: String,
    pub verdict: Verdict,
}

/// How a search ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The first commit with the new behaviour.
    Found(String),
    /// Skipped commits leave it uncertain: one of these.
    OneOf(Vec<String>),
}

/// A search in progress, as Git has it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Search {
    pub terms: Terms,
    /// Where it started (a branch's name, or a commit), to go back to.
    pub started_on: String,
    /// Only following first parents (`--first-parent`).
    pub first_parent: bool,
    /// The answers so far, in order.
    pub answers: Vec<Answer>,
    /// The newest commit marked broken, and those marked working.
    pub broken: Option<String>,
    pub works: Vec<String>,
    pub skipped: Vec<String>,
    /// The commit checked out to test.
    pub testing: Option<String>,
    /// The commits still in question (newest first), the broken one too.
    pub range: Vec<String>,
    /// About how many steps are left.
    pub steps: usize,
    /// Found (or narrowed down as far as skips allow).
    pub outcome: Option<Outcome>,
}

impl Search {
    /// How many commits are still in question, the broken one aside.
    pub fn left(&self) -> usize {
        self.range.len().saturating_sub(1)
    }
}

fn git_dir(path: &str) -> Result<PathBuf> {
    Ok(Repository::discover(path)?.path().to_path_buf())
}

fn read(dir: &std::path::Path, name: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(name))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Whether a search is going on.
pub fn is_searching(path: &str) -> bool {
    git_dir(path).is_ok_and(|d| d.join("BISECT_START").exists())
}

/// Starts a search: `broken` has the new behaviour, `works` the old (at
/// least one). Optionally following first parents only, and with the
/// person's own words. Git checks out the first commit to test.
pub fn start(
    path: &str,
    broken: &str,
    works: &[String],
    first_parent: bool,
    terms: Option<&Terms>,
) -> Result<Search> {
    let mut args = vec!["bisect".to_string(), "start".to_string()];
    if first_parent {
        args.push("--first-parent".into());
    }
    if let Some(t) = terms.filter(|t| !t.is_default()) {
        args.push(format!("--term-new={}", t.new));
        args.push(format!("--term-old={}", t.old));
    }
    args.push(broken.to_string());
    args.extend(works.iter().cloned());
    args.push("--".into());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    cli::run(path, &args, None)?;
    read_search(path)?.ok_or_else(|| Error::Command("The search didn't start".into()))
}

/// Answers for the commit checked out.
pub fn answer(path: &str, verdict: Verdict) -> Result<Search> {
    let search =
        read_search(path)?.ok_or_else(|| Error::Command("No search is going on".into()))?;
    let term = match verdict {
        Verdict::New => search.terms.new.as_str(),
        Verdict::Old => search.terms.old.as_str(),
        Verdict::Skip => "skip",
    };
    // Git says it can't go on when only skipped commits are left: that's
    // an answer too (one of them, or the broken one).
    let said = cli::run(path, &["bisect", term], None);
    let search = read_search(path)?.ok_or_else(|| Error::Command("The search ended".into()))?;
    match said {
        Err(e) if search.outcome.is_none() => Err(e),
        _ => Ok(search),
    }
}

/// Takes back the last answer: Git's log of the search, replayed without it.
pub fn undo(path: &str) -> Result<Search> {
    let log = cli::run(path, &["bisect", "log"], None)?;
    let mut lines: Vec<&str> = log.lines().collect();
    // The last answer: the last "git bisect <term> …" line that isn't the start.
    let Some(last) = lines
        .iter()
        .rposition(|l| l.starts_with("git bisect ") && !l.starts_with("git bisect start"))
    else {
        return read_search(path)?.ok_or_else(|| Error::Command("No search is going on".into()));
    };
    // Its comment line just before goes too ("# good: [id] summary").
    let from = if last > 0 && lines[last - 1].starts_with('#') {
        last - 1
    } else {
        last
    };
    // Answers given at the start (with `start`) are in its line: those stay.
    lines.truncate(from);
    let file = git_dir(path)?.join("GIT_RACCOON_BISECT_REPLAY");
    std::fs::write(&file, format!("{}\n", lines.join("\n")))
        .map_err(|e| Error::Command(format!("Couldn't write the search's log: {e}")))?;
    let replayed = cli::run(path, &["bisect", "replay", &file.to_string_lossy()], None);
    let _ = std::fs::remove_file(&file);
    replayed?;
    read_search(path)?.ok_or_else(|| Error::Command("The search ended".into()))
}

/// Ends the search, back where it started.
pub fn stop(path: &str) -> Result<()> {
    cli::run(path, &["bisect", "reset"], None).map(drop)
}

/// What a search would cover, before it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// The commits in question, the broken one aside.
    pub commits: usize,
    /// About how many answers it takes.
    pub steps: usize,
}

/// The commits a search from `broken` back to `works` would go through,
/// or why it can't: `works` has to be older, on `broken`'s history.
pub fn preview(path: &str, broken: &str, works: &str, first_parent: bool) -> Result<Preview> {
    let repo = Repository::discover(path)?;
    let resolve =
        |r: &str| -> Result<git2::Oid> { Ok(repo.revparse_single(r)?.peel_to_commit()?.id()) };
    let (new, old) = (resolve(broken)?, resolve(works)?);
    if new == old {
        return Err(Error::Command(
            "That's the same commit: pick one from before it broke".into(),
        ));
    }
    if !repo.graph_descendant_of(new, old)? {
        return Err(Error::Command(
            "That commit isn't in the broken one's history: pick one it came from".into(),
        ));
    }
    let mut args = vec!["rev-list", "--count"];
    if first_parent {
        args.push("--first-parent");
    }
    let (new, old) = (new.to_string(), format!("^{old}"));
    args.push(&new);
    args.push(&old);
    let count: usize = cli::run(path, &args, None)?.trim().parse().unwrap_or(0);
    let commits = count.saturating_sub(1);
    Ok(Preview {
        commits,
        steps: steps_for(commits),
    })
}

/// Tags in `broken`'s history (not on it), newest first: where it last
/// worked, most likely.
pub fn tags_below(path: &str, broken: &str) -> Result<Vec<(String, String)>> {
    let out = cli::run(
        path,
        &[
            "tag",
            "--merged",
            broken,
            "--sort=-creatordate",
            "--format=%(refname:short)%09%(objectname)%09%(*objectname)",
        ],
        None,
    )?;
    let repo = Repository::discover(path)?;
    let at = repo
        .revparse_single(broken)?
        .peel_to_commit()?
        .id()
        .to_string();
    Ok(out
        .lines()
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let name = parts.next()?.to_string();
            let direct = parts.next().unwrap_or_default();
            // An annotated tag's commit is the one it points at.
            let commit = parts
                .next()
                .filter(|p| !p.is_empty())
                .unwrap_or(direct)
                .to_string();
            (commit != at).then_some((name, commit))
        })
        .collect())
}

/// The name of the stash uncommitted changes are put aside in while a
/// search goes on, to find them again at the end (even after a restart).
pub const PUT_ASIDE: &str = "Put aside by Git Raccoon while finding the commit that broke it";

/// Puts uncommitted changes (untracked files too) aside, in a stash named
/// [`PUT_ASIDE`]. Returns whether there were any.
pub fn put_aside(path: &str) -> Result<bool> {
    let before = crate::stash::top(path)?;
    crate::stash::save(path, PUT_ASIDE, true, false)?;
    Ok(crate::stash::top(path)? != before)
}

/// Brings back what [`put_aside`] put aside, if it's there. Returns
/// whether it was.
pub fn bring_back(path: &str) -> Result<bool> {
    let mut repo = Repository::discover(path)?;
    let mut found = None;
    repo.stash_foreach(|_, message, id| {
        if message.contains(PUT_ASIDE) {
            found = Some(id.to_string());
            false
        } else {
            true
        }
    })?;
    drop(repo);
    match found {
        Some(id) => crate::stash::pop(path, &id).map(|()| true),
        None => Ok(false),
    }
}

/// A search that ended: what it found, how, and that it's back where it
/// started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finished {
    pub outcome: Outcome,
    pub answers: Vec<Answer>,
    pub terms: Terms,
    /// Where it went back to (a branch's name, or a commit).
    pub back_on: String,
    /// What was put aside came back.
    pub restored: bool,
}

/// Ends a search that found its answer: back where it started, with what
/// was put aside brought back.
pub fn finish(path: &str, search: &Search) -> Result<Option<Finished>> {
    let Some(outcome) = search.outcome.clone() else {
        return Ok(None);
    };
    stop(path)?;
    let restored = bring_back(path)?;
    Ok(Some(Finished {
        outcome,
        answers: search.answers.clone(),
        terms: search.terms.clone(),
        back_on: search.started_on.clone(),
        restored,
    }))
}

/// The search going on, if any, as Git has it in the repository.
pub fn read_search(path: &str) -> Result<Option<Search>> {
    let dir = git_dir(path)?;
    let Some(started_on) = read(&dir, "BISECT_START") else {
        return Ok(None);
    };
    let terms = read(&dir, "BISECT_TERMS")
        .and_then(|t| {
            let mut lines = t.lines();
            Some(Terms {
                new: lines.next()?.trim().to_string(),
                old: lines.next()?.trim().to_string(),
            })
        })
        .unwrap_or_default();
    let first_parent = dir.join("BISECT_FIRST_PARENT").exists();
    let repo = Repository::discover(path)?;
    // The marks: refs/bisect/<new>, refs/bisect/<old>-<id>, refs/bisect/skip-<id>.
    let mut broken = None;
    let mut works = Vec::new();
    let mut skipped = Vec::new();
    for r in repo.references_glob("refs/bisect/*")?.flatten() {
        let (Some(name), Some(id)) = (r.name(), r.target()) else {
            continue;
        };
        let name = name.trim_start_matches("refs/bisect/");
        let id = id.to_string();
        if name == terms.new {
            broken = Some(id);
        } else if name.starts_with(&format!("{}-", terms.old)) {
            works.push(id);
        } else if name.starts_with("skip-") {
            skipped.push(id);
        }
    }
    let answers = log_answers(&dir, &terms);
    let testing = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .map(|id| id.to_string());
    let mut search = Search {
        terms,
        started_on,
        first_parent,
        answers,
        broken,
        works,
        skipped,
        testing,
        ..Search::default()
    };
    if let Some(broken) = search.broken.clone()
        && !search.works.is_empty()
    {
        let mut args = vec!["rev-list".to_string()];
        if search.first_parent {
            args.push("--first-parent".into());
        }
        args.push(broken.clone());
        args.extend(search.works.iter().map(|w| format!("^{w}")));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let range: Vec<String> = cli::run(path, &args, None)?
            .lines()
            .map(str::to_string)
            .collect();
        let untested: Vec<&String> = range
            .iter()
            .filter(|c| **c != broken && !search.skipped.contains(c))
            .collect();
        search.steps = steps_for(untested.len());
        if untested.is_empty() {
            let maybe: Vec<String> = range
                .iter()
                .filter(|c| **c == broken || search.skipped.contains(c))
                .cloned()
                .collect();
            search.outcome = Some(if maybe.len() <= 1 {
                Outcome::Found(broken.clone())
            } else {
                Outcome::OneOf(maybe)
            });
        }
        search.range = range;
    }
    Ok(Some(search))
}

/// About how many more answers `n` commits in question take (halving).
fn steps_for(n: usize) -> usize {
    let mut steps = 0;
    let mut left = n;
    while left > 0 {
        left /= 2;
        steps += 1;
    }
    steps
}

/// The answers in Git's log, in order: "git bisect <term> <id>…" lines and
/// the start's own ("git bisect start '<new>' '<old>'…").
fn log_answers(dir: &std::path::Path, terms: &Terms) -> Vec<Answer> {
    let Some(log) = read(dir, "BISECT_LOG") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in log.lines() {
        let Some(rest) = line.strip_prefix("git bisect ") else {
            continue;
        };
        let words: Vec<&str> = rest
            .split_whitespace()
            .map(|w| w.trim_matches('\''))
            .collect();
        let Some((&what, ids)) = words.split_first() else {
            continue;
        };
        if what == "start" {
            // `start` with ids: the first is the new behaviour, the rest the
            // old (options and the "--" aside). Its comment lines say the
            // same, which is what's read below for answers given later.
            continue;
        }
        let verdict = if what == terms.new {
            Verdict::New
        } else if what == terms.old {
            Verdict::Old
        } else if what == "skip" {
            Verdict::Skip
        } else {
            continue;
        };
        for id in ids {
            out.push(Answer {
                id: (*id).to_string(),
                verdict,
            });
        }
    }
    // The start's answers come as comments before it: "# bad: [id] …".
    let mut start = Vec::new();
    for line in log.lines() {
        if line.starts_with("git bisect start") {
            break;
        }
        let Some(rest) = line.strip_prefix("# ") else {
            continue;
        };
        let Some((term, tail)) = rest.split_once(": [") else {
            continue;
        };
        let Some((id, _)) = tail.split_once(']') else {
            continue;
        };
        let verdict = if term == terms.new {
            Verdict::New
        } else if term == terms.old {
            Verdict::Old
        } else {
            continue;
        };
        start.push(Answer {
            id: id.to_string(),
            verdict,
        });
    }
    start.extend(out);
    start
}

/// What a command said about one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// Its exit code (none when it was stopped or timed out).
    pub code: Option<i32>,
    pub verdict: Option<Verdict>,
    pub took: Duration,
    pub timed_out: bool,
}

/// What an exit code says, as `git bisect run` reads it: 0 works, 125
/// can't be tested, anything else broken.
pub fn verdict_of(code: i32) -> Verdict {
    match code {
        0 => Verdict::Old,
        125 => Verdict::Skip,
        _ => Verdict::New,
    }
}

/// Runs `command` (through the user's shell) in the repository's folder,
/// sending its output (both streams) to `output` as it comes. Stops it when
/// `stop` is set, or after `limit`.
pub fn run_command(
    path: &str,
    command: &str,
    limit: Option<Duration>,
    stop: &std::sync::atomic::AtomicBool,
    output: &mut dyn FnMut(&str),
) -> Result<Run> {
    use std::io::Read as _;
    use std::sync::atomic::Ordering;
    let root = Repository::discover(path)?
        .workdir()
        .map_or_else(|| PathBuf::from(path), std::path::Path::to_path_buf);
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".into());
        let mut c = std::process::Command::new(shell);
        c.arg("-c").arg(command);
        c
    };
    cmd.current_dir(&root)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        cmd.creation_flags(0x0800_0000);
    }
    let started = Instant::now();
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Command(format!("Couldn't run {command}: {e}")))?;
    // Both streams, read as they come, into one channel.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut readers = Vec::new();
    let streams: [Option<Box<dyn std::io::Read + Send>>; 2] = [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ];
    for mut stream in streams.into_iter().flatten() {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let _ = tx.send(String::from_utf8_lossy(&buf[..n]).into_owned());
            }
        }));
    }
    drop(tx);
    let mut timed_out = false;
    let status = loop {
        while let Ok(text) = rx.try_recv() {
            output(&text);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| Error::Command(format!("{command} didn't finish: {e}")))?
        {
            break Some(status);
        }
        let over = limit.is_some_and(|l| started.elapsed() >= l);
        if stop.load(Ordering::Relaxed) || over {
            timed_out = over;
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    for r in readers {
        let _ = r.join();
    }
    while let Ok(text) = rx.try_recv() {
        output(&text);
    }
    let code = status.and_then(|s| s.code());
    Ok(Run {
        code,
        verdict: code.map(verdict_of),
        took: started.elapsed(),
        timed_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    /// Ten commits, the fifth (index 4) breaking "ok" into "broken".
    fn history() -> (TempRepo, Vec<String>) {
        let t = TempRepo::new();
        let mut ids = Vec::new();
        for i in 0..10 {
            t.write("state", if i < 4 { "ok" } else { "broken" });
            t.write("n", i.to_string());
            ids.push(t.commit_all(&format!("commit {i}")).to_string());
        }
        (t, ids)
    }

    fn is_broken(t: &TempRepo) -> bool {
        std::fs::read_to_string(std::path::Path::new(t.path()).join("state")).unwrap() == "broken"
    }

    /// Answers until it's found, as a person testing would.
    fn answer_all(t: &TempRepo) -> Search {
        for _ in 0..20 {
            let s = read_search(t.path()).unwrap().unwrap();
            if s.outcome.is_some() {
                return s;
            }
            let v = if is_broken(t) {
                Verdict::New
            } else {
                Verdict::Old
            };
            answer(t.path(), v).unwrap();
        }
        panic!("never found");
    }

    #[test]
    fn finds_the_commit_that_broke_it() {
        let (t, ids) = history();
        assert!(!is_searching(t.path()));
        let s = start(t.path(), &ids[9], &[ids[0].clone()], false, None).unwrap();
        assert!(is_searching(t.path()));
        assert_eq!(s.broken.as_deref(), Some(ids[9].as_str()));
        assert_eq!(s.works, [ids[0].clone()]);
        assert_eq!(s.range.len(), 9, "commits 1 to 9");
        assert_eq!(s.left(), 8);
        assert!(s.steps >= 3);
        assert!(s.testing.is_some());
        assert_eq!(
            s.answers.iter().map(|a| a.verdict).collect::<Vec<_>>(),
            [Verdict::New, Verdict::Old],
            "the start's two answers"
        );
        let found = answer_all(&t);
        assert_eq!(found.outcome, Some(Outcome::Found(ids[4].clone())));
        stop(t.path()).unwrap();
        assert!(!is_searching(t.path()));
        assert_eq!(
            t.repo.head().unwrap().shorthand(),
            t.repo.head().unwrap().shorthand(),
        );
        assert!(t.repo.head().unwrap().is_branch(), "back on the branch");
    }

    #[test]
    fn takes_back_the_last_answer() {
        let (t, ids) = history();
        start(t.path(), &ids[9], &[ids[0].clone()], false, None).unwrap();
        let before = read_search(t.path()).unwrap().unwrap();
        let after = answer(t.path(), Verdict::Skip).unwrap();
        assert_eq!(after.answers.len(), before.answers.len() + 1);
        assert_eq!(after.skipped.len(), 1);
        let undone = undo(t.path()).unwrap();
        assert_eq!(undone.answers, before.answers);
        assert!(undone.skipped.is_empty());
        assert_eq!(undone.testing, before.testing, "the same commit to test");
        // Nothing to take back past the start.
        let again = undo(t.path()).unwrap();
        assert_eq!(again.answers.len(), 2);
        assert_eq!(answer_all(&t).outcome, Some(Outcome::Found(ids[4].clone())));
        stop(t.path()).unwrap();
    }

    #[test]
    fn skips_can_leave_it_uncertain() {
        let (t, ids) = history();
        start(t.path(), &ids[5], &[ids[3].clone()], false, None).unwrap();
        // Two in question (4 and 5); 4 can't be tested.
        let s = read_search(t.path()).unwrap().unwrap();
        assert_eq!(s.testing.as_deref(), Some(ids[4].as_str()));
        let s = answer(t.path(), Verdict::Skip).unwrap();
        match s.outcome {
            Some(Outcome::OneOf(ids_left)) => {
                assert_eq!(ids_left.len(), 2);
                assert!(ids_left.contains(&ids[4]) && ids_left.contains(&ids[5]));
            }
            other => panic!("{other:?}"),
        }
        stop(t.path()).unwrap();
    }

    #[test]
    fn uses_the_persons_own_words_and_first_parents() {
        let (t, ids) = history();
        let terms = Terms {
            new: "fast".into(),
            old: "slow".into(),
        };
        let s = start(t.path(), &ids[9], &[ids[0].clone()], true, Some(&terms)).unwrap();
        assert_eq!(s.terms, terms);
        assert!(s.first_parent);
        // Answering uses them.
        let s = answer(t.path(), Verdict::New).unwrap();
        assert_eq!(s.answers.last().unwrap().verdict, Verdict::New);
        stop(t.path()).unwrap();
    }

    #[test]
    fn picks_up_a_search_started_in_a_terminal() {
        let (t, ids) = history();
        cli::run(t.path(), &["bisect", "start", &ids[9], &ids[0]], None).unwrap();
        cli::run(t.path(), &["bisect", "good"], None).ok();
        let s = read_search(t.path()).unwrap().unwrap();
        assert_eq!(s.answers.len(), 3);
        assert_eq!(s.terms, Terms::default());
        stop(t.path()).unwrap();
    }

    #[test]
    fn puts_changes_aside_and_brings_them_back() {
        let (t, ids) = history();
        t.write("notes.txt", "mine");
        assert!(put_aside(t.path()).unwrap());
        assert!(!std::path::Path::new(t.path()).join("notes.txt").exists());
        start(t.path(), &ids[9], &[ids[0].clone()], false, None).unwrap();
        let done = super::finish(t.path(), &answer_all(&t)).unwrap().unwrap();
        assert_eq!(done.outcome, Outcome::Found(ids[4].clone()));
        assert!(done.restored);
        assert!(!is_searching(t.path()));
        assert_eq!(
            std::fs::read_to_string(std::path::Path::new(t.path()).join("notes.txt")).unwrap(),
            "mine"
        );
        assert!(!bring_back(t.path()).unwrap(), "nothing left to bring back");
    }

    #[test]
    fn previews_the_range_and_suggests_tags() {
        let (t, ids) = history();
        cli::run(t.path(), &["tag", "v1", &ids[2]], None).unwrap();
        cli::run(t.path(), &["tag", "-a", "v2", "-m", "two", &ids[6]], None).unwrap();
        cli::run(t.path(), &["tag", "tip", &ids[9]], None).unwrap();
        let p = preview(t.path(), &ids[9], &ids[0], false).unwrap();
        assert_eq!(
            p,
            Preview {
                commits: 8,
                steps: 4
            }
        );
        assert!(preview(t.path(), &ids[0], &ids[9], false).is_err(), "newer");
        assert!(
            preview(t.path(), &ids[3], &ids[3], false).is_err(),
            "the same"
        );
        let tags = tags_below(t.path(), &ids[9]).unwrap();
        let names: Vec<&str> = tags.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"v1") && names.contains(&"v2"));
        assert!(!names.contains(&"tip"), "not the broken commit's own");
        let v2 = tags.iter().find(|(n, _)| n == "v2").unwrap();
        assert_eq!(v2.1, ids[6], "an annotated tag's commit");
    }

    #[test]
    fn runs_a_command_and_reads_its_exit() {
        let (t, _) = history();
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut out = String::new();
        let cmd = if cfg!(windows) {
            "echo hi & exit 3"
        } else {
            "echo hi; exit 3"
        };
        let r = run_command(t.path(), cmd, None, &stop, &mut |s| out.push_str(s)).unwrap();
        assert_eq!(r.code, Some(3));
        assert_eq!(r.verdict, Some(Verdict::New));
        assert!(out.contains("hi"));
        assert_eq!(verdict_of(0), Verdict::Old);
        assert_eq!(verdict_of(125), Verdict::Skip);
        if !cfg!(windows) {
            let r = run_command(
                t.path(),
                "sleep 5",
                Some(Duration::from_millis(200)),
                &stop,
                &mut |_| {},
            )
            .unwrap();
            assert!(r.timed_out && r.code.is_none());
        }
        assert_eq!(steps_for(8), 4);
        assert_eq!(steps_for(1), 1);
        assert_eq!(steps_for(0), 0);
    }
}
