//! Feedback: a bug report, feedback or an idea, posted as a GitHub issue,
//! which the project's auto-add workflow puts on its board.
//!
//! Release builds carry a token (from the `FEEDBACK_TOKEN` secret, added
//! by build.rs) that can only create and edit issues in this repository,
//! and post the issue themselves. Builds without one open the issue form
//! on GitHub instead (.github/ISSUE_TEMPLATE; its field ids are the query
//! parameters in [`issue_url`]), for the person to post.
//!
//! A screenshot can go with it. GitHub's API can't attach images to an
//! issue, so it's uploaded to a separate repository of feedback images
//! (with a second token that can only write there) and shown in the issue
//! from there.

use crate::crash::{ISSUES, encode};

include!(concat!(env!("OUT_DIR"), "/feedback_token.rs"));

const API: &str = "https://api.github.com/repos/Trashpanda-GameDev/Git-Raccoon/issues";

/// Where screenshots are uploaded.
const IMAGES: &str =
    "https://api.github.com/repos/Trashpanda-GameDev/git-raccoon-feedback-images/contents";

fn unscramble(scrambled: &[u8]) -> Option<String> {
    let bytes: Vec<u8> = scrambled
        .iter()
        .zip(KEY.iter().cycle())
        .map(|(b, k)| b ^ k)
        .collect();
    String::from_utf8(bytes).ok().filter(|t| !t.is_empty())
}

/// The token this build posts issues with, if it has one.
fn token() -> Option<String> {
    unscramble(SCRAMBLED)
}

/// The token this build uploads screenshots with, if it has one.
fn screenshot_token() -> Option<String> {
    unscramble(SCREENSHOT_SCRAMBLED)
}

/// Whether this build posts feedback itself (or opens GitHub for it).
pub fn posts_itself() -> bool {
    token().is_some()
}

/// Whether this build can send a screenshot with the feedback.
pub fn uploads_screenshots() -> bool {
    posts_itself() && screenshot_token().is_some()
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

/// Uploads a screenshot. Returns the address it can be seen at.
fn upload_to(api: &str, token: &str, png: &[u8]) -> Result<String, String> {
    let name = format!(
        "{}-{:08x}.png",
        chrono::Utc::now().format("%Y-%m-%d-%H%M%S"),
        rand_suffix()
    );
    let body = serde_json::json!({
        "message": "A screenshot sent with feedback",
        "content": base64(png),
    });
    let response = github(ureq::put(&format!("{api}/{name}")), token)
        .send(body.to_string())
        .map_err(|e| match e {
            ureq::Error::StatusCode(code) => format!("GitHub answered {code}"),
            other => format!("couldn't reach GitHub: {other}"),
        })?;
    let text = response
        .into_body()
        .read_to_string()
        .map_err(|e| e.to_string())?;
    let made: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    made["content"]["download_url"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "GitHub's answer had no address for it".to_string())
}

/// A few random bits for a file name.
fn rand_suffix() -> u32 {
    use std::hash::{BuildHasher as _, Hasher as _};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    #[expect(clippy::cast_possible_truncation, reason = "a few bits are plenty")]
    let bits = h.finish() as u32;
    bits
}

/// The issue for some feedback: its title, body and label.
fn issue(kind: Kind, title: &str, details: &str, screenshot: Option<&str>) -> serde_json::Value {
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
    body.push_str("\n\n---\n_Sent from the feedback form in Git Raccoon._");
    serde_json::json!({
        "title": format!("{prefix}: {title}"),
        "body": body,
        "labels": [label],
    })
}

/// Posts the feedback as an issue, with a screenshot (a PNG) if given.
/// Returns its number. Blocks: run it off the UI thread.
pub fn post(
    kind: Kind,
    title: &str,
    details: &str,
    screenshot: Option<&[u8]>,
) -> Result<u64, String> {
    let token = token().ok_or("this build can't post feedback itself")?;
    let shot = match (screenshot, screenshot_token()) {
        (Some(png), Some(t)) => Some(upload_to(IMAGES, &t, png)?),
        _ => None,
    };
    post_to(API, &token, kind, title, details, shot.as_deref())
}

fn post_to(
    api: &str,
    token: &str,
    kind: Kind,
    title: &str,
    details: &str,
    screenshot: Option<&str>,
) -> Result<u64, String> {
    let response = github(ureq::post(api), token)
        .send(issue(kind, title, details, screenshot).to_string())
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
        let number = post_to(
            &api,
            "t0ken",
            Kind::Feedback,
            "Lovely",
            "Really.",
            Some("https://img/x.png"),
        )
        .unwrap();
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
    fn uploads_a_screenshot_as_a_png() {
        use std::io::{BufRead as _, BufReader, Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let api = format!("http://{}/contents", listener.local_addr().unwrap());
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
            let reply = r#"{"content": {"download_url": "https://raw.example/shot.png"}}"#;
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
            (head, String::from_utf8(body).unwrap())
        });
        let image = eframe::egui::ColorImage::new([3, 2], vec![eframe::egui::Color32::RED; 6]);
        let png = png(&image).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        let url = upload_to(&api, "img-t0ken", &png).unwrap();
        assert_eq!(url, "https://raw.example/shot.png");
        let (head, body) = server.join().unwrap();
        assert!(
            head.starts_with("PUT /contents/") && head.contains(".png "),
            "{head}"
        );
        assert!(
            head.to_lowercase()
                .contains("authorization: bearer img-t0ken")
        );
        let sent: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(sent["content"].as_str().unwrap(), base64(&png));
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
        let bug = issue(Kind::Bug, " It crashed ", "When I clicked.", None);
        assert_eq!(bug["title"], "Bug: It crashed");
        assert_eq!(bug["labels"][0], "bug");
        let body = bug["body"].as_str().unwrap();
        assert!(body.starts_with("When I clicked.\n\n**Version and system:** Git Raccoon "));
        assert!(body.ends_with("_Sent from the feedback form in Git Raccoon._"));
        let long = "word ".repeat(40);
        let long_one = issue(Kind::Idea, &long, "Details.", None);
        let title = long_one["title"].as_str().unwrap();
        assert!(title.chars().count() <= 106 && title.ends_with('…'));
        assert!(
            long_one["body"]
                .as_str()
                .unwrap()
                .starts_with(&format!("**{}**", long.trim()))
        );
        let idea = issue(Kind::Idea, "More raccoons", "", None);
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
