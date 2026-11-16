//! Feedback: a bug report, feedback or an idea, posted as a GitHub issue
//! from a prefilled form. The issue forms in .github/ISSUE_TEMPLATE label
//! it (bug, feedback, enhancement) and put it on the project board; their
//! field ids are the query parameters here.

use crate::crash::{ISSUES, encode};

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
