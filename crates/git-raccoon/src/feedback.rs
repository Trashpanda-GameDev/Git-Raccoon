//! Feedback: a bug report, feedback or an idea, posted as a GitHub issue,
//! which the project's auto-add workflow puts on its board.
//!
//! Release builds carry a token (from the `FEEDBACK_TOKEN` secret, added
//! by build.rs) that can only create and edit issues in this repository,
//! and post the issue themselves. Builds without one open the issue form
//! on GitHub instead (.github/ISSUE_TEMPLATE; its field ids are the query
//! parameters in [`issue_url`]), for the person to post.

use crate::crash::{ISSUES, encode};

include!(concat!(env!("OUT_DIR"), "/feedback_token.rs"));

const API: &str = "https://api.github.com/repos/Trashpanda-GameDev/Git-Raccoon/issues";

/// The token this build posts issues with, if it has one.
fn token() -> Option<String> {
    let bytes: Vec<u8> = SCRAMBLED
        .iter()
        .zip(KEY.iter().cycle())
        .map(|(b, k)| b ^ k)
        .collect();
    String::from_utf8(bytes).ok().filter(|t| !t.is_empty())
}

/// Whether this build posts feedback itself (or opens GitHub for it).
pub fn posts_itself() -> bool {
    token().is_some()
}

/// The issue for some feedback: its title, body and label.
fn issue(kind: Kind, title: &str, details: &str) -> serde_json::Value {
    let (prefix, label) = match kind {
        Kind::Bug => ("Bug", "bug"),
        Kind::Feedback => ("Feedback", "feedback"),
        Kind::Idea => ("Idea", "enhancement"),
    };
    // A long title is shortened; the whole of it starts the body.
    let full = title.trim();
    let title: String = if full.chars().count() > 100 {
        full.chars().take(99).chain(['…']).collect()
    } else {
        full.to_string()
    };
    let mut body = String::new();
    if title != full {
        body.push_str(&format!("**{full}**\n\n"));
    }
    body.extend(details.trim().chars().take(8000));
    if kind == Kind::Bug {
        body.push_str(&format!("\n\n**Version and system:** {}", system()));
    }
    body.push_str("\n\n---\n_Sent from the feedback form in Git Raccoon._");
    serde_json::json!({
        "title": format!("{prefix}: {title}"),
        "body": body,
        "labels": [label],
    })
}

/// Posts the feedback as an issue. Returns its number. Blocks: run it off
/// the UI thread.
pub fn post(kind: Kind, title: &str, details: &str) -> Result<u64, String> {
    let token = token().ok_or("this build can't post feedback itself")?;
    post_to(API, &token, kind, title, details)
}

fn post_to(api: &str, token: &str, kind: Kind, title: &str, details: &str) -> Result<u64, String> {
    let response = ureq::post(api)
        .header("Authorization", &format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(
            "User-Agent",
            &format!("Git-Raccoon/{}", env!("CARGO_PKG_VERSION")),
        )
        .header("Content-Type", "application/json")
        .send(issue(kind, title, details).to_string())
        .map_err(|e| match e {
            ureq::Error::StatusCode(401 | 403) => {
                "GitHub turned it away (the feedback token may have expired)".to_string()
            }
            ureq::Error::StatusCode(code) => format!("GitHub answered {code}"),
            other => format!("couldn't reach GitHub: {other}"),
        })?;
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let created: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    created["number"]
        .as_u64()
        .ok_or_else(|| "GitHub's answer had no issue number".to_string())
}

/// What's being sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bug,
    Feedback,
    Idea,
}

impl Kind {
    /// The issue form it goes through.
    fn template(self) -> &'static str {
        match self {
            Kind::Bug => "bug.yml",
            Kind::Feedback => "feedback.yml",
            Kind::Idea => "idea.yml",
        }
    }

    pub fn title_hint(self) -> &'static str {
        match self {
            Kind::Bug => "What went wrong, in a line",
            Kind::Feedback => "What it's about, in a line",
            Kind::Idea => "The idea, in a line",
        }
    }

    pub fn details_hint(self) -> &'static str {
        match self {
            Kind::Bug => "What you did, what happened, and what you expected",
            Kind::Feedback => "What you like, what's awkward, anything at all",
            Kind::Idea => "What it would do, and what it would help with",
        }
    }
}

/// The version and system, for a bug report.
pub fn system() -> String {
    format!(
        "Git Raccoon {} on {} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// The link that opens the prefilled issue form on GitHub.
pub fn issue_url(kind: Kind, title: &str, details: &str) -> String {
    let title: String = title.trim().chars().take(120).collect();
    let details: String = details.trim().chars().take(4000).collect();
    let mut url = format!(
        "{ISSUES}?template={}&title={}&details={}",
        kind.template(),
        encode(&title),
        encode(&details)
    );
    if kind == Kind::Bug {
        url.push_str("&version=");
        url.push_str(&encode(&system()));
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posts_the_issue_and_reads_its_number() {
        use std::io::{BufRead as _, BufReader, Read as _, Write as _};
        // A stand-in for GitHub's API.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}/issues", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if let Some(n) = line.to_lowercase().strip_prefix("content-length:") {
                    length = n.trim().parse().unwrap();
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let reply = "{\"number\": 42}";
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
            (head, String::from_utf8(body).unwrap())
        });
        let number = post_to(&api, "t0ken", Kind::Feedback, "Lovely", "Really.").unwrap();
        assert_eq!(number, 42);
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("POST /issues "));
        assert!(head.to_lowercase().contains("authorization: bearer t0ken"));
        let sent: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(sent["title"], "Feedback: Lovely");
        assert_eq!(sent["labels"][0], "feedback");
    }

    #[test]
    fn writes_the_issue() {
        let bug = issue(Kind::Bug, " It crashed ", "When I clicked.");
        assert_eq!(bug["title"], "Bug: It crashed");
        assert_eq!(bug["labels"][0], "bug");
        let body = bug["body"].as_str().unwrap();
        assert!(body.starts_with("When I clicked.\n\n**Version and system:** Git Raccoon "));
        assert!(body.ends_with("_Sent from the feedback form in Git Raccoon._"));
        let long = "word ".repeat(40);
        let long_one = issue(Kind::Idea, &long, "Details.");
        let title = long_one["title"].as_str().unwrap();
        assert!(title.chars().count() <= 106 && title.ends_with('…'));
        assert!(
            long_one["body"]
                .as_str()
                .unwrap()
                .starts_with(&format!("**{}**", long.trim()))
        );
        let idea = issue(Kind::Idea, "More raccoons", "");
        assert_eq!(idea["labels"][0], "enhancement");
        assert!(!idea["body"].as_str().unwrap().contains("Version"));
    }

    #[test]
    fn builds_a_prefilled_issue_link() {
        let url = issue_url(Kind::Idea, " Dark mode for raccoons ", "Please & thanks");
        assert!(url.starts_with(
            "https://github.com/Trashpanda-GameDev/Git-Raccoon/issues/new?template=idea.yml&"
        ));
        assert!(url.contains("&title=Dark%20mode%20for%20raccoons&"));
        assert!(url.contains("&details=Please%20%26%20thanks"));
        assert!(
            !url.contains("version="),
            "only bug reports carry the system"
        );
        assert!(issue_url(Kind::Bug, "x", "y").contains("&version=Git%20Raccoon%20"));
    }
}
