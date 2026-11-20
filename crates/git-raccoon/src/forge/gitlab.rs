//! GitLab's REST API (gitlab.com, or a self-managed server).

use serde_json::{Value, json};

use super::http::{self, Auth, encode, text};
use super::review::{
    Anchor, Change, Check, Comment, Commit, DiffRefs, Draft, Event, File, Finish, Line, Merge,
    Method, Outcome, Overview, State, Suggestion, Thread, Verdict,
};
use super::{Checks, Choices, Issue, NewRequest, Request, Review, Session, timestamp};

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

/// Page `page` (from 1) of a list, and whether there are more.
fn page(s: &Session, path: &str, page: u32) -> Result<(Value, bool), String> {
    let (list, next) = http::get_page(
        FORGE,
        &format!("{}{path}&page={page}", api(s)),
        &Auth::Bearer(&s.token),
    )?;
    Ok((list, next.is_some()))
}

/// Every page of a list; `cut` notes if it stopped short of the end.
fn all(s: &Session, path: &str, cut: &mut bool) -> Result<Value, String> {
    let (list, short) =
        http::get_all(FORGE, &format!("{}{path}", api(s)), &Auth::Bearer(&s.token))?;
    *cut |= short;
    Ok(list)
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

pub fn requests(s: &Session, n: u32) -> Result<(Vec<Request>, bool), String> {
    let (list, more) = page(
        s,
        "/merge_requests?state=opened&per_page=50&order_by=created_at&sort=desc",
        n,
    )?;
    Ok((
        list.as_array().into_iter().flatten().map(request).collect(),
        more,
    ))
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

fn note(n: &Value) -> Comment {
    let path = text(&n["position"], "new_path");
    Comment {
        id: n["id"].as_u64().unwrap_or_default().to_string(),
        author: text(&n["author"], "username"),
        body: text(n, "body"),
        created: timestamp(n["created_at"].as_str().unwrap_or_default()),
        suggestions: n["suggestions"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|g| {
                let line = |key: &str| u32::try_from(g[key].as_u64().unwrap_or(0)).unwrap_or(0);
                let content = |key: &str| text(g, key).trim_end_matches('\n').to_string();
                Suggestion {
                    id: g["id"].as_u64().unwrap_or_default().to_string(),
                    path: path.clone(),
                    start: line("from_line"),
                    end: line("to_line"),
                    from: content("from_content"),
                    to: content("to_content"),
                    applied: g["applied"].as_bool().unwrap_or(false),
                    appliable: g["appliable"].as_bool().unwrap_or(false),
                }
            })
            .collect(),
        on_code: !n["position"].is_null(),
    }
}

fn line_of(old: &Value, new: &Value) -> Option<Line> {
    let line = Line {
        old: old.as_u64().and_then(|n| u32::try_from(n).ok()),
        new: new.as_u64().and_then(|n| u32::try_from(n).ok()),
    };
    (line.old.is_some() || line.new.is_some()).then_some(line)
}

fn anchor(p: &Value) -> Option<Anchor> {
    if p.is_null() {
        return None;
    }
    let line = line_of(&p["old_line"], &p["new_line"])?;
    let start = line_of(
        &p["line_range"]["start"]["old_line"],
        &p["line_range"]["start"]["new_line"],
    )
    .filter(|s| *s != line);
    let path = text(p, "new_path");
    let old = text(p, "old_path");
    Some(Anchor {
        old_path: (!old.is_empty() && old != path).then_some(old),
        path,
        line,
        start,
    })
}

/// How a pipeline job went.
fn outcome(status: &str) -> Outcome {
    match status {
        "success" | "skipped" | "manual" => Outcome::Passed,
        "failed" | "canceled" => Outcome::Failed,
        _ => Outcome::Running,
    }
}

/// Why GitLab won't merge it, from its `detailed_merge_status`.
fn blocked(status: &str, target: &str) -> Option<String> {
    Some(match status {
        "mergeable" | "checking" | "unchecked" | "preparing" | "" => return None,
        "not_approved" => "It needs approvals first".into(),
        "ci_must_pass" => "Its pipeline has to pass first".into(),
        "ci_still_running" => "Its pipeline is still running".into(),
        "conflict" | "broken_status" => format!("It has conflicts with {target}"),
        "discussions_not_resolved" => "Its threads have to be resolved first".into(),
        "draft_status" => "It's a draft: mark it ready first".into(),
        "need_rebase" => format!("It has to be rebased onto {target} first"),
        "not_open" => "It isn't open".into(),
        "blocked_status" => "Another merge request has to be merged first".into(),
        "requested_changes" => "A reviewer asked for changes".into(),
        other => format!("GitLab won't merge it yet ({})", other.replace('_', " ")),
    })
}

/// Everything for a request's view.
pub fn view(s: &Session, number: u64) -> Result<(Request, Overview), String> {
    let v = get(s, &format!("/merge_requests/{number}"))?;
    let mut r = request(&v);
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
    let mut o = Overview {
        state: match v["state"].as_str() {
            Some("merged") => State::Merged,
            Some("closed" | "locked") => State::Closed,
            _ => State::Open,
        },
        refs: DiffRefs {
            base: text(&v["diff_refs"], "base_sha"),
            start: text(&v["diff_refs"], "start_sha"),
            head: text(&v["diff_refs"], "head_sha"),
        },
        ..Overview::default()
    };
    // The pipeline's jobs are its checks.
    if let Some(id) = v["head_pipeline"]["id"].as_u64() {
        let jobs = all(
            s,
            &format!("/pipelines/{id}/jobs?per_page=100"),
            &mut o.cut_short,
        )?;
        o.checks = jobs
            .as_array()
            .into_iter()
            .flatten()
            .map(|j| Check {
                name: text(j, "name"),
                outcome: outcome(j["status"].as_str().unwrap_or_default()),
                url: text(j, "web_url"),
            })
            .collect();
    }
    for c in &o.checks {
        match c.outcome {
            Outcome::Passed => r.checks.passed += 1,
            Outcome::Failed => r.checks.failed += 1,
            Outcome::Running => r.checks.pending += 1,
        }
    }
    // No jobs listed: the pipeline is the one check.
    if o.checks.is_empty() {
        match v["head_pipeline"]["status"].as_str() {
            Some("success") => r.checks.passed = 1,
            Some("failed" | "canceled") => r.checks.failed = 1,
            Some(_) => r.checks.pending = 1,
            None => {}
        }
    }
    let diffs = all(
        s,
        &format!("/merge_requests/{number}/diffs?per_page=100"),
        &mut o.cut_short,
    )?;
    o.files = diffs
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let path = text(f, "new_path");
            let old = text(f, "old_path");
            File {
                change: if f["new_file"].as_bool().unwrap_or(false) {
                    Change::Added
                } else if f["deleted_file"].as_bool().unwrap_or(false) {
                    Change::Deleted
                } else if f["renamed_file"].as_bool().unwrap_or(false) {
                    Change::Renamed
                } else {
                    Change::Modified
                },
                old_path: (old != path).then_some(old),
                path,
                patch: text(f, "diff"),
            }
        })
        .collect();
    let commits = all(
        s,
        &format!("/merge_requests/{number}/commits?per_page=100"),
        &mut o.cut_short,
    )?;
    o.commits = commits
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| Commit {
            sha: text(c, "id"),
            summary: text(c, "title"),
            author: text(c, "author_name"),
            created: timestamp(c["created_at"].as_str().unwrap_or_default()),
        })
        .rev() // GitLab lists them newest first.
        .collect();
    let discussions = all(
        s,
        &format!("/merge_requests/{number}/discussions?per_page=100"),
        &mut o.cut_short,
    )?;
    for d in discussions.as_array().into_iter().flatten() {
        let notes: Vec<&Value> = d["notes"].as_array().into_iter().flatten().collect();
        let Some(first) = notes.first() else { continue };
        if first["system"].as_bool().unwrap_or(false) {
            // Of what GitLab notes itself, approvals belong in the conversation.
            if text(first, "body").starts_with("approved this merge request") {
                o.events.push(Event::Review {
                    author: text(&first["author"], "username"),
                    verdict: Verdict::Approved,
                    body: String::new(),
                    created: timestamp(first["created_at"].as_str().unwrap_or_default()),
                });
            }
            continue;
        }
        if d["individual_note"].as_bool().unwrap_or(false) && first["position"].is_null() {
            o.events.push(Event::Comment(note(first)));
            continue;
        }
        let id = text(d, "id");
        o.threads.push(Thread {
            at: anchor(&first["position"]),
            resolved: first["resolved"].as_bool().unwrap_or(false),
            resolvable: first["resolvable"].as_bool().unwrap_or(false),
            outdated: false,
            hunk: String::new(),
            reply: id.clone(),
            id,
            comments: notes.iter().map(|n| note(n)).collect(),
        });
    }
    // Your draft notes (only you see them).
    let drafts = all(
        s,
        &format!("/merge_requests/{number}/draft_notes?per_page=100"),
        &mut o.cut_short,
    )?;
    o.pending = drafts
        .as_array()
        .into_iter()
        .flatten()
        .map(|n| Draft {
            at: anchor(&n["position"]),
            reply_to: n["discussion_id"].as_str().map(str::to_string),
            body: text(n, "note"),
            remote: n["id"].as_u64().map(|id| id.to_string()),
            local: 0,
        })
        .collect();
    o.events.extend((0..o.threads.len()).map(Event::Thread));
    o.sort();
    let project = get(s, "")?;
    // How the project merges ("merge", "rebase_merge" or fast-forward
    // only), and whether it squashes.
    let plain = if project["merge_method"].as_str() == Some("ff") {
        Method::FastForward
    } else {
        Method::Merge
    };
    o.merge.methods = match project["squash_option"].as_str() {
        Some("never") => vec![plain],
        Some("always") => vec![Method::Squash],
        Some("default_on") => vec![Method::Squash, plain],
        _ => vec![plain, Method::Squash],
    };
    o.merge.can_update = o.state == State::Open
        && v["detailed_merge_status"].as_str() == Some("need_rebase")
        && !r.from_fork;
    o.merge.can_delete_branch = !r.from_fork;
    o.merge.blocked = match o.state {
        State::Merged => Some("It's merged already".into()),
        State::Closed => Some("It's closed".into()),
        State::Open => blocked(
            v["detailed_merge_status"].as_str().unwrap_or_default(),
            &r.target,
        ),
    };
    Ok((r, o))
}

/// GitLab's name for a line of a file's diff.
fn line_code(path: &str, line: &Line) -> String {
    use sha1::{Digest as _, Sha1};
    let hash: String = Sha1::digest(path.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{hash}_{}_{}", line.old.unwrap_or(0), line.new.unwrap_or(0))
}

fn position(at: &Anchor, refs: &DiffRefs) -> Value {
    let old_path = at.old_path.clone().unwrap_or_else(|| at.path.clone());
    let mut p = json!({
        "position_type": "text",
        "base_sha": refs.base,
        "start_sha": refs.start,
        "head_sha": refs.head,
        "old_path": old_path,
        "new_path": at.path,
    });
    if let Some(n) = at.line.old {
        p["old_line"] = json!(n);
    }
    if let Some(n) = at.line.new {
        p["new_line"] = json!(n);
    }
    if let Some(start) = &at.start {
        let end = |l: &Line| {
            json!({
                "line_code": line_code(&at.path, l),
                "type": if l.new.is_some() && l.old.is_none() { "new" } else if l.new.is_none() { "old" } else { "" },
                "old_line": l.old,
                "new_line": l.new,
            })
        };
        p["line_range"] = json!({"start": end(start), "end": end(&at.line)});
    }
    p
}

/// Keeps a draft as a GitLab draft note (or changes it there). Returns
/// its id there.
pub fn save_draft(s: &Session, number: u64, o: &Overview, d: &Draft) -> Result<String, String> {
    let auth = Auth::Bearer(&s.token);
    let base = format!("{}/merge_requests/{number}/draft_notes", api(s));
    if let Some(id) = &d.remote {
        http::put(
            FORGE,
            &format!("{base}/{id}"),
            &auth,
            &json!({"note": d.body}),
        )?;
        return Ok(id.clone());
    }
    let mut body = json!({"note": d.body});
    if let Some(to) = &d.reply_to {
        body["in_reply_to_discussion_id"] = json!(to);
    } else if let Some(at) = &d.at {
        body["position"] = position(at, &o.refs);
    }
    let made = http::post(FORGE, &base, &auth, &body)?;
    made["id"]
        .as_u64()
        .map(|id| id.to_string())
        .ok_or_else(|| format!("{FORGE} didn't say what it kept the draft as"))
}

/// Deletes a draft note.
pub fn delete_draft(s: &Session, number: u64, remote: &str) -> Result<(), String> {
    http::delete(
        FORGE,
        &format!("{}/merge_requests/{number}/draft_notes/{remote}", api(s)),
        &Auth::Bearer(&s.token),
    )?;
    Ok(())
}

/// Sends a review: drafts not on GitLab yet go there, then all of them are
/// published at once, then the summary and the approval.
pub fn submit(
    s: &Session,
    number: u64,
    o: &Overview,
    (drafts, summary, finish): (&[Draft], &str, Finish),
) -> Result<(), String> {
    let auth = Auth::Bearer(&s.token);
    let base = format!("{}/merge_requests/{number}", api(s));
    for d in drafts.iter().filter(|d| d.remote.is_none()) {
        save_draft(s, number, o, d)?;
    }
    if !drafts.is_empty() {
        http::post(
            FORGE,
            &format!("{base}/draft_notes/bulk_publish"),
            &auth,
            &json!({}),
        )?;
    }
    if !summary.trim().is_empty() {
        comment(s, number, summary)?;
    }
    if finish == Finish::Approve {
        http::post(FORGE, &format!("{base}/approve"), &auth, &json!({}))?;
    }
    Ok(())
}

/// Rebases a request onto its target, on GitLab (it carries on there).
pub fn update(s: &Session, number: u64) -> Result<(), String> {
    http::put(
        FORGE,
        &format!("{}/merge_requests/{number}/rebase", api(s)),
        &Auth::Bearer(&s.token),
        &json!({}),
    )?;
    Ok(())
}

/// Changes a comment's (note's) text.
pub fn edit_comment(s: &Session, number: u64, c: &Comment, body: &str) -> Result<(), String> {
    http::put(
        FORGE,
        &format!("{}/merge_requests/{number}/notes/{}", api(s), c.id),
        &Auth::Bearer(&s.token),
        &json!({"body": body}),
    )?;
    Ok(())
}

/// Deletes a comment (note).
pub fn delete_comment(s: &Session, number: u64, c: &Comment) -> Result<(), String> {
    http::delete(
        FORGE,
        &format!("{}/merge_requests/{number}/notes/{}", api(s), c.id),
        &Auth::Bearer(&s.token),
    )?;
    Ok(())
}

/// Applies suggestions, in one commit on the request's branch.
pub fn apply(s: &Session, ids: &[String], message: &str) -> Result<(), String> {
    let ids: Vec<u64> = ids.iter().filter_map(|i| i.parse().ok()).collect();
    let mut body = json!({"ids": ids});
    if !message.trim().is_empty() {
        body["commit_message"] = json!(message.trim());
    }
    http::put(
        FORGE,
        &format!("{}/api/v4/suggestions/batch_apply", s.project.base),
        &Auth::Bearer(&s.token),
        &body,
    )?;
    Ok(())
}

/// The ids of users, by username.
fn user_ids(s: &Session, names: &[String]) -> Result<Vec<u64>, String> {
    names
        .iter()
        .map(|name| {
            let found = http::get(
                FORGE,
                &format!("{}/api/v4/users?username={}", s.project.base, encode(name)),
                &Auth::Bearer(&s.token),
            )?;
            found[0]["id"]
                .as_u64()
                .ok_or_else(|| format!("{FORGE} doesn't know {name}"))
        })
        .collect()
}

/// A comment on the whole request.
pub fn comment(s: &Session, number: u64, body: &str) -> Result<(), String> {
    http::post(
        FORGE,
        &format!("{}/merge_requests/{number}/notes", api(s)),
        &Auth::Bearer(&s.token),
        &json!({"body": body}),
    )?;
    Ok(())
}

pub fn resolve(s: &Session, number: u64, thread: &str, resolved: bool) -> Result<(), String> {
    http::put(
        FORGE,
        &format!(
            "{}/merge_requests/{number}/discussions/{thread}?resolved={resolved}",
            api(s)
        ),
        &Auth::Bearer(&s.token),
        &json!({}),
    )?;
    Ok(())
}

pub fn merge(s: &Session, r: &Request, how: &Merge) -> Result<(), String> {
    let squash = how.method == Method::Squash;
    let mut body = json!({
        "squash": squash,
        "should_remove_source_branch": how.delete_branch && !r.from_fork,
    });
    if !how.message.trim().is_empty() {
        let key = if squash {
            "squash_commit_message"
        } else {
            "merge_commit_message"
        };
        body[key] = json!(how.message.trim());
    }
    http::put(
        FORGE,
        &format!("{}/merge_requests/{}/merge", api(s), r.number),
        &Auth::Bearer(&s.token),
        &body,
    )?;
    Ok(())
}

pub fn issues(s: &Session, n: u32) -> Result<(Vec<Issue>, bool), String> {
    let (list, more) = page(
        s,
        "/issues?state=opened&per_page=50&order_by=created_at&sort=desc",
        n,
    )?;
    let issues: Vec<Issue> = list
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
        .collect();
    Ok((issues, more))
}

pub fn default_branch(s: &Session) -> Result<String, String> {
    Ok(text(&get(s, "")?, "default_branch"))
}

pub fn choices(s: &Session) -> Result<Choices, String> {
    // Members, with those inherited from the project's groups.
    let mut cut = false;
    let people = all(s, "/members/all?per_page=100", &mut cut)?;
    let labels = all(s, "/labels?per_page=100", &mut cut)?;
    let mut logins: Vec<String> = people
        .as_array()
        .into_iter()
        .flatten()
        .filter(|p| p["state"].as_str().is_none_or(|st| st == "active"))
        .map(|p| text(p, "username"))
        .collect();
    logins.sort();
    logins.dedup();
    Ok(Choices {
        people: logins,
        labels: labels
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| (text(l, "name"), text(l, "color")))
            .collect(),
    })
}

pub fn create(s: &Session, new: &NewRequest) -> Result<Request, String> {
    let title = if new.draft {
        format!("Draft: {}", new.title)
    } else {
        new.title.clone()
    };
    let mut body = json!({
        "source_branch": new.source,
        "target_branch": new.target,
        "title": title,
        "description": new.body,
        "remove_source_branch": new.remove_source,
        "squash": new.squash,
    });
    if !new.labels.is_empty() {
        body["labels"] = json!(new.labels.join(","));
    }
    // GitLab takes people by their ids.
    for (key, names) in [
        ("reviewer_ids", &new.reviewers),
        ("assignee_ids", &new.assignees),
    ] {
        if !names.is_empty() {
            body[key] = json!(user_ids(s, names)?);
        }
    }
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
        let list = s.requests(1).unwrap().0;
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
            "GET /api/v4/projects/team%2Fsub%2Fapp/merge_requests?state=opened&per_page=50&order_by=created_at&sort=desc&page=1"
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
        let issues = s.issues(1).unwrap().0;
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
            ..NewRequest::default()
        })
        .unwrap();
        let seen = seen.lock().unwrap();
        let sent: Value = serde_json::from_str(&seen[1].2).unwrap();
        assert_eq!(sent["title"], "Draft: Pipeline cache");
        assert_eq!(sent["source_branch"], "cache");
    }

    #[test]
    fn views_a_merge_request_and_sends_a_review() {
        let mr = MR.replace(
            r#""head_pipeline": {"status": "running"}"#,
            r#""head_pipeline": {"id": 99, "status": "failed"}, "state": "opened",
               "detailed_merge_status": "need_rebase",
               "diff_refs": {"base_sha": "b", "start_sha": "s", "head_sha": "h"}"#,
        );
        let (base, seen) = mock::serve(vec![
            (200, mr),
            (200, r#"{"approved_by": []}"#.into()),
            (200, r#"[{"name":"test","status":"failed","web_url":"j"}]"#.into()),
            (
                200,
                r#"[{"old_path":"a.rs","new_path":"a.rs","diff":"@@ -1 +1 @@\n-x\n+y\n"}]"#.into(),
            ),
            (
                200,
                r#"[{"id":"c2","title":"Second","author_name":"Ada","created_at":"2026-11-02T00:00:00Z"},
                    {"id":"c1","title":"First","author_name":"Ada","created_at":"2026-11-01T00:00:00Z"}]"#
                    .into(),
            ),
            (
                200,
                r#"[{"id":"d1","individual_note":true,"notes":[{"id":1,"body":"Hi","author":{"username":"milo"},"created_at":"2026-11-02T00:00:00Z","system":false}]},
                    {"id":"d2","individual_note":true,"notes":[{"id":2,"body":"approved this merge request","author":{"username":"rocky"},"created_at":"2026-11-03T00:00:00Z","system":true}]},
                    {"id":"d3","individual_note":false,"notes":[{"id":3,"body":"Why y?","author":{"username":"milo"},"created_at":"2026-11-04T00:00:00Z","system":false,
                        "suggestions":[{"id":77,"from_line":1,"to_line":1,"from_content":"y\n","to_content":"z\n","applied":false,"appliable":true}],
                        "resolvable":true,"resolved":true,
                        "position":{"old_path":"a.rs","new_path":"a.rs","old_line":null,"new_line":1}}]}]"#
                    .into(),
            ),
            // A draft note of yours, kept there.
            (
                200,
                r#"[{"id":55,"note":"Not sure","discussion_id":null,
                    "position":{"old_path":"a.rs","new_path":"a.rs","old_line":null,"new_line":1}}]"#
                    .into(),
            ),
            (200, r#"{"squash_option":"default_off","merge_method":"ff"}"#.into()),
            // The review: two drafts, publishing, the summary, approving.
            (201, r#"{"id": 56}"#.into()),
            (201, r#"{"id": 57}"#.into()),
            (204, String::new()),
            (201, "{}".into()),
            (201, "{}".into()),
            // Applying a suggestion.
            (200, "[]".into()),
            // Rebasing it, editing a comment and deleting it.
            (202, r#"{"rebase_in_progress":true}"#.into()),
            (200, "{}".into()),
            (204, String::new()),
            // Changing that draft note, and deleting it.
            (200, r#"{"id":55}"#.into()),
            (204, String::new()),
        ]);
        let s = session(&base);
        let (r, o) = s.view(12).unwrap();
        assert_eq!(r.checks.failed, 1);
        assert_eq!(o.files[0].change, Change::Modified);
        assert_eq!(
            o.commits
                .iter()
                .map(|c| c.summary.as_str())
                .collect::<Vec<_>>(),
            ["First", "Second"]
        );
        assert_eq!(o.threads.len(), 1);
        assert!(o.threads[0].resolved);
        assert_eq!(o.threads[0].at.as_ref().unwrap().line.new, Some(1));
        assert_eq!(o.unresolved(), 0);
        let g = &o.threads[0].comments[0].suggestions[0];
        assert_eq!(
            (
                g.id.as_str(),
                g.path.as_str(),
                g.from.as_str(),
                g.to.as_str()
            ),
            ("77", "a.rs", "y", "z")
        );
        assert_eq!(o.events.len(), 3);
        assert!(
            matches!(&o.events[1], Event::Review { verdict: Verdict::Approved, author, .. } if author == "rocky")
        );
        // Fast-forward only, and behind: GitLab can rebase it.
        assert_eq!(o.merge.methods, [Method::FastForward, Method::Squash]);
        assert_eq!(
            o.merge.blocked.as_deref(),
            Some("It has to be rebased onto main first")
        );
        assert!(o.merge.can_update);

        let drafts = [
            Draft {
                at: Some(Anchor {
                    path: "a.rs".into(),
                    old_path: None,
                    line: Line {
                        old: None,
                        new: Some(1),
                    },
                    start: Some(Line {
                        old: Some(1),
                        new: None,
                    }),
                }),
                reply_to: None,
                body: "Hm".into(),
                ..Draft::default()
            },
            Draft {
                at: None,
                reply_to: Some("d3".into()),
                body: "Because.".into(),
                ..Draft::default()
            },
        ];
        // The draft note already there isn't sent again.
        assert_eq!(o.pending.len(), 1);
        assert_eq!(o.pending[0].remote.as_deref(), Some("55"));
        assert_eq!(o.pending[0].at.as_ref().and_then(|a| a.line.new), Some(1));
        let all: Vec<Draft> = o.pending.iter().cloned().chain(drafts).collect();
        s.submit(12, &o, None, (&all, "Thanks", Finish::Approve))
            .unwrap();
        s.apply(&["77".into()], "Use z").unwrap();
        s.update(&r, &o.refs.head).unwrap();
        let mine = &o.threads[0].comments[0];
        s.edit_comment(12, mine, "Why z?").unwrap();
        s.delete_comment(12, mine).unwrap();
        let mut kept = o.pending[0].clone();
        kept.body = "Sure now".into();
        assert_eq!(
            s.save_draft(12, &o, None, &kept).unwrap().0.as_deref(),
            Some("55")
        );
        s.delete_draft(12, "55").unwrap();
        let seen = seen.lock().unwrap();
        let api = "/api/v4/projects/team%2Fsub%2Fapp/merge_requests/12";
        assert_eq!(
            seen[2].0,
            "GET /api/v4/projects/team%2Fsub%2Fapp/pipelines/99/jobs?per_page=100"
        );
        assert_eq!(seen[8].0, format!("POST {api}/draft_notes"));
        let first: Value = serde_json::from_str(&seen[8].2).unwrap();
        assert_eq!(first["position"]["head_sha"], "h");
        assert_eq!(first["position"]["new_line"], 1);
        assert_eq!(first["position"]["line_range"]["start"]["type"], "old");
        let reply: Value = serde_json::from_str(&seen[9].2).unwrap();
        assert_eq!(reply["in_reply_to_discussion_id"], "d3");
        assert_eq!(seen[10].0, format!("POST {api}/draft_notes/bulk_publish"));
        assert_eq!(seen[11].0, format!("POST {api}/notes"));
        assert_eq!(seen[12].0, format!("POST {api}/approve"));
        assert_eq!(seen[13].0, "PUT /api/v4/suggestions/batch_apply");
        assert_eq!(seen[14].0, format!("PUT {api}/rebase"));
        assert_eq!(seen[15].0, format!("PUT {api}/notes/3"));
        assert_eq!(seen[15].2, r#"{"body":"Why z?"}"#);
        assert_eq!(seen[16].0, format!("DELETE {api}/notes/3"));
        assert_eq!(seen[6].0, format!("GET {api}/draft_notes?per_page=100"));
        assert_eq!(seen[17].0, format!("PUT {api}/draft_notes/55"));
        assert_eq!(seen[17].2, r#"{"note":"Sure now"}"#);
        assert_eq!(seen[18].0, format!("DELETE {api}/draft_notes/55"));
        assert_eq!(
            serde_json::from_str::<Value>(&seen[13].2).unwrap(),
            json!({"ids": [77], "commit_message": "Use z"})
        );
    }

    #[test]
    fn lists_who_and_which_labels() {
        // The members come in two pages.
        let (base, seen) = mock::serve_paged(vec![
            (
                200,
                r#"<{base}/api/v4/projects/team%2Fsub%2Fapp/members/all?per_page=100&page=2>; rel="next""#.into(),
                r#"[{"username":"milo","state":"active"},{"username":"ada","state":"active"}]"#.into(),
            ),
            (
                200,
                String::new(),
                r#"[{"username":"gone","state":"blocked"},{"username":"ada","state":"active"}]"#.into(),
            ),
            (200, String::new(), r##"[{"name":"perf","color":"#d73a4a"}]"##.into()),
        ]);
        let c = session(&base).choices().unwrap();
        assert_eq!(c.people, ["ada", "milo"]);
        assert_eq!(c.labels, [("perf".to_string(), "#d73a4a".to_string())]);
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0].0,
            "GET /api/v4/projects/team%2Fsub%2Fapp/members/all?per_page=100"
        );
        assert_eq!(
            seen[1].0,
            "GET /api/v4/projects/team%2Fsub%2Fapp/members/all?per_page=100&page=2"
        );
    }
}
