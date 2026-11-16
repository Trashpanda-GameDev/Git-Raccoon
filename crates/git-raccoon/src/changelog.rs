//! What's new: the notes for each version, from CHANGELOG.md (built in).

const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");

/// One version's notes.
#[derive(Debug, Clone, PartialEq)]
pub struct Notes {
    pub version: String,
    /// Its bullet points (Markdown, one per line).
    pub lines: Vec<String>,
}

/// "0.5.3" as numbers, for comparing; `None` if it isn't one.
fn parse(version: &str) -> Option<Vec<u32>> {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().ok())
        .collect()
}

/// Every version's notes in `text`, newest first.
fn all(text: &str) -> Vec<Notes> {
    let mut out: Vec<Notes> = Vec::new();
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            out.push(Notes {
                version: heading.trim().to_string(),
                lines: Vec::new(),
            });
        } else if let Some(notes) = out.last_mut()
            && !line.trim().is_empty()
        {
            notes.lines.push(line.trim().to_string());
        }
    }
    out
}

/// The notes for the versions after `last` up to `current`, newest first:
/// what changed since the version last run.
pub fn since(last: &str, current: &str) -> Vec<Notes> {
    since_in(CHANGELOG, last, current)
}

/// The notes for one version.
pub fn of(version: &str) -> Option<Notes> {
    all(CHANGELOG).into_iter().find(|n| n.version == version)
}

fn since_in(text: &str, last: &str, current: &str) -> Vec<Notes> {
    let (Some(last), Some(current)) = (parse(last), parse(current)) else {
        return Vec::new();
    };
    all(text)
        .into_iter()
        .filter(|n| parse(&n.version).is_some_and(|v| v > last && v <= current))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# Notes\n\nIntro.\n\n## 0.6.0\n\n- Next\n\n## 0.5.10\n\n- Ten\n- **Bold**\n\n## 0.5.2\n\n- Two\n";

    #[test]
    fn picks_the_versions_since_the_last_one_run() {
        let notes = since_in(TEXT, "0.5.2", "0.5.10");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].version, "0.5.10", "compared as numbers, not text");
        assert_eq!(notes[0].lines, ["- Ten", "- **Bold**"]);
        // Not ones after the version running.
        assert!(since_in(TEXT, "0.5.10", "0.5.10").is_empty());
        let both: Vec<String> = since_in(TEXT, "0.4.0", "0.6.0")
            .into_iter()
            .map(|n| n.version)
            .collect();
        assert_eq!(both, ["0.6.0", "0.5.10", "0.5.2"]);
        assert!(since_in(TEXT, "nonsense", "0.6.0").is_empty());
    }

    #[test]
    fn the_built_in_changelog_has_this_version() {
        assert!(
            of(env!("CARGO_PKG_VERSION")).is_some_and(|n| !n.lines.is_empty()),
            "CHANGELOG.md needs a section for {}",
            env!("CARGO_PKG_VERSION")
        );
    }
}
