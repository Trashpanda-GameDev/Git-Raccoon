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

/// How the graph's lines bend between lanes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineStyle {
    /// A smooth S-curve between rows.
    #[default]
    Curved,
    /// Along a commit's row, then into the other lane with a rounded
    /// corner: a merge leaves its commit sideways, a branch comes into
    /// its parent sideways.
    Rounded,
}

impl LineStyle {
    pub const ALL: [LineStyle; 2] = [LineStyle::Curved, LineStyle::Rounded];

    pub fn name(self) -> &'static str {
        match self {
            LineStyle::Curved => "Curved",
            LineStyle::Rounded => "Rounded",
        }
    }
}

/// A column of the graph's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Column {
    Refs,
    Graph,
    Message,
    Author,
    Date,
    Sha,
}

impl Column {
    /// Every column, in the usual order.
    pub const ALL: [Column; 6] = [
        Column::Refs,
        Column::Graph,
        Column::Message,
        Column::Author,
        Column::Date,
        Column::Sha,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Column::Refs => "Branch / Tag",
            Column::Graph => "Graph",
            Column::Message => "Commit message",
            Column::Author => "Author",
            Column::Date => "Date",
            Column::Sha => "SHA",
        }
    }

    /// `order` with each column once: unknown repeats dropped, missing
    /// ones added at the end (so a damaged setting can't lose one).
    pub fn sanitise(order: &[Column]) -> Vec<Column> {
        let mut out: Vec<Column> = Vec::new();
        for c in order.iter().chain(Column::ALL.iter()) {
            if !out.contains(c) {
                out.push(*c);
            }
        }
        out
    }
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
    /// How Fetch goes: pruning remote branches deleted there, and letting
    /// fetched refs overwrite local ones.
    pub fetch_prune: bool,
    pub fetch_force: bool,
    /// How Pull goes.
    pub pull: PullOptions,
    pub hide_sidebar: bool,
    /// Where the last clone went; new clones start there.
    pub clone_dir: Option<String>,
    pub theme: ThemeChoice,
    /// Dark, light, or following the system.
    pub theme_mode: ThemeMode,
    /// How big everything is drawn; `None` for normal (100%).
    pub zoom: Option<f32>,
    /// Don't look for updates.
    pub no_update_checks: bool,
    /// The version last run, to show what's new after an update.
    pub last_version: Option<String>,
    /// Signed in to these forges (their tokens are in the keychain).
    pub accounts: Vec<crate::forge::Account>,
    /// Changed files shown as a tree of folders, rather than by path.
    pub file_tree: bool,
    /// How the graph's lines bend where branches fork and merge.
    pub branch_lines: LineStyle,
    /// Themes made here (or imported).
    pub themes: Vec<crate::themes::CustomTheme>,
    /// The one in use, by name; `None` for the built-in look in `theme`.
    pub custom_theme: Option<String>,
    /// The graph's columns, in the order they're shown.
    pub columns: Vec<Column>,
    /// Widths given to the graph's columns by dragging their edges (the
    /// commit message takes the room that's left). Unlisted: the default.
    pub column_widths: Vec<(Column, f32)>,
    /// The command that opens a terminal ("{path}" is the folder); empty
    /// for the system's own.
    pub terminal: String,
    /// Which defaults these settings were saved with (see `upgrade`):
    /// 0 when missing, from a version before there was this field.
    #[serde(default)]
    pub defaults: u32,
}

/// The current defaults: 1 since Raccoon became the default look, 2 since
/// every look comes dark and light, 3 since the version run is noted.
const DEFAULTS: u32 = 3;

/// The look: which family of colours (each comes dark and light; see
/// [`ThemeMode`]).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    /// From settings before 0.5.3 only, where it meant Classic following
    /// the system ([`Settings::upgrade`] turns it into that).
    System,
    /// From settings before 0.5.3 only: Classic, dark.
    Dark,
    /// From settings before 0.5.3 only: Classic, light.
    Light,
    /// Git Raccoon's own look, and the default.
    #[default]
    Raccoon,
    Pastel,
    /// From settings before 0.5.3 only: a look since removed (it was too
    /// like Classic), which [`Settings::upgrade`] turns into Classic.
    Abyss,
    Forest,
    Paper,
    HighContrast,
    /// The original look: slate and teal.
    Classic,
}

impl ThemeChoice {
    /// The choices offered, in order.
    pub const ALL: [ThemeChoice; 6] = [
        ThemeChoice::Raccoon,
        ThemeChoice::Classic,
        ThemeChoice::Pastel,
        ThemeChoice::Forest,
        ThemeChoice::Paper,
        ThemeChoice::HighContrast,
    ];

    pub fn family(self) -> crate::theme::Family {
        use crate::theme::Family;
        match self {
            ThemeChoice::System
            | ThemeChoice::Dark
            | ThemeChoice::Light
            | ThemeChoice::Abyss
            | ThemeChoice::Classic => Family::Classic,
            ThemeChoice::Raccoon => Family::Raccoon,
            ThemeChoice::Pastel => Family::Pastel,
            ThemeChoice::Forest => Family::Forest,
            ThemeChoice::Paper => Family::Paper,
            ThemeChoice::HighContrast => Family::HighContrast,
        }
    }
}

/// Whether the look is shown dark, light, or as the system is.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
}

/// How Pull goes: how it joins the upstream's new commits with yours, and
/// what it does with uncommitted changes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PullOptions {
    pub strategy: PullStrategy,
    pub changes: PullChanges,
}

/// How Pull joins the upstream's new commits with yours.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PullStrategy {
    /// Only a fast-forward; when both sides have new commits, it asks
    /// whether to merge or rebase.
    #[default]
    FastForwardOnly,
    /// A fast-forward if it can be, otherwise a merge commit.
    FastForwardOrMerge,
    /// Your commits replayed on top of the upstream's.
    Rebase,
}

impl PullStrategy {
    pub const ALL: [PullStrategy; 3] = [
        PullStrategy::FastForwardOnly,
        PullStrategy::FastForwardOrMerge,
        PullStrategy::Rebase,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PullStrategy::FastForwardOnly => "Fast-forward only",
            PullStrategy::FastForwardOrMerge => "Fast-forward if possible, else merge",
            PullStrategy::Rebase => "Rebase",
        }
    }

    pub fn mode(self) -> git_raccoon_core::sync::PullMode {
        use git_raccoon_core::sync::PullMode;
        match self {
            PullStrategy::FastForwardOnly => PullMode::FastForward,
            PullStrategy::FastForwardOrMerge => PullMode::Merge,
            PullStrategy::Rebase => PullMode::Rebase,
        }
    }
}

/// What Pull does with uncommitted changes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PullChanges {
    /// Put aside before, and back after.
    #[default]
    Stash,
    /// Left as they are; Git refuses if they're in the way.
    Leave,
    /// Thrown away first (to the trash), after asking.
    Discard,
}

impl PullChanges {
    pub const ALL: [PullChanges; 3] =
        [PullChanges::Stash, PullChanges::Leave, PullChanges::Discard];

    pub fn label(self) -> &'static str {
        match self {
            PullChanges::Stash => "Put aside and back",
            PullChanges::Leave => "Leave them",
            PullChanges::Discard => "Discard them (asks first)",
        }
    }
}

/// The interface sizes offered.
pub const ZOOMS: [f32; 5] = [0.9, 1.0, 1.1, 1.25, 1.5];

/// The sizes Ctrl+plus and Ctrl+minus step through.
const ZOOM_STEPS: [f32; 11] = [0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

impl Default for Settings {
    fn default() -> Self {
        Settings {
            recent: Vec::new(),
            last_repo: None,
            open_tabs: Vec::new(),
            active_tab: 0,
            split_diff: false,
            fetch_prune: true,
            fetch_force: false,
            pull: PullOptions::default(),
            hide_sidebar: false,
            clone_dir: None,
            theme: ThemeChoice::default(),
            theme_mode: ThemeMode::default(),
            zoom: None,
            no_update_checks: false,
            last_version: None,
            accounts: Vec::new(),
            file_tree: false,
            terminal: String::new(),
            columns: Column::ALL.to_vec(),
            themes: Vec::new(),
            branch_lines: LineStyle::default(),
            custom_theme: None,
            column_widths: Vec::new(),
            defaults: DEFAULTS,
        }
    }
}

impl Settings {
    pub const KEY: &'static str = "settings";

    /// How Fetch goes.
    pub fn fetch_options(&self) -> git_raccoon_core::sync::FetchOptions {
        git_raccoon_core::sync::FetchOptions {
            prune: self.fetch_prune,
            force: self.fetch_force,
        }
    }

    /// How big everything is drawn (1 is normal).
    pub fn zoom(&self) -> f32 {
        self.zoom.unwrap_or(1.0)
    }

    /// Sets the interface size, kept between the smallest and largest steps
    /// and rounded to a whole percent.
    /// The custom theme in use, if one is (and still exists).
    pub fn current_theme(&self) -> Option<&crate::themes::CustomTheme> {
        let name = self.custom_theme.as_deref()?;
        self.themes.iter().find(|t| t.name == name)
    }

    pub fn set_zoom(&mut self, zoom: f32) {
        let zoom =
            (zoom.clamp(ZOOM_STEPS[0], ZOOM_STEPS[ZOOM_STEPS.len() - 1]) * 100.0).round() / 100.0;
        self.zoom = ((zoom - 1.0).abs() > 0.001).then_some(zoom);
    }

    /// One size bigger (Ctrl+plus) or smaller (Ctrl+minus).
    pub fn zoom_step(&mut self, bigger: bool) {
        let now = self.zoom();
        let next = if bigger {
            ZOOM_STEPS.iter().find(|z| **z > now + 0.001)
        } else {
            ZOOM_STEPS.iter().rev().find(|z| **z < now - 0.001)
        };
        if let Some(z) = next {
            self.set_zoom(*z);
        }
    }

    /// Brings settings saved by an older version up to date. Before 0.5.2
    /// the default look was System, so a saved System then was most
    /// likely never chosen: it becomes Raccoon, the default now.
    #[must_use]
    pub fn upgrade(mut self) -> Self {
        if self.defaults < 1 && self.theme == ThemeChoice::System {
            self.theme = ThemeChoice::Raccoon;
        }
        // Before 0.5.3 each look was dark or light, and System, Dark and
        // Light were the original look: now a look and a mode.
        if self.defaults < 2 {
            let (theme, mode) = match self.theme {
                ThemeChoice::System => (ThemeChoice::Classic, ThemeMode::System),
                ThemeChoice::Dark => (ThemeChoice::Classic, ThemeMode::Dark),
                ThemeChoice::Light => (ThemeChoice::Classic, ThemeMode::Light),
                light @ (ThemeChoice::Pastel | ThemeChoice::Paper) => (light, ThemeMode::Light),
                other => (other, ThemeMode::Dark),
            };
            self.theme = theme;
            self.theme_mode = mode;
        }
        // Saved before the version run was noted (it came with 0.5.3): from
        // 0.5.2 or earlier, so what's new since then is shown.
        if self.defaults < 3 && self.last_version.is_none() {
            self.last_version = Some("0.5.2".to_string());
        }
        // Abyss is gone: Classic is the nearest, in the same mode.
        if self.theme == ThemeChoice::Abyss {
            self.theme = ThemeChoice::Classic;
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

    #[test]
    fn a_column_order_always_has_every_column_once() {
        use Column::{Author, Date, Graph, Message, Refs, Sha};
        assert_eq!(Column::sanitise(&[]), Column::ALL);
        assert_eq!(
            Column::sanitise(&[Sha, Graph, Sha]),
            [Sha, Graph, Refs, Message, Author, Date]
        );
        let saved: Settings = serde_json::from_str(r#"{"columns": ["Date", "Refs"]}"#).unwrap();
        assert_eq!(
            Column::sanitise(&saved.columns),
            [Date, Refs, Graph, Message, Author, Sha]
        );
        // Widths come back as saved; settings from before them have none.
        assert!(saved.column_widths.is_empty());
        let widths = Settings {
            column_widths: vec![(Refs, 260.0), (Author, 96.5)],
            ..Settings::default()
        };
        let back: Settings =
            serde_json::from_str(&serde_json::to_string(&widths).unwrap()).unwrap();
        assert_eq!(back.column_widths, widths.column_widths);
    }

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
    fn older_settings_move_to_a_look_and_a_mode_once() {
        // Saved by 0.5.1 (as eframe saves it, in RON), with the old default.
        let old: Settings = ron::from_str("(theme: System, split_diff: true)").unwrap();
        assert_eq!(old.defaults, 0);
        let new = old.upgrade();
        assert_eq!(
            (new.theme, new.theme_mode),
            (ThemeChoice::Raccoon, ThemeMode::Dark)
        );
        assert!(new.split_diff, "the rest is kept");
        // Looks chosen before: the original light one, and a light look.
        let chose: Settings = ron::from_str("(theme: Light)").unwrap();
        let chose = chose.upgrade();
        assert_eq!(
            (chose.theme, chose.theme_mode),
            (ThemeChoice::Classic, ThemeMode::Light)
        );
        let paper: Settings = ron::from_str("(theme: Paper, defaults: 1)").unwrap();
        assert_eq!(paper.upgrade().theme_mode, ThemeMode::Light);
        // System chosen in 0.5.2: the original look, following the system.
        let system: Settings = ron::from_str("(theme: System, defaults: 1)").unwrap();
        let system = system.upgrade();
        assert_eq!(
            (system.theme, system.theme_mode),
            (ThemeChoice::Classic, ThemeMode::System)
        );
        // Saved now: kept as it is.
        let now = Settings {
            theme: ThemeChoice::Forest,
            theme_mode: ThemeMode::System,
            ..Settings::default()
        };
        let saved: Settings = ron::from_str(&ron::to_string(&now).unwrap()).unwrap();
        let saved = saved.upgrade();
        assert_eq!(
            (saved.theme, saved.theme_mode),
            (ThemeChoice::Forest, ThemeMode::System)
        );
        // From before the version run was noted: as if from 0.5.2. Noted
        // since: kept.
        assert_eq!(new.last_version.as_deref(), Some("0.5.2"));
        let noted: Settings =
            ron::from_str("(last_version: Some(\"0.5.3\"), defaults: 2)").unwrap();
        assert_eq!(noted.upgrade().last_version.as_deref(), Some("0.5.3"));
        assert_eq!(Settings::default().last_version, None, "a fresh install");
        // Abyss, removed since: Classic, keeping its mode.
        let abyss: Settings =
            ron::from_str("(theme: Abyss, theme_mode: Light, defaults: 2)").unwrap();
        let abyss = abyss.upgrade();
        assert_eq!(
            (abyss.theme, abyss.theme_mode),
            (ThemeChoice::Classic, ThemeMode::Light)
        );
    }

    #[test]
    fn zooms_in_steps_and_by_scrolling() {
        let mut s = Settings::default();
        s.zoom_step(true);
        assert_eq!(s.zoom, Some(1.1));
        s.zoom_step(false);
        s.zoom_step(false);
        assert_eq!(s.zoom, Some(0.9));
        // From between steps, to the next one.
        s.set_zoom(1.17);
        s.zoom_step(true);
        assert_eq!(s.zoom, Some(1.25));
        // Scrolling multiplies; there are limits; normal is None.
        s.set_zoom(s.zoom() * 10.0);
        assert_eq!(s.zoom, Some(2.0));
        s.zoom_step(true);
        assert_eq!(s.zoom, Some(2.0));
        s.set_zoom(1.0);
        assert_eq!(s.zoom, None);
    }

    #[test]
    fn names_repos_by_last_path_component() {
        assert_eq!(repo_name("/home/me/git-raccoon/"), "git-raccoon");
        assert_eq!(repo_name(r"F:\Projects\Rust\Git-Raccoon"), "Git-Raccoon");
        assert_eq!(repo_name("solo"), "solo");
    }
}
