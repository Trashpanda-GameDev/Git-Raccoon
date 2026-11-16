//! Everything the app can do from the keyboard: the commands, their
//! shortcuts, and matching what's typed in the command palette.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

/// Something the app can do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Palette,
    Settings,
    OpenRepo,
    CloneRepo,
    NewRepo,
    CloseRepo,
    NewTab,
    NextTab,
    PreviousTab,
    Refresh,
    Find,
    ToggleSidebar,
    ToggleSplitDiff,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Undo,
    Fetch,
    Pull,
    Push,
    ShowWorking,
    WriteCommit,
    StageAll,
    UnstageAll,
    Stash,
    PopStash,
    NewBranch,
    NewTag,
    AddRemote,
    Continue,
    Abort,
    /// The open file's history or blame.
    FileHistory,
    Blame,
    /// For one branch.
    Checkout(String),
    CheckoutRemote(String),
    Merge(String),
    Rebase(String),
    /// Select the commit a branch or tag points at.
    GoTo {
        name: String,
        id: String,
    },
}

const CTRL: Modifiers = Modifiers::COMMAND;
const CTRL_SHIFT: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::COMMAND
};

/// The fixed commands with their keyboard shortcuts (if any). Shortcuts
/// with Shift also work while typing in a text field.
pub const SHORTCUTS: &[(Command, Option<KeyboardShortcut>)] = &[
    (Command::Palette, Some(KeyboardShortcut::new(CTRL, Key::K))),
    (
        Command::Palette,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::P)),
    ),
    (
        Command::Settings,
        Some(KeyboardShortcut::new(CTRL, Key::Comma)),
    ),
    (Command::OpenRepo, Some(KeyboardShortcut::new(CTRL, Key::O))),
    (Command::CloneRepo, None),
    (Command::NewRepo, None),
    (
        Command::CloseRepo,
        Some(KeyboardShortcut::new(CTRL, Key::W)),
    ),
    (Command::NewTab, Some(KeyboardShortcut::new(CTRL, Key::T))),
    (
        Command::PreviousTab,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::Tab)),
    ),
    (
        Command::NextTab,
        Some(KeyboardShortcut::new(CTRL, Key::Tab)),
    ),
    (
        Command::Refresh,
        Some(KeyboardShortcut::new(Modifiers::NONE, Key::F5)),
    ),
    (Command::Find, Some(KeyboardShortcut::new(CTRL, Key::F))),
    (
        Command::ToggleSidebar,
        Some(KeyboardShortcut::new(CTRL, Key::B)),
    ),
    (Command::ToggleSplitDiff, None),
    (Command::Undo, Some(KeyboardShortcut::new(CTRL, Key::Z))),
    (
        Command::ZoomIn,
        Some(KeyboardShortcut::new(CTRL, Key::Plus)),
    ),
    (
        Command::ZoomIn,
        Some(KeyboardShortcut::new(CTRL, Key::Equals)),
    ),
    (
        Command::ZoomOut,
        Some(KeyboardShortcut::new(CTRL, Key::Minus)),
    ),
    (
        Command::ZoomReset,
        Some(KeyboardShortcut::new(CTRL, Key::Num0)),
    ),
    (
        Command::Fetch,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::F)),
    ),
    (
        Command::Pull,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::D)),
    ),
    (
        Command::Push,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::U)),
    ),
    (
        Command::ShowWorking,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::W)),
    ),
    (
        Command::WriteCommit,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::M)),
    ),
    (
        Command::StageAll,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::S)),
    ),
    (
        Command::UnstageAll,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::A)),
    ),
    (
        Command::Stash,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::H)),
    ),
    (Command::PopStash, None),
    (
        Command::NewBranch,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::B)),
    ),
    (
        Command::NewTag,
        Some(KeyboardShortcut::new(CTRL_SHIFT, Key::T)),
    ),
    (Command::AddRemote, None),
    (Command::FileHistory, None),
    (Command::Blame, None),
    (Command::Continue, None),
    (Command::Abort, None),
];

/// The first shortcut for a command.
pub fn shortcut(command: &Command) -> Option<KeyboardShortcut> {
    SHORTCUTS
        .iter()
        .find(|(c, s)| c == command && s.is_some())
        .and_then(|(_, s)| *s)
}

/// One line of the palette.
#[derive(Debug, Clone)]
pub struct Entry {
    pub command: Command,
    pub title: String,
    pub enabled: bool,
}

/// How well `query` matches `text`: its characters must appear in order
/// (ignoring case). Higher is better: runs of characters, and matches at
/// the start of words, count more. `None` if it doesn't match.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let query: Vec<char> = query
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if query.is_empty() {
        return Some(0);
    }
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let mut score = 0;
    let mut qi = 0;
    let mut last: Option<usize> = None;
    for (ti, &c) in text.iter().enumerate() {
        if qi < query.len() && c == query[qi] {
            score += 1;
            if last == Some(ti.wrapping_sub(1)) {
                score += 3; // a run
            }
            if ti == 0 || !text[ti - 1].is_alphanumeric() {
                score += 4; // start of a word
            }
            last = Some(ti);
            qi += 1;
        }
    }
    (qi == query.len()).then_some(score - i32::try_from(text.len() / 10).unwrap_or(0))
}

/// The entries matching `query`, best first (ties keep their order).
pub fn filter<'a>(entries: &'a [Entry], query: &str) -> Vec<&'a Entry> {
    let mut scored: Vec<(i32, usize, &Entry)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| score(query, &e.title).map(|s| (s, i, e)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, e)| e).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str) -> Entry {
        Entry {
            command: Command::Refresh,
            title: title.into(),
            enabled: true,
        }
    }

    #[test]
    fn matches_in_order_ignoring_case() {
        assert!(score("pll", "Pull").is_some());
        assert!(score("PUSH", "push").is_some());
        assert!(score("hsup", "Push").is_none());
        assert_eq!(score("", "anything"), Some(0));
    }

    #[test]
    fn prefers_word_starts_and_runs() {
        let entries = [
            entry("Undo last action"),
            entry("Check out feature"),
            entry("Fetch from all remotes"),
            entry("Create branch…"),
        ];
        let titles = |q: &str| -> Vec<String> {
            filter(&entries, q)
                .iter()
                .map(|e| e.title.clone())
                .collect()
        };
        assert_eq!(titles("fetch")[0], "Fetch from all remotes");
        assert_eq!(titles("co feat")[0], "Check out feature");
        assert_eq!(titles("cb")[0], "Create branch…");
        assert!(titles("zzz").is_empty());
        assert_eq!(titles("").len(), 4, "nothing typed: everything, in order");
    }

    #[test]
    fn every_shortcut_is_used_once() {
        let mut seen: Vec<KeyboardShortcut> = Vec::new();
        for (_, s) in SHORTCUTS {
            if let Some(s) = s {
                assert!(!seen.contains(s), "{s:?} is used twice");
                seen.push(*s);
            }
        }
    }
}
