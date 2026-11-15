//! Crash reports, opt-in: if the app hits a bug, a report is written next
//! to its settings. Nothing is sent. On the next start the app says so,
//! shows the report, and the user chooses whether to send it (as a GitHub
//! issue they review before posting), copy it, or throw it away.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Where reports are kept: beside the settings (eframe's folder for the app).
pub fn folder() -> Option<PathBuf> {
    eframe::storage_dir("git-raccoon").map(|d| d.join("crashes"))
}

/// Where reports can be posted: the project's GitHub issues.
const ISSUES: &str = "https://github.com/Trashpanda-GameDev/Git-Raccoon/issues/new";

/// Writes a report whenever anything panics, then carries on as before.
pub fn install() {
    let Some(dir) = folder() else { return };
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = write_report(&dir, &report(info));
        previous(info);
    }));
}

fn report(info: &std::panic::PanicHookInfo<'_>) -> String {
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".into());
    let place = info
        .location()
        .map_or_else(String::new, |l| format!("{}:{}", l.file(), l.line()));
    let thread = std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_string();
    let mut out = String::new();
    let _ = writeln!(out, "Git Raccoon {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(
        out,
        "System: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let _ = writeln!(out, "Thread: {thread}");
    let _ = writeln!(out, "Where: {place}");
    let _ = writeln!(out, "What: {message}");
    let _ = writeln!(
        out,
        "\nBacktrace:\n{}",
        std::backtrace::Backtrace::force_capture()
    );
    out
}

fn write_report(dir: &Path, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    std::fs::write(dir.join(format!("crash-{stamp}.txt")), text)
}

/// A report waiting for the user's decision.
pub struct Report {
    pub path: PathBuf,
    pub text: String,
}

/// Reports left by earlier runs, oldest first.
pub fn pending(dir: &Path) -> Vec<Report> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            Some(Report { path, text })
        })
        .collect()
}

/// Done with a report (sent, copied or declined).
pub fn forget(report: &Report) {
    let _ = std::fs::remove_file(&report.path);
}

/// The new-issue link for a report. Links can only be so long, so a long
/// backtrace is cut; the full text goes on the clipboard too.
pub fn issue_url(report: &Report) -> String {
    let what = report
        .text
        .lines()
        .find_map(|l| l.strip_prefix("What: "))
        .unwrap_or("a crash");
    let title: String = format!("Crash: {what}").chars().take(100).collect();
    let mut body = String::from(
        "<!-- Thank you! Please say what you were doing when it happened. The full report is on your clipboard if it was cut short here. -->\n\nWhat I was doing:\n\n\n```\n",
    );
    body.extend(report.text.chars().take(5000));
    body.push_str("\n```\n");
    format!("{ISSUES}?title={}&body={}", encode(&title), encode(&body))
}

/// Percent-encodes text for a URL query.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b));
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_reports_until_dealt_with() {
        let dir = std::env::temp_dir().join(format!("git-raccoon-crash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(pending(&dir).is_empty());
        write_report(&dir, "Git Raccoon 0.5.0\nWhat: index out of bounds\n").unwrap();
        let reports = pending(&dir);
        assert_eq!(reports.len(), 1);
        let url = issue_url(&reports[0]);
        assert!(url.starts_with(ISSUES));
        assert!(
            url.contains("title=Crash%3A%20index%20out%20of%20bounds"),
            "{url}"
        );
        assert!(url.contains("0.5.0"));
        forget(&reports[0]);
        assert!(pending(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn encodes_for_links() {
        assert_eq!(encode("a b/c&é"), "a%20b%2Fc%26%C3%A9");
    }
}
