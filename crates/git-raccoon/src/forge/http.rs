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
    serde_json::from_str(&text).map_err(|e| format!("{forge} answered something unexpected: {e}"))
}

/// GET some JSON.
pub fn get(forge: &str, url: &str, auth: &Auth<'_>) -> Result<Value, String> {
    let req = with_auth(agent_for().get(url), auth).header("X-GitHub-Api-Version", "2022-11-28");
    read(forge, req.call())
}

/// POST some JSON, and get JSON back.
pub fn post(forge: &str, url: &str, auth: &Auth<'_>, body: &Value) -> Result<Value, String> {
    let req = with_auth(agent_for().post(url), auth)
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("Content-Type", "application/json");
    read(forge, req.send(body.to_string()))
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
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let seen: Seen = Arc::default();
        let log = Arc::clone(&seen);
        std::thread::spawn(move || {
            for (status, body) in replies {
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
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        });
        (base, seen)
    }
}
