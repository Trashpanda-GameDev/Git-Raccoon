//! Talking to a forge's API: JSON in and out, with the account's token.

use serde_json::Value;

/// How a request authenticates.
pub enum Auth<'a> {
    /// A token, as "Authorization: Bearer" (GitHub and GitLab both take
    /// personal and OAuth tokens that way).
    Bearer(&'a str),
    None,
}

fn agent() -> String {
    format!("Git-Raccoon/{}", env!("CARGO_PKG_VERSION"))
}

/// A plain sentence for an HTTP failure.
fn explain(forge: &str, error: ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(401) => format!("{forge} turned it away: sign in again"),
        ureq::Error::StatusCode(403) => {
            format!("{forge} turned it away (no permission, or too many requests for now)")
        }
        ureq::Error::StatusCode(404) => {
            format!("{forge} doesn't know it (or it's private and this account can't see it)")
        }
        ureq::Error::StatusCode(422) => format!("{forge} wouldn't take it (it may already exist)"),
        ureq::Error::StatusCode(code) if code >= 500 => {
            format!("{forge} had a problem on its side ({code})")
        }
        ureq::Error::StatusCode(code) => format!("{forge} answered {code}"),
        other => format!("couldn't reach {forge}: {other}"),
    }
}

fn agent_for() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(true)
        .build()
        .into()
}

fn with_auth<B>(mut req: ureq::RequestBuilder<B>, auth: &Auth<'_>) -> ureq::RequestBuilder<B> {
    req = req
        .header("User-Agent", &agent())
        .header("Accept", "application/json");
    match auth {
        Auth::Bearer(t) => req.header("Authorization", &format!("Bearer {t}")),
        Auth::None => req,
    }
}

fn read(
    forge: &str,
    response: Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<Value, String> {
    let response = response.map_err(|e| explain(forge, e))?;
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    // Some answers have no body ("204 No Content").
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| format!("{forge} answered something unexpected: {e}"))
}

/// The part of an API address that says what was asked for:
/// ".../merge_requests/313/diffs" for any project.
fn what_of(url: &str) -> String {
    let path = url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.find('/').map(|i| &rest[i..]))
        .unwrap_or(url);
    let path = path.split('?').next().unwrap_or(path);
    // The project's own part goes: GitLab's "/projects/<id>", GitHub's
    // "/repos/<owner>/<name>".
    let short = if let Some(i) = path.find("/projects/") {
        let rest = &path[i + "/projects/".len()..];
        rest.find('/').map_or("", |j| &rest[j..])
    } else if let Some(i) = path.find("/repos/") {
        let rest = &path[i + "/repos/".len()..];
        rest.splitn(3, '/').nth(2).map_or("", |r| r)
    } else {
        path
    };
    if short.is_empty() {
        "the project".into()
    } else {
        format!("…/{}", short.trim_start_matches('/'))
    }
}

/// A GET, tried again once after a moment if the server failed (a 5xx):
/// busy self-managed servers do now and then. A failure says which call.
fn call_get(
    forge: &str,
    url: &str,
    auth: &Auth<'_>,
) -> Result<ureq::http::Response<ureq::Body>, String> {
    let attempt = || {
        with_auth(agent_for().get(url), auth)
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call()
    };
    let response = match attempt() {
        Err(ureq::Error::StatusCode(code)) if code >= 500 => {
            std::thread::sleep(std::time::Duration::from_millis(600));
            attempt()
        }
        other => other,
    };
    response.map_err(|e| format!("{}, for {}", explain(forge, e), what_of(url)))
}

/// GET some JSON.
pub fn get(forge: &str, url: &str, auth: &Auth<'_>) -> Result<Value, String> {
    read(forge, Ok(call_get(forge, url, auth)?))
}

/// The most pages [`get_all`] reads (of 100 each).
pub const MAX_PAGES: usize = 50;

/// GET one page of a list, and where the next one is, if there is one
/// (from the `Link` header, which GitHub and GitLab both send).
pub fn get_page(
    forge: &str,
    url: &str,
    auth: &Auth<'_>,
) -> Result<(Value, Option<String>), String> {
    let response = call_get(forge, url, auth)?;
    let next = response
        .headers()
        .get("link")
        .and_then(|l| l.to_str().ok())
        .and_then(next_link);
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let value = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text)
            .map_err(|e| format!("{forge} answered something unexpected: {e}"))?
    };
    Ok((value, next))
}

/// GET every page of a list (up to [`MAX_PAGES`]), as one list. Returns
/// whether it stopped short of the end.
pub fn get_all(forge: &str, url: &str, auth: &Auth<'_>) -> Result<(Value, bool), String> {
    let mut items = Vec::new();
    let mut next = Some(url.to_string());
    let mut pages = 0;
    while let Some(at) = next.take() {
        let (page, more) = get_page(forge, &at, auth)?;
        let Value::Array(list) = page else {
            // Not a list after all.
            return Ok((page, false));
        };
        items.extend(list);
        pages += 1;
        if pages == MAX_PAGES {
            return Ok((Value::Array(items), more.is_some()));
        }
        next = more;
    }
    Ok((Value::Array(items), false))
}

/// The `rel="next"` address in a `Link` header.
fn next_link(header: &str) -> Option<String> {
    header.split(',').find_map(|part| {
        let (url, params) = part.split_once(';')?;
        params
            .split(';')
            .any(|p| p.trim().replace(' ', "") == "rel=\"next\"")
            .then(|| {
                url.trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_string()
            })
    })
}

/// GET a file's bytes (an image), and its type if the server says.
pub fn get_bytes(url: &str, auth: &Auth<'_>) -> Result<(Vec<u8>, Option<String>), String> {
    let response = with_auth(agent_for().get(url), auth)
        .header("Accept", "*/*")
        .call()
        .map_err(|e| explain("The forge", e))?;
    let mime = response
        .headers()
        .get("content-type")
        .and_then(|t| t.to_str().ok())
        .map(str::to_string);
    let bytes = response
        .into_body()
        .with_config()
        .limit(20 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    Ok((bytes, mime))
}

/// POST some JSON, and get JSON back.
pub fn post(forge: &str, url: &str, auth: &Auth<'_>, body: &Value) -> Result<Value, String> {
    let req = with_auth(agent_for().post(url), auth)
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("Content-Type", "application/json");
    read(forge, req.send(body.to_string()))
}

/// PUT some JSON, and get JSON back.
pub fn put(forge: &str, url: &str, auth: &Auth<'_>, body: &Value) -> Result<Value, String> {
    let req = with_auth(agent_for().put(url), auth)
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("Content-Type", "application/json");
    read(forge, req.send(body.to_string()))
}

/// PATCH some JSON, and get JSON back.
pub fn patch(forge: &str, url: &str, auth: &Auth<'_>, body: &Value) -> Result<Value, String> {
    let req = with_auth(agent_for().patch(url), auth)
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("Content-Type", "application/json");
    read(forge, req.send(body.to_string()))
}

/// DELETE something.
pub fn delete(forge: &str, url: &str, auth: &Auth<'_>) -> Result<Value, String> {
    let req = with_auth(agent_for().delete(url), auth).header("X-GitHub-Api-Version", "2022-11-28");
    read(forge, req.call())
}

/// GET or POST (with `body`), and get the status and JSON back whatever
/// they are: for calls whose refusals say why in their body.
pub fn call_answered(
    url: &str,
    auth: &Auth<'_>,
    body: Option<&Value>,
) -> Result<(u16, Value), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let response = match body {
        Some(b) => with_auth(agent.post(url), auth)
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("Content-Type", "application/json")
            .send(b.to_string()),
        None => with_auth(agent.get(url), auth)
            .header("X-GitHub-Api-Version", "2022-11-28")
            .call(),
    }
    .map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let text = response.into_body().read_to_string().unwrap_or_default();
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

/// POST a form (OAuth), and get JSON back. Error statuses are answers
/// here (OAuth reports "still waiting" that way), so they're read too.
pub fn post_form(forge: &str, url: &str, form: &[(&str, &str)]) -> Result<Value, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let req = with_auth(agent.post(url), &Auth::None);
    read(forge, req.send_form(form.iter().copied()))
}

/// A string field, or "".
pub fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}

/// Percent-encodes a path segment (GitLab's project ids are their paths).
pub fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b));
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
pub mod mock {
    //! A stand-in forge for tests: answers each request from a list, and
    //! keeps what it was sent.

    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::sync::{Arc, Mutex};

    /// What came in: "METHOD /path", the headers (lower case) and body.
    pub type Seen = Arc<Mutex<Vec<(String, String, String)>>>;

    /// Serves `replies` (status, JSON) in order, one per request. Returns
    /// its address ("http://127.0.0.1:port") and what it saw.
    pub fn serve(replies: Vec<(u16, String)>) -> (String, Seen) {
        serve_paged(
            replies
                .into_iter()
                .map(|(s, b)| (s, String::new(), b))
                .collect(),
        )
    }

    /// [`serve`], with a `Link` header for each reply (`{base}` in it is
    /// the server's own address; empty for none).
    pub fn serve_paged(replies: Vec<(u16, String, String)>) -> (String, Seen) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let seen: Seen = Arc::default();
        let log = Arc::clone(&seen);
        let here = base.clone();
        std::thread::spawn(move || {
            for (status, link, body) in replies {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut first = String::new();
                let _ = reader.read_line(&mut first);
                let mut head = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(n) = line.to_lowercase().strip_prefix("content-length:") {
                        length = n.trim().parse().unwrap_or(0);
                    }
                    head.push_str(&line.to_lowercase());
                }
                let mut data = vec![0; length];
                let _ = reader.read_exact(&mut data);
                let request = first.split(' ').take(2).collect::<Vec<_>>().join(" ");
                log.lock().expect("log").push((
                    request,
                    head,
                    String::from_utf8_lossy(&data).into_owned(),
                ));
                let mut stream = stream;
                let link = if link.is_empty() {
                    String::new()
                } else {
                    format!("Link: {}\r\n", link.replace("{base}", &here))
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\n{link}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (base, seen)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_which_call_failed_after_trying_again() {
        assert_eq!(
            what_of(
                "https://git.example.com/api/v4/projects/team%2Fapp/merge_requests/313/diffs?per_page=100"
            ),
            "…/merge_requests/313/diffs"
        );
        assert_eq!(
            what_of("https://api.github.com/repos/rocky/stash-box/pulls/7/files"),
            "…/pulls/7/files"
        );
        assert_eq!(
            what_of("https://git.example.com/api/v4/projects/team%2Fapp"),
            "the project"
        );
        // A 500, then a 500 again: it's an error, naming the call.
        let (base, seen) = mock::serve(vec![(500, "{}".into()), (500, "{}".into())]);
        let e = get(
            "GitLab",
            &format!("{base}/api/v4/projects/a%2Fb/merge_requests/313/diffs"),
            &Auth::None,
        )
        .unwrap_err();
        assert_eq!(
            e,
            "GitLab had a problem on its side (500), for …/merge_requests/313/diffs"
        );
        assert_eq!(seen.lock().unwrap().len(), 2, "tried twice");
        // A 500, then an answer: fine.
        let (base, _) = mock::serve(vec![(502, "{}".into()), (200, "[1]".into())]);
        assert_eq!(
            get("GitLab", &base, &Auth::None).unwrap(),
            serde_json::json!([1])
        );
    }

    #[test]
    fn follows_next_pages() {
        assert_eq!(
            next_link(r#"<https://x/a?page=2>; rel="next", <https://x/a?page=9>; rel="last""#)
                .as_deref(),
            Some("https://x/a?page=2")
        );
        assert_eq!(next_link(r#"<https://x/a?page=1>; rel="prev""#), None);
        let (base, seen) = mock::serve_paged(vec![
            (
                200,
                r#"<{base}/list?page=2>; rel="next""#.into(),
                "[1, 2]".into(),
            ),
            (200, String::new(), "[3]".into()),
        ]);
        let (all, cut) = get_all("Test", &format!("{base}/list"), &Auth::None).unwrap();
        assert_eq!(all, serde_json::json!([1, 2, 3]));
        assert!(!cut);
        assert_eq!(seen.lock().unwrap()[1].0, "GET /list?page=2");
    }
}
