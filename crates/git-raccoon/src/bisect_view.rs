//! Finding the commit that broke something, in a repository's view: the
//! search going on (read from Git, so one started in a terminal shows
//! too), answering by hand or with a command, and the answer.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use git_raccoon_core::bisect::{self, Finished, Outcome, Search, Terms, Verdict};
use git_raccoon_core::details::CommitDetails;

use crate::task::Task;

/// The words for the two sides, as the person sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    /// "broken", or the person's own word for the new behaviour.
    pub new: String,
    /// "works", or theirs for the old one.
    pub old: String,
}

impl Words {
    pub fn of(terms: &Terms) -> Words {
        if terms.is_default() {
            Words {
                new: "broken".into(),
                old: "works".into(),
            }
        } else {
            Words {
                new: terms.new.clone(),
                old: terms.old.clone(),
            }
        }
    }

    pub fn is_default(&self) -> bool {
        self.new == "broken" && self.old == "works"
    }

    /// "Finding the commit that broke it", or "…that's first fast".
    pub fn title(&self) -> String {
        if self.is_default() {
            "Finding the commit that broke it".into()
        } else {
            format!("Finding the first {} commit", self.new)
        }
    }

    /// "It's broken" / "It works", or "It's fast" / "It's slow".
    pub fn button(&self, v: Verdict) -> String {
        match (v, self.is_default()) {
            (Verdict::New, true) => "It's broken".into(),
            (Verdict::Old, true) => "It works".into(),
            (Verdict::New, false) => format!("It's {}", self.new),
            (Verdict::Old, false) => format!("It's {}", self.old),
            (Verdict::Skip, _) => "Skip this one".into(),
        }
    }

    pub fn verdict(&self, v: Verdict) -> &str {
        match v {
            Verdict::New => &self.new,
            Verdict::Old => &self.old,
            Verdict::Skip => "skipped",
        }
    }
}

/// The mark an answer gets in the graph and the lists.
pub fn mark(v: Verdict) -> &'static str {
    use egui_phosphor::regular as icon;
    match v {
        Verdict::New => icon::X,
        Verdict::Old => icon::CHECK,
        Verdict::Skip => icon::MINUS,
    }
}

/// The mark on the commit being tested.
pub const TESTING: &str = egui_phosphor::regular::CARET_RIGHT;

/// What the start window has chosen.
#[derive(Debug, Clone)]
pub struct Start {
    /// The broken commit, and how it's named ("main · Show the avatar").
    pub broken: String,
    pub broken_label: String,
    /// Where it last worked: a tag, branch or commit id, as typed or picked.
    pub works: String,
    /// Tags below the broken commit, to pick from.
    pub tags: Vec<(String, String)>,
    pub first_parent: bool,
    /// What's being looked for: 0 when it broke, 1 when it changed, 2 the
    /// person's own words.
    pub looking_for: usize,
    pub own_new: String,
    pub own_old: String,
    /// Test with this command instead of by hand.
    pub command: String,
    /// Stop a step that runs longer than this many minutes (empty: never).
    pub limit_minutes: String,
    /// The range, worked out for the commit chosen (or why it won't do).
    pub preview: Option<(String, bool, Result<bisect::Preview, String>)>,
    pub dirty: bool,
    pub error: Option<String>,
}

impl Start {
    pub fn terms(&self) -> Option<Terms> {
        match self.looking_for {
            0 => None,
            1 => Some(Terms {
                new: "new".into(),
                old: "old".into(),
            }),
            _ => Some(Terms {
                new: own_term(&self.own_new, "new"),
                old: own_term(&self.own_old, "old"),
            }),
        }
    }

    /// The words it'll use.
    pub fn words(&self) -> Words {
        match self.looking_for {
            0 => Words::of(&Terms::default()),
            1 => Words {
                new: "new behaviour".into(),
                old: "old behaviour".into(),
            },
            _ => Words {
                new: own_term(&self.own_new, "new"),
                old: own_term(&self.own_old, "old"),
            },
        }
    }
}

/// A word Git takes as a term: one word, lower case, not one of its own.
fn own_term(typed: &str, fallback: &str) -> String {
    let word: String = typed
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let reserved = [
        "help",
        "start",
        "skip",
        "next",
        "reset",
        "visualize",
        "view",
        "replay",
        "log",
        "run",
        "terms",
    ];
    if word.is_empty() || reserved.contains(&word.as_str()) {
        fallback.to_string()
    } else {
        word
    }
}

/// How a search starts: the broken commit, where it worked, and how.
#[derive(Debug, Clone, Default)]
pub struct Begin {
    pub broken: String,
    pub works: Vec<String>,
    pub first_parent: bool,
    pub terms: Option<Terms>,
    /// Put uncommitted changes aside first.
    pub dirty: bool,
    /// Test each commit with this, instead of by hand.
    pub command: Option<String>,
    pub limit: Option<Duration>,
}

/// One commit a command was run on.
#[derive(Debug, Clone)]
pub struct Step {
    pub id: String,
    pub summary: String,
    pub verdict: Option<Verdict>,
    pub code: Option<i32>,
    pub took: Duration,
    pub timed_out: bool,
    pub output: String,
}

/// A command testing each commit.
#[derive(Default)]
pub struct RunLog {
    pub command: String,
    pub steps: Vec<Step>,
    /// The commit being tested now, since when, and its output so far.
    pub current: Option<(String, String, Instant)>,
    pub output: String,
    pub done: bool,
    pub error: Option<String>,
}

pub struct Running {
    pub log: Arc<Mutex<RunLog>>,
    stop: Arc<AtomicBool>,
    task: Task<Result<Option<Box<Found>>, String>>,
}

impl Running {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

/// The answer, with the commit's details to show.
#[derive(Debug)]
pub struct Found {
    pub finished: Finished,
    /// The first broken commit's details (or, uncertain, each candidate's).
    pub details: Vec<CommitDetails>,
    /// Each answer's commit summary, by id.
    pub summaries: HashMap<String, String>,
}

impl Found {
    pub fn words(&self) -> Words {
        Words::of(&self.finished.terms)
    }

    /// "Found it in 4 steps" counts the answers after the start's two.
    pub fn steps(&self) -> usize {
        let at_start = self
            .finished
            .answers
            .iter()
            .take_while(|a| a.verdict != Verdict::Skip)
            .take(2)
            .count();
        self.finished.answers.len().saturating_sub(at_start).max(1)
    }

    /// A summary for an issue: the commit, the range and the answers.
    pub fn summary(&self) -> String {
        let words = self.words();
        let mut out = String::new();
        match &self.finished.outcome {
            Outcome::Found(id) => {
                let title = self.summaries.get(id).cloned().unwrap_or_default();
                out.push_str(&format!(
                    "First {} commit: {} {title}\n",
                    words.new,
                    &id[..id.len().min(10)]
                ));
            }
            Outcome::OneOf(ids) => {
                out.push_str(&format!(
                    "The first {} commit is one of (some couldn't be tested):\n",
                    words.new
                ));
                for id in ids {
                    let title = self.summaries.get(id).cloned().unwrap_or_default();
                    out.push_str(&format!("- {} {title}\n", &id[..id.len().min(10)]));
                }
            }
        }
        out.push_str("\nAnswers, in order:\n");
        for a in &self.finished.answers {
            let title = self.summaries.get(&a.id).cloned().unwrap_or_default();
            out.push_str(&format!(
                "- {} {} {title} ({})\n",
                mark(a.verdict),
                &a.id[..a.id.len().min(10)],
                words.verdict(a.verdict)
            ));
        }
        out
    }
}

/// What a step of work comes back with.
enum Done {
    Search(Option<Box<Search>>),
    Found(Box<Found>),
}

/// The range worked out for a start window's choice: the commit, first
/// parents only, and the range or why it won't do.
type Checked = (String, bool, Result<bisect::Preview, String>);

/// Starting the search, run first on the command's own thread.
type Starting = Box<dyn FnOnce() -> Result<Search, String> + Send>;

/// The search in a repository.
#[derive(Default)]
pub struct Bisect {
    /// The search going on, as last read from Git.
    pub search: Option<Search>,
    /// The start window, while it's open.
    pub starting: Option<Start>,
    /// The answer, until it's put away.
    pub found: Option<Box<Found>>,
    /// A command testing each commit.
    pub running: Option<Running>,
    pub error: Option<String>,
    /// An agent is the one answering (shown in the bar).
    pub agent: Option<String>,
    /// The commits' summaries, by id (for the answers and the bar).
    pub summaries: HashMap<String, String>,
    /// Marks for the graph.
    pub marks: Marks,
    work: Option<Task<Result<Done, String>>>,
    preview: Option<Task<Checked>>,
    /// The repository changed (answers given in a terminal, say): read the
    /// search again.
    stale: bool,
    /// Work here checked out another commit: the repository's view is
    /// read again.
    moved: bool,
    /// The work going on checks out commits.
    work_moves: bool,
}

/// What the graph marks while searching.
#[derive(Debug, Clone, Default)]
pub struct Marks {
    pub active: bool,
    /// The commits still in question.
    pub range: HashSet<String>,
    pub answers: HashMap<String, Verdict>,
    pub testing: Option<String>,
}

impl Bisect {
    /// Answering, starting, stopping or a command running (reading the
    /// search again doesn't count: what's asked replaces it).
    pub fn is_busy(&self) -> bool {
        (self.work.is_some() && self.work_moves) || self.running.is_some()
    }

    pub fn searching(&self) -> bool {
        self.search.is_some()
    }

    fn set_search(&mut self, search: Option<Search>, path: &str) {
        self.marks = match &search {
            Some(s) => Marks {
                active: true,
                range: s.range.iter().cloned().collect(),
                answers: s
                    .answers
                    .iter()
                    .map(|a| (a.id.clone(), a.verdict))
                    .collect(),
                testing: s.testing.clone(),
            },
            None => Marks::default(),
        };
        if let Some(s) = &search {
            // Summaries for what's shown: the answers and the one testing.
            let wanted: Vec<String> = s
                .answers
                .iter()
                .map(|a| a.id.clone())
                .chain(s.testing.clone())
                .filter(|id| !self.summaries.contains_key(id))
                .collect();
            for (id, summary) in summaries(path, &wanted) {
                self.summaries.insert(id, summary);
            }
        }
        if search.is_none() {
            self.agent = None;
        }
        self.search = search;
    }

    /// Takes in finished work; reads the search again when the repository's
    /// status says one is (or isn't) going on.
    pub fn poll(&mut self, ctx: &egui::Context, path: &str, git_says_searching: bool) {
        if let Some(result) = self.work.as_ref().and_then(Task::poll) {
            self.work = None;
            // Only work that checks out commits (not reading) moves things.
            self.moved = std::mem::take(&mut self.work_moves);
            match result {
                Ok(Done::Search(s)) => self.set_search(s.map(|s| *s), path),
                Ok(Done::Found(f)) => {
                    self.set_search(None, path);
                    self.found = Some(f);
                }
                Err(e) => {
                    self.error = Some(e);
                    self.read(ctx, path);
                }
            }
        }
        if let Some(r) = &self.running
            && let Some(result) = r.task.poll()
        {
            self.moved = true;
            let error = r.log.lock().ok().and_then(|l| l.error.clone());
            self.running = None;
            match result {
                Ok(Some(found)) => {
                    self.set_search(None, path);
                    self.found = Some(found);
                }
                Ok(None) => self.read(ctx, path),
                Err(e) => {
                    self.error = Some(error.unwrap_or(e));
                    self.read(ctx, path);
                }
            }
        }
        if let Some(p) = self.preview.as_ref().and_then(Task::poll) {
            self.preview = None;
            if let Some(start) = &mut self.starting {
                start.preview = Some(p);
            }
        }
        // Started, answered or ended outside the app (or by an operation here).
        let changed = git_says_searching != self.searching() || (self.stale && git_says_searching);
        if self.work.is_none() && self.running.is_none() && changed {
            self.stale = false;
            self.read(ctx, path);
        }
        if self.running.is_some() {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }

    /// Reads the search from Git again (answers given in a terminal, say).
    pub fn read(&mut self, ctx: &egui::Context, path: &str) {
        if self.work.is_some() {
            return;
        }
        self.work_moves = false;
        let path = path.to_string();
        self.work = Some(Task::spawn(ctx, move || {
            bisect::read_search(&path)
                .map(|s| Done::Search(s.map(Box::new)))
                .map_err(|e| e.to_string())
        }));
    }

    /// Opens the start window for `broken`, with where it last worked
    /// suggested (the latest tag below it).
    pub fn open_start(
        &mut self,
        ctx: &egui::Context,
        path: &str,
        broken: &str,
        label: String,
        dirty: bool,
    ) {
        let tags = bisect::tags_below(path, broken).unwrap_or_default();
        let works = tags
            .first()
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
        self.found = None;
        self.error = None;
        self.starting = Some(Start {
            broken: broken.to_string(),
            broken_label: label,
            works,
            tags,
            first_parent: false,
            looking_for: 0,
            own_new: String::new(),
            own_old: String::new(),
            command: String::new(),
            limit_minutes: String::new(),
            preview: None,
            dirty,
            error: None,
        });
        self.check_start(ctx, path);
    }

    /// Works out the range for the start window's choice, when it changed.
    pub fn check_start(&mut self, ctx: &egui::Context, path: &str) {
        let Some(start) = &self.starting else { return };
        let wanted = (start.works.trim().to_string(), start.first_parent);
        let current = start.preview.as_ref().map(|(w, fp, _)| (w.clone(), *fp));
        if current.as_ref() == Some(&wanted) || self.preview.is_some() {
            return;
        }
        if wanted.0.is_empty() {
            if let Some(s) = &mut self.starting {
                s.preview = Some((
                    String::new(),
                    wanted.1,
                    Err("Pick a commit where it still worked".into()),
                ));
            }
            return;
        }
        let (path, broken) = (path.to_string(), start.broken.clone());
        self.preview = Some(Task::spawn(ctx, move || {
            let result =
                bisect::preview(&path, &broken, &wanted.0, wanted.1).map_err(|e| e.to_string());
            (wanted.0, wanted.1, result)
        }));
    }

    /// Starts the search chosen in the start window: by hand, or (with a
    /// command) testing each commit with it.
    pub fn start(&mut self, ctx: &egui::Context, path: &str, with_command: bool) {
        let Some(start) = self.starting.take() else {
            return;
        };
        let terms = start.terms();
        let works = vec![start.works.trim().to_string()];
        let command = start.command.trim().to_string();
        let limit = start
            .limit_minutes
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|m| *m > 0.0)
            .map(|m| Duration::from_secs_f64(m * 60.0));
        self.start_with(
            ctx,
            path,
            Begin {
                broken: start.broken.clone(),
                works,
                first_parent: start.first_parent,
                terms,
                dirty: start.dirty,
                command: (with_command && !command.is_empty()).then_some(command),
                limit,
            },
        );
    }

    /// Starts a search (from the start window, or for an agent).
    pub fn start_with(&mut self, ctx: &egui::Context, path: &str, b: Begin) {
        let Begin {
            broken,
            works,
            first_parent,
            terms,
            dirty,
            command,
            limit,
        } = b;
        let path_s = path.to_string();
        self.starting = None;
        self.error = None;
        self.found = None;
        let begun = move || -> Result<Search, String> {
            let put_aside = dirty && bisect::put_aside(&path_s).map_err(|e| e.to_string())?;
            match bisect::start(&path_s, &broken, &works, first_parent, terms.as_ref()) {
                Ok(s) => Ok(s),
                Err(e) => {
                    // Not started (or half: Git couldn't check out the first
                    // commit): ended, and what was put aside comes back.
                    if bisect::is_searching(&path_s) {
                        let _ = bisect::stop(&path_s);
                    }
                    if put_aside {
                        let _ = bisect::bring_back(&path_s);
                    }
                    Err(e.to_string())
                }
            }
        };
        if let Some(command) = command {
            self.run_with(ctx, path, command, limit, Some(Box::new(begun)));
            return;
        }
        self.work_moves = true;
        let path_s = path.to_string();
        self.work = Some(Task::spawn(ctx, move || {
            let s = begun()?;
            settle(&path_s, s)
        }));
    }

    /// Answers for the commit being tested.
    pub fn answer(&mut self, ctx: &egui::Context, path: &str, verdict: Verdict) {
        if self.is_busy() {
            return;
        }
        self.work_moves = true;
        let path = path.to_string();
        self.error = None;
        self.work = Some(Task::spawn(ctx, move || {
            let s = bisect::answer(&path, verdict).map_err(|e| e.to_string())?;
            settle(&path, s)
        }));
    }

    pub fn undo(&mut self, ctx: &egui::Context, path: &str) {
        if self.is_busy() {
            return;
        }
        self.work_moves = true;
        let path = path.to_string();
        self.error = None;
        self.work = Some(Task::spawn(ctx, move || {
            bisect::undo(&path)
                .map(|s| Done::Search(Some(Box::new(s))))
                .map_err(|e| e.to_string())
        }));
    }

    /// Stops the search: back where it started, with what was put aside.
    pub fn stop(&mut self, ctx: &egui::Context, path: &str) {
        if let Some(r) = &self.running {
            r.stop();
            return;
        }
        if self.is_busy() {
            return;
        }
        self.work_moves = true;
        let path = path.to_string();
        self.error = None;
        self.work = Some(Task::spawn(ctx, move || {
            bisect::stop(&path).map_err(|e| e.to_string())?;
            bisect::bring_back(&path).map_err(|e| e.to_string())?;
            Ok(Done::Search(None))
        }));
    }

    /// Tests each commit with `command` from here on.
    pub fn run_command(
        &mut self,
        ctx: &egui::Context,
        path: &str,
        command: String,
        limit: Option<Duration>,
    ) {
        if self.is_busy() || command.trim().is_empty() {
            return;
        }
        self.run_with(ctx, path, command.trim().to_string(), limit, None);
    }

    fn run_with(
        &mut self,
        ctx: &egui::Context,
        path: &str,
        command: String,
        limit: Option<Duration>,
        begin: Option<Starting>,
    ) {
        let log = Arc::new(Mutex::new(RunLog {
            command: command.clone(),
            ..RunLog::default()
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (log2, stop2, path, ctx2) = (
            Arc::clone(&log),
            Arc::clone(&stop),
            path.to_string(),
            ctx.clone(),
        );
        self.error = None;
        let task = Task::spawn(ctx, move || {
            let fail = |e: String| {
                if let Ok(mut l) = log2.lock() {
                    l.error = Some(e.clone());
                    l.done = true;
                }
                e
            };
            let mut search = match begin {
                Some(b) => b().map_err(fail)?,
                None => bisect::read_search(&path)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| fail("No search is going on".into()))?,
            };
            loop {
                if search.outcome.is_some() {
                    return match settle(&path, search).map_err(fail)? {
                        Done::Found(f) => Ok(Some(f)),
                        Done::Search(_) => Ok(None),
                    };
                }
                if stop2.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                let id = search.testing.clone().unwrap_or_default();
                let summary = summaries(&path, std::slice::from_ref(&id))
                    .pop()
                    .map(|(_, s)| s)
                    .unwrap_or_default();
                if let Ok(mut l) = log2.lock() {
                    l.current = Some((id.clone(), summary.clone(), Instant::now()));
                    l.output.clear();
                }
                let mut sink = |text: &str| {
                    if let Ok(mut l) = log2.lock() {
                        l.output.push_str(text);
                        // The last part is what's shown: keep it bounded.
                        if l.output.len() > 200_000 {
                            let cut = l.output.len() - 100_000;
                            let cut = (cut..l.output.len())
                                .find(|i| l.output.is_char_boundary(*i))
                                .unwrap_or(0);
                            l.output.drain(..cut);
                        }
                    }
                    ctx2.request_repaint();
                };
                let run = bisect::run_command(&path, &command, limit, &stop2, &mut sink)
                    .map_err(|e| fail(e.to_string()))?;
                let stopped = run.code.is_none() && !run.timed_out;
                // A step that ran too long can't be told: skipped.
                let verdict = if run.timed_out {
                    Some(Verdict::Skip)
                } else {
                    run.verdict
                };
                if let Ok(mut l) = log2.lock() {
                    let output = std::mem::take(&mut l.output);
                    l.current = None;
                    l.steps.push(Step {
                        id: id.clone(),
                        summary,
                        verdict: if stopped { None } else { verdict },
                        code: run.code,
                        took: run.took,
                        timed_out: run.timed_out,
                        output,
                    });
                }
                if stopped {
                    return Ok(None);
                }
                let Some(v) = verdict else {
                    return Ok(None);
                };
                search = bisect::answer(&path, v).map_err(|e| fail(e.to_string()))?;
            }
        });
        self.running = Some(Running { log, stop, task });
    }

    /// Whether work here checked out another commit since last asked (the
    /// repository's view is to be read again).
    pub fn take_moved(&mut self) -> bool {
        std::mem::take(&mut self.moved)
    }

    /// The repository's status was read again: so is the search.
    pub fn status_changed(&mut self) {
        self.stale = true;
    }

    /// Puts the answer away.
    pub fn close_found(&mut self) {
        self.found = None;
    }
}

/// After an answer: the search goes on, or it's found and finished (back
/// where it started, with what was put aside).
fn settle(path: &str, search: Search) -> Result<Done, String> {
    if search.outcome.is_none() {
        return Ok(Done::Search(Some(Box::new(search))));
    }
    let finished = bisect::finish(path, &search)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "The search hasn't found it yet".to_string())?;
    let ids: Vec<String> = match &finished.outcome {
        Outcome::Found(id) => vec![id.clone()],
        Outcome::OneOf(ids) => ids.clone(),
    };
    let details = ids
        .iter()
        .filter_map(|id| git_raccoon_core::details::commit_details(path, id).ok())
        .collect();
    let all: Vec<String> = finished
        .answers
        .iter()
        .map(|a| a.id.clone())
        .chain(ids)
        .collect();
    let summaries = summaries(path, &all).into_iter().collect();
    Ok(Done::Found(Box::new(Found {
        finished,
        details,
        summaries,
    })))
}

/// Each commit's message's first line.
fn summaries(path: &str, ids: &[String]) -> Vec<(String, String)> {
    ids.iter()
        .filter_map(|id| {
            git_raccoon_core::details::commit_details(path, id)
                .ok()
                .map(|d| (id.clone(), d.summary))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_terms() {
        assert_eq!(
            Words::of(&Terms::default()).button(Verdict::New),
            "It's broken"
        );
        assert_eq!(
            Words::of(&Terms::default()).title(),
            "Finding the commit that broke it"
        );
        let own = Words::of(&Terms {
            new: "fast".into(),
            old: "slow".into(),
        });
        assert_eq!(own.button(Verdict::Old), "It's slow");
        assert_eq!(own.title(), "Finding the first fast commit");
        assert_eq!(own_term(" Fast Now ", "new"), "fastnow");
        assert_eq!(own_term("skip", "new"), "new", "Git's own word");
        assert_eq!(own_term("", "old"), "old");
    }
}
