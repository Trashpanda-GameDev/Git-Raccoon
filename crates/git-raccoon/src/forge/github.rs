//! GitHub's REST API.

use serde_json::{Value, json};

use super::http::{self, Auth, text};
use super::{Checks, Issue, NewRequest, Request, Review, Session, timestamp};

const FORGE: &str = "GitHub";

/// The API's address: api.github.com (or a test server's /api/v3).
fn api(s: &Session) -> String {
    if s.project.base == "https://github.com" {
        format!("https://api.github.com/repos/{}", s.project.path)
    } else {
        format!("{}/api/v3/repos/{}", s.project.base, s.project.path)
    }
}

fn get(s: &Session, path: &str) -> Result<Value, String> {
    http::get(FORGE, &format!("{}{path}", api(s)), &Auth::Bearer(&s.token))
}

fn request(v: &Value) -> Request {
    let head_repo = v["head"]["repo"]["full_name"].as_str();
    let base_repo = v["base"]["repo"]["full_name"].as_str();
    Request {
        number: v["number"].as_u64().unwrap_or_default(),
        title: text(v, "title"),
        author: text(&v["user"], "login"),
        source: text(&v["head"], "ref"),
        target: text(&v["base"], "ref"),
        from_fork: head_repo.is_some() && head_repo != base_repo,
        draft: v["draft"].as_bool().unwrap_or(false),
        url: text(v, "html_url"),
        body: text(v, "body"),
        created: timestamp(v["created_at"].as_str().unwrap_or_default()),
        comments: v["comments"].as_u64().unwrap_or(0) + v["review_comments"].as_u64().unwrap_or(0),
        reviews: Vec::new(),
        review: Review::None,
        checks: Checks::default(),
    }
}

pub fn requests(s: &Session) -> Result<Vec<Request>, String> {
    let list = get(
        s,
        "/pulls?state=open&per_page=50&sort=created&direction=desc",
    )?;
    Ok(list.as_array().into_iter().flatten().map(request).collect())
}

pub fn details(s: &Session, number: u64) -> Result<Request, String> {
    let v = get(s, &format!("/pulls/{number}"))?;
    let mut r = request(&v);
    // Each reviewer's latest say.
    let reviews = get(s, &format!("/pulls/{number}/reviews?per_page=100"))?;
    let mut latest: Vec<(String, Review)> = Vec::new();
    for review in reviews.as_array().into_iter().flatten() {
        let who = text(&review["user"], "login");
        let state = match review["state"].as_str() {
            Some("APPROVED") => Review::Approved,
            Some("CHANGES_REQUESTED") => Review::ChangesRequested,
            _ => continue,
        };
        latest.retain(|(w, _)| *w != who);
        latest.push((who, state));
    }
    r.review = if latest.iter().any(|(_, s)| *s == Review::ChangesRequested) {
        Review::ChangesRequested
    } else if latest.is_empty() {
        Review::None
    } else {
        Review::Approved
    };
    r.reviews = latest;
    // Checks: check runs (Actions and apps), and older commit statuses.
    let sha = text(&v["head"], "sha");
    if !sha.is_empty() {
        let runs = get(s, &format!("/commits/{sha}/check-runs?per_page=100"))?;
        for run in runs["check_runs"].as_array().into_iter().flatten() {
            match (run["status"].as_str(), run["conclusion"].as_str()) {
                (Some("completed"), Some("success" | "neutral" | "skipped")) => {
                    r.checks.passed += 1
                }
                (Some("completed"), _) => r.checks.failed += 1,
                _ => r.checks.pending += 1,
            }
        }
        let statuses = get(s, &format!("/commits/{sha}/status"))?;
        for status in statuses["statuses"].as_array().into_iter().flatten() {
            match status["state"].as_str() {
                Some("success") => r.checks.passed += 1,
                Some("pending") => r.checks.pending += 1,
                _ => r.checks.failed += 1,
            }
        }
    }
    Ok(r)
}

pub fn issues(s: &Session) -> Result<Vec<Issue>, String> {
    let list = get(
        s,
        "/issues?state=open&per_page=50&sort=created&direction=desc",
    )?;
    Ok(list
        .as_array()
        .into_iter()
        .flatten()
        // The issues list has pull requests in it too.
        .filter(|v| v.get("pull_request").is_none())
        .map(|v| Issue {
            number: v["number"].as_u64().unwrap_or_default(),
            title: text(v, "title"),
            author: text(&v["user"], "login"),
            labels: v["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|l| text(l, "name"))
                .collect(),
            url: text(v, "html_url"),
            body: text(v, "body"),
            created: timestamp(v["created_at"].as_str().unwrap_or_default()),
            comments: v["comments"].as_u64().unwrap_or(0),
        })
        .collect())
}

pub fn default_branch(s: &Session) -> Result<String, String> {
    Ok(text(&get(s, "")?, "default_branch"))
}

pub fn create(s: &Session, new: &NewRequest) -> Result<Request, String> {
    let body = json!({
        "title": new.title,
        "body": new.body,
        "head": new.source,
        "base": new.target,
        "draft": new.draft,
    });
    let v = http::post(
        FORGE,
        &format!("{}/pulls", api(s)),
        &Auth::Bearer(&s.token),
        &body,
    )?;
    Ok(request(&v))
}

#[cfg(test)]
mod tests {
    use super::super::http::mock;
    use super::super::{Kind, Project};
    use super::*;

    fn session(base: &str) -> Session {
        Session::new(
            Project {
                kind: Kind::GitHub,
                base: base.into(),
                host: "test".into(),
                path: "rocky/stash-box".into(),
            },
            "tok".into(),
        )
    }

    const PR: &str = r#"{"number": 7, "title": "Faster lookups", "user": {"login": "ada"},
        "head": {"ref": "index", "sha": "abc", "repo": {"full_name": "ada/stash-box"}},
        "base": {"ref": "main", "repo": {"full_name": "rocky/stash-box"}},
        "draft": true, "html_url": "https://github.com/rocky/stash-box/pull/7",
        "body": "Uses a hash map.", "created_at": "2026-11-01T00:00:00Z",
        "comments": 2, "review_comments": 1}"#;

    #[test]
    fn lists_and_details_pull_requests() {
        let (base, seen) = mock::serve(vec![
            (200, format!("[{PR}]")),
            (200, PR.into()),
            (
                200,
                r#"[{"user":{"login":"rocky"},"state":"CHANGES_REQUESTED"},
                    {"user":{"login":"rocky"},"state":"APPROVED"},
                    {"user":{"login":"milo"},"state":"COMMENTED"}]"#
                    .into(),
            ),
            (
                200,
                r#"{"check_runs":[{"status":"completed","conclusion":"success"},
                    {"status":"in_progress","conclusion":null}]}"#
                    .into(),
            ),
            (200, r#"{"statuses":[{"state":"failure"}]}"#.into()),
        ]);
        let s = session(&base);
        let list = s.requests().unwrap();
        assert_eq!(list.len(), 1);
        let pr = &list[0];
        assert_eq!(
            (
                pr.number,
                pr.author.as_str(),
                pr.source.as_str(),
                pr.target.as_str()
            ),
            (7, "ada", "index", "main")
        );
        assert!(pr.from_fork && pr.draft);
        assert_eq!(pr.comments, 3);
        let d = s.details(7).unwrap();
        assert_eq!(
            d.reviews,
            [("rocky".to_string(), Review::Approved)],
            "the latest say counts"
        );
        assert_eq!(d.review, Review::Approved);
        assert_eq!(
            d.checks,
            Checks {
                passed: 1,
                failed: 1,
                pending: 1
            }
        );
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0].0,
            "GET /api/v3/repos/rocky/stash-box/pulls?state=open&per_page=50&sort=created&direction=desc"
        );
        assert!(seen[0].1.contains("authorization: bearer tok"));
        assert_eq!(
            seen[3].0,
            "GET /api/v3/repos/rocky/stash-box/commits/abc/check-runs?per_page=100"
        );
        assert_eq!(s.head_ref(7), "pull/7/head");
    }

    #[test]
    fn lists_issues_without_pull_requests_and_creates_one() {
        let (base, seen) = mock::serve(vec![
            (
                200,
                r#"[{"number": 3, "title": "Crash", "user": {"login": "milo"},
                    "labels": [{"name": "bug"}], "html_url": "u", "body": "b",
                    "created_at": "2026-11-01T00:00:00Z", "comments": 4},
                   {"number": 7, "title": "A PR", "pull_request": {}}]"#
                    .into(),
            ),
            (201, PR.into()),
        ]);
        let s = session(&base);
        let issues = s.issues().unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(
            (
                issues[0].number,
                issues[0].labels.clone(),
                issues[0].comments
            ),
            (3, vec!["bug".into()], 4)
        );
        let made = s
            .create(&NewRequest {
                source: "index".into(),
                target: "main".into(),
                title: "Faster lookups".into(),
                body: "Uses a hash map.".into(),
                draft: true,
            })
            .unwrap();
        assert_eq!(made.number, 7);
        let seen = seen.lock().unwrap();
        assert_eq!(seen[1].0, "POST /api/v3/repos/rocky/stash-box/pulls");
        let sent: Value = serde_json::from_str(&seen[1].2).unwrap();
        assert_eq!(
            (
                sent["head"].as_str(),
                sent["base"].as_str(),
                sent["draft"].as_bool()
            ),
            (Some("index"), Some("main"), Some(true))
        );
    }
}
