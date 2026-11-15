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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub recent: Vec<RecentRepo>,
    /// Reopened on start (from before tabs; `open_tabs` wins).
    pub last_repo: Option<String>,
    /// The repositories open in tabs, reopened on start.
    pub open_tabs: Vec<String>,
    /// Which of them was showing.
    pub active_tab: usize,
    pub split_diff: bool,
    pub hide_sidebar: bool,
    /// Where the last clone went; new clones start there.
    pub clone_dir: Option<String>,
    pub theme: ThemeChoice,
    /// How big everything is drawn; `None` for normal (100%).
    pub zoom: Option<f32>,
    /// Don't look for updates.
    pub no_update_checks: bool,
    /// Changed files shown as a tree of folders, rather than by path.
    pub file_tree: bool,
    /// Which defaults these settings were saved with (see `upgrade`):
    /// 0 when missing, from a version before there was this field.
    #[serde(default)]
    pub defaults: u32,
}

/// The current defaults: 1 since Raccoon became the default look.
const DEFAULTS: u32 = 1;

/// A look, or light or dark following the system.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    System,
    Dark,
    Light,
    /// Git Raccoon's own look, and the default.
    #[default]
    Raccoon,
    Pastel,
    Abyss,
    Forest,
    Paper,
    HighContrast,
}

/// The interface sizes offered.
pub const ZOOMS: [f32; 5] = [0.9, 1.0, 1.1, 1.25, 1.5];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            recent: Vec::new(),
            last_repo: None,
            open_tabs: Vec::new(),
            active_tab: 0,
            split_diff: false,
            hide_sidebar: false,
            clone_dir: None,
            theme: ThemeChoice::default(),
            zoom: None,
            no_update_checks: false,
            file_tree: false,
            defaults: DEFAULTS,
        }
    }
}

impl Settings {
    pub const KEY: &'static str = "settings";

    /// Brings settings saved by an older version up to date. Before 0.5.2
    /// the default look was System, so a saved System then was most
    /// likely never chosen: it becomes Raccoon, the default now.
    #[must_use]
    pub fn upgrade(mut self) -> Self {
        if self.defaults < 1 && self.theme == ThemeChoice::System {
            self.theme = ThemeChoice::Raccoon;
        }
        self.defaults = DEFAULTS;
        self
    }

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
    fn older_settings_move_to_the_raccoon_look_once() {
        // Saved by 0.5.1 (as eframe saves it, in RON), with the old default.
        let old: Settings = ron::from_str("(theme: System, split_diff: true)").unwrap();
        assert_eq!(old.defaults, 0);
        let new = old.upgrade();
        assert_eq!(new.theme, ThemeChoice::Raccoon);
        assert!(new.split_diff, "the rest is kept");
        // A look chosen before is kept.
        let chose: Settings = ron::from_str("(theme: Light)").unwrap();
        assert_eq!(chose.upgrade().theme, ThemeChoice::Light);
        // System chosen after the change is kept too.
        let now = Settings {
            theme: ThemeChoice::System,
            ..Settings::default()
        };
        let saved: Settings = ron::from_str(&ron::to_string(&now).unwrap()).unwrap();
        assert_eq!(saved.upgrade().theme, ThemeChoice::System);
    }

    #[test]
    fn names_repos_by_last_path_component() {
        assert_eq!(repo_name("/home/me/git-raccoon/"), "git-raccoon");
        assert_eq!(repo_name(r"F:\Projects\Rust\Git-Raccoon"), "Git-Raccoon");
        assert_eq!(repo_name("solo"), "solo");
    }
}
