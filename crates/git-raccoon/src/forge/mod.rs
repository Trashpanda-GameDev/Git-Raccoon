//! Forges: GitHub, GitLab.com and self-managed GitLab. Which one a
//! repository is on (from its remote), the accounts signed in to them,
//! and their pull/merge requests and issues.
//!
//! The network calls block: run them off the UI thread.

pub mod accounts;
mod github;
mod gitlab;
mod http;
pub mod oauth;

use serde::{Deserialize, Serialize};

pub use accounts::Account;

/// What kind of forge a host is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    GitHub,
    GitLab,
}

impl Kind {
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

    /// The open pull/merge requests, newest first.
    pub fn requests(&self) -> Result<Vec<Request>, String> {
        match self.project.kind {
            Kind::GitHub => github::requests(self),
            Kind::GitLab => gitlab::requests(self),
        }
    }

    /// One request with its reviews and checks.
    pub fn details(&self, number: u64) -> Result<Request, String> {
        match self.project.kind {
            Kind::GitHub => github::details(self, number),
            Kind::GitLab => gitlab::details(self, number),
        }
    }

    /// The open issues, newest first.
    pub fn issues(&self) -> Result<Vec<Issue>, String> {
        match self.project.kind {
            Kind::GitHub => github::issues(self),
            Kind::GitLab => gitlab::issues(self),
        }
    }

    /// The branch new requests go into by default.
    pub fn default_branch(&self) -> Result<String, String> {
        match self.project.kind {
            Kind::GitHub => github::default_branch(self),
            Kind::GitLab => gitlab::default_branch(self),
        }
    }

    /// Opens a pull/merge request. Returns it.
    pub fn create(&self, new: &NewRequest) -> Result<Request, String> {
        match self.project.kind {
            Kind::GitHub => github::create(self, new),
            Kind::GitLab => gitlab::create(self, new),
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
    fn names_requests_the_forges_way() {
        assert_eq!(Kind::GitHub.number(12), "#12");
        assert_eq!(Kind::GitLab.number(12), "!12");
        assert_eq!(Kind::GitLab.request(), "Merge request");
        assert_eq!(timestamp("1970-01-01T00:01:00Z"), 60);
        assert_eq!(timestamp("nonsense"), 0);
    }
}
