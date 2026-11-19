//! GitHub's REST API.

use serde_json::{Value, json};

use super::http::{self, Auth, text};
use super::review::{
    self, Anchor, Change, Check, Comment, Commit, DiffRefs, Draft, Event, File, Finish, Line,
    Merge, Method, Outcome, Overview, State, Suggestion, Thread, Verdict,
};
use super::{Checks, Choices, Issue, NewRequest, Request, Review, Session, timestamp};

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

/// Each reviewer's latest say, and what it comes to.
fn verdicts(reviews: &Value) -> (Vec<(String, Review)>, Review) {
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
    let overall = if latest.iter().any(|(_, s)| *s == Review::ChangesRequested) {
        Review::ChangesRequested
    } else if latest.is_empty() {
        Review::None
    } else {
        Review::Approved
    };
    (latest, overall)
}

/// Check runs (Actions and apps), and older commit statuses.
fn checks(s: &Session, sha: &str) -> Result<(Checks, Vec<Check>), String> {
    let mut counts = Checks::default();
    let mut list = Vec::new();
    if sha.is_empty() {
        return Ok((counts, list));
    }
    let runs = get(s, &format!("/commits/{sha}/check-runs?per_page=100"))?;
    for run in runs["check_runs"].as_array().into_iter().flatten() {
        let outcome = match (run["status"].as_str(), run["conclusion"].as_str()) {
            (Some("completed"), Some("success" | "neutral" | "skipped")) => Outcome::Passed,
            (Some("completed"), _) => Outcome::Failed,
            _ => Outcome::Running,
        };
        list.push(Check {
            name: text(run, "name"),
            outcome,
            url: text(run, "html_url"),
        });
    }
    let statuses = get(s, &format!("/commits/{sha}/status"))?;
    for status in statuses["statuses"].as_array().into_iter().flatten() {
        let outcome = match status["state"].as_str() {
            Some("success") => Outcome::Passed,
            Some("pending") => Outcome::Running,
            _ => Outcome::Failed,
        };
        list.push(Check {
            name: text(status, "context"),
            outcome,
            url: text(status, "target_url"),
        });
    }
    for c in &list {
        match c.outcome {
            Outcome::Passed => counts.passed += 1,
            Outcome::Failed => counts.failed += 1,
            Outcome::Running => counts.pending += 1,
        }
    }
    Ok((counts, list))
}

pub fn details(s: &Session, number: u64) -> Result<Request, String> {
    let v = get(s, &format!("/pulls/{number}"))?;
    let mut r = request(&v);
    let reviews = get(s, &format!("/pulls/{number}/reviews?per_page=100"))?;
    (r.reviews, r.review) = verdicts(&reviews);
    (r.checks, _) = checks(s, &text(&v["head"], "sha"))?;
    Ok(r)
}

/// GitHub's GraphQL address.
fn graphql(s: &Session) -> String {
    if s.project.base == "https://github.com" {
        "https://api.github.com/graphql".into()
    } else {
        format!("{}/api/graphql", s.project.base)
    }
}

fn query(s: &Session, query: &str, variables: Value) -> Result<Value, String> {
    let v = http::post(
        FORGE,
        &graphql(s),
        &Auth::Bearer(&s.token),
        &json!({"query": query, "variables": variables}),
    )?;
    if let Some(e) = v["errors"].as_array().and_then(|e| e.first()) {
        return Err(format!("{FORGE} said: {}", text(e, "message")));
    }
    Ok(v)
}

const THREADS: &str = "query($owner: String!, $name: String!, $n: Int!) {
  repository(owner: $owner, name: $name) { pullRequest(number: $n) {
    reviewThreads(first: 100) { nodes {
      id isResolved isOutdated path line startLine originalLine originalStartLine
      diffSide startDiffSide
      comments(first: 100) { nodes {
        databaseId body createdAt diffHunk author { login }
      } }
    } }
  } }
}";

fn owner_name(s: &Session) -> (String, String) {
    let (owner, name) = s
        .project
        .path
        .split_once('/')
        .unwrap_or(("", &s.project.path));
    (owner.to_string(), name.to_string())
}

fn line_on(side: Option<&str>, n: Option<u64>) -> Option<Line> {
    let n = u32::try_from(n?).ok()?;
    Some(if side == Some("LEFT") {
        Line {
            old: Some(n),
            new: None,
        }
    } else {
        Line {
            old: None,
            new: Some(n),
        }
    })
}

fn thread(v: &Value) -> Thread {
    let comments: Vec<Comment> = v["comments"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| Comment {
            id: c["databaseId"].as_u64().unwrap_or_default().to_string(),
            author: text(&c["author"], "login"),
            body: text(c, "body"),
            created: timestamp(c["createdAt"].as_str().unwrap_or_default()),
            suggestions: Vec::new(),
        })
        .collect();
    let outdated = v["isOutdated"].as_bool().unwrap_or(false) || v["line"].is_null();
    let (line, start) = if v["line"].is_null() {
        (v["originalLine"].as_u64(), v["originalStartLine"].as_u64())
    } else {
        (v["line"].as_u64(), v["startLine"].as_u64())
    };
    let side = v["diffSide"].as_str();
    let start_side = v["startDiffSide"].as_str().or(side);
    let at = line_on(side, line).map(|line| Anchor {
        path: text(v, "path"),
        old_path: None,
        line,
        start: line_on(start_side, start).filter(|s| *s != line),
    });
    Thread {
        id: text(v, "id"),
        at,
        resolved: v["isResolved"].as_bool().unwrap_or(false),
        resolvable: true,
        outdated,
        hunk: v["comments"]["nodes"][0]["diffHunk"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        reply: comments.first().map(|c| c.id.clone()).unwrap_or_default(),
        comments,
    }
}

/// Everything for a request's view.
pub fn view(s: &Session, number: u64) -> Result<(Request, Overview), String> {
    let v = get(s, &format!("/pulls/{number}"))?;
    let mut r = request(&v);
    let reviews = get(s, &format!("/pulls/{number}/reviews?per_page=100"))?;
    (r.reviews, r.review) = verdicts(&reviews);
    let mut o = Overview {
        state: if v["merged"].as_bool().unwrap_or(false) || !v["merged_at"].is_null() {
            State::Merged
        } else if v["state"].as_str() == Some("closed") {
            State::Closed
        } else {
            State::Open
        },
        refs: DiffRefs {
            base: text(&v["base"], "sha"),
            start: text(&v["base"], "sha"),
            head: text(&v["head"], "sha"),
        },
        ..Overview::default()
    };
    (r.checks, o.checks) = checks(s, &o.refs.head)?;
    let files = get(s, &format!("/pulls/{number}/files?per_page=100"))?;
    o.files = files
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| File {
            path: text(f, "filename"),
            old_path: f["previous_filename"].as_str().map(str::to_string),
            change: match f["status"].as_str() {
                Some("added") => Change::Added,
                Some("removed") => Change::Deleted,
                Some("renamed") => Change::Renamed,
                _ => Change::Modified,
            },
            patch: text(f, "patch"),
        })
        .collect();
    let commits = get(s, &format!("/pulls/{number}/commits?per_page=100"))?;
    o.commits = commits
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| Commit {
            sha: text(c, "sha"),
            summary: c["commit"]["message"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .next()
                .unwrap_or_default()
                .to_string(),
            author: c["author"]["login"]
                .as_str()
                .or(c["commit"]["author"]["name"].as_str())
                .unwrap_or_default()
                .to_string(),
            created: timestamp(c["commit"]["author"]["date"].as_str().unwrap_or_default()),
        })
        .collect();
    let comments = get(s, &format!("/issues/{number}/comments?per_page=100"))?;
    for c in comments.as_array().into_iter().flatten() {
        o.events.push(Event::Comment(Comment {
            id: c["id"].as_u64().unwrap_or_default().to_string(),
            author: text(&c["user"], "login"),
            body: text(c, "body"),
            created: timestamp(c["created_at"].as_str().unwrap_or_default()),
            suggestions: Vec::new(),
        }));
    }
    for review in reviews.as_array().into_iter().flatten() {
        let verdict = match review["state"].as_str() {
            Some("APPROVED") => Verdict::Approved,
            Some("CHANGES_REQUESTED") => Verdict::ChangesRequested,
            Some("COMMENTED") => Verdict::Commented,
            _ => continue,
        };
        let body = text(review, "body");
        // A review that's only line comments shows as its threads.
        if verdict == Verdict::Commented && body.trim().is_empty() {
            continue;
        }
        o.events.push(Event::Review {
            author: text(&review["user"], "login"),
            verdict,
            body,
            created: timestamp(review["submitted_at"].as_str().unwrap_or_default()),
        });
    }
    let (owner, name) = owner_name(s);
    let threads = query(
        s,
        THREADS,
        json!({"owner": owner, "name": name, "n": number}),
    )?;
    o.threads = threads["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .map(thread)
        .collect();
    // GitHub keeps suggestions in the comments' text: on the lines the
    // thread is on, as the request has them.
    for t in &mut o.threads {
        let Some(at) = t.at.as_ref().filter(|_| !t.outdated) else {
            continue;
        };
        let Some(end) = at.line.new else { continue };
        let start = at.start.and_then(|l| l.new).unwrap_or(end);
        let patch = o
            .files
            .iter()
            .find(|f| f.path == at.path)
            .map(|f| f.patch.as_str())
            .unwrap_or_default();
        let from = review::new_lines(patch, start, end);
        for c in &mut t.comments {
            for (i, part) in review::parts(&c.body).into_iter().enumerate() {
                if let review::Part::Suggestion { to, .. } = part {
                    c.suggestions.push(Suggestion {
                        id: format!("{}-{i}", c.id),
                        path: at.path.clone(),
                        start,
                        end,
                        from: from.clone().unwrap_or_default(),
                        to,
                        applied: false,
                        appliable: from.is_some() && !t.resolved,
                    });
                }
            }
        }
    }
    o.events.extend((0..o.threads.len()).map(Event::Thread));
    o.sort();
    let repo = get(s, "")?;
    let allowed = |key: &str| repo[key].as_bool().unwrap_or(true);
    o.merge.methods = [
        ("allow_merge_commit", Method::Merge),
        ("allow_squash_merge", Method::Squash),
        ("allow_rebase_merge", Method::Rebase),
    ]
    .into_iter()
    .filter(|(key, _)| allowed(key))
    .map(|(_, m)| m)
    .collect();
    o.merge.can_delete_branch = !r.from_fork;
    o.merge.blocked = match (o.state, v["mergeable_state"].as_str()) {
        (State::Merged, _) => Some("It's merged already".into()),
        (State::Closed, _) => Some("It's closed".into()),
        (_, Some("dirty")) => Some(format!("It has conflicts with {}", r.target)),
        (_, Some("draft")) => Some("It's a draft: mark it ready for review first".into()),
        (_, Some("behind")) => Some(format!(
            "It's behind {}: bring it up to date first",
            r.target
        )),
        (_, Some("blocked")) => {
            Some("The repository's rules block it for now (approvals or required checks)".into())
        }
        _ => None,
    };
    if o.merge.methods.is_empty() {
        o.merge.blocked = Some("The repository allows no way of merging".into());
    }
    Ok((r, o))
}

fn review_comment(d: &Draft) -> Option<Value> {
    let at = d.at.as_ref()?;
    let place = |l: &Line| match (l.new, l.old) {
        (Some(n), _) => ("RIGHT", n),
        (None, Some(n)) => ("LEFT", n),
        (None, None) => ("RIGHT", 0),
    };
    let (side, line) = place(&at.line);
    let mut c = json!({"path": at.path, "body": d.body, "line": line, "side": side});
    if let Some(start) = &at.start {
        let (start_side, start_line) = place(start);
        c["start_line"] = json!(start_line);
        c["start_side"] = json!(start_side);
    }
    Some(c)
}

/// Sends a review: replies, comments on the request, then the review with
/// its line comments and verdict.
pub fn submit(
    s: &Session,
    number: u64,
    head: &str,
    drafts: &[Draft],
    summary: &str,
    finish: Finish,
) -> Result<(), String> {
    let auth = Auth::Bearer(&s.token);
    for d in drafts {
        if let Some(to) = &d.reply_to {
            http::post(
                FORGE,
                &format!("{}/pulls/{number}/comments/{to}/replies", api(s)),
                &auth,
                &json!({"body": d.body}),
            )?;
        } else if d.at.is_none() {
            comment(s, number, &d.body)?;
        }
    }
    let comments: Vec<Value> = drafts
        .iter()
        .filter(|d| d.reply_to.is_none())
        .filter_map(review_comment)
        .collect();
    if finish == Finish::Comment && comments.is_empty() && summary.trim().is_empty() {
        return Ok(());
    }
    let event = match finish {
        Finish::Comment => "COMMENT",
        Finish::Approve => "APPROVE",
        Finish::RequestChanges => "REQUEST_CHANGES",
    };
    http::post(
        FORGE,
        &format!("{}/pulls/{number}/reviews", api(s)),
        &auth,
        &json!({"commit_id": head, "body": summary, "event": event, "comments": comments}),
    )?;
    Ok(())
}

/// A comment on the whole request.
pub fn comment(s: &Session, number: u64, body: &str) -> Result<(), String> {
    http::post(
        FORGE,
        &format!("{}/issues/{number}/comments", api(s)),
        &Auth::Bearer(&s.token),
        &json!({"body": body}),
    )?;
    Ok(())
}

pub fn resolve(s: &Session, thread: &str, resolved: bool) -> Result<(), String> {
    let mutation = if resolved {
        "mutation($id: ID!) { resolveReviewThread(input: {threadId: $id}) { thread { id } } }"
    } else {
        "mutation($id: ID!) { unresolveReviewThread(input: {threadId: $id}) { thread { id } } }"
    };
    query(s, mutation, json!({"id": thread}))?;
    Ok(())
}

pub fn merge(s: &Session, r: &Request, how: &Merge) -> Result<(), String> {
    let (title, message) = how
        .message
        .split_once('\n')
        .map_or((how.message.as_str(), ""), |(t, m)| (t, m));
    let method = match how.method {
        Method::Merge => "merge",
        Method::Squash => "squash",
        Method::Rebase => "rebase",
    };
    let mut body = json!({"merge_method": method});
    if how.method != Method::Rebase && !title.trim().is_empty() {
        body["commit_title"] = json!(title.trim());
        body["commit_message"] = json!(message.trim());
    }
    http::put(
        FORGE,
        &format!("{}/pulls/{}/merge", api(s), r.number),
        &Auth::Bearer(&s.token),
        &body,
    )?;
    if how.delete_branch && !r.from_fork {
        http::delete(
            FORGE,
            &format!("{}/git/refs/heads/{}", api(s), r.source),
            &Auth::Bearer(&s.token),
        )
        .map_err(|e| format!("it's merged, but its branch wasn't deleted: {e}"))?;
    }
    Ok(())
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

pub fn choices(s: &Session) -> Result<Choices, String> {
    let people = get(s, "/assignees?per_page=100")?;
    let labels = get(s, "/labels?per_page=100")?;
    Ok(Choices {
        people: people
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| text(p, "login"))
            .collect(),
        labels: labels
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| {
                let colour = text(l, "color");
                (
                    text(l, "name"),
                    if colour.is_empty() {
                        colour
                    } else {
                        format!("#{colour}")
                    },
                )
            })
            .collect(),
    })
}

pub fn create(s: &Session, new: &NewRequest) -> Result<Request, String> {
    let body = json!({
        "title": new.title,
        "body": new.body,
        "head": new.source,
        "base": new.target,
        "draft": new.draft,
    });
    let auth = Auth::Bearer(&s.token);
    let v = http::post(FORGE, &format!("{}/pulls", api(s)), &auth, &body)?;
    let made = request(&v);
    // Who reviews it, who it's assigned to, and its labels: once it's there.
    let extra = |what: &str, e: String| format!("it's open, but its {what} weren't set: {e}");
    if !new.reviewers.is_empty() {
        http::post(
            FORGE,
            &format!("{}/pulls/{}/requested_reviewers", api(s), made.number),
            &auth,
            &json!({"reviewers": new.reviewers}),
        )
        .map_err(|e| extra("reviewers", e))?;
    }
    if !new.assignees.is_empty() || !new.labels.is_empty() {
        http::patch(
            FORGE,
            &format!("{}/issues/{}", api(s), made.number),
            &auth,
            &json!({"assignees": new.assignees, "labels": new.labels}),
        )
        .map_err(|e| extra("assignees and labels", e))?;
    }
    Ok(made)
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
                ..NewRequest::default()
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

    #[test]
    fn views_a_pull_request_and_sends_a_review() {
        let pr = PR.replace(
            r#""draft": true"#,
            r#""draft": false, "state": "open", "merged": false, "mergeable_state": "dirty""#,
        );
        let (base, seen) = mock::serve(vec![
            (200, pr),
            (
                200,
                r#"[{"user":{"login":"rocky"},"state":"APPROVED","body":"Nice","submitted_at":"2026-11-03T00:00:00Z"},
                    {"user":{"login":"milo"},"state":"COMMENTED","body":"","submitted_at":"2026-11-04T00:00:00Z"}]"#
                    .into(),
            ),
            (200, r#"{"check_runs":[{"name":"build","status":"completed","conclusion":"failure","html_url":"c"}]}"#.into()),
            (200, r#"{"statuses":[]}"#.into()),
            (
                200,
                r#"[{"filename":"src/map.rs","status":"modified","patch":"@@ -1,2 +1,2 @@\n-a\n+b\n c"},
                    {"filename":"new.rs","previous_filename":"old.rs","status":"renamed"}]"#
                    .into(),
            ),
            (
                200,
                r#"[{"sha":"c1","commit":{"message":"Add the map\n\nbody","author":{"name":"Ada","date":"2026-11-01T00:00:00Z"}},"author":{"login":"ada"}}]"#
                    .into(),
            ),
            (
                200,
                r#"[{"id":5,"user":{"login":"milo"},"body":"**Looks** good","created_at":"2026-11-02T00:00:00Z"}]"#
                    .into(),
            ),
            (
                200,
                r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[
                    {"id":"T1","isResolved":false,"isOutdated":false,"path":"src/map.rs","line":2,"startLine":1,
                     "originalLine":2,"originalStartLine":1,"diffSide":"RIGHT","startDiffSide":"RIGHT",
                     "comments":{"nodes":[{"databaseId":42,"body":"Why b?\n```suggestion\nB\nc\n```","createdAt":"2026-11-05T00:00:00Z","diffHunk":"@@","author":{"login":"milo"}}]}}
                ]}}}}}"#
                    .into(),
            ),
            (200, r#"{"allow_merge_commit":true,"allow_squash_merge":true,"allow_rebase_merge":false}"#.into()),
            // The review.
            (201, "{}".into()),
            (201, "{}".into()),
            (200, "{}".into()),
            // Merging, and deleting the branch.
            (200, r#"{"merged":true}"#.into()),
            (204, String::new()),
        ]);
        let s = session(&base);
        let (r, o) = s.view(7).unwrap();
        assert_eq!(r.review, Review::Approved);
        assert_eq!(r.checks.failed, 1);
        assert_eq!(o.state, State::Open);
        assert_eq!(o.files.len(), 2);
        assert_eq!(o.files[1].change, Change::Renamed);
        assert_eq!(o.files[1].old_path.as_deref(), Some("old.rs"));
        assert_eq!(o.commits[0].summary, "Add the map");
        assert_eq!(o.checks[0].name, "build");
        assert_eq!(o.threads.len(), 1);
        let t = &o.threads[0];
        let at = t.at.as_ref().unwrap();
        assert_eq!(
            (at.line.new, at.start.and_then(|l| l.new)),
            (Some(2), Some(1))
        );
        assert_eq!(t.reply, "42");
        // Its suggestion: lines 1 and 2 as the request has them, and what
        // they'd be.
        let g = &t.comments[0].suggestions[0];
        assert_eq!(
            (g.start, g.end, g.from.as_str(), g.to.as_str(), g.appliable),
            (1, 2, "b\nc", "B\nc", true)
        );
        assert_eq!(o.unresolved(), 1);
        // The comment, the approval, then the thread; the empty review isn't there.
        assert!(matches!(&o.events[0], Event::Comment(c) if c.author == "milo"));
        assert!(matches!(
            &o.events[1],
            Event::Review {
                verdict: Verdict::Approved,
                ..
            }
        ));
        assert!(matches!(&o.events[2], Event::Thread(0)));
        assert_eq!(o.merge.methods, [Method::Merge, Method::Squash]);
        assert_eq!(
            o.merge.blocked.as_deref(),
            Some("It has conflicts with main")
        );

        let drafts = [
            Draft {
                at: Some(Anchor {
                    path: "src/map.rs".into(),
                    old_path: None,
                    line: Line {
                        old: Some(1),
                        new: None,
                    },
                    start: None,
                }),
                reply_to: None,
                body: "Gone?".into(),
            },
            Draft {
                at: None,
                reply_to: Some("42".into()),
                body: "Because.".into(),
            },
        ];
        s.submit(7, &o.refs, &drafts, "Close", Finish::RequestChanges)
            .unwrap();
        s.resolve(7, "T1", true).unwrap();
        // Its branch is deleted after (it's not from a fork, now).
        let own = Request {
            from_fork: false,
            ..r.clone()
        };
        s.merge(
            &own,
            &Merge {
                method: Method::Squash,
                message: "Faster lookups (#7)\n\nUses a hash map.".into(),
                delete_branch: true,
            },
        )
        .unwrap();

        let seen = seen.lock().unwrap();
        assert_eq!(seen[7].0, "POST /api/graphql");
        assert_eq!(
            seen[9].0,
            "POST /api/v3/repos/rocky/stash-box/pulls/7/comments/42/replies"
        );
        assert_eq!(
            seen[10].0,
            "POST /api/v3/repos/rocky/stash-box/pulls/7/reviews"
        );
        let review: Value = serde_json::from_str(&seen[10].2).unwrap();
        assert_eq!(review["event"], "REQUEST_CHANGES");
        assert_eq!(review["commit_id"], "abc");
        assert_eq!(
            review["comments"],
            json!([{"path": "src/map.rs", "body": "Gone?", "line": 1, "side": "LEFT"}])
        );
        assert!(seen[11].2.contains("resolveReviewThread"));
        assert_eq!(
            seen[12].0,
            "PUT /api/v3/repos/rocky/stash-box/pulls/7/merge"
        );
        let merge: Value = serde_json::from_str(&seen[12].2).unwrap();
        assert_eq!(
            merge,
            json!({"merge_method": "squash", "commit_title": "Faster lookups (#7)", "commit_message": "Uses a hash map."})
        );
        assert_eq!(
            seen[13].0,
            "DELETE /api/v3/repos/rocky/stash-box/git/refs/heads/index"
        );
    }

    #[test]
    fn lists_who_and_which_labels() {
        let (base, seen) = mock::serve(vec![
            (200, r#"[{"login":"ada"},{"login":"milo"}]"#.into()),
            (200, r#"[{"name":"bug","color":"d73a4a"}]"#.into()),
        ]);
        let c = session(&base).choices().unwrap();
        assert_eq!(c.people, ["ada", "milo"]);
        assert_eq!(c.labels, [("bug".to_string(), "#d73a4a".to_string())]);
        assert_eq!(
            seen.lock().unwrap()[0].0,
            "GET /api/v3/repos/rocky/stash-box/assignees?per_page=100"
        );
    }
}
