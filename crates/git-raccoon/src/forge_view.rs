//! A repository's forge: which one it's on, and its pull/merge requests
//! and issues, loaded in the background.

use eframe::egui;

use crate::forge::{self, Account, Issue, Project, Request};
use crate::task::Task;

/// What's open in the details pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opened {
    Request(u64),
    Issue(u64),
}

type Lists = (Result<Vec<Request>, String>, Result<Vec<Issue>, String>);

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
    pub open: Option<Opened>,
    /// The open request's details (reviews, checks), once loaded.
    pub details: Option<(u64, Result<Request, String>)>,
    details_task: Option<(u64, Task<Result<Request, String>>)>,
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
            open: None,
            details: None,
            details_task: None,
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
                Ok(s) => (s.requests(), s.issues()),
                Err(e) => (Err(e.clone()), Err(e)),
            }
        }));
        if let Some(Opened::Request(n)) = self.open {
            self.load_details(ctx, n);
        }
    }

    /// Whether it's loading.
    pub fn is_loading(&self) -> bool {
        self.loading.is_some() || self.details_task.is_some()
    }

    /// Opens a request in the details pane, loading its details.
    pub fn open_request(&mut self, ctx: &egui::Context, number: u64) {
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
                forge::accounts::session(&account, &project)?.details(number)
            }),
        ));
    }

    /// Takes in what's finished loading.
    pub fn poll(&mut self) {
        if let Some((requests, issues)) = self.loading.as_ref().and_then(Task::poll) {
            self.loading = None;
            self.requests = Some(requests);
            self.issues = Some(issues);
        }
        if let Some((n, task)) = &self.details_task
            && let Some(result) = task.poll()
        {
            self.details = Some((*n, result));
            self.details_task = None;
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
            Some((n, Ok(r))) if *n == number => Some(r),
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
            oauth: false,
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
