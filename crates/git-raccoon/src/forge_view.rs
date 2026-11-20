//! A repository's forge: which one it's on, and its pull/merge requests
//! and issues, loaded in the background.

use eframe::egui;

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::forge::review::{Anchor, Draft, Finish, Method, Overview};
use crate::forge::{self, Account, Issue, Project, Request};
use crate::highlight::RenderedDiff;
use crate::task::Task;
use git_raccoon_core::file_history::Blame;

/// What's open in the details pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    Request(u64),
    Issue(u64),
}

/// A request with everything its view shows.
pub type Viewed = (Request, Overview);

/// The tabs of a request's view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Conversation,
    Commits,
    Changes,
}

/// A comment being written: on a line or range, a reply, or (both `None`)
/// on the whole request; `editing` is the draft it changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Composer {
    pub at: Option<Anchor>,
    pub reply_to: Option<String>,
    pub body: String,
    pub editing: Option<usize>,
}

/// The Finish review window.
#[derive(Debug, Clone, PartialEq)]
pub struct Finishing {
    pub summary: String,
    pub finish: Finish,
}

/// The Merge window.
#[derive(Debug, Clone, PartialEq)]
pub struct Merging {
    pub method: Method,
    pub message: String,
    pub delete_branch: bool,
}

/// Something sent to the forge for the open request.
pub enum Sent {
    Review,
    Resolve,
    Merge,
    Apply,
    Update,
    Comment,
}

/// How the open request's view stands: what's picked and being written.
#[derive(Default)]
pub struct RequestState {
    pub tab: Tab,
    /// The file open in Changes.
    pub file: Option<String>,
    /// Lines picked in its diff: (hunk, line), and where Shift picks from.
    pub picked: BTreeSet<(usize, usize)>,
    pub pick_from: Option<(usize, usize)>,
    pub composer: Option<Composer>,
    /// The comment box at the bottom of the conversation.
    pub comment: String,
    pub finishing: Option<Finishing>,
    pub merging: Option<Merging>,
    /// Files' diffs, laid out (by path).
    pub rendered: HashMap<String, Arc<RenderedDiff>>,
    /// What's being sent, and how the last one went.
    pub sending: Option<(Sent, Task<Result<(), String>>)>,
    pub sent: Option<Result<String, String>>,
    /// A review went: its drafts can go.
    pub reviewed: bool,
    /// Laid-out Markdown.
    pub markdown: egui_commonmark::CommonMarkCache,
    /// Suggestions picked to apply together (GitLab), and the commit's message.
    pub batch: BTreeSet<String>,
    pub batch_message: String,
    /// This frame: the request is on GitLab, and its branch is checked out.
    pub gitlab: bool,
    /// Who's signed in (their comments can be edited and deleted).
    pub me: Option<String>,
    /// A posted comment being edited (its id, and the new text), or about
    /// to be deleted (its id).
    pub editing_comment: Option<(String, String)>,
    pub deleting_comment: Option<String>,
    pub on_branch: bool,
    /// How Changes shows a file: side by side, the whole file, with blame
    /// (kept from one request to the next).
    pub split: bool,
    pub whole: bool,
    pub blame: bool,
    /// Checking out met uncommitted changes: ask what to do.
    pub changes_in_the_way: bool,
    /// Your pending review on GitHub (its id), which drafts go in.
    pub pending_review: Option<String>,
    /// Drafts to keep on the forge, in order, the one going now, and how
    /// those done went (for the app to note their ids).
    pub syncs: std::collections::VecDeque<Sync>,
    syncing: Option<Task<Synced>>,
    pub synced: Vec<Synced>,
    /// The view was (re)loaded: the forge's drafts are to be merged in.
    pub merge_pending: bool,
    /// Its commits fetched here (without a branch), which the whole file and
    /// blame need: `None` until that's done.
    pub local: Option<Result<(), String>>,
    fetching: Option<Task<Result<(), String>>>,
    /// Files shown whole, and blamed, by path (`Err` if they couldn't be).
    pub wholes: HashMap<String, Result<Arc<RenderedDiff>, String>>,
    pub blames: HashMap<String, Result<Arc<Blame>, String>>,
    loading: Vec<(String, Task<Result<Loaded, String>>)>,
    /// The thread gone to with Previous / Next in Changes (its id), and
    /// whether it's still to be scrolled to.
    pub at_thread: Option<String>,
    pub scroll_to_thread: bool,
}

/// Keeping a draft on the forge: saving (or changing) it, or deleting it.
pub enum Sync {
    Save(Draft),
    Delete(String),
}

/// How keeping a draft on the forge went: the draft (by its local id), its
/// id there, and the pending review's (GitHub).
pub struct Synced {
    pub local: u64,
    pub remote: Result<Option<String>, String>,
    pub review: Option<String>,
}

/// A file loaded from the request's commits.
enum Loaded {
    Whole(Arc<RenderedDiff>),
    Blame(Arc<Blame>),
}

/// A page of requests and one of issues, each with whether there are more.
type Lists = (
    Result<(Vec<Request>, bool), String>,
    Result<(Vec<Issue>, bool), String>,
);

/// The next page of requests (`true`) or issues, loading.
type More = (bool, Task<Result<(Vec<Request>, Vec<Issue>, bool), String>>);

/// The forge of a repository, and what it has.
pub struct ForgeView {
    pub project: Project,
    /// The remote it's on ("origin").
    pub remote: String,
    /// The account for it, if signed in.
    pub account: Option<Account>,
    /// `None` while loading.
    pub requests: Option<Result<Vec<Request>, String>>,
    pub issues: Option<Result<Vec<Issue>, String>>,
    loading: Option<Task<Lists>>,
    /// There are more requests, or issues, than listed; the pages listed.
    pub more_requests: bool,
    pub more_issues: bool,
    pages: (u32, u32),
    more: Option<More>,
    pub open: Option<Opened>,
    /// The open request's view (with its reviews and checks), once loaded.
    pub details: Option<(u64, Result<Viewed, String>)>,
    details_task: Option<(u64, Task<Result<Viewed, String>>)>,
    pub state: RequestState,
    /// Why the next page couldn't be listed.
    pub load_more_error: Option<String>,
    /// Something was sent: reload the view (and the list) next.
    pub reloading: bool,
}

impl ForgeView {
    /// The forge of a repository with these remotes (name, URL), if it's on
    /// one: "origin" first.
    pub fn find(remotes: &[(String, Option<String>)], accounts: &[Account]) -> Option<ForgeView> {
        let managed: Vec<String> = accounts
            .iter()
            .filter(|a| a.kind == forge::Kind::GitLab)
            .map(|a| a.base.clone())
            .collect();
        let mut ordered: Vec<&(String, Option<String>)> = remotes.iter().collect();
        ordered.sort_by_key(|(name, _)| name != "origin");
        let (remote, project) = ordered.into_iter().find_map(|(name, url)| {
            url.as_deref()
                .and_then(|u| forge::project(u, &managed))
                .map(|p| (name.clone(), p))
        })?;
        let account = accounts.iter().find(|a| a.serves(&project)).cloned();
        Some(ForgeView {
            project,
            remote,
            account,
            requests: None,
            issues: None,
            loading: None,
            more_requests: false,
            more_issues: false,
            pages: (1, 1),
            more: None,
            open: None,
            details: None,
            details_task: None,
            state: RequestState::default(),
            load_more_error: None,
            reloading: false,
        })
    }

    /// Whether this is the same forge and account as `other` (so what's
    /// loaded still holds).
    pub fn same_as(&self, other: &ForgeView) -> bool {
        self.project == other.project
            && self.account == other.account
            && self.remote == other.remote
    }

    /// Loads (again) the requests and issues, if signed in.
    pub fn load(&mut self, ctx: &egui::Context) {
        let Some(account) = self.account.clone() else {
            return;
        };
        let project = self.project.clone();
        self.loading = Some(Task::spawn(ctx, move || {
            match forge::accounts::session(&account, &project) {
                Ok(s) => (s.requests(1), s.issues(1)),
                Err(e) => (Err(e.clone()), Err(e)),
            }
        }));
        if let Some(Opened::Request(n)) = self.open {
            self.load_details(ctx, n);
        }
    }

    /// Lists the next page of requests (`true`) or issues.
    pub fn load_more(&mut self, ctx: &egui::Context, requests: bool) {
        let Some(account) = self.account.clone() else {
            return;
        };
        if self.more.is_some() {
            return;
        }
        self.load_more_error = None;
        let project = self.project.clone();
        let page = if requests { self.pages.0 } else { self.pages.1 } + 1;
        self.more = Some((
            requests,
            Task::spawn(ctx, move || {
                let s = forge::accounts::session(&account, &project)?;
                if requests {
                    s.requests(page)
                        .map(|(list, more)| (list, Vec::new(), more))
                } else {
                    s.issues(page).map(|(list, more)| (Vec::new(), list, more))
                }
            }),
        ));
    }

    /// Whether the next page of requests (`true`) or issues is loading.
    pub fn loading_more(&self, requests: bool) -> bool {
        self.more.as_ref().is_some_and(|(r, _)| *r == requests)
    }

    /// Whether it's loading.
    pub fn is_loading(&self) -> bool {
        self.loading.is_some() || self.details_task.is_some()
    }

    /// Opens a request in the details pane, loading its details.
    pub fn open_request(&mut self, ctx: &egui::Context, number: u64) {
        if self.open != Some(Opened::Request(number)) {
            let RequestState {
                split,
                whole,
                blame,
                ..
            } = self.state;
            self.state = RequestState {
                split,
                whole,
                blame,
                ..RequestState::default()
            };
        }
        self.open = Some(Opened::Request(number));
        if self.details.as_ref().is_none_or(|(n, _)| *n != number) {
            self.details = None;
            self.load_details(ctx, number);
        }
    }

    fn load_details(&mut self, ctx: &egui::Context, number: u64) {
        let Some(account) = self.account.clone() else {
            return;
        };
        let project = self.project.clone();
        self.details_task = Some((
            number,
            Task::spawn(ctx, move || {
                forge::accounts::session(&account, &project)?.view(number)
            }),
        ));
    }

    /// Reloads the open request's view (after sending something).
    pub fn reload_view(&mut self, ctx: &egui::Context) {
        if let Some(Opened::Request(n)) = self.open {
            self.load_details(ctx, n);
        }
    }

    /// Whether the open request's view is loading.
    pub fn view_loading(&self) -> bool {
        self.details_task.is_some()
    }

    /// Sends something about the open request, in the background.
    pub fn send(
        &mut self,
        ctx: &egui::Context,
        what: Sent,
        work: impl FnOnce(&forge::Session) -> Result<(), String> + Send + 'static,
    ) {
        let Some(account) = self.account.clone() else {
            return;
        };
        let project = self.project.clone();
        self.state.sent = None;
        self.state.sending = Some((
            what,
            Task::spawn(ctx, move || {
                work(&forge::accounts::session(&account, &project)?)
            }),
        ));
    }

    /// The ref a request's changes can be fetched from.
    pub fn session_head_ref(&self, number: u64) -> String {
        match self.project.kind {
            forge::Kind::GitHub => format!("pull/{number}/head"),
            forge::Kind::GitLab => format!("merge-requests/{number}/head"),
        }
    }

    /// Keeps drafts on the forge, one at a time (GitHub's pending review is
    /// made by the first).
    pub fn keep_drafts(&mut self, ctx: &egui::Context) {
        let Some(Opened::Request(number)) = self.open else {
            return;
        };
        if let Some(task) = &self.state.syncing
            && let Some(done) = task.poll()
        {
            if done.review.is_some() {
                self.state.pending_review = done.review.clone();
            }
            self.state.synced.push(done);
            self.state.syncing = None;
        }
        if self.state.syncing.is_some() {
            return;
        }
        let (Some(account), Some((_, o))) = (self.account.clone(), self.viewed().cloned()) else {
            return;
        };
        let Some(job) = self.state.syncs.pop_front() else {
            return;
        };
        let project = self.project.clone();
        let review = self.state.pending_review.clone();
        self.state.syncing = Some(Task::spawn(ctx, move || {
            let session = match forge::accounts::session(&account, &project) {
                Ok(s) => s,
                Err(e) => {
                    return Synced {
                        local: 0,
                        remote: Err(e),
                        review: None,
                    };
                }
            };
            match job {
                Sync::Save(d) => match session.save_draft(number, &o, review, &d) {
                    Ok((remote, review)) => Synced {
                        local: d.local,
                        remote: Ok(remote),
                        review,
                    },
                    Err(e) => Synced {
                        local: d.local,
                        remote: Err(e),
                        review: None,
                    },
                },
                Sync::Delete(remote) => Synced {
                    local: 0,
                    remote: session.delete_draft(number, &remote).map(|()| None),
                    review: None,
                },
            }
        }));
    }

    /// Fetches the open request's commits here, once its view is loaded,
    /// and loads the open file whole, or blamed, when that's shown.
    pub fn keep_local(&mut self, ctx: &egui::Context, root: &str) {
        let Some((r, o)) = self.viewed().cloned() else {
            return;
        };
        let state = &mut self.state;
        if let Some(task) = &state.fetching
            && let Some(done) = task.poll()
        {
            state.local = Some(done);
            state.fetching = None;
        }
        if state.local.is_none() && state.fetching.is_none() {
            let (root, remote) = (root.to_string(), self.remote.clone());
            let head = match self.project.kind {
                forge::Kind::GitHub => format!("pull/{}/head", r.number),
                forge::Kind::GitLab => format!("merge-requests/{}/head", r.number),
            };
            let (target, sha) = (r.target.clone(), o.refs.head.clone());
            state.fetching = Some(Task::spawn(ctx, move || {
                use git_raccoon_core::request;
                if !request::has(&root, &sha) {
                    request::fetch(&root, &remote, &[&head, &target])
                        .map_err(|e| format!("its commits couldn't be fetched: {e}"))?;
                }
                if request::has(&root, &sha) {
                    Ok(())
                } else {
                    Err("its commits aren't on the remote".into())
                }
            }));
        }
        // What's finished loading.
        let mut i = 0;
        while i < state.loading.len() {
            if let Some(result) = state.loading[i].1.poll() {
                let (key, _) = state.loading.remove(i);
                let (kind, path) = key.split_once('\0').unwrap_or(("", &key));
                let path = path.to_string();
                match result {
                    Ok(Loaded::Whole(d)) => {
                        state.wholes.insert(path, Ok(d));
                    }
                    Ok(Loaded::Blame(b)) => {
                        state.blames.insert(path, Ok(b));
                    }
                    Err(e) => {
                        if kind == "blame" {
                            state.blames.insert(path, Err(e));
                        } else {
                            state.wholes.insert(path, Err(e));
                        }
                    }
                }
            } else {
                i += 1;
            }
        }
        if !matches!(state.local, Some(Ok(()))) {
            return;
        }
        let Some(file) = state
            .file
            .as_ref()
            .and_then(|p| o.files.iter().find(|f| &f.path == p))
            .cloned()
        else {
            return;
        };
        let busy = |state: &RequestState, key: &str| state.loading.iter().any(|(k, _)| k == key);
        let whole_key = format!("whole\0{}", file.path);
        if state.whole && !state.wholes.contains_key(&file.path) && !busy(state, &whole_key) {
            let (root, refs, file) = (root.to_string(), o.refs.clone(), file.clone());
            let task = Task::spawn(ctx, move || {
                use git_raccoon_core::{details, request};
                let base = request::merge_base(&root, &refs.head, &refs.base)
                    .map_err(|e| e.to_string())?;
                let old_path = file.old_path.as_deref().unwrap_or(&file.path);
                let old = request::file_at(&root, &base, old_path)
                    .map_err(|e| e.to_string())?
                    .unwrap_or_default();
                let diff = crate::ui::request::file_diff(&file);
                let runs = details::unchanged_around(&diff, &old);
                Ok(Loaded::Whole(Arc::new(RenderedDiff::whole(
                    diff, runs, true,
                ))))
            });
            state.loading.push((whole_key, task));
        }
        let blame_key = format!("blame\0{}", file.path);
        if state.blame && !state.blames.contains_key(&file.path) && !busy(state, &blame_key) {
            let (root, head, path) = (root.to_string(), o.refs.head.clone(), file.path.clone());
            let task = Task::spawn(ctx, move || {
                git_raccoon_core::file_history::blame(&root, &path, Some(&head))
                    .map(|b| Loaded::Blame(Arc::new(b)))
                    .map_err(|e| e.to_string())
            });
            state.loading.push((blame_key, task));
        }
    }

    /// The key a request's drafts are kept under.
    pub fn drafts_key(&self, number: u64) -> String {
        format!(
            "{}/{}{}",
            self.project.base,
            self.project.path,
            self.project.kind.number(number)
        )
    }

    /// The open request with everything its view shows, once loaded.
    pub fn viewed(&self) -> Option<&Viewed> {
        match (&self.details, self.open) {
            (Some((n, Ok(v))), Some(Opened::Request(m))) if *n == m => Some(v),
            _ => None,
        }
    }

    /// Takes in what's finished loading.
    pub fn poll(&mut self) {
        if let Some((requests, issues)) = self.loading.as_ref().and_then(Task::poll) {
            self.loading = None;
            self.more_requests = requests.as_ref().is_ok_and(|(_, more)| *more);
            self.more_issues = issues.as_ref().is_ok_and(|(_, more)| *more);
            self.pages = (1, 1);
            self.more = None;
            self.requests = Some(requests.map(|(list, _)| list));
            self.issues = Some(issues.map(|(list, _)| list));
        }
        if let Some((requests, task)) = &self.more
            && let Some(result) = task.poll()
        {
            let requests = *requests;
            self.more = None;
            match result {
                Ok((new_requests, new_issues, more)) => {
                    if requests {
                        self.pages.0 += 1;
                        self.more_requests = more;
                        if let Some(Ok(list)) = &mut self.requests {
                            for r in new_requests {
                                if !list.iter().any(|x| x.number == r.number) {
                                    list.push(r);
                                }
                            }
                        }
                    } else {
                        self.pages.1 += 1;
                        self.more_issues = more;
                        if let Some(Ok(list)) = &mut self.issues {
                            for i in new_issues {
                                if !list.iter().any(|x| x.number == i.number) {
                                    list.push(i);
                                }
                            }
                        }
                    }
                }
                Err(e) => self.load_more_error = Some(e),
            }
        }
        if let Some((n, task)) = &self.details_task
            && let Some(result) = task.poll()
        {
            if let Ok((_, o)) = &result {
                self.state.rendered.clear();
                self.state.pending_review = o.pending_review.clone();
                self.state.merge_pending = true;
            }
            // A reload that failed keeps what was shown.
            if result.is_ok() || self.details.as_ref().is_none_or(|(m, _)| m != n) {
                self.details = Some((*n, result));
            }
            self.details_task = None;
        }
        // What was sent: say how it went.
        if let Some((what, task)) = &self.state.sending
            && let Some(result) = task.poll()
        {
            let done = match what {
                Sent::Review => "Review sent.",
                Sent::Resolve => "Thread updated.",
                Sent::Merge => "Merged.",
                Sent::Apply => "Applied: GitLab committed it to the branch (pull to get it here).",
                Sent::Update => "It's being brought up to date: reload in a moment to see.",
                Sent::Comment => "Saved.",
            };
            let merged = matches!(what, Sent::Merge) && result.is_ok();
            if result.is_ok() {
                if matches!(what, Sent::Review) {
                    self.state.finishing = None;
                    self.state.reviewed = true;
                    self.state.pending_review = None;
                }
                if merged {
                    self.state.merging = None;
                }
                if matches!(what, Sent::Comment) {
                    self.state.editing_comment = None;
                    self.state.deleting_comment = None;
                }
                if matches!(what, Sent::Apply) {
                    self.state.batch.clear();
                    self.state.batch_message.clear();
                }
            }
            self.state.sent = Some(result.map(|()| done.to_string()));
            self.state.sending = None;
            self.reloading = true;
        }
    }

    /// The open request, as listed (before its details load).
    pub fn request(&self, number: u64) -> Option<&Request> {
        self.requests
            .as_ref()?
            .as_ref()
            .ok()?
            .iter()
            .find(|r| r.number == number)
    }

    pub fn issue(&self, number: u64) -> Option<&Issue> {
        self.issues
            .as_ref()?
            .as_ref()
            .ok()?
            .iter()
            .find(|i| i.number == number)
    }

    /// A request with its details, once loaded.
    pub fn detailed(&self, number: u64) -> Option<&Request> {
        match &self.details {
            Some((n, Ok((r, _)))) if *n == number => Some(r),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::Kind;

    fn account(kind: Kind, base: &str, login: &str) -> Account {
        Account {
            kind,
            base: base.into(),
            login: login.into(),
            client: None,
        }
    }

    #[test]
    fn signed_in_to_several_forges_at_once() {
        let accounts = [
            account(Kind::GitHub, "https://github.com", "milo"),
            account(Kind::GitLab, "https://gitlab.com", "milo"),
            account(Kind::GitLab, "https://git.example.com", "rocky"),
        ];
        let login = |url: &str| {
            let remotes = [("origin".to_string(), Some(url.to_string()))];
            ForgeView::find(&remotes, &accounts)
                .and_then(|f| f.account)
                .map(|a| (a.base, a.login))
        };
        assert_eq!(
            login("git@github.com:a/b.git"),
            Some(("https://github.com".into(), "milo".into()))
        );
        assert_eq!(
            login("https://gitlab.com/g/p.git"),
            Some(("https://gitlab.com".into(), "milo".into()))
        );
        assert_eq!(
            login("git@git.example.com:team/app.git"),
            Some(("https://git.example.com".into(), "rocky".into()))
        );
    }
}
