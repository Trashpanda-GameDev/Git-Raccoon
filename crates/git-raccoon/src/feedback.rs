//! Feedback: a bug report, feedback or an idea, posted as a GitHub issue,
//! which the project's auto-add workflow puts on its board.
//!
//! How it's posted depends on who's asking ([`route`]):
//!
//! - Signed in to GitHub in Git Raccoon: with their own account, so the
//!   issue is theirs and replies reach them.
//! - Otherwise, through the feedback relay (a small Cloudflare Worker, in
//!   `feedback-relay/`), which posts it as the "Git Raccoon Feedback"
//!   GitHub App. The app holds no token for that: the relay does.
//! - Builds that don't know a relay open the issue form on GitHub instead
//!   (.github/ISSUE_TEMPLATE; its field ids are the query parameters in
//!   [`issue_url`]), for the person to post.
//!
//! A screenshot can go with it either way it's posted. GitHub's API can't
//! attach images to an issue, so the relay keeps it in a repository of
//! feedback images and the issue shows it from there.

use crate::crash::{ISSUES, encode};
use crate::forge::Account;

const API: &str = "https://api.github.com/repos/Trashpanda-GameDev/Git-Raccoon/issues";

/// The feedback relay's address: from the build (the release workflow sets
/// it), or for trying one out, from the environment when run.
pub fn relay() -> Option<String> {
    std::env::var("GIT_RACCOON_FEEDBACK_RELAY")
        .ok()
        .or_else(|| option_env!("GIT_RACCOON_FEEDBACK_RELAY").map(str::to_string))
        .map(|u| u.trim().trim_end_matches('/').to_string())
        .filter(|u| !u.is_empty())
}

/// How feedback is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// Posted with this GitHub account, as its owner.
    AsYou(Account),
    /// Posted by the feedback bot, through the relay.
    Bot,
    /// Continued on GitHub's own issue form, in the browser.
    OnGitHub,
}

impl Route {
    /// Whether it's posted from here (rather than on GitHub's page).
    pub fn posts(&self) -> bool {
        !matches!(self, Route::OnGitHub)
    }

    /// Whether a screenshot can go with it (the relay keeps those).
    pub fn attaches(&self) -> bool {
        self.posts() && relay().is_some()
    }
}

/// How feedback goes for someone signed in to `accounts`.
pub fn route(accounts: &[Account]) -> Route {
    let github = accounts
        .iter()
        .find(|a| a.kind == crate::forge::Kind::GitHub && a.base == "https://github.com");
    match (github, relay()) {
        (Some(account), _) => Route::AsYou(account.clone()),
        (None, Some(_)) => Route::Bot,
        (None, None) => Route::OnGitHub,
    }
}

/// A screenshot as a PNG file.
pub fn png(image: &eframe::egui::ColorImage) -> Result<Vec<u8>, String> {
    let [w, h] = image.size;
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
    let buffer = image::RgbaImage::from_raw(
        u32::try_from(w).map_err(|e| e.to_string())?,
        u32::try_from(h).map_err(|e| e.to_string())?,
        rgba,
    )
    .ok_or("the screenshot was the wrong size")?;
    let mut out = std::io::Cursor::new(Vec::new());
    buffer
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// A smaller copy of a screenshot, for showing: at most `side` pixels
/// either way (a whole high-resolution window is too big for a texture).
pub fn thumbnail(image: &eframe::egui::ColorImage, side: usize) -> eframe::egui::ColorImage {
    let [w, h] = image.size;
    let scale = (side as f64 / w.max(h).max(1) as f64).min(1.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive size, no bigger than the original"
    )]
    let (tw, th) = (
        ((w as f64 * scale).round() as usize).max(1),
        ((h as f64 * scale).round() as usize).max(1),
    );
    let pixels = (0..th)
        .flat_map(|y| (0..tw).map(move |x| (x, y)))
        .map(|(x, y)| image.pixels[(y * h / th) * w + x * w / tw])
        .collect();
    eframe::egui::ColorImage::new([tw, th], pixels)
}

/// Base64, as GitHub's contents API takes files.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ABC[(n >> (18 - 6 * i)) as usize & 63]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn github(
    request: ureq::RequestBuilder<ureq::typestate::WithBody>,
    token: &str,
) -> ureq::RequestBuilder<ureq::typestate::WithBody> {
    request
        .header("Authorization", &format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header(
            "User-Agent",
            &format!("Git-Raccoon/{}", env!("CARGO_PKG_VERSION")),
        )
        .header("Content-Type", "application/json")
}

/// Sends JSON to the relay; its answer, or why there wasn't one.
fn to_relay(
    relay: &str,
    path: &str,
    json: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let response = ureq::post(&format!("{relay}{path}"))
        .header(
            "User-Agent",
            &format!("Git-Raccoon/{}", env!("CARGO_PKG_VERSION")),
        )
        .header("Content-Type", "application/json")
        .send(json.to_string())
        .map_err(|e| match e {
            ureq::Error::StatusCode(429) => {
                "too many were sent from here in the last hour; try again later".to_string()
            }
            ureq::Error::StatusCode(400) => "the feedback service turned it down".to_string(),
            ureq::Error::StatusCode(code) => format!("the feedback service answered {code}"),
            other => format!("couldn't reach the feedback service: {other}"),
        })?;
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Uploads a screenshot through the relay. Returns the address it can be
/// seen at.
fn upload_via(relay: &str, png: &[u8]) -> Result<String, String> {
    let made = to_relay(
        relay,
        "/screenshot",
        &serde_json::json!({ "png": base64(png) }),
    )?;
    made["url"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "the feedback service didn't say where the screenshot is".to_string())
}

/// Who an issue is from: the person (their account posts it), or someone
/// unnamed (the bot does), who may say how to reach them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sender<'a> {
    You,
    Someone { contact: &'a str },
}

/// The issue for some feedback: its title, body and label.
fn issue(
    kind: Kind,
    title: &str,
    details: &str,
    screenshot: Option<&str>,
    from: Sender<'_>,
) -> serde_json::Value {
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
    if let Some(url) = screenshot {
        body.push_str(&format!("\n\n![Screenshot]({url})"));
    }
    if kind == Kind::Bug {
        body.push_str(&format!("\n\n**Version and system:** {}", system()));
    }
    match from {
        Sender::You => body.push_str("\n\n---\n_Sent from the feedback form in Git Raccoon._"),
        Sender::Someone { contact } => {
            let contact: String = contact.trim().chars().take(200).collect();
            if !contact.is_empty() {
                body.push_str(&format!("\n\n**How to reach them:** {contact}"));
            }
            body.push_str("\n\n---\n_Sent anonymously from the feedback form in Git Raccoon._");
        }
    }
    serde_json::json!({
        "title": format!("{prefix}: {title}"),
        "body": body,
        "labels": [label],
    })
}

/// Posts the feedback as an issue, the way `route` says, with a
/// screenshot (a PNG) if given. Returns its number. Blocks: run it off the
/// UI thread.
pub fn post(
    route: &Route,
    (kind, title, details): (Kind, &str, &str),
    contact: &str,
    screenshot: Option<&[u8]>,
) -> Result<u64, String> {
    let relay = relay();
    let shot = match (screenshot, relay.as_deref()) {
        (Some(png), Some(relay)) => Some(upload_via(relay, png)?),
        _ => None,
    };
    match route {
        Route::AsYou(account) => {
            let token = crate::forge::accounts::load(account)?.token;
            let body = issue(kind, title, details, shot.as_deref(), Sender::You);
            post_to(API, &token, &body)
        }
        Route::Bot => {
            let relay = relay.ok_or("this build has no feedback service")?;
            let body = issue(
                kind,
                title,
                details,
                shot.as_deref(),
                Sender::Someone { contact },
            );
            let made = to_relay(&relay, "/issue", &body)?;
            made["number"]
                .as_u64()
                .ok_or_else(|| "the feedback service didn't say which issue it is".to_string())
        }
        Route::OnGitHub => Err("this build can't post feedback itself".into()),
    }
}

/// Posts an issue with a person's own token.
fn post_to(api: &str, token: &str, issue: &serde_json::Value) -> Result<u64, String> {
    let response = github(ureq::post(api), token)
        .send(issue.to_string())
        .map_err(|e| match e {
            ureq::Error::StatusCode(401 | 403 | 404) => {
                "GitHub didn't let your account post it (sign in again under Settings › Accounts)"
                    .to_string()
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

    /// A stand-in server: answers one request with `status` and `reply`,
    /// and gives back what it was sent (head, body).
    fn stand_in(
        status: &'static str,
        reply: &'static str,
    ) -> (String, std::thread::JoinHandle<(String, String)>) {
        use std::io::{BufRead as _, BufReader, Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let (mut head, mut length) = (String::new(), 0);
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
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
            (head, String::from_utf8(body).unwrap())
        });
        (base, server)
    }

    #[test]
    fn posts_the_issue_with_their_own_token() {
        let (base, server) = stand_in("201 Created", "{\"number\": 42}");
        let body = issue(
            Kind::Feedback,
            "Lovely",
            "Really.",
            Some("https://img/x.png"),
            Sender::You,
        );
        let number = post_to(&format!("{base}/issues"), "t0ken", &body).unwrap();
        assert_eq!(number, 42);
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("POST /issues "));
        assert!(head.to_lowercase().contains("authorization: bearer t0ken"));
        let sent: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(sent["title"], "Feedback: Lovely");
        assert_eq!(sent["labels"][0], "feedback");
        assert!(
            sent["body"]
                .as_str()
                .unwrap()
                .contains("Really.\n\n![Screenshot](https://img/x.png)")
        );
    }

    #[test]
    fn says_so_when_their_account_cant_post() {
        let (base, server) = stand_in("403 Forbidden", "{}");
        let body = issue(Kind::Idea, "x", "y", None, Sender::You);
        let err = post_to(&format!("{base}/issues"), "t0ken", &body).unwrap_err();
        assert!(err.contains("sign in again"), "{err}");
        server.join().unwrap();
    }

    #[test]
    fn posts_through_the_relay_with_no_token() {
        let (base, server) = stand_in(
            "201 Created",
            r#"{"number": 77, "url": "https://github.com/x/y/issues/77"}"#,
        );
        let body = issue(
            Kind::Bug,
            "It broke",
            "Badly.",
            None,
            Sender::Someone {
                contact: " me@example.com ",
            },
        );
        let made = to_relay(&base, "/issue", &body).unwrap();
        assert_eq!(made["number"], 77);
        let (head, sent) = server.join().unwrap();
        assert!(head.starts_with("POST /issue "), "{head}");
        assert!(
            !head.to_lowercase().contains("authorization"),
            "no token goes out"
        );
        let sent: serde_json::Value = serde_json::from_str(&sent).unwrap();
        let text = sent["body"].as_str().unwrap();
        assert!(text.contains("**How to reach them:** me@example.com"));
        assert!(text.ends_with("_Sent anonymously from the feedback form in Git Raccoon._"));
    }

    #[test]
    fn explains_a_busy_relay() {
        let (base, server) = stand_in("429 Too Many Requests", "{}");
        let err = to_relay(&base, "/issue", &serde_json::json!({})).unwrap_err();
        assert!(err.contains("try again later"), "{err}");
        server.join().unwrap();
    }

    #[test]
    fn uploads_a_screenshot_through_the_relay() {
        let (base, server) = stand_in("201 Created", r#"{"url": "https://raw.example/shot.png"}"#);
        let image = eframe::egui::ColorImage::new([3, 2], vec![eframe::egui::Color32::RED; 6]);
        let png = png(&image).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        let url = upload_via(&base, &png).unwrap();
        assert_eq!(url, "https://raw.example/shot.png");
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("POST /screenshot "), "{head}");
        let sent: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(sent["png"].as_str().unwrap(), base64(&png));
    }

    #[test]
    fn goes_as_them_when_signed_in_to_github() {
        use crate::forge::Kind as Forge;
        let account = |kind, base: &str| Account {
            kind,
            base: base.into(),
            login: "rocky".into(),
            client: None,
        };
        let github = account(Forge::GitHub, "https://github.com");
        let gitlab = account(Forge::GitLab, "https://gitlab.com");
        assert_eq!(
            route(&[gitlab.clone(), github.clone()]),
            Route::AsYou(github)
        );
        // Without a GitHub account: the bot, if this build knows a relay.
        let other = route(&[gitlab]);
        assert!(matches!(other, Route::Bot | Route::OnGitHub));
        assert_eq!(other == Route::Bot, relay().is_some());
    }

    #[test]
    fn shrinks_a_screenshot_for_showing() {
        use eframe::egui::{Color32, ColorImage};
        let big = ColorImage::new([2560, 1600], vec![Color32::BLUE; 2560 * 1600]);
        let small = thumbnail(&big, 1024);
        assert_eq!(small.size, [1024, 640]);
        assert_eq!(small.pixels.len(), 1024 * 640);
        let tiny = ColorImage::new([10, 5], vec![Color32::RED; 50]);
        assert_eq!(thumbnail(&tiny, 1024).size, [10, 5], "never bigger");
    }

    #[test]
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe]), "//4=");
    }

    #[test]
    fn writes_the_issue() {
        let bug = issue(
            Kind::Bug,
            " It crashed ",
            "When I clicked.",
            None,
            Sender::You,
        );
        assert_eq!(bug["title"], "Bug: It crashed");
        assert_eq!(bug["labels"][0], "bug");
        let body = bug["body"].as_str().unwrap();
        assert!(body.starts_with("When I clicked.\n\n**Version and system:** Git Raccoon "));
        assert!(body.ends_with("_Sent from the feedback form in Git Raccoon._"));
        let long = "word ".repeat(40);
        let long_one = issue(Kind::Idea, &long, "Details.", None, Sender::You);
        let title = long_one["title"].as_str().unwrap();
        assert!(title.chars().count() <= 106 && title.ends_with('…'));
        assert!(
            long_one["body"]
                .as_str()
                .unwrap()
                .starts_with(&format!("**{}**", long.trim()))
        );
        let idea = issue(
            Kind::Idea,
            "More raccoons",
            "",
            None,
            Sender::Someone { contact: "" },
        );
        assert_eq!(idea["labels"][0], "enhancement");
        assert!(!idea["body"].as_str().unwrap().contains("Version"));
        assert!(
            !idea["body"].as_str().unwrap().contains("How to reach"),
            "no contact given"
        );
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
