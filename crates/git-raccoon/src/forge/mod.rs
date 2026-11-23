//! Forges: GitHub, GitLab.com and self-managed GitLab. Which one a
//! repository is on (from its remote), the accounts signed in to them,
//! and their pull/merge requests and issues.
//!
//! The network calls block: run them off the UI thread.

pub mod accounts;
mod github;
mod gitlab;
mod http;
pub mod images;
pub mod oauth;
pub mod published;
pub mod review;
pub mod ssh_keys;
pub mod templates;

use serde::{Deserialize, Serialize};

pub use accounts::Account;

/// What kind of forge a host is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    GitHub,
    GitLab,
}

impl Kind {
    /// Whether a review can ask for changes (GitLab's API can't).
    pub fn can_request_changes(self) -> bool {
        self == Kind::GitHub
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::GitHub => "GitHub",
            Kind::GitLab => "GitLab",
        }
    }

    /// "Pull request" or "Merge request".
    pub fn request(self) -> &'static str {
        match self {
            Kind::GitHub => "Pull request",
            Kind::GitLab => "Merge request",
        }
    }

    /// "#12" or "!12".
    pub fn number(self, n: u64) -> String {
        match self {
            Kind::GitHub => format!("#{n}"),
            Kind::GitLab => format!("!{n}"),
        }
    }
}

/// A repository on a forge: where it is, and its path there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub kind: Kind,
    /// The forge's web address, like "https://github.com".
    pub base: String,
    /// Its host, like "github.com" or "gitlab.example.com:8443".
    pub host: String,
    /// "owner/repo", or "group/subgroup/project" on GitLab.
    pub path: String,
}

impl Project {
    /// Its web page.
    pub fn url(&self) -> String {
        format!("{}/{}", self.base, self.path)
    }

    /// GitLab puts a project's own pages under "/-/".
    fn pages(&self) -> String {
        match self.kind {
            Kind::GitHub => self.url(),
            Kind::GitLab => format!("{}/-", self.url()),
        }
    }

    /// A commit's page.
    pub fn commit_url(&self, id: &str) -> String {
        format!("{}/commit/{id}", self.pages())
    }

    /// A branch's or tag's files.
    pub fn tree_url(&self, name: &str) -> String {
        format!("{}/tree/{}", self.pages(), path_encode(name))
    }
}

/// A ref name made safe for a URL path (its slashes kept).
fn path_encode(name: &str) -> String {
    name.split('/')
        .map(http::encode)
        .collect::<Vec<_>>()
        .join("/")
}

/// The web pages of a repository's remotes that are on a forge.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Web {
    /// Remote name and project, "origin" first.
    remotes: Vec<(String, Project)>,
}

impl Web {
    /// From the remotes (name, URL); `self_managed` as for [`project`].
    pub fn new(remotes: &[(String, Option<String>)], self_managed: &[String]) -> Web {
        let mut remotes: Vec<(String, Project)> = remotes
            .iter()
            .filter_map(|(name, url)| {
                project(url.as_deref()?, self_managed).map(|p| (name.clone(), p))
            })
            .collect();
        remotes.sort_by_key(|(name, _)| name != "origin");
        Web { remotes }
    }

    /// "Open on GitHub" and the like.
    pub fn label(project: &Project) -> String {
        format!("Open on {}", project.kind.name())
    }

    /// A remote's project.
    pub fn remote(&self, name: &str) -> Option<&Project> {
        self.remotes.iter().find(|(n, _)| n == name).map(|(_, p)| p)
    }

    /// The main remote's project ("origin", or else the first).
    pub fn main(&self) -> Option<&Project> {
        self.remotes.first().map(|(_, p)| p)
    }

    /// A commit's page, on the main remote.
    pub fn commit(&self, id: &str) -> Option<(String, String)> {
        let p = self.main()?;
        Some((Web::label(p), p.commit_url(id)))
    }

    /// A remote branch's page ("origin/feature").
    pub fn branch(&self, full_name: &str) -> Option<(String, String)> {
        // The longest remote name that prefixes it (names can hold "/").
        let (remote, p) = self
            .remotes
            .iter()
            .filter(|(n, _)| full_name.starts_with(&format!("{n}/")))
            .max_by_key(|(n, _)| n.len())?;
        let branch = &full_name[remote.len() + 1..];
        Some((Web::label(p), p.tree_url(branch)))
    }

    /// A remote's page.
    pub fn remote_page(&self, name: &str) -> Option<(String, String)> {
        let p = self.remote(name)?;
        Some((Web::label(p), p.url()))
    }

    /// A tag's page, on the main remote.
    pub fn tag(&self, name: &str) -> Option<(String, String)> {
        let p = self.main()?;
        Some((Web::label(p), p.tree_url(name)))
    }
}

/// The host and path in a remote URL: "https://github.com/a/b.git",
/// "git@github.com:a/b.git", "ssh://git@host:2222/g/s/p.git". The host
/// keeps a port only for web addresses (an SSH port isn't the web's).
fn host_and_path(url: &str) -> Option<(String, String, bool)> {
    let url = url.trim();
    let (host, path, web) = if let Some(rest) = url
        .strip_prefix("https://")
        .map(|r| (r, true))
        .or_else(|| url.strip_prefix("http://").map(|r| (r, false)))
    {
        let (rest, secure) = rest;
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next()?;
        // A plain-http address is only ever a local test server.
        if !secure && !host.starts_with("127.0.0.1") && !host.starts_with("localhost") {
            return None;
        }
        (host.to_string(), path.to_string(), true)
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        let (authority, path) = rest.split_once('/')?;
        let host = authority.rsplit('@').next()?;
        let host = host.split(':').next()?;
        (host.to_string(), path.to_string(), false)
    } else {
        // scp-like: [user@]host:path
        let (authority, path) = url.split_once(':')?;
        if authority.contains('/') || path.starts_with("//") {
            return None;
        }
        let host = authority.rsplit('@').next()?;
        (host.to_string(), path.to_string(), false)
    };
    let path = path
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string();
    (!host.is_empty() && path.contains('/')).then_some((host.to_lowercase(), path, web))
}

/// The forge project a remote URL points at, if it's GitHub, GitLab.com or
/// one of the `self_managed` GitLab hosts (from accounts; web addresses
/// like "https://gitlab.example.com").
pub fn project(remote_url: &str, self_managed: &[String]) -> Option<Project> {
    let (host, path, _) = host_and_path(remote_url)?;
    let bare = host.split(':').next().unwrap_or(&host).to_string();
    let (kind, base) = match bare.as_str() {
        "github.com" => (Kind::GitHub, "https://github.com".to_string()),
        "gitlab.com" => (Kind::GitLab, "https://gitlab.com".to_string()),
        _ => {
            let base = self_managed.iter().find(|base| {
                let (b, _, _) = host_and_path(&format!("{base}/x/y")).unwrap_or_default();
                b == host || b.split(':').next() == Some(bare.as_str())
            })?;
            (Kind::GitLab, base.trim_end_matches('/').to_string())
        }
    };
    let host = base
        .split_once("://")
        .map_or(base.as_str(), |(_, h)| h)
        .to_string();
    Some(Project {
        kind,
        base,
        host,
        path,
    })
}

/// How a pull/merge request's reviews stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Review {
    None,
    Approved,
    ChangesRequested,
}

/// How its checks (CI) stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Checks {
    pub passed: u32,
    pub failed: u32,
    pub pending: u32,
}

impl Checks {
    pub fn is_empty(&self) -> bool {
        self.passed + self.failed + self.pending == 0
    }
}

/// A pull request (GitHub) or merge request (GitLab).
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub number: u64,
    pub title: String,
    pub author: String,
    /// The branch with the changes, and the one it goes into.
    pub source: String,
    pub target: String,
    /// From a fork, not a branch of this repository.
    pub from_fork: bool,
    pub draft: bool,
    pub url: String,
    pub body: String,
    /// Seconds since the Unix epoch.
    pub created: i64,
    pub comments: u64,
    /// Filled in by [`details`]: who reviewed it, and how its checks went.
    pub reviews: Vec<(String, Review)>,
    pub review: Review,
    pub checks: Checks,
}

/// An issue.
#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub labels: Vec<String>,
    pub url: String,
    pub body: String,
    pub created: i64,
    pub comments: u64,
}

/// What's asked for a new pull/merge request.
#[derive(Debug, Clone, Default)]
pub struct NewRequest {
    pub source: String,
    pub target: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
    /// Logins.
    pub reviewers: Vec<String>,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    /// GitLab: delete the source branch, and squash, when it's merged.
    pub remove_source: bool,
    pub squash: bool,
}

/// Who can review and be assigned a project's requests, and its labels:
/// what a new request can be given.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Choices {
    /// Logins.
    pub people: Vec<String>,
    /// Names, with their colours ("#d73a4a", or "" if none).
    pub labels: Vec<(String, String)>,
}

/// A signed-in connection to a project.
pub struct Session {
    pub project: Project,
    token: String,
}

impl Session {
    pub fn new(project: Project, token: String) -> Self {
        Session { project, token }
    }

    /// What fetching the project's images with this account needs.
    pub fn for_images(&self) -> images::Signed {
        let host = self
            .project
            .host
            .split(':')
            .next()
            .unwrap_or_default()
            .to_string();
        let mut hosts = vec![self.project.host.clone(), host];
        hosts.dedup();
        images::Signed {
            kind: self.project.kind,
            project_web: format!("{}/{}", self.project.base, self.project.path),
            project_api: match self.project.kind {
                Kind::GitHub => String::new(),
                Kind::GitLab => format!(
                    "{}/api/v4/projects/{}",
                    self.project.base,
                    http::encode(&self.project.path)
                ),
            },
            hosts,
            token: self.token.clone(),
        }
    }

    /// A page (from 1) of the open pull/merge requests, newest first, and
    /// whether there are more.
    pub fn requests(&self, page: u32) -> Result<(Vec<Request>, bool), String> {
        match self.project.kind {
            Kind::GitHub => github::requests(self, page),
            Kind::GitLab => gitlab::requests(self, page),
        }
    }

    /// One request with its reviews and checks.
    pub fn details(&self, number: u64) -> Result<Request, String> {
        match self.project.kind {
            Kind::GitHub => github::details(self, number),
            Kind::GitLab => gitlab::details(self, number),
        }
    }

    /// A page (from 1) of the open issues, newest first, and whether there
    /// are more.
    pub fn issues(&self, page: u32) -> Result<(Vec<Issue>, bool), String> {
        match self.project.kind {
            Kind::GitHub => github::issues(self, page),
            Kind::GitLab => gitlab::issues(self, page),
        }
    }

    /// The branch new requests go into by default.
    pub fn default_branch(&self) -> Result<String, String> {
        match self.project.kind {
            Kind::GitHub => github::default_branch(self),
            Kind::GitLab => gitlab::default_branch(self),
        }
    }

    /// Who can be asked to review or be assigned, and the labels.
    pub fn choices(&self) -> Result<Choices, String> {
        match self.project.kind {
            Kind::GitHub => github::choices(self),
            Kind::GitLab => gitlab::choices(self),
        }
    }

    /// Opens a pull/merge request. Returns it.
    pub fn create(&self, new: &NewRequest) -> Result<Request, String> {
        match self.project.kind {
            Kind::GitHub => github::create(self, new),
            Kind::GitLab => gitlab::create(self, new),
        }
    }

    /// Everything for a request's view: the request with its reviews and
    /// checks, and its files, commits, conversation and threads.
    pub fn view(&self, number: u64) -> Result<(Request, review::Overview), String> {
        match self.project.kind {
            Kind::GitHub => github::view(self, number),
            Kind::GitLab => gitlab::view(self, number),
        }
    }

    /// Keeps a draft on the forge too (GitLab's draft notes, GitHub's
    /// pending review), or changes it there. Returns its id there (`None`
    /// if the forge can't hold it) and, on GitHub, the pending review's.
    pub fn save_draft(
        &self,
        number: u64,
        o: &review::Overview,
        pending_review: Option<String>,
        draft: &review::Draft,
    ) -> Result<(Option<String>, Option<String>), String> {
        match self.project.kind {
            Kind::GitHub => github::save_draft(self, o, pending_review, draft),
            Kind::GitLab => gitlab::save_draft(self, number, o, draft).map(|id| (Some(id), None)),
        }
    }

    /// Takes a draft off the forge.
    pub fn delete_draft(&self, number: u64, remote: &str) -> Result<(), String> {
        match self.project.kind {
            Kind::GitHub => github::delete_draft(self, remote),
            Kind::GitLab => gitlab::delete_draft(self, number, remote),
        }
    }

    /// Sends a review: its drafts, a summary, and how it ends.
    pub fn submit(
        &self,
        number: u64,
        o: &review::Overview,
        pending_review: Option<String>,
        (drafts, summary, finish): (&[review::Draft], &str, review::Finish),
    ) -> Result<(), String> {
        match self.project.kind {
            Kind::GitHub => {
                github::submit(self, number, o, pending_review, (drafts, summary, finish))
            }
            Kind::GitLab => gitlab::submit(self, number, o, (drafts, summary, finish)),
        }
    }

    /// Applies suggestions on the forge, in one commit (GitLab only:
    /// GitHub's API can't).
    pub fn apply(&self, ids: &[String], message: &str) -> Result<(), String> {
        match self.project.kind {
            Kind::GitHub => Err("GitHub can't apply suggestions through its API".into()),
            Kind::GitLab => gitlab::apply(self, ids, message),
        }
    }

    /// Changes a posted comment's text.
    pub fn edit_comment(
        &self,
        number: u64,
        comment: &review::Comment,
        body: &str,
    ) -> Result<(), String> {
        match self.project.kind {
            Kind::GitHub => github::edit_comment(self, comment, body),
            Kind::GitLab => gitlab::edit_comment(self, number, comment, body),
        }
    }

    /// Deletes a posted comment.
    pub fn delete_comment(&self, number: u64, comment: &review::Comment) -> Result<(), String> {
        match self.project.kind {
            Kind::GitHub => github::delete_comment(self, comment),
            Kind::GitLab => gitlab::delete_comment(self, number, comment),
        }
    }

    /// Brings a request up to date with its target, on the forge.
    pub fn update(&self, request: &Request, head: &str) -> Result<(), String> {
        match self.project.kind {
            Kind::GitHub => github::update(self, request, head),
            Kind::GitLab => gitlab::update(self, request.number),
        }
    }

    /// Posts a comment on the whole request, now.
    pub fn comment(&self, number: u64, body: &str) -> Result<(), String> {
        match self.project.kind {
            Kind::GitHub => github::comment(self, number, body),
            Kind::GitLab => gitlab::comment(self, number, body),
        }
    }

    /// Resolves a thread, or opens it again.
    pub fn resolve(&self, number: u64, thread: &str, resolved: bool) -> Result<(), String> {
        match self.project.kind {
            Kind::GitHub => github::resolve(self, thread, resolved),
            Kind::GitLab => gitlab::resolve(self, number, thread, resolved),
        }
    }

    /// Merges a request.
    pub fn merge(&self, request: &Request, how: &review::Merge) -> Result<(), String> {
        match self.project.kind {
            Kind::GitHub => github::merge(self, request, how),
            Kind::GitLab => gitlab::merge(self, request, how),
        }
    }

    /// The ref a request's changes can be fetched from, on the remote.
    pub fn head_ref(&self, number: u64) -> String {
        match self.project.kind {
            Kind::GitHub => format!("pull/{number}/head"),
            Kind::GitLab => format!("merge-requests/{number}/head"),
        }
    }
}

/// What a call gave, or `None` with a note of `what` couldn't be loaded
/// and why (in `missing`): for the parts of a view it can do without.
fn soft<T>(result: Result<T, String>, what: &str, missing: &mut Vec<String>) -> Option<T> {
    result
        .map_err(|e| missing.push(format!("{what} ({e})")))
        .ok()
}

/// Seconds since the Unix epoch for an RFC 3339 time ("2026-11-16T10:00:00Z").
fn timestamp(text: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(text).map_or(0, |t| t.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(url: &str, managed: &[&str]) -> Option<(Kind, String, String)> {
        let managed: Vec<String> = managed.iter().map(ToString::to_string).collect();
        project(url, &managed).map(|p| (p.kind, p.base, p.path))
    }

    #[test]
    fn recognises_the_forge_from_a_remote() {
        let gh = Some((
            Kind::GitHub,
            "https://github.com".to_string(),
            "Trashpanda-GameDev/Git-Raccoon".to_string(),
        ));
        for url in [
            "https://github.com/Trashpanda-GameDev/Git-Raccoon.git",
            "https://github.com/Trashpanda-GameDev/Git-Raccoon",
            "https://someone@github.com/Trashpanda-GameDev/Git-Raccoon.git/",
            "git@github.com:Trashpanda-GameDev/Git-Raccoon.git",
            "ssh://git@github.com/Trashpanda-GameDev/Git-Raccoon.git",
        ] {
            assert_eq!(found(url, &[]), gh, "{url}");
        }
        assert_eq!(
            found("git@gitlab.com:group/sub/project.git", &[]),
            Some((
                Kind::GitLab,
                "https://gitlab.com".into(),
                "group/sub/project".into()
            ))
        );
        // Self-managed GitLab: only hosts with an account.
        assert_eq!(found("git@git.example.com:team/app.git", &[]), None);
        let managed = Some((
            Kind::GitLab,
            "https://git.example.com".to_string(),
            "team/app".to_string(),
        ));
        for url in [
            "git@git.example.com:team/app.git",
            "ssh://git@git.example.com:2222/team/app.git",
            "https://git.example.com/team/app.git",
        ] {
            assert_eq!(found(url, &["https://git.example.com/"]), managed, "{url}");
        }
        // Not forges, or not repositories.
        assert_eq!(found("/home/me/repo.git", &[]), None);
        assert_eq!(found("http://github.com/a/b", &[]), None, "plain http");
        assert_eq!(found("https://github.com/justone", &[]), None);
    }

    #[test]
    fn finds_the_web_pages_of_commits_branches_and_remotes() {
        let remotes = [
            (
                "mirror".to_string(),
                Some("git@gitlab.com:team/app.git".to_string()),
            ),
            (
                "origin".to_string(),
                Some("https://github.com/a/b.git".to_string()),
            ),
            (
                "team/fork".to_string(),
                Some("git@git.example.com:t/app.git".to_string()),
            ),
            ("local".to_string(), Some("/srv/app.git".to_string())),
        ];
        let web = Web::new(&remotes, &["https://git.example.com".to_string()]);
        assert_eq!(
            web.commit("abc"),
            Some((
                "Open on GitHub".into(),
                "https://github.com/a/b/commit/abc".into()
            ))
        );
        assert_eq!(
            web.branch("mirror/fix/ui#2").map(|w| w.1),
            Some("https://gitlab.com/team/app/-/tree/fix/ui%232".into())
        );
        assert_eq!(
            web.branch("team/fork/main").map(|w| w.1),
            Some("https://git.example.com/t/app/-/tree/main".into())
        );
        assert_eq!(web.branch("local/main"), None);
        assert_eq!(
            web.remote_page("mirror").map(|w| w.1),
            Some("https://gitlab.com/team/app".into())
        );
        assert_eq!(
            web.tag("v1.0").map(|w| w.1),
            Some("https://github.com/a/b/tree/v1.0".into())
        );
        assert_eq!(Web::new(&remotes[3..], &[]).commit("abc"), None);
    }

    #[test]
    fn names_requests_the_forges_way() {
        assert_eq!(Kind::GitHub.number(12), "#12");
        assert_eq!(Kind::GitLab.number(12), "!12");
        assert_eq!(Kind::GitLab.request(), "Merge request");
        assert_eq!(timestamp("1970-01-01T00:01:00Z"), 60);
        assert_eq!(timestamp("nonsense"), 0);
    }
}
