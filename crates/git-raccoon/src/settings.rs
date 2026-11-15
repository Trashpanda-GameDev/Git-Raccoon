//! Preferences and recently opened repositories, saved by eframe between runs.

use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentRepo {
    pub path: String,
    /// Seconds since the Unix epoch.
    pub opened_at: i64,
}

impl RecentRepo {
    pub fn name(&self) -> &str {
        repo_name(&self.path)
    }
}

/// The last component of a path, ignoring trailing separators.
pub fn repo_name(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(trimmed)
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub recent: Vec<RecentRepo>,
    /// Reopened on start.
    pub last_repo: Option<String>,
    pub split_diff: bool,
    pub hide_sidebar: bool,
    /// Where the last clone went; new clones start there.
    pub clone_dir: Option<String>,
}

impl Settings {
    pub const KEY: &'static str = "settings";

    /// Moves `path` to the front of the recent list, adding it if new.
    pub fn remember(&mut self, path: &str) {
        let path = path.trim_end_matches(['/', '\\']).to_string();
        self.recent.retain(|r| r.path != path);
        self.recent.insert(
            0,
            RecentRepo {
                path,
                opened_at: now(),
            },
        );
        self.recent.truncate(MAX_RECENT);
    }

    pub fn forget(&mut self, path: &str) {
        self.recent.retain(|r| r.path != path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembers_most_recent_first_without_duplicates() {
        let mut s = Settings::default();
        s.remember("/a/one");
        s.remember("/a/two/");
        s.remember("/a/one");
        let paths: Vec<&str> = s.recent.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["/a/one", "/a/two"]);
        s.forget("/a/one");
        assert_eq!(s.recent.len(), 1);
        for i in 0..20 {
            s.remember(&format!("/r/{i}"));
        }
        assert_eq!(s.recent.len(), MAX_RECENT);
    }

    #[test]
    fn names_repos_by_last_path_component() {
        assert_eq!(repo_name("/home/me/git-raccoon/"), "git-raccoon");
        assert_eq!(repo_name(r"F:\Projects\Rust\Git-Raccoon"), "Git-Raccoon");
        assert_eq!(repo_name("solo"), "solo");
    }
}
