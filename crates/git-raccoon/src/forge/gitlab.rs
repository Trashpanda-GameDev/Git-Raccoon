//! GitLab's REST API (gitlab.com, or a self-managed server).

use serde_json::{Value, json};

use super::http::{self, Auth, encode, text};
use super::{Checks, Issue, NewRequest, Request, Review, Session, timestamp};

const FORGE: &str = "GitLab";

/// The project's API address (its id is its encoded path).
fn api(s: &Session) -> String {
    format!(
        "{}/api/v4/projects/{}",
        s.project.base,
        encode(&s.project.path)
    )
}

fn get(s: &Session, path: &str) -> Result<Value, String> {
    // Personal and OAuth tokens both go as a bearer token.
    http::get(FORGE, &format!("{}{path}", api(s)), &Auth::Bearer(&s.token))
}

fn request(v: &Value) -> Request {
    let title = text(v, "title");
    let draft =
        v["draft"].as_bool().unwrap_or(false) || v["work_in_progress"].as_bool().unwrap_or(false);
    Request {
        number: v["iid"].as_u64().unwrap_or_default(),
        // GitLab marks drafts in the title too.
        title: title
            .strip_prefix("Draft: ")
            .or_else(|| title.strip_prefix("Draft:"))
            .unwrap_or(&title)
            .trim()
            .to_string(),
        author: text(&v["author"], "username"),
        source: text(v, "source_branch"),
        target: text(v, "target_branch"),
        from_fork: v["source_project_id"] != v["target_project_id"],
        draft,
        url: text(v, "web_url"),
        body: text(v, "description"),
        created: timestamp(v["created_at"].as_str().unwrap_or_default()),
        comments: v["user_notes_count"].as_u64().unwrap_or(0),
        reviews: Vec::new(),
        review: Review::None,
        checks: Checks::default(),
    }
}

pub fn requests(s: &Session) -> Result<Vec<Request>, String> {
    let list = get(
        s,
        "/merge_requests?state=opened&per_page=50&order_by=created_at&sort=desc",
    )?;
    Ok(list.as_array().into_iter().flatten().map(request).collect())
}

pub fn details(s: &Session, number: u64) -> Result<Request, String> {
    let v = get(s, &format!("/merge_requests/{number}"))?;
    let mut r = request(&v);
    // The pipeline is its checks: one, passed, failed or still going.
    match v["head_pipeline"]["status"].as_str() {
        Some("success") => r.checks.passed = 1,
        Some("failed" | "canceled") => r.checks.failed = 1,
        Some(_) => r.checks.pending = 1,
        None => {}
    }
    let approvals = get(s, &format!("/merge_requests/{number}/approvals"))?;
    r.reviews = approvals["approved_by"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| (text(&a["user"], "username"), Review::Approved))
        .collect();
    r.review = if r.reviews.is_empty() {
        Review::None
    } else {
        Review::Approved
    };
    Ok(r)
}

pub fn issues(s: &Session) -> Result<Vec<Issue>, String> {
    let list = get(
        s,
        "/issues?state=opened&per_page=50&order_by=created_at&sort=desc",
    )?;
    Ok(list
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| Issue {
            number: v["iid"].as_u64().unwrap_or_default(),
            title: text(v, "title"),
            author: text(&v["author"], "username"),
            labels: v["labels"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| l.as_str().map(str::to_string))
                .collect(),
            url: text(v, "web_url"),
            body: text(v, "description"),
            created: timestamp(v["created_at"].as_str().unwrap_or_default()),
            comments: v["user_notes_count"].as_u64().unwrap_or(0),
        })
        .collect())
}

pub fn default_branch(s: &Session) -> Result<String, String> {
    Ok(text(&get(s, "")?, "default_branch"))
}

pub fn create(s: &Session, new: &NewRequest) -> Result<Request, String> {
    let title = if new.draft {
        format!("Draft: {}", new.title)
    } else {
        new.title.clone()
    };
    let body = json!({
        "source_branch": new.source,
        "target_branch": new.target,
        "title": title,
        "description": new.body,
        "remove_source_branch": false,
    });
    let v = http::post(
        FORGE,
        &format!("{}/merge_requests", api(s)),
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
                kind: Kind::GitLab,
                base: base.into(),
                host: "test".into(),
                path: "team/sub/app".into(),
            },
            "tok".into(),
        )
    }

    const MR: &str = r#"{"iid": 12, "title": "Draft: Pipeline cache", "author": {"username": "ada"},
        "source_branch": "cache", "target_branch": "main", "source_project_id": 5,
        "target_project_id": 5, "draft": true, "web_url": "https://git.example.com/team/sub/app/-/merge_requests/12",
        "description": "Caches.", "created_at": "2026-11-01T00:00:00Z", "user_notes_count": 3,
        "head_pipeline": {"status": "running"}}"#;

    #[test]
    fn lists_and_details_merge_requests() {
        let (base, seen) = mock::serve(vec![
            (200, format!("[{MR}]")),
            (200, MR.into()),
            (
                200,
                r#"{"approved_by": [{"user": {"username": "rocky"}}]}"#.into(),
            ),
        ]);
        let s = session(&base);
        let list = s.requests().unwrap();
        let mr = &list[0];
        assert_eq!(
            (mr.number, mr.title.as_str(), mr.source.as_str()),
            (12, "Pipeline cache", "cache")
        );
        assert!(mr.draft && !mr.from_fork);
        let d = s.details(12).unwrap();
        assert_eq!(
            d.checks,
            Checks {
                passed: 0,
                failed: 0,
                pending: 1
            }
        );
        assert_eq!(d.review, Review::Approved);
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0].0,
            "GET /api/v4/projects/team%2Fsub%2Fapp/merge_requests?state=opened&per_page=50&order_by=created_at&sort=desc"
        );
        assert!(seen[0].1.contains("authorization: bearer tok"));
        assert_eq!(s.head_ref(12), "merge-requests/12/head");
    }

    #[test]
    fn lists_issues_and_creates_a_draft() {
        let (base, seen) = mock::serve(vec![
            (
                200,
                r#"[{"iid": 4, "title": "Slow", "author": {"username": "milo"}, "labels": ["perf"],
                    "web_url": "u", "description": "d", "created_at": "2026-11-01T00:00:00Z",
                    "user_notes_count": 1}]"#
                    .into(),
            ),
            (201, MR.into()),
        ]);
        let s = session(&base);
        let issues = s.issues().unwrap();
        assert_eq!(
            (issues[0].number, issues[0].labels.clone()),
            (4, vec!["perf".into()])
        );
        s.create(&NewRequest {
            source: "cache".into(),
            target: "main".into(),
            title: "Pipeline cache".into(),
            body: "Caches.".into(),
            draft: true,
        })
        .unwrap();
        let seen = seen.lock().unwrap();
        let sent: Value = serde_json::from_str(&seen[1].2).unwrap();
        assert_eq!(sent["title"], "Draft: Pipeline cache");
        assert_eq!(sent["source_branch"], "cache");
    }
}
