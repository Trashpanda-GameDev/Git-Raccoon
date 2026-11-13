//! Text formatting for dates, SHAs, paths and commit messages.

use chrono::{DateTime, Local, TimeZone};

/// "3 days ago", "yesterday", "just now", relative to `now` (both in Unix seconds).
pub fn age(seconds: i64, now: i64) -> String {
    let diff = now - seconds;
    let ago = diff >= 0;
    let d = diff.unsigned_abs();
    let units: [(&str, u64); 6] = [
        ("year", 31_536_000),
        ("month", 2_592_000),
        ("week", 604_800),
        ("day", 86_400),
        ("hour", 3_600),
        ("minute", 60),
    ];
    for (unit, size) in units {
        if d >= size {
            // Round to the nearest whole unit, like Intl.RelativeTimeFormat.
            let n = (d + size / 2) / size;
            return match (n, unit, ago) {
                (1, "day", true) => "yesterday".into(),
                (1, "day", false) => "tomorrow".into(),
                (1, _, true) => format!("1 {unit} ago"),
                (_, _, true) => format!("{n} {unit}s ago"),
                (1, _, false) => format!("in 1 {unit}"),
                (_, _, false) => format!("in {n} {unit}s"),
            };
        }
    }
    "just now".into()
}

/// "Nov 13, 2026, 12:46 AM" in local time.
pub fn date_time(seconds: i64) -> String {
    match Local.timestamp_opt(seconds, 0).single() {
        Some(t) => t.format("%b %-d, %Y, %-I:%M %p").to_string(),
        None => DateTime::from_timestamp(seconds, 0)
            .map(|t| t.to_rfc2822())
            .unwrap_or_default(),
    }
}

pub fn short_sha(id: &str) -> &str {
    id.get(..7).unwrap_or(id)
}

/// Splits "src/app/main.rs" into ("src/app/", "main.rs").
pub fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(i) => path.split_at(i + 1),
        None => ("", path),
    }
}

/// Commit messages are usually hard-wrapped at ~72 columns. Joins wrapped
/// lines back into paragraphs so they reflow, keeping blank-line paragraph
/// breaks, list items and indented (code) lines on their own lines.
pub fn unwrap_paragraphs(body: &str) -> Vec<String> {
    let mut paragraphs = Vec::new();
    for para in body
        .split("\n\n")
        .map(str::trim_end)
        .filter(|p| !p.trim().is_empty())
    {
        let mut out = String::new();
        for line in para.lines() {
            let trimmed = line.trim_start();
            let is_list = trimmed.starts_with(['-', '*', '+', '•'])
                && trimmed.chars().nth(1).is_some_and(char::is_whitespace)
                || trimmed.split_once(['.', ')']).is_some_and(|(n, rest)| {
                    !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) && rest.starts_with(' ')
                });
            let indented = line.starts_with("  ") || line.starts_with('\t');
            if out.is_empty() {
                out.push_str(line);
            } else if is_list || indented {
                out.push('\n');
                out.push_str(line);
            } else {
                out.push(' ');
                out.push_str(trimmed);
            }
        }
        paragraphs.push(out);
    }
    paragraphs
}

/// Up to two initials for an avatar.
pub fn initials(name: &str) -> String {
    let parts: Vec<&str> = name.split_whitespace().collect();
    let first = |s: &str| {
        s.chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
    };
    match parts.as_slice() {
        [] => "?".into(),
        [one] => first(one),
        [a, .., b] => first(a) + &first(b),
    }
}

/// A stable hue (0–360) for an email, so each author keeps a colour.
pub fn hue(email: &str) -> f32 {
    let h = email.chars().fold(0u32, |h, c| (h * 31 + c as u32) % 360);
    #[expect(clippy::cast_precision_loss, reason = "h < 360")]
    let hue = h as f32;
    hue
}

pub fn bytes(n: usize) -> String {
    #[expect(clippy::cast_precision_loss, reason = "display only")]
    let f = n as f64;
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", f / 1024.0)
    } else {
        format!("{:.1} MB", f / 1024.0 / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_ages() {
        let now = 1_000_000_000;
        assert_eq!(age(now - 10, now), "just now");
        assert_eq!(age(now - 3 * 3600, now), "3 hours ago");
        assert_eq!(age(now - 86_400, now), "yesterday");
        assert_eq!(age(now - 5 * 86_400, now), "5 days ago");
        assert_eq!(age(now - 14 * 86_400, now), "2 weeks ago");
        assert_eq!(age(now - 400 * 86_400, now), "1 year ago");
    }

    #[test]
    fn reflows_hard_wrapped_messages() {
        let body = "Bare repositories have no working tree, so the staging\nviews cannot work.\n\n- one\n- two\n\n    code line";
        assert_eq!(
            unwrap_paragraphs(body),
            [
                "Bare repositories have no working tree, so the staging views cannot work.",
                "- one\n- two",
                "    code line",
            ]
        );
    }

    #[test]
    fn splits_paths_and_names() {
        assert_eq!(split_path("src/app/main.rs"), ("src/app/", "main.rs"));
        assert_eq!(split_path("README.md"), ("", "README.md"));
        assert_eq!(initials("Rocky Raccoon"), "RR");
        assert_eq!(initials("Ana Maria Lopez"), "AL");
        assert_eq!(initials("  "), "?");
        assert_eq!(short_sha("a89ee03aaaaa"), "a89ee03");
        assert_eq!(bytes(5_222), "5.1 KB");
    }
}
