//! Answering agents' tool calls (see [`crate::agent`]): reads straight away
//! (on a thread of their own), ways to show things at once, and changes
//! once the person allows them, reporting how they went.

use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver};

use eframe::egui::{self, RichText};
use git_raccoon_core::{details, git_output, refs, working};
use serde_json::{Value, json};

use super::App;
use crate::agent::{Call, Server};
use crate::forge::review::{Anchor, Draft, Line};
use crate::repo_view::{Op, RepoView};
use crate::theme;

/// Answers longer than this are cut short (an agent reads all of it).
const MOST: usize = 120_000;

/// An agent's change, waiting for the person's answer.
pub(super) struct Asking {
    call: Call,
    /// "commit the staged changes"
    what: String,
    /// More on it: the message, the files.
    detail: Vec<String>,
    /// Asked every time (it can lose work), even when the agent is allowed
    /// for the session.
    risky: bool,
    /// The repository it's for.
    repo: String,
    act: Act,
}

/// What an allowed change does.
enum Act {
    Op(Op),
    /// Finding the commit that broke something.
    Bisect(Bisect),
    Commit {
        message: String,
        amend: bool,
    },
    Draft(Draft),
    Resolve(String, bool),
}

/// What an agent asked of a search.
enum Bisect {
    Start(crate::bisect_view::Begin),
    Answer(git_raccoon_core::bisect::Verdict),
    Run(String, Option<std::time::Duration>),
    Stop,
}

/// A change running, for an agent waiting to hear how it went.
pub(super) struct Waiting {
    call: Call,
    until: Until,
    /// What to tell the agent when it went well.
    done: String,
}

enum Until {
    /// The operation or commit says.
    Report(Receiver<Result<(), String>>),
    /// The forge's answer comes in (for this repository).
    Forge(String),
}

/// How agents stand, for the Settings page: listening, who's connected,
/// and why it couldn't start.
#[derive(Clone, Default)]
pub struct AgentsStatus {
    pub on: bool,
    pub clients: Vec<String>,
    pub error: Option<String>,
}

pub fn status_id() -> egui::Id {
    egui::Id::new("agents-status")
}

/// Text cut to what an agent should read.
fn capped(mut text: String) -> String {
    if text.len() > MOST {
        let mut end = MOST;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n… (cut short: ask for less, e.g. one file)");
    }
    text
}

fn pretty(value: &Value) -> String {
    capped(serde_json::to_string_pretty(value).unwrap_or_default())
}

/// A revision or path given by an agent, refused if Git could take it for
/// an option.
fn plain(what: &str, value: &str) -> Result<(), String> {
    if value.starts_with('-') {
        Err(format!(
            "That {what} starts with \"-\"; Git would read it as an option."
        ))
    } else {
        Ok(())
    }
}

impl App {
    /// Lets agents in while the setting is on, answers their calls, and
    /// asks the person about changes.
    pub(super) fn agents(&mut self, ctx: &egui::Context) {
        match (&self.agent, self.settings.agents) {
            (None, true) if self.agent_error.is_none() => {
                let wake = ctx.clone();
                match Server::start(self.agent_file.clone(), move || wake.request_repaint()) {
                    Ok(server) => self.agent = Some(server),
                    Err(e) => self.agent_error = Some(format!("Couldn't let agents in: {e}.")),
                }
            }
            (Some(_), false) => self.stop_agents(),
            (None, false) => self.agent_error = None,
            _ => {}
        }
        ctx.data_mut(|d| {
            d.insert_temp(
                status_id(),
                AgentsStatus {
                    on: self.agent.is_some(),
                    clients: self.agent_names(),
                    error: self.agent_error.clone(),
                },
            );
        });
        // One question at a time: calls wait while the person is asked.
        while self.agent_asking.is_none()
            && let Some(call) = self.agent.as_ref().and_then(Server::poll)
        {
            self.agent_call(ctx, call);
        }
        self.agent_progress();
        self.agent_question(ctx);
    }

    /// The agents connected now, by name.
    pub(super) fn agent_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .agent
            .as_ref()
            .map(|s| s.clients().into_iter().map(|c| c.name).collect())
            .unwrap_or_default();
        names.dedup();
        names
    }

    fn stop_agents(&mut self) {
        self.agent = None;
        self.agent_trusted.clear();
        if let Some(asking) = self.agent_asking.take() {
            asking.call.fail("Agent access was turned off.");
        }
        for w in self.agent_waiting.drain(..) {
            w.call
                .fail("Agent access was turned off before it finished.");
        }
    }

    /// The path of the open repository `call` means (its `repository`, or
    /// the one in front).
    fn agent_repo(&self, call: &Call) -> Result<String, String> {
        let open: Vec<String> = self.tabs.iter().filter_map(|t| t.path.clone()).collect();
        match call.str("repository") {
            Some(asked) => {
                let asked = asked.trim_end_matches(['/', '\\']).to_string();
                open.into_iter().find(|p| *p == asked).ok_or_else(|| {
                    format!("{asked} isn't open in Git Raccoon (list_repositories says which are).")
                })
            }
            None => self
                .repo
                .as_ref()
                .map(|r| r.path().trim_end_matches(['/', '\\']).to_string())
                .ok_or_else(|| "No repository is open in Git Raccoon.".to_string()),
        }
    }

    /// The open repository at `path`, in front or in a background tab.
    fn repo_at(&mut self, path: &str) -> Option<&mut RepoView> {
        let same = |r: &RepoView| r.path().trim_end_matches(['/', '\\']) == path;
        if self.repo.as_ref().is_some_and(same) {
            return self.repo.as_mut();
        }
        self.tabs
            .iter_mut()
            .filter_map(|t| t.parked.as_mut().and_then(|p| p.0.as_mut()))
            .find(|r| same(r))
    }

    /// Brings the tab with `path` to the front (changes happen where the
    /// person can see them).
    fn front(&mut self, ctx: &egui::Context, path: &str) {
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(path))
        {
            self.switch_tab(ctx, i);
        }
    }

    fn agent_call(&mut self, ctx: &egui::Context, call: Call) {
        match call.tool.as_str() {
            "list_repositories" => {
                let front = self.active;
                let list: Vec<Value> = self
                    .tabs
                    .iter()
                    .enumerate()
                    .filter_map(|(i, t)| {
                        t.path
                            .as_ref()
                            .map(|p| json!({ "path": p, "in_front": i == front }))
                    })
                    .collect();
                call.ok(pretty(&json!(list)));
            }
            "get_view" => {
                let text = self.view_now();
                call.ok(text);
            }
            "list_requests" => match self.agent_repo(&call) {
                Ok(path) => {
                    let text = self.requests_list(&path);
                    call.answer(text);
                }
                Err(e) => call.fail(e),
            },
            "get_request" => {
                let text = self.open_request();
                call.answer(text);
            }
            "show_commit" => self.show_commit(ctx, call),
            "show_request" => match (self.agent_repo(&call), call.number("number")) {
                (Ok(path), Some(n)) => {
                    self.front(ctx, &path);
                    match self.repo.as_mut().and_then(|r| r.forge.as_mut()) {
                        Some(f) => {
                            f.open_request(ctx, n);
                            call.ok(format!(
                                "Opening {}; get_request reads it once it's loaded.",
                                f.project.kind.number(n)
                            ));
                        }
                        None => call.fail(
                            "This repository isn't on GitHub or GitLab (or isn't signed in).",
                        ),
                    }
                }
                (Err(e), _) => call.fail(e),
                (_, None) => call.fail("Which request? Give its `number`."),
            },
            "open_repository" => match call.str("path") {
                Some(path) if std::path::Path::new(&path).is_dir() => {
                    self.open(ctx, path.clone());
                    call.ok(format!("Opening {path}."));
                }
                Some(path) => call.fail(format!("There's no folder at {path}.")),
                None => call.fail("Which folder? Give its `path`."),
            },
            "start_request" => self.start_request(ctx, call),
            "get_bisect" => match self.agent_repo(&call) {
                Ok(path) => {
                    self.front(ctx, &path);
                    let text = self.bisect_now();
                    call.ok(text);
                }
                Err(e) => call.fail(e),
            },
            // In a search the person allowed this agent, answers and commands
            // go ahead.
            "bisect_answer" | "bisect_run"
                if self.repo.as_ref().is_some_and(|r| {
                    r.bisect.agent.as_deref() == Some(call.client.as_str()) && r.bisect.searching()
                }) && self.agent_repo(&call).is_ok_and(|p| {
                    self.repo.as_ref().is_some_and(|r| {
                        r.path().trim_end_matches(['/', '\\']) == p.trim_end_matches(['/', '\\'])
                    })
                }) =>
            {
                let path = self
                    .repo
                    .as_ref()
                    .map(|r| r.path().trim_end_matches(['/', '\\']).to_string())
                    .unwrap_or_default();
                match self.change(&call, &path) {
                    Err(e) => call.fail(e),
                    Ok((what, detail, risky, act)) => {
                        let asking = Asking {
                            call,
                            what,
                            detail,
                            risky,
                            repo: path,
                            act,
                        };
                        self.agent_do(ctx, asking);
                    }
                }
            }
            "propose_resolution" => self.propose_resolution(ctx, call),
            "get_status" | "get_log" | "search_commits" | "get_commit" | "get_diff"
            | "get_refs" | "get_conflicts" => match self.agent_repo(&call) {
                Ok(path) => {
                    std::thread::spawn(move || {
                        let result = read(&call, &path);
                        call.answer(result);
                    });
                }
                Err(e) => call.fail(e),
            },
            _ => match self.agent_repo(&call) {
                Err(e) => call.fail(e),
                Ok(repo) => match self.change(&call, &repo) {
                    Err(e) => call.fail(e),
                    Ok((what, detail, risky, act)) => {
                        let asking = Asking {
                            call,
                            what,
                            detail,
                            risky,
                            repo,
                            act,
                        };
                        // Allowed for the session: no need to ask (unless risky).
                        if !asking.risky && self.agent_trusted.contains(&asking.call.connection) {
                            self.agent_do(ctx, asking);
                        } else {
                            self.agent_asking = Some(asking);
                        }
                    }
                },
            },
        }
    }

    /// Where finding the commit that broke something stands.
    fn bisect_now(&self) -> String {
        use git_raccoon_core::bisect::{Outcome, Verdict};
        let Some(repo) = &self.repo else {
            return pretty(&json!({ "searching": false }));
        };
        let b = &repo.bisect;
        let name = |v: Verdict, w: &crate::bisect_view::Words| w.verdict(v).to_string();
        if let Some(found) = &b.found {
            let w = found.words();
            let outcome = match &found.finished.outcome {
                Outcome::Found(id) => json!({ "first": id, "summary": found.summaries.get(id) }),
                Outcome::OneOf(ids) => json!({ "one_of": ids }),
            };
            return pretty(&json!({
                "searching": false,
                "found": outcome,
                "answers": found.finished.answers.iter().map(|a| json!({ "commit": a.id, "answer": name(a.verdict, &w) })).collect::<Vec<_>>(),
                "back_on": found.finished.back_on,
            }));
        }
        let Some(s) = &b.search else {
            return pretty(&json!({ "searching": false, "error": b.error }));
        };
        let w = crate::bisect_view::Words::of(&s.terms);
        let running = b.running.as_ref().and_then(|r| r.log.lock().ok()).map(|l| {
            json!({
                "command": l.command,
                "steps": l.steps.iter().map(|st| json!({ "commit": st.id, "summary": st.summary, "exit": st.code, "answer": st.verdict.map(|v| name(v, &w)) })).collect::<Vec<_>>(),
                "testing_now": l.current.as_ref().map(|c| c.0.clone()),
            })
        });
        pretty(&json!({
            "searching": true,
            "words": { "new": w.new, "old": w.old },
            "testing": s.testing,
            "testing_summary": s.testing.as_ref().and_then(|t| b.summaries.get(t)),
            "commits_left": s.left(),
            "steps_left": s.steps,
            "answers": s.answers.iter().map(|a| json!({ "commit": a.id, "answer": name(a.verdict, &w) })).collect::<Vec<_>>(),
            "only_main_line": s.first_parent,
            "command": running,
            "busy": b.is_busy(),
            "error": b.error,
        }))
    }

    /// What the person is looking at.
    fn view_now(&self) -> String {
        let Some(repo) = &self.repo else {
            return pretty(&json!({ "repository": null, "note": "No repository is open." }));
        };
        let mut view = json!({
            "repository": repo.path().trim_end_matches(['/', '\\']),
            "branch": repo.head_branch(),
        });
        if repo.working_selected {
            view["selected"] = json!("the working changes");
        } else if let Some(sel) = &repo.selected {
            view["selected_commit"] = json!(sel.id);
        }
        if let Some(file) = &repo.active_file {
            view["open_file"] = json!({
                "path": file,
                "showing": if repo.working_selected {
                    if repo.active_side == git_raccoon_core::working::Side::Staged { "staged changes" } else { "unstaged changes" }
                } else { "the selected commit's changes" },
            });
        }
        if let Some(f) = &repo.forge {
            match f.open {
                Some(crate::forge_view::Opened::Request(n)) => {
                    view["open_request"] =
                        json!({ "number": n, "title": f.viewed().map(|(r, _)| r.title.clone()) });
                }
                Some(crate::forge_view::Opened::Issue(n)) => {
                    view["open_issue"] =
                        json!({ "number": n, "title": f.issue(n).map(|i| i.title.clone()) });
                }
                None => {}
            }
        }
        pretty(&view)
    }

    fn requests_list(&mut self, path: &str) -> Result<String, String> {
        let repo = self.repo_at(path).ok_or("That repository isn't open.")?;
        let f = repo
            .forge
            .as_ref()
            .ok_or("This repository isn't on GitHub or GitLab (or isn't signed in).")?;
        let kind = f.project.kind;
        let requests = match &f.requests {
            None => json!("still loading"),
            Some(Err(e)) => json!(format!("couldn't be listed: {e}")),
            Some(Ok(list)) => json!(
                list.iter()
                    .map(|r| json!({
                        "number": r.number,
                        "title": r.title,
                        "author": r.author,
                        "from": r.source,
                        "into": r.target,
                        "draft": r.draft,
                        "review": format!("{:?}", r.review),
                        "url": r.url,
                    }))
                    .collect::<Vec<_>>()
            ),
        };
        let issues = match &f.issues {
            None => json!("still loading"),
            Some(Err(e)) => json!(format!("couldn't be listed: {e}")),
            Some(Ok(list)) => json!(list
                .iter()
                .map(|i| json!({ "number": i.number, "title": i.title, "author": i.author, "labels": i.labels, "url": i.url }))
                .collect::<Vec<_>>()),
        };
        Ok(pretty(&json!({
            "forge": kind.name(),
            "project": f.project.path,
            "requests": requests,
            "more_requests": f.more_requests,
            "issues": issues,
            "more_issues": f.more_issues,
        })))
    }

    /// The pull/merge request open in front, in full.
    fn open_request(&self) -> Result<String, String> {
        let repo = self.repo.as_ref().ok_or("No repository is open.")?;
        let f = repo
            .forge
            .as_ref()
            .ok_or("This repository isn't on GitHub or GitLab (or isn't signed in).")?;
        let Some(crate::forge_view::Opened::Request(n)) = f.open else {
            return Err("No pull/merge request is open: show_request opens one.".into());
        };
        let Some((r, o)) = f.viewed() else {
            return Err(format!(
                "{} is still loading; try again in a moment.",
                f.project.kind.number(n)
            ));
        };
        let drafts = self
            .settings
            .review_drafts
            .get(&f.drafts_key(n))
            .cloned()
            .unwrap_or_default();
        let comment = |c: &crate::forge::review::Comment| json!({ "id": c.id, "author": c.author, "body": c.body, "created": c.created });
        let threads: Vec<Value> = o
            .threads
            .iter()
            .map(|t| {
                json!({
                    "id": t.reply,
                    "path": t.at.as_ref().map(|a| a.path.clone()),
                    "line": t.at.as_ref().map(|a| a.line.new.or(a.line.old)),
                    "resolved": t.resolved,
                    "outdated": t.outdated,
                    "comments": t.comments.iter().map(comment).collect::<Vec<_>>(),
                })
            })
            .collect();
        let conversation: Vec<Value> = o
            .events
            .iter()
            .filter_map(|e| match e {
                crate::forge::review::Event::Comment(c) => Some(comment(c)),
                crate::forge::review::Event::Review {
                    author,
                    verdict,
                    body,
                    created,
                } => Some(json!({ "review_by": author, "verdict": format!("{verdict:?}"), "body": body, "created": created })),
                crate::forge::review::Event::Thread(_) => None,
            })
            .collect();
        Ok(pretty(&json!({
            "number": r.number,
            "title": r.title,
            "author": r.author,
            "state": format!("{:?}", o.state),
            "draft": r.draft,
            "from": r.source,
            "into": r.target,
            "url": r.url,
            "description": r.body,
            "review": format!("{:?}", r.review),
            "checks": o.checks.iter().map(|c| json!({ "name": c.name, "outcome": format!("{:?}", c.outcome), "url": c.url })).collect::<Vec<_>>(),
            "threads": threads,
            "conversation": conversation,
            "files": o.files.iter().map(|f| json!({ "path": f.path, "old_path": f.old_path, "change": format!("{:?}", f.change) })).collect::<Vec<_>>(),
            "commits": o.commits.iter().map(|c| json!({ "id": c.sha, "summary": c.summary, "author": c.author })).collect::<Vec<_>>(),
            "your_drafts": drafts.iter().map(|d| json!({
                "body": d.body,
                "path": d.at.as_ref().map(|a| a.path.clone()),
                "line": d.at.as_ref().map(|a| a.line.new.or(a.line.old)),
                "reply_to": d.reply_to,
            })).collect::<Vec<_>>(),
            "not_loaded": o.missing,
        })))
    }

    fn show_commit(&mut self, ctx: &egui::Context, call: Call) {
        let (path, id) = match (self.agent_repo(&call), call.str("id")) {
            (Ok(p), Some(id)) => (p, id),
            (Err(e), _) => return call.fail(e),
            (_, None) => return call.fail("Which commit? Give its `id`."),
        };
        if let Err(e) = plain("commit", &id) {
            return call.fail(e);
        }
        let full = match git_output(
            &path,
            &["rev-parse", "--verify", &format!("{id}^{{commit}}")],
        ) {
            Ok(out) => out.trim().to_string(),
            Err(_) => return call.fail(format!("There's no commit {id}.")),
        };
        self.front(ctx, &path);
        if let Some(repo) = self.repo.as_mut() {
            // The history is where commits show (not a request's view).
            if let Some(f) = repo.forge.as_mut() {
                f.open = None;
            }
            repo.select(&full);
        }
        call.ok(format!("Showing {}.", &full[..full.len().min(10)]));
    }

    /// Shows an agent's resolution of a conflicted file, for the person to
    /// use, edit or discard.
    fn propose_resolution(&mut self, ctx: &egui::Context, call: Call) {
        let path = match self.agent_repo(&call) {
            Ok(p) => p,
            Err(e) => return call.fail(e),
        };
        let (Some(file), Some(reasoning)) = (call.str("path"), call.str("reasoning")) else {
            return call.fail("Give the file's `path`, its resolved `text`, and your `reasoning`.");
        };
        let Some(text) = call
            .args
            .get("text")
            .and_then(Value::as_str)
            .map(str::to_string)
        else {
            return call.fail("Give the file's resolved `text` (the whole file).");
        };
        if git_raccoon_core::conflict::parse(&text).is_some() {
            return call.fail(
                "The text still has conflict markers: resolve every conflict in it, then propose it.",
            );
        }
        self.front(ctx, &path);
        let Some(repo) = self.repo.as_mut() else {
            return call.fail("No repository is open in Git Raccoon.");
        };
        if !repo.conflict_files.iter().any(|f| f.path == file) {
            return call.fail(format!(
                "{file} isn't conflicted (get_conflicts lists the files that are)."
            ));
        }
        if let Some(f) = repo.forge.as_mut() {
            f.open = None;
        }
        repo.planner = None;
        repo.history_over_conflicts = false;
        if repo.conflict.as_ref().is_none_or(|c| c.file != file) {
            repo.open_conflict(file.clone());
        }
        repo.proposal = Some(crate::repo_view::Proposal {
            file: file.clone(),
            text,
            reasoning,
            agent: call.client.clone(),
        });
        call.ok(format!(
            "Shown to the person with your reasoning. Nothing is written unless they choose to use it (they may edit it first); get_conflicts says whether {file} is still conflicted."
        ));
    }

    fn start_request(&mut self, ctx: &egui::Context, call: Call) {
        let path = match self.agent_repo(&call) {
            Ok(p) => p,
            Err(e) => return call.fail(e),
        };
        self.front(ctx, &path);
        let Some(repo) = self.repo.as_ref() else {
            return call.fail("That repository isn't open.");
        };
        let Some(mut form) = super::new_form(repo, None) else {
            return call.fail(
                "No request can be started here: the repository needs to be on GitHub or GitLab, signed in, with a branch checked out.",
            );
        };
        if let Some(title) = call.str("title") {
            form.title = title;
        }
        if let Some(body) = call.args.get("body").and_then(Value::as_str) {
            form.body = body.to_string();
        }
        if let Some(target) = call.str("target") {
            form.target = target;
        }
        let what = format!("{} into {}", form.source, form.target);
        self.new_request = Some(form);
        call.ok(format!(
            "Git Raccoon shows the new request ({what}) for the person to check and open."
        ));
    }

    /// What a change would do, to ask about: (what, more on it, risky, act).
    fn change(
        &mut self,
        call: &Call,
        path: &str,
    ) -> Result<(String, Vec<String>, bool, Act), String> {
        let pull = self.settings.pull;
        let mut fetch = self.settings.fetch_options();
        fetch.force = false;
        let repo = self.repo_at(path).ok_or("That repository isn't open.")?;
        let list = |paths: &[String]| paths.iter().map(|p| format!("  {p}")).collect::<Vec<_>>();
        let need = |name: &str| call.str(name).ok_or_else(|| format!("`{name}` is needed."));
        Ok(match call.tool.as_str() {
            "stage" | "unstage" => {
                let stage = call.tool == "stage";
                let paths = call.strings("paths");
                let verb = if stage { "stage" } else { "unstage" };
                if call.bool("all") == Some(true) {
                    let op = if stage { Op::StageAll } else { Op::UnstageAll };
                    (
                        format!("{verb} all changes"),
                        Vec::new(),
                        false,
                        Act::Op(op),
                    )
                } else if paths.is_empty() {
                    return Err("Which files? Give `paths`, or `all`: true.".into());
                } else {
                    let ops = paths
                        .iter()
                        .map(|p| {
                            if stage {
                                Op::StageFile(p.clone())
                            } else {
                                Op::UnstageFile(p.clone())
                            }
                        })
                        .collect();
                    (
                        format!(
                            "{verb} changes to {} file{}",
                            paths.len(),
                            if paths.len() == 1 { "" } else { "s" }
                        ),
                        list(&paths),
                        false,
                        Act::Op(Op::Then(ops)),
                    )
                }
            }
            "start_bisect" => {
                let works = need("works")?;
                plain("commit", &works)?;
                let broken = call.str("broken").unwrap_or_else(|| "HEAD".into());
                plain("commit", &broken)?;
                if repo.bisect.searching() {
                    return Err(
                        "A search is going on already (get_bisect says where it stands).".into(),
                    );
                }
                let terms = match (call.str("new_word"), call.str("old_word")) {
                    (Some(new), Some(old)) => Some(git_raccoon_core::bisect::Terms { new, old }),
                    _ => None,
                };
                let command = call.str("command").filter(|c| !c.trim().is_empty());
                let limit = call
                    .number("time_limit_seconds")
                    .map(std::time::Duration::from_secs);
                let dirty = repo.status.as_ref().is_some_and(|s| !s.is_clean());
                let mut detail = vec![format!("  It's broken at {broken}, and worked at {works}.")];
                if let Some(c) = &command {
                    detail.push(format!("  Each commit is tested with: {c}"));
                }
                if dirty {
                    detail.push(
                        "  Uncommitted changes are put aside first, and back at the end.".into(),
                    );
                }
                (
                    "find which commit broke something (commits are checked out in turn)".into(),
                    detail,
                    false,
                    Act::Bisect(Bisect::Start(crate::bisect_view::Begin {
                        broken,
                        works: vec![works],
                        first_parent: call.bool("first_parent") == Some(true),
                        terms,
                        dirty,
                        command,
                        limit,
                    })),
                )
            }
            "bisect_answer" => {
                let Some(search) = &repo.bisect.search else {
                    return Err("No search is going on (start_bisect starts one).".into());
                };
                let said = need("answer")?.to_lowercase();
                let words = crate::bisect_view::Words::of(&search.terms);
                use git_raccoon_core::bisect::Verdict;
                let verdict = if said == "skip" {
                    Verdict::Skip
                } else if said == words.new
                    || said == search.terms.new
                    || said == "broken"
                    || said == "bad"
                {
                    Verdict::New
                } else if said == words.old
                    || said == search.terms.old
                    || said == "works"
                    || said == "good"
                {
                    Verdict::Old
                } else {
                    return Err(format!("Answer {}, {} or skip.", words.new, words.old));
                };
                let testing = search.testing.clone().unwrap_or_default();
                (
                    format!(
                        "say {} is {}",
                        &testing[..testing.len().min(7)],
                        words.verdict(verdict)
                    ),
                    Vec::new(),
                    false,
                    Act::Bisect(Bisect::Answer(verdict)),
                )
            }
            "bisect_run" => {
                if repo.bisect.search.is_none() {
                    return Err("No search is going on (start_bisect starts one).".into());
                }
                let command = need("command")?;
                let limit = call
                    .number("time_limit_seconds")
                    .map(std::time::Duration::from_secs);
                (
                    "test each commit of the search with a command".into(),
                    vec![format!("  {command}")],
                    false,
                    Act::Bisect(Bisect::Run(command, limit)),
                )
            }
            "stop_bisect" => {
                if repo.bisect.search.is_none() {
                    return Err("No search is going on.".into());
                }
                (
                    "stop finding the commit that broke something (back where it started)".into(),
                    Vec::new(),
                    false,
                    Act::Bisect(Bisect::Stop),
                )
            }
            "commit" => {
                let message = need("message")?;
                let amend = call.bool("amend") == Some(true);
                let staged = repo.status.as_ref().map_or(0, |s| s.staged.len());
                if staged == 0 && !amend {
                    return Err("Nothing is staged to commit (stage files first).".into());
                }
                let what = if amend {
                    "amend the last commit".to_string()
                } else {
                    format!(
                        "commit {staged} staged file{}",
                        if staged == 1 { "" } else { "s" }
                    )
                };
                (
                    what,
                    message.lines().map(|l| format!("  {l}")).collect(),
                    false,
                    Act::Commit { message, amend },
                )
            }
            "create_branch" => {
                let name = need("name")?;
                if !git_raccoon_core::branch::is_valid_branch_name(&name) {
                    return Err(format!("\"{name}\" can't be a branch name."));
                }
                let start = call.str("start").unwrap_or_else(|| "HEAD".into());
                plain("start", &start)?;
                let checkout = call.bool("checkout") != Some(false);
                let at = if start == "HEAD" {
                    String::new()
                } else {
                    format!(" at {start}")
                };
                let then = if checkout { " and check it out" } else { "" };
                (
                    format!("create the branch {name}{at}{then}"),
                    Vec::new(),
                    false,
                    Act::Op(Op::CreateBranch {
                        name,
                        start,
                        checkout,
                    }),
                )
            }
            "checkout" => {
                let branch = need("branch")?;
                let op = if repo.refs.local.iter().any(|b| b.name == branch) {
                    Op::Checkout(branch.clone())
                } else if repo
                    .refs
                    .remotes
                    .iter()
                    .flat_map(|r| &r.branches)
                    .any(|b| b.full_name == branch)
                {
                    Op::CheckoutRemote(branch.clone())
                } else {
                    return Err(format!(
                        "There's no branch {branch} (local, or remote like origin/{branch})."
                    ));
                };
                (
                    format!("check out {branch}"),
                    Vec::new(),
                    false,
                    Act::Op(op),
                )
            }
            "stash" => {
                let message = call.str("message").unwrap_or_default();
                let untracked = call.bool("include_untracked") != Some(false);
                let detail = if message.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("  {message}")]
                };
                (
                    "put the uncommitted changes aside in a stash".into(),
                    detail,
                    false,
                    Act::Op(Op::Stash {
                        message,
                        untracked,
                        staged: false,
                        paths: Vec::new(),
                    }),
                )
            }
            "stash_pop" => {
                let index = call.number("index").unwrap_or(0);
                let stash = repo
                    .refs
                    .stashes
                    .iter()
                    .find(|s| s.index as u64 == index)
                    .ok_or_else(|| format!("There's no stash {index}."))?;
                (
                    format!("pop stash {index}"),
                    vec![format!("  {}", stash.message)],
                    false,
                    Act::Op(Op::StashPop(stash.target.clone())),
                )
            }
            "create_tag" => {
                let name = need("name")?;
                if !git_raccoon_core::branch::is_valid_tag_name(&name) {
                    return Err(format!("\"{name}\" can't be a tag name."));
                }
                let target = call.str("target").unwrap_or_else(|| "HEAD".into());
                plain("target", &target)?;
                let message = call.str("message").unwrap_or_default();
                (
                    format!("create the tag {name} at {target}"),
                    Vec::new(),
                    false,
                    Act::Op(Op::CreateTag {
                        name,
                        target,
                        message,
                    }),
                )
            }
            "merge" => {
                let branch = need("branch")?;
                plain("branch", &branch)?;
                (
                    format!(
                        "merge {branch} into {}",
                        repo.head_branch().unwrap_or("HEAD")
                    ),
                    Vec::new(),
                    false,
                    Act::Op(Op::Merge {
                        branch,
                        mode: git_raccoon_core::merge::MergeMode::Default,
                    }),
                )
            }
            "fetch" => (
                "fetch from every remote".into(),
                Vec::new(),
                false,
                Act::Op(Op::Fetch(fetch)),
            ),
            "pull" => {
                let branch = repo
                    .head_branch()
                    .ok_or("No branch is checked out.")?
                    .to_string();
                // Never throws changes away for an agent: they're stashed instead.
                let stash = pull.changes != crate::settings::PullChanges::Leave;
                (
                    format!("pull {branch}"),
                    Vec::new(),
                    false,
                    Act::Op(Op::Pull {
                        from: None,
                        mode: pull.strategy.mode(),
                        stash,
                    }),
                )
            }
            "push" => {
                let head = repo
                    .refs
                    .local
                    .iter()
                    .find(|b| b.is_head)
                    .ok_or("No branch is checked out.")?;
                let to = if head.upstream.is_none() {
                    let remotes = &repo.refs.remotes;
                    let remote = remotes
                        .iter()
                        .find(|r| r.name == "origin")
                        .or_else(|| remotes.first())
                        .ok_or("There's no remote to push to.")?;
                    Some((remote.name.clone(), head.name.clone()))
                } else {
                    None
                };
                let where_to = match &to {
                    Some((remote, name)) => format!(" to {remote}/{name}"),
                    None => String::new(),
                };
                (
                    format!("push {}{where_to}", head.name),
                    Vec::new(),
                    false,
                    Act::Op(Op::Push { force: false, to }),
                )
            }
            "discard" => {
                let paths = call.strings("paths");
                if paths.is_empty() {
                    return Err("Which files? Give `paths`.".into());
                }
                let changed: HashSet<&str> = repo
                    .status
                    .as_ref()
                    .map(|s| s.unstaged.iter().map(|f| f.path.as_str()).collect())
                    .unwrap_or_default();
                if let Some(p) = paths.iter().find(|p| !changed.contains(p.as_str())) {
                    return Err(format!("{p} has no unstaged changes to discard."));
                }
                (
                    format!(
                        "discard the unstaged changes to {} file{}",
                        paths.len(),
                        if paths.len() == 1 { "" } else { "s" }
                    ),
                    list(&paths),
                    true,
                    Act::Op(Op::Discard(paths, repo.trash_override.clone())),
                )
            }
            "draft_comment" => {
                let body = need("body")?;
                let f = repo
                    .forge
                    .as_ref()
                    .ok_or("No pull/merge request is open.")?;
                if !matches!(f.open, Some(crate::forge_view::Opened::Request(_)))
                    || f.viewed().is_none()
                {
                    return Err("No pull/merge request is open (or it's still loading): show_request opens one.".into());
                }
                let reply_to = call.str("reply_to");
                let at = match (call.str("path"), call.number("line")) {
                    (Some(path), Some(line)) => {
                        let n = u32::try_from(line).map_err(|_| "That line number is too big.")?;
                        let line = if call.str("side").as_deref() == Some("old") {
                            Line {
                                old: Some(n),
                                new: None,
                            }
                        } else {
                            Line {
                                old: None,
                                new: Some(n),
                            }
                        };
                        Some(Anchor {
                            path,
                            old_path: None,
                            line,
                            start: None,
                        })
                    }
                    (Some(_), None) => {
                        return Err("A comment on a file needs its `line` too.".into());
                    }
                    _ => None,
                };
                let on = match (&at, &reply_to) {
                    (_, Some(t)) => format!("a reply to thread {t}"),
                    (Some(a), None) => format!(
                        "a comment on {}, line {}",
                        a.path,
                        a.line.new.or(a.line.old).unwrap_or(0)
                    ),
                    (None, None) => "a comment on the request".into(),
                };
                (
                    format!("add {on} to your drafts"),
                    body.lines().map(|l| format!("  {l}")).collect(),
                    false,
                    Act::Draft(Draft {
                        at,
                        reply_to,
                        body,
                        ..Draft::default()
                    }),
                )
            }
            "resolve_thread" => {
                let thread = need("thread")?;
                let to = call.bool("resolved") != Some(false);
                let f = repo
                    .forge
                    .as_ref()
                    .ok_or("No pull/merge request is open.")?;
                let (_, o) = f
                    .viewed()
                    .ok_or("No pull/merge request is open (or it's still loading).")?;
                let t = o
                    .threads
                    .iter()
                    .find(|t| t.reply == thread || t.id == thread)
                    .ok_or_else(|| {
                        format!("There's no thread {thread} (get_request lists them).")
                    })?;
                let first = t
                    .comments
                    .first()
                    .map(|c| {
                        format!(
                            "  {}: {}",
                            c.author,
                            c.body.lines().next().unwrap_or_default()
                        )
                    })
                    .into_iter()
                    .collect();
                (
                    format!(
                        "{} a thread, on {}",
                        if to { "resolve" } else { "reopen" },
                        f.project.kind.name()
                    ),
                    first,
                    false,
                    Act::Resolve(t.id.clone(), to),
                )
            }
            other => return Err(format!("Git Raccoon has no tool {other}.")),
        })
    }

    /// Does an allowed change.
    fn agent_do(&mut self, ctx: &egui::Context, asking: Asking) {
        let Asking {
            call,
            repo: path,
            act,
            what,
            ..
        } = asking;
        self.front(ctx, &path);
        let pull = self.settings.pull;
        let Some(repo) = self.repo.as_mut() else {
            return call.fail("That repository isn't open any more.");
        };
        let busy = "Git Raccoon is busy with something else there; try again in a moment.";
        match act {
            Act::Bisect(b) => {
                if repo.bisect.is_busy() {
                    return call.fail(busy);
                }
                let path = repo.path().to_string();
                match b {
                    Bisect::Start(begin) => {
                        repo.bisect.start_with(ctx, &path, begin);
                        repo.bisect.agent = Some(call.client.clone());
                        call.ok("Started: Git Raccoon checks out the first commit to test. get_bisect says which, and what's left; answer with bisect_answer (or bisect_run with a command).");
                    }
                    Bisect::Answer(v) => {
                        repo.bisect.answer(ctx, &path, v);
                        call.ok("Answered. get_bisect says what's next (or what was found).");
                    }
                    Bisect::Run(command, limit) => {
                        repo.bisect.run_command(ctx, &path, command, limit);
                        call.ok("Running it on each commit. get_bisect says how it's going, and what was found.");
                    }
                    Bisect::Stop => {
                        repo.bisect.stop(ctx, &path);
                        call.ok("Stopping: back where it started.");
                    }
                }
            }
            Act::Op(op) => {
                let (report, until) = mpsc::channel();
                if repo.run_reporting(ctx, op, report) {
                    self.agent_waiting.push(Waiting {
                        call,
                        until: Until::Report(until),
                        done: format!("Done: {what}."),
                    });
                } else {
                    call.fail(busy);
                }
            }
            Act::Commit { message, amend } => {
                let (report, until) = mpsc::channel();
                if repo.commit_reporting(ctx, message, amend, report) {
                    self.agent_waiting.push(Waiting {
                        call,
                        until: Until::Report(until),
                        done: "Committed.".into(),
                    });
                } else {
                    call.fail(busy);
                }
            }
            Act::Draft(draft) => {
                let action = crate::ui::request::Action::SaveDraft(draft, None);
                match super::request_action(
                    repo,
                    &mut self.settings.review_drafts,
                    ctx,
                    action,
                    pull,
                ) {
                    Ok(()) => {
                        call.ok("Added to the drafts: the person sends it with their review.")
                    }
                    Err(e) => call.fail(e),
                }
            }
            Act::Resolve(thread, to) => {
                let action = crate::ui::request::Action::Resolve(thread, to);
                match super::request_action(
                    repo,
                    &mut self.settings.review_drafts,
                    ctx,
                    action,
                    pull,
                ) {
                    Ok(()) => self.agent_waiting.push(Waiting {
                        call,
                        until: Until::Forge(path),
                        done: if to {
                            "Resolved.".into()
                        } else {
                            "Reopened.".into()
                        },
                    }),
                    Err(e) => call.fail(e),
                }
            }
        }
    }

    /// Tells agents how their changes went, once they have.
    fn agent_progress(&mut self) {
        let waiting = std::mem::take(&mut self.agent_waiting);
        for w in waiting {
            let outcome = match &w.until {
                Until::Report(until) => match until.try_recv() {
                    Ok(result) => Some(result),
                    Err(mpsc::TryRecvError::Empty) => None,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Some(Err("It stopped before it finished.".into()))
                    }
                },
                Until::Forge(path) => {
                    let path = path.clone();
                    match self.repo_at(&path).and_then(|r| r.forge.as_ref()) {
                        None => Some(Err(
                            "The request's view closed before the forge answered.".into()
                        )),
                        Some(f) if f.state.sending.is_some() => None,
                        Some(f) => Some(match &f.state.sent {
                            Some(Err(e)) => Err(e.clone()),
                            _ => Ok(()),
                        }),
                    }
                }
            };
            match outcome {
                None => self.agent_waiting.push(w),
                Some(Ok(())) => w.call.ok(w.done),
                Some(Err(e)) => w.call.fail(format!("It didn't work: {e}")),
            }
        }
    }

    /// Asks the person about an agent's change.
    fn agent_question(&mut self, ctx: &egui::Context) {
        let Some(asking) = &self.agent_asking else {
            return;
        };
        let mut answer: Option<Answer> = None;
        let name = std::path::Path::new(&asking.repo)
            .file_name()
            .map_or_else(|| asking.repo.clone(), |n| n.to_string_lossy().into_owned());
        let modal = egui::Modal::new(egui::Id::new("agent-asks")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(
                RichText::new(format!("{} would like to {}", asking.call.client, asking.what))
                    .strong()
                    .size(15.0),
            );
            ui.label(RichText::new(format!("In {name}")).color(theme::muted()));
            if !asking.detail.is_empty() {
                ui.add_space(6.0);
                egui::Frame::new()
                    .fill(theme::bg_raised())
                    .corner_radius(4)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            for line in &asking.detail {
                                ui.label(RichText::new(line).font(theme::mono(theme::MONO_SIZE)));
                            }
                        });
                    });
            }
            if asking.risky {
                ui.add_space(4.0);
                ui.label(
                    RichText::new("This can't be allowed for the whole session: it's asked every time.")
                        .size(12.0)
                        .color(theme::muted()),
                );
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                // Red for what can lose work.
                let allow = egui::Button::new(RichText::new("Allow").color(theme::on_accent()))
                    .fill(if asking.risky { theme::danger() } else { theme::accent_fill() });
                if ui.add(allow).clicked() {
                    answer = Some(Answer::Allow);
                }
                if !asking.risky
                    && ui
                        .button("Allow for this session")
                        .on_hover_text("Its changes from now on go ahead without asking, until it disconnects (except risky ones).")
                        .clicked()
                {
                    answer = Some(Answer::Session);
                }
                if ui.button("Deny").clicked() {
                    answer = Some(Answer::Deny);
                }
            });
        });
        if modal.should_close() && answer.is_none() {
            answer = Some(Answer::Deny);
        }
        let Some(answer) = answer else { return };
        let Some(asking) = self.agent_asking.take() else {
            return;
        };
        match answer {
            Answer::Deny => asking
                .call
                .fail("The person said no to this. Ask them what they'd like instead."),
            Answer::Session => {
                self.agent_trusted.insert(asking.call.connection);
                self.agent_do(ctx, asking);
            }
            Answer::Allow => self.agent_do(ctx, asking),
        }
    }
}

enum Answer {
    Allow,
    Session,
    Deny,
}

/// Answers a read, from the repository at `path`.
fn read(call: &Call, path: &str) -> Result<String, String> {
    let git = |args: &[&str]| git_output(path, args).map_err(|e| e.to_string());
    match call.tool.as_str() {
        "get_status" => {
            let status = working::working_status(path)
                .map_err(|e| e.to_string())?
                .unwrap_or_default();
            let refs = refs::load_refs(path).map_err(|e| e.to_string())?;
            let head = refs.local.iter().find(|b| b.is_head);
            let files = |list: &[working::WorkingFile]| {
                list.iter()
                    .map(|f| json!({ "path": f.path, "old_path": f.old_path, "change": format!("{:?}", f.status).to_lowercase() }))
                    .collect::<Vec<_>>()
            };
            Ok(pretty(&json!({
                "branch": head.map(|b| b.name.clone()),
                "detached": head.is_none(),
                "upstream": head.and_then(|b| b.upstream.as_ref()).map(|u| json!({
                    "name": u.name, "ahead": u.ahead, "behind": u.behind, "gone": u.gone,
                })),
                "staged": files(&status.staged),
                "unstaged": files(&status.unstaged),
                "conflicted": files(&status.conflicted),
                "in_progress": status.in_progress.map(|s| json!({ "what": format!("{:?}", s.what).to_lowercase(), "detail": s.detail })),
            })))
        }
        "get_log" | "search_commits" => {
            let searching = call.tool == "search_commits";
            let limit = call
                .number("limit")
                .unwrap_or(30)
                .clamp(1, if searching { 200 } else { 500 });
            let limit = limit.to_string();
            let skip = call.number("skip").unwrap_or(0).to_string();
            let mut args = vec![
                "log".to_string(),
                "--format=%H%x1f%an%x1f%ae%x1f%aI%x1f%D%x1f%s%x1e".into(),
                "-n".into(),
                limit,
                "--skip".into(),
                skip,
            ];
            if searching {
                let query = call
                    .str("query")
                    .ok_or("What to search for? Give `query`.")?;
                args.push("--all".into());
                match call.str("in").as_deref().unwrap_or("message") {
                    "author" => args.extend([
                        "-i".into(),
                        "--fixed-strings".into(),
                        format!("--author={query}"),
                    ]),
                    "changes" => args.push(format!("-S{query}")),
                    _ => args.extend([
                        "-i".into(),
                        "--fixed-strings".into(),
                        format!("--grep={query}"),
                    ]),
                }
            } else {
                match call.str("revision") {
                    Some(r) if r == "--all" => args.push(r),
                    Some(r) => {
                        plain("revision", &r)?;
                        args.push(r);
                    }
                    None => {}
                }
                if let Some(p) = call.str("path") {
                    args.extend(["--".into(), p]);
                }
            }
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let out = git(&args)?;
            let commits: Vec<Value> = out
                .split('\u{1e}')
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .filter_map(|r| {
                    let f: Vec<&str> = r.split('\u{1f}').collect();
                    (f.len() == 6).then(|| {
                        json!({
                            "id": f[0], "author": f[1], "email": f[2], "date": f[3],
                            "refs": if f[4].is_empty() { Value::Null } else { json!(f[4]) },
                            "summary": f[5],
                        })
                    })
                })
                .collect();
            Ok(pretty(&json!(commits)))
        }
        "get_commit" => {
            let id = call.str("id").ok_or("Which commit? Give its `id`.")?;
            plain("commit", &id)?;
            let d = details::commit_details(path, &id).map_err(|e| e.to_string())?;
            Ok(pretty(&json!({
                "id": d.id,
                "parents": d.parents,
                "summary": d.summary,
                "body": d.body,
                "author": { "name": d.author.name, "email": d.author.email, "time": d.author.time },
                "committer": { "name": d.committer.name, "email": d.committer.email, "time": d.committer.time },
                "additions": d.additions,
                "deletions": d.deletions,
                "files": d.files.iter().map(|f| json!({
                    "path": f.path, "old_path": f.old_path, "change": format!("{:?}", f.status).to_lowercase(),
                    "additions": f.additions, "deletions": f.deletions, "binary": f.binary,
                })).collect::<Vec<_>>(),
            })))
        }
        "get_diff" => {
            let mut args: Vec<String> = match call.str("commit") {
                Some(id) => {
                    plain("commit", &id)?;
                    vec![
                        "show".into(),
                        "--format=commit %H%n%n    %s%n".into(),
                        "--patch".into(),
                        id,
                    ]
                }
                None if call.bool("staged") == Some(true) => vec!["diff".into(), "--cached".into()],
                None => vec!["diff".into()],
            };
            if let Some(p) = call.str("path") {
                args.extend(["--".into(), p]);
            }
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            let out = git(&args)?;
            Ok(if out.trim().is_empty() {
                "No changes.".to_string()
            } else {
                capped(out)
            })
        }
        "get_refs" => {
            let r = refs::load_refs(path).map_err(|e| e.to_string())?;
            Ok(pretty(&json!({
                "branches": r.local.iter().map(|b| json!({
                    "name": b.name, "commit": b.target, "checked_out": b.is_head,
                    "upstream": b.upstream.as_ref().map(|u| json!({ "name": u.name, "ahead": u.ahead, "behind": u.behind, "gone": u.gone })),
                })).collect::<Vec<_>>(),
                "remotes": r.remotes.iter().map(|rm| json!({
                    "name": rm.name, "url": rm.url,
                    "branches": rm.branches.iter().map(|b| json!({ "name": b.full_name, "commit": b.target })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "tags": r.tags.iter().map(|t| json!({ "name": t.name, "commit": t.target, "annotated": t.annotated })).collect::<Vec<_>>(),
                "stashes": r.stashes.iter().map(|s| json!({ "index": s.index, "message": s.message, "commit": s.target })).collect::<Vec<_>>(),
            })))
        }
        "get_conflicts" => conflicts_now(path, call.str("path").as_deref()),
        other => Err(format!("Git Raccoon has no tool {other}.")),
    }
}

/// The conflicts in progress, for an agent: the sides, and each file's
/// conflicts (all, or `only` one file).
fn conflicts_now(path: &str, only: Option<&str>) -> Result<String, String> {
    use git_raccoon_core::{conflict, merge};
    let Some(state) = merge::in_progress(path).map_err(|e| e.to_string())? else {
        return Ok("Nothing is in progress: no merge, rebase, cherry-pick or revert.".into());
    };
    let side =
        |s: &merge::Side| json!({ "name": s.name, "commit": s.commit, "summary": s.summary });
    // What rules' merge tools did during it (merge drivers).
    let tools_did = git_raccoon_core::rules::driver_results_now(path);
    let mut files = Vec::new();
    for f in conflict::files(path).map_err(|e| e.to_string())? {
        if only.is_some_and(|o| o != f.path) {
            continue;
        }
        let kind = match f.kind {
            conflict::Kind::BothChanged => "both changed it",
            conflict::Kind::BothAdded => "both added it",
            conflict::Kind::DeletedOnOurs => "deleted by ours, changed by theirs",
            conflict::Kind::DeletedOnTheirs => "changed by ours, deleted by theirs",
            conflict::Kind::Other => "other",
        };
        let mut entry = json!({ "path": f.path, "how": kind, "binary": f.binary });
        if let Some(r) = tools_did.iter().find(|r| r.file == f.path && !r.merged) {
            entry["tool_left_it"] = json!({ "tool": r.tool, "why": r.why });
        }
        if !f.binary
            && let Ok(text) = conflict::read(path, &f.path)
            && let Some(mut chunks) = conflict::parse(&text)
        {
            conflict::with_bases(path, &f.path, &mut chunks);
            entry["conflicts"] = json!(
                conflict::conflicts(&chunks)
                    .map(|c| json!({ "base": c.base, "ours": c.ours, "theirs": c.theirs }))
                    .collect::<Vec<_>>()
            );
            entry["text"] = json!(text);
        }
        files.push(entry);
    }
    if let Some(o) = only
        && files.is_empty()
    {
        return Err(format!("{o} isn't conflicted."));
    }
    Ok(capped(pretty(&json!({
        "in_progress": format!("{:?}", state.what).to_lowercase(),
        "detail": state.detail,
        "ours": side(&state.ours),
        "theirs": side(&state.theirs),
        "step": state.step.map(|(n, of)| json!({ "step": n, "of": of })),
        "files": files,
        "merged_by_tools": tools_did
            .iter()
            .filter(|r| r.merged)
            .map(|r| json!({ "path": r.file, "tool": r.tool }))
            .collect::<Vec<_>>(),
        "next": "propose_resolution shows the person a file's resolved text, with your reasoning.",
    }))))
}
