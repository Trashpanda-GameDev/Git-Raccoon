//! GitHub's REST API.

use serde_json::{Value, json};

use super::http::{self, Auth, text};
use super::review::{
    self, Anchor, Change, Check, Comment, Commit, DiffRefs, Draft, Event, File, Finish, Line,
    Merge, Method, Outcome, Overview, ReactTo, Reaction, State, Suggestion, Thread, Verdict,
};
use super::{Checks, Choices, Issue, NewRequest, Request, Review, Session, soft, timestamp};

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

pub fn requests(s: &Session, n: u32) -> Result<(Vec<Request>, bool), String> {
    let (list, more) = page(
        s,
        "/pulls?state=open&per_page=50&sort=created&direction=desc",
        n,
    )?;
    Ok((
        list.as_array().into_iter().flatten().map(request).collect(),
        more,
    ))
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
    // Check runs come in pages, inside an object.
    let mut runs = Vec::new();
    let mut next = Some(format!("{}/commits/{sha}/check-runs?per_page=100", api(s)));
    let mut pages = 0;
    while let Some(at) = next.take() {
        let (page, more) = http::get_page(FORGE, &at, &Auth::Bearer(&s.token))?;
        runs.extend(page["check_runs"].as_array().into_iter().flatten().cloned());
        pages += 1;
        next = more.filter(|_| pages < http::MAX_PAGES);
    }
    for run in &runs {
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

const THREADS: &str = "query($owner: String!, $name: String!, $n: Int!, $after: String) {
  repository(owner: $owner, name: $name) { pullRequest(number: $n) {
    reviewThreads(first: 100, after: $after) {
      pageInfo { hasNextPage endCursor }
      nodes {
        id isResolved isOutdated path line startLine originalLine originalStartLine
        diffSide startDiffSide
        comments(first: 100) {
          pageInfo { hasNextPage endCursor }
          nodes { databaseId body createdAt diffHunk author { login } reactionGroups { content viewerHasReacted reactors { totalCount } } }
        }
      }
    }
  } }
}";

/// The rest of a thread's comments, past its first 100.
const MORE_COMMENTS: &str = "query($id: ID!, $after: String) {
  node(id: $id) { ... on PullRequestReviewThread {
    comments(first: 100, after: $after) {
      pageInfo { hasNextPage endCursor }
      nodes { databaseId body createdAt diffHunk author { login } reactionGroups { content viewerHasReacted reactors { totalCount } } }
    }
  } }
}";

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

/// Every review thread of a request, with all their comments.
fn threads(s: &Session, number: u64, cut: &mut bool) -> Result<Vec<Value>, String> {
    let (owner, name) = owner_name(s);
    let mut nodes: Vec<Value> = Vec::new();
    let mut after = Value::Null;
    for page in 0.. {
        if page == http::MAX_PAGES {
            *cut = true;
            break;
        }
        let v = query(
            s,
            THREADS,
            json!({"owner": owner, "name": name, "n": number, "after": after}),
        )?;
        let threads = &v["data"]["repository"]["pullRequest"]["reviewThreads"];
        nodes.extend(threads["nodes"].as_array().into_iter().flatten().cloned());
        if threads["pageInfo"]["hasNextPage"].as_bool() != Some(true) {
            break;
        }
        after = threads["pageInfo"]["endCursor"].clone();
    }
    // A long thread's comments come in pages of their own.
    for node in &mut nodes {
        let mut info = node["comments"]["pageInfo"].clone();
        let mut pages = 1;
        while info["hasNextPage"].as_bool() == Some(true) {
            if pages == http::MAX_PAGES {
                *cut = true;
                break;
            }
            let v = query(
                s,
                MORE_COMMENTS,
                json!({"id": node["id"], "after": info["endCursor"]}),
            )?;
            let more = &v["data"]["node"]["comments"];
            if let Some(list) = node["comments"]["nodes"].as_array_mut() {
                list.extend(more["nodes"].as_array().into_iter().flatten().cloned());
            }
            info = more["pageInfo"].clone();
            pages += 1;
        }
    }
    Ok(nodes)
}

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

/// The reactions REST lists with a comment ({"+1": 2, "heart": 1, ...}),
/// in GitHub's order.
fn rest_reactions(v: &Value) -> Vec<Reaction> {
    [
        "+1", "-1", "laugh", "hooray", "confused", "heart", "rocket", "eyes",
    ]
    .into_iter()
    .filter_map(|key| {
        let count = u32::try_from(v[key].as_u64()?).ok()?;
        let name = Reaction::github(key)?;
        (count > 0).then(|| Reaction {
            name: name.to_string(),
            count,
            ..Reaction::default()
        })
    })
    .collect()
}

/// GraphQL's reaction groups: each kind, how many, and whether you gave it.
fn groups(v: &Value) -> Vec<Reaction> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| {
            let count = u32::try_from(g["reactors"]["totalCount"].as_u64()?).ok()?;
            let name = Reaction::github(g["content"].as_str()?)?;
            (count > 0).then(|| Reaction {
                name: name.to_string(),
                count,
                mine: g["viewerHasReacted"].as_bool().unwrap_or(false),
                by: Vec::new(),
            })
        })
        .collect()
}

/// The request's own reactions and its comments' (whether they're yours,
/// which REST doesn't say), the last 100 comments' at most.
const REACTIONS: &str = "query($owner: String!, $name: String!, $n: Int!) {
  repository(owner: $owner, name: $name) { pullRequest(number: $n) {
    reactionGroups { content viewerHasReacted reactors { totalCount } }
    comments(last: 100) { nodes { databaseId
      reactionGroups { content viewerHasReacted reactors { totalCount } } } }
  } }
}";

/// Fills in the request's reactions, and which of its comments' are yours.
fn reactions(s: &Session, number: u64, o: &mut Overview) -> Result<(), String> {
    let (owner, name) = owner_name(s);
    let v = query(
        s,
        REACTIONS,
        json!({"owner": owner, "name": name, "n": number}),
    )?;
    let pr = &v["data"]["repository"]["pullRequest"];
    if pr.is_null() {
        return Ok(());
    }
    o.reactions = groups(&pr["reactionGroups"]);
    for node in pr["comments"]["nodes"].as_array().into_iter().flatten() {
        let id = node["databaseId"].as_u64().unwrap_or_default().to_string();
        for e in &mut o.events {
            if let Event::Comment(c) = e
                && c.id == id
            {
                c.reactions = groups(&node["reactionGroups"]);
            }
        }
    }
    Ok(())
}

/// Where a reaction goes: the request (an issue, to GitHub), or a comment.
fn reactions_url(s: &Session, number: u64, to: &ReactTo) -> String {
    match to {
        ReactTo::Request => format!("{}/issues/{number}/reactions", api(s)),
        ReactTo::Comment(c) => format!("{}/reactions", comment_url(s, c)),
    }
}

/// Adds your reaction `name`, or takes it back.
pub fn react(s: &Session, number: u64, to: &ReactTo, name: &str, add: bool) -> Result<(), String> {
    let content = Reaction::github_content(name)
        .ok_or_else(|| "GitHub only takes 👍 👎 😄 🎉 😕 ❤ 🚀 👀".to_string())?;
    let url = reactions_url(s, number, to);
    let auth = Auth::Bearer(&s.token);
    // Adding one that's there gives it back: that's how its id is found to
    // take it back.
    let given = http::post(FORGE, &url, &auth, &json!({"content": content}))?;
    if !add {
        let id = given["id"]
            .as_u64()
            .ok_or_else(|| format!("{FORGE} didn't say which reaction was yours"))?;
        http::delete(FORGE, &format!("{url}/{id}"), &auth)?;
    }
    Ok(())
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
            on_code: true,
            reactions: groups(&c["reactionGroups"]),
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
    // Only the request itself is needed; what else can't be loaded is
    // noted, and the rest shows.
    let mut cut = false;
    let mut missing = Vec::new();
    let reviews = soft(
        all(
            s,
            &format!("/pulls/{number}/reviews?per_page=100"),
            &mut cut,
        ),
        "its reviews",
        &mut missing,
    )
    .unwrap_or(Value::Null);
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
        missing,
        ..Overview::default()
    };
    if let Some(found) = soft(checks(s, &o.refs.head), "its checks", &mut o.missing) {
        (r.checks, o.checks) = found;
    }
    o.cut_short = cut;
    let files = soft(
        all(
            s,
            &format!("/pulls/{number}/files?per_page=100"),
            &mut o.cut_short,
        ),
        "its changes",
        &mut o.missing,
    )
    .unwrap_or(Value::Null);
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
    let commits = soft(
        all(
            s,
            &format!("/pulls/{number}/commits?per_page=100"),
            &mut o.cut_short,
        ),
        "its commits",
        &mut o.missing,
    )
    .unwrap_or(Value::Null);
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
    let comments = soft(
        all(
            s,
            &format!("/issues/{number}/comments?per_page=100"),
            &mut o.cut_short,
        ),
        "its comments",
        &mut o.missing,
    )
    .unwrap_or(Value::Null);
    for c in comments.as_array().into_iter().flatten() {
        o.events.push(Event::Comment(Comment {
            id: c["id"].as_u64().unwrap_or_default().to_string(),
            author: text(&c["user"], "login"),
            body: text(c, "body"),
            created: timestamp(c["created_at"].as_str().unwrap_or_default()),
            suggestions: Vec::new(),
            on_code: false,
            reactions: rest_reactions(&c["reactions"]),
        }));
    }
    // Reactions: the request's, and which are yours.
    soft(
        reactions(s, number, &mut o),
        "its reactions",
        &mut o.missing,
    );
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
    // Your pending review (only you see it): its comments are your drafts.
    o.node_id = text(&v, "node_id");
    if let Some(p) = reviews
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["state"].as_str() == Some("PENDING"))
    {
        o.pending_review = Some(text(p, "node_id"));
        let id = p["id"].as_u64().unwrap_or_default();
        let comments = soft(
            all(
                s,
                &format!("/pulls/{number}/reviews/{id}/comments?per_page=100"),
                &mut o.cut_short,
            ),
            "your pending review's comments",
            &mut o.missing,
        )
        .unwrap_or(Value::Null);
        o.pending = comments
            .as_array()
            .into_iter()
            .flatten()
            .map(pending_draft)
            .collect();
    }
    let threads = threads(s, number, &mut o.cut_short);
    o.threads = soft(threads, "its threads on the code", &mut o.missing)
        .unwrap_or_default()
        .iter()
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
    let repo = soft(
        get(s, ""),
        "the repository's merge settings",
        &mut o.missing,
    )
    .unwrap_or(Value::Null);
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
    o.merge.can_update = o.state == State::Open && v["mergeable_state"].as_str() == Some("behind");
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

/// A comment in your pending review, as a draft.
fn pending_draft(c: &Value) -> Draft {
    let line = |n: &Value, side: &Value| line_on(side.as_str(), n.as_u64());
    let reply_to = c["in_reply_to_id"].as_u64().map(|id| id.to_string());
    let at = if reply_to.is_some() {
        None
    } else {
        line(&c["line"], &c["side"]).map(|l| Anchor {
            path: text(c, "path"),
            old_path: None,
            line: l,
            start: line(&c["start_line"], &c["start_side"]).filter(|s| *s != l),
        })
    };
    Draft {
        at,
        reply_to,
        body: text(c, "body"),
        remote: Some(text(c, "node_id")),
        local: 0,
    }
}

/// Your pending review's id, making one if there isn't one yet.
fn ensure_review(s: &Session, o: &Overview, review: Option<String>) -> Result<String, String> {
    if let Some(id) = review {
        return Ok(id);
    }
    let v = query(
        s,
        "mutation($pr: ID!, $head: GitObjectID) {
          addPullRequestReview(input: {pullRequestId: $pr, commitOID: $head}) { pullRequestReview { id } }
        }",
        json!({"pr": o.node_id, "head": o.refs.head}),
    )?;
    let id = text(
        &v["data"]["addPullRequestReview"]["pullRequestReview"],
        "id",
    );
    if id.is_empty() {
        Err(format!("{FORGE} didn't start a review"))
    } else {
        Ok(id)
    }
}

fn side_of(l: &Line) -> (&'static str, u32) {
    match (l.new, l.old) {
        (Some(n), _) => ("RIGHT", n),
        (None, Some(n)) => ("LEFT", n),
        (None, None) => ("RIGHT", 0),
    }
}

/// Keeps a draft in your pending review (making one if needed), or
/// changes it there. Returns its id there (`None` for a comment on the
/// whole request, which a review can't hold) and the review's id.
pub fn save_draft(
    s: &Session,
    o: &Overview,
    review: Option<String>,
    d: &Draft,
) -> Result<(Option<String>, Option<String>), String> {
    if let Some(remote) = &d.remote {
        query(
            s,
            "mutation($id: ID!, $body: String!) {
              updatePullRequestReviewComment(input: {pullRequestReviewCommentId: $id, body: $body}) {
                pullRequestReviewComment { id }
              }
            }",
            json!({"id": remote, "body": d.body}),
        )?;
        return Ok((Some(remote.clone()), review));
    }
    if !d.goes_in_review(false) {
        return Ok((None, review));
    }
    let review = ensure_review(s, o, review)?;
    let id = if let Some(to) = &d.reply_to {
        let thread = o
            .threads
            .iter()
            .find(|t| t.reply == *to)
            .map(|t| t.id.clone())
            .ok_or("the thread isn't there any more")?;
        let v = query(
            s,
            "mutation($review: ID!, $thread: ID!, $body: String!) {
              addPullRequestReviewThreadReply(input: {pullRequestReviewId: $review,
                pullRequestReviewThreadId: $thread, body: $body}) { comment { id } }
            }",
            json!({"review": review, "thread": thread, "body": d.body}),
        )?;
        text(
            &v["data"]["addPullRequestReviewThreadReply"]["comment"],
            "id",
        )
    } else {
        let at = d.at.as_ref().ok_or("nowhere to put it")?;
        let (side, line) = side_of(&at.line);
        let mut vars = json!({"review": review, "path": at.path, "line": line, "side": side,
            "body": d.body, "startLine": null, "startSide": null});
        if let Some(start) = &at.start {
            let (start_side, start_line) = side_of(start);
            vars["startLine"] = json!(start_line);
            vars["startSide"] = json!(start_side);
        }
        let v = query(
            s,
            "mutation($review: ID!, $path: String!, $line: Int!, $side: DiffSide!,
                $startLine: Int, $startSide: DiffSide, $body: String!) {
              addPullRequestReviewThread(input: {pullRequestReviewId: $review, path: $path,
                line: $line, side: $side, startLine: $startLine, startSide: $startSide,
                body: $body}) { thread { comments(first: 1) { nodes { id } } } }
            }",
            vars,
        )?;
        text(
            &v["data"]["addPullRequestReviewThread"]["thread"]["comments"]["nodes"][0],
            "id",
        )
    };
    Ok(((!id.is_empty()).then_some(id), Some(review)))
}

/// Takes a draft out of your pending review.
pub fn delete_draft(s: &Session, remote: &str) -> Result<(), String> {
    query(
        s,
        "mutation($id: ID!) {
          deletePullRequestReviewComment(input: {id: $id}) { pullRequestReview { id } }
        }",
        json!({"id": remote}),
    )?;
    Ok(())
}

/// Sends a review: comments on the whole request, then the pending review
/// (with any drafts not in it yet) and its verdict.
pub fn submit(
    s: &Session,
    number: u64,
    o: &Overview,
    review: Option<String>,
    (drafts, summary, finish): (&[Draft], &str, Finish),
) -> Result<(), String> {
    for d in drafts.iter().filter(|d| !d.goes_in_review(false)) {
        comment(s, number, &d.body)?;
    }
    // Drafts the pending review doesn't have yet (kept only here).
    let mut review = review;
    for d in drafts
        .iter()
        .filter(|d| d.goes_in_review(false) && d.remote.is_none())
    {
        review = save_draft(s, o, review, d)?.1;
    }
    let event = match finish {
        Finish::Comment => "COMMENT",
        Finish::Approve => "APPROVE",
        Finish::RequestChanges => "REQUEST_CHANGES",
    };
    match review {
        Some(id) => {
            query(
                s,
                "mutation($id: ID!, $event: PullRequestReviewEvent!, $body: String) {
                  submitPullRequestReview(input: {pullRequestReviewId: $id, event: $event, body: $body}) {
                    pullRequestReview { id }
                  }
                }",
                json!({"id": id, "event": event, "body": summary}),
            )?;
        }
        None if finish == Finish::Comment && summary.trim().is_empty() => {}
        None => {
            http::post(
                FORGE,
                &format!("{}/pulls/{number}/reviews", api(s)),
                &Auth::Bearer(&s.token),
                &json!({"commit_id": o.refs.head, "body": summary, "event": event}),
            )?;
        }
    }
    Ok(())
}

/// Where a posted comment is kept: on the request, or on its code.
fn comment_url(s: &Session, c: &Comment) -> String {
    let kind = if c.on_code { "pulls" } else { "issues" };
    format!("{}/{kind}/comments/{}", api(s), c.id)
}

/// Changes a comment's text.
pub fn edit_comment(s: &Session, c: &Comment, body: &str) -> Result<(), String> {
    http::patch(
        FORGE,
        &comment_url(s, c),
        &Auth::Bearer(&s.token),
        &json!({"body": body}),
    )?;
    Ok(())
}

/// Deletes a comment.
pub fn delete_comment(s: &Session, c: &Comment) -> Result<(), String> {
    http::delete(FORGE, &comment_url(s, c), &Auth::Bearer(&s.token))?;
    Ok(())
}

/// Brings a request up to date with its target: GitHub merges the target
/// into its branch.
pub fn update(s: &Session, r: &Request, head: &str) -> Result<(), String> {
    http::put(
        FORGE,
        &format!("{}/pulls/{}/update-branch", api(s), r.number),
        &Auth::Bearer(&s.token),
        &json!({"expected_head_sha": head}),
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
        Method::Rebase | Method::FastForward => "rebase",
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

pub fn issues(s: &Session, n: u32) -> Result<(Vec<Issue>, bool), String> {
    let (list, more) = page(
        s,
        "/issues?state=open&per_page=50&sort=created&direction=desc",
        n,
    )?;
    let issues: Vec<Issue> = list
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
        .collect();
    Ok((issues, more))
}

pub fn default_branch(s: &Session) -> Result<String, String> {
    Ok(text(&get(s, "")?, "default_branch"))
}

pub fn choices(s: &Session) -> Result<Choices, String> {
    let mut cut = false;
    let people = all(s, "/assignees?per_page=100", &mut cut)?;
    let labels = all(s, "/labels?per_page=100", &mut cut)?;
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

    #[test]
    fn reacts_and_takes_it_back() {
        let (base, seen) = mock::serve(vec![
            (201, r#"{"id": 1}"#.into()),
            (201, r#"{"id": 2}"#.into()),
            // Taking one back: adding it again says which is yours.
            (200, r#"{"id": 31}"#.into()),
            (204, String::new()),
        ]);
        let s = session(&base);
        let on_code = Comment {
            id: "42".into(),
            on_code: true,
            ..Comment::default()
        };
        s.react(7, &ReactTo::Request, "tada", true, "").unwrap();
        s.react(7, &ReactTo::Comment(on_code.clone()), "thumbsup", true, "")
            .unwrap();
        s.react(7, &ReactTo::Comment(on_code.clone()), "thumbsup", false, "")
            .unwrap();
        let other = s.react(7, &ReactTo::Comment(on_code), "party_parrot", true, "");
        assert!(other.unwrap_err().starts_with("GitHub only takes"));
        let seen = seen.lock().unwrap();
        let api = "/api/v3/repos/rocky/stash-box";
        assert_eq!(seen[0].0, format!("POST {api}/issues/7/reactions"));
        assert_eq!(seen[0].2, r#"{"content":"hooray"}"#);
        assert_eq!(seen[1].0, format!("POST {api}/pulls/comments/42/reactions"));
        assert_eq!(seen[1].2, r#"{"content":"+1"}"#);
        assert_eq!(seen[2].0, format!("POST {api}/pulls/comments/42/reactions"));
        assert_eq!(
            seen[3].0,
            format!("DELETE {api}/pulls/comments/42/reactions/31")
        );
        assert_eq!(seen.len(), 4);
    }

    #[test]
    fn reads_reactions() {
        let rest = json!({"+1": 2, "-1": 0, "laugh": 0, "hooray": 1, "confused": 0,
            "heart": 0, "rocket": 3, "eyes": 0, "total_count": 6, "url": "u"});
        let names: Vec<(String, u32)> = rest_reactions(&rest)
            .into_iter()
            .map(|r| (r.name, r.count))
            .collect();
        assert_eq!(
            names,
            [
                ("thumbsup".into(), 2),
                ("tada".into(), 1),
                ("rocket".into(), 3)
            ]
        );
        let t = thread(&json!({"id": "t", "line": 3, "comments": {"nodes": [{
            "databaseId": 1, "body": "b", "createdAt": "2026-11-01T00:00:00Z",
            "author": {"login": "a"},
            "reactionGroups": [
                {"content": "THUMBS_UP", "viewerHasReacted": true, "reactors": {"totalCount": 1}},
                {"content": "EYES", "reactors": {"totalCount": 0}},
                {"content": "HEART", "reactors": {"totalCount": 4}}
            ]
        }]}}));
        assert_eq!(
            t.comments[0].reactions,
            [
                Reaction {
                    name: "thumbsup".into(),
                    count: 1,
                    mine: true,
                    by: Vec::new()
                },
                Reaction {
                    name: "heart".into(),
                    count: 4,
                    ..Reaction::default()
                }
            ]
        );
    }

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
        let list = s.requests(1).unwrap().0;
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
            "GET /api/v3/repos/rocky/stash-box/pulls?state=open&per_page=50&sort=created&direction=desc&page=1"
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
        let issues = s.issues(1).unwrap().0;
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
                    {"user":{"login":"milo"},"state":"COMMENTED","body":"","submitted_at":"2026-11-04T00:00:00Z"},
                    {"id":9,"node_id":"R0","user":{"login":"me"},"state":"PENDING","body":""}]"#
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
                r#"[{"id":5,"user":{"login":"milo"},"body":"**Looks** good","created_at":"2026-11-02T00:00:00Z",
                    "reactions":{"+1":1}}]"#
                    .into(),
            ),
            // Reactions: the request's, and which of the comments' are yours.
            (
                200,
                r#"{"data":{"repository":{"pullRequest":{
                    "reactionGroups":[{"content":"ROCKET","viewerHasReacted":false,"reactors":{"totalCount":2}}],
                    "comments":{"nodes":[{"databaseId":5,"reactionGroups":[
                        {"content":"THUMBS_UP","viewerHasReacted":true,"reactors":{"totalCount":1}}]}]}}}}}"#
                    .into(),
            ),
            // The pending review's comment.
            (
                200,
                r#"[{"node_id":"PC1","body":"Hm","path":"src/map.rs","line":2,"side":"RIGHT"}]"#.into(),
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
            // The review: one draft (and pending review) already on GitHub,
            // two to add to it, then it's submitted, and a thread resolved.
            (201, json!({"data": {"addPullRequestReviewThread": {"thread": {"comments": {"nodes": [{"id": "C1"}]}}}}}).to_string()),
            (201, json!({"data": {"addPullRequestReviewThreadReply": {"comment": {"id": "C2"}}}}).to_string()),
            (200, json!({"data": {"submitPullRequestReview": {"pullRequestReview": {"id": "R0"}}}}).to_string()),
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
        assert_eq!(o.reactions[0].name, "rocket");
        assert_eq!(o.reactions[0].count, 2);
        let Some(Event::Comment(looks)) = o.events.iter().find(|e| matches!(e, Event::Comment(_)))
        else {
            panic!("the comment")
        };
        assert!(looks.reactions[0].mine, "yours, which REST doesn't say");
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
                ..Draft::default()
            },
            Draft {
                at: None,
                reply_to: Some("42".into()),
                body: "Because.".into(),
                ..Draft::default()
            },
        ];
        // The draft already in the pending review.
        assert_eq!(o.pending_review.as_deref(), Some("R0"));
        assert_eq!(o.pending.len(), 1);
        assert_eq!(o.pending[0].remote.as_deref(), Some("PC1"));
        assert_eq!(o.pending[0].at.as_ref().map(|a| a.line.new), Some(Some(2)));
        let all: Vec<Draft> = o.pending.iter().cloned().chain(drafts).collect();
        s.submit(
            7,
            &o,
            o.pending_review.clone(),
            (&all, "Close", Finish::RequestChanges),
        )
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
        assert_eq!(
            seen[8].0,
            "GET /api/v3/repos/rocky/stash-box/pulls/7/reviews/9/comments?per_page=100"
        );
        assert_eq!(seen[9].0, "POST /api/graphql");
        // The new line comment, and the reply, go into the pending review.
        let line: Value = serde_json::from_str(&seen[11].2).unwrap();
        assert!(
            line["query"]
                .as_str()
                .unwrap()
                .contains("addPullRequestReviewThread(")
        );
        assert_eq!(
            line["variables"],
            json!({"review": "R0", "path": "src/map.rs", "line": 1, "side": "LEFT",
                "body": "Gone?", "startLine": null, "startSide": null})
        );
        let reply: Value = serde_json::from_str(&seen[12].2).unwrap();
        assert_eq!(reply["variables"]["thread"], "T1");
        let submit: Value = serde_json::from_str(&seen[13].2).unwrap();
        assert!(
            submit["query"]
                .as_str()
                .unwrap()
                .contains("submitPullRequestReview")
        );
        assert_eq!(
            submit["variables"],
            json!({"id": "R0", "event": "REQUEST_CHANGES", "body": "Close"})
        );
        assert!(seen[14].2.contains("resolveReviewThread"));
        assert_eq!(
            seen[15].0,
            "PUT /api/v3/repos/rocky/stash-box/pulls/7/merge"
        );
        let merge: Value = serde_json::from_str(&seen[15].2).unwrap();
        assert_eq!(
            merge,
            json!({"merge_method": "squash", "commit_title": "Faster lookups (#7)", "commit_message": "Uses a hash map."})
        );
        assert_eq!(
            seen[16].0,
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

    #[test]
    fn reads_every_page_of_threads_and_their_comments() {
        let node = |id: &str, more: bool| {
            json!({"id": id, "isResolved": false, "path": "a.rs", "line": 1, "diffSide": "RIGHT",
                "comments": {"pageInfo": {"hasNextPage": more, "endCursor": "c1"},
                    "nodes": [{"databaseId": 1, "body": "first", "author": {"login": "ada"}}]}})
        };
        let page = |nodes: Vec<Value>, more: bool| {
            json!({"data": {"repository": {"pullRequest": {"reviewThreads": {
                "pageInfo": {"hasNextPage": more, "endCursor": "t1"}, "nodes": nodes}}}}})
            .to_string()
        };
        let (base, seen) = mock::serve(vec![
            (200, page(vec![node("T1", true)], true)),
            (200, page(vec![node("T2", false)], false)),
            (
                200,
                json!({"data": {"node": {"comments": {"pageInfo": {"hasNextPage": false},
                    "nodes": [{"databaseId": 2, "body": "second", "author": {"login": "milo"}}]}}}})
                .to_string(),
            ),
        ]);
        let mut cut = false;
        let nodes = threads(&session(&base), 7, &mut cut).unwrap();
        let all: Vec<Thread> = nodes.iter().map(thread).collect();
        assert_eq!(all.len(), 2);
        assert_eq!(
            all[0]
                .comments
                .iter()
                .map(|c| c.body.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        assert!(!cut);
        let seen = seen.lock().unwrap();
        assert!(seen[1].2.contains(r#""after":"t1""#), "{}", seen[1].2);
        assert!(seen[2].2.contains(r#""after":"c1""#), "{}", seen[2].2);
    }

    #[test]
    fn updates_a_branch_and_edits_and_deletes_comments() {
        let (base, seen) = mock::serve(vec![
            (202, r#"{"message":"Updating pull request branch."}"#.into()),
            (200, "{}".into()),
            (204, String::new()),
        ]);
        let s = session(&base);
        let r = request(&serde_json::from_str::<Value>(PR).unwrap());
        s.update(&r, "abc").unwrap();
        let on_code = Comment {
            id: "42".into(),
            on_code: true,
            ..Comment::default()
        };
        s.edit_comment(7, &on_code, "Better").unwrap();
        let on_request = Comment {
            id: "5".into(),
            ..Comment::default()
        };
        s.delete_comment(7, &on_request).unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0].0,
            "PUT /api/v3/repos/rocky/stash-box/pulls/7/update-branch"
        );
        assert_eq!(seen[0].2, r#"{"expected_head_sha":"abc"}"#);
        assert_eq!(
            seen[1].0,
            "PATCH /api/v3/repos/rocky/stash-box/pulls/comments/42"
        );
        assert_eq!(
            seen[2].0,
            "DELETE /api/v3/repos/rocky/stash-box/issues/comments/5"
        );
    }
}
