//! Changed files, shown either by their paths or as a tree of folders.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use eframe::egui::{self, Align2, Pos2, Sense};
use egui_phosphor::regular as icon;

use crate::theme;

/// How far each folder level is indented.
const INDENT: f32 = 14.0;
pub const FOLDER_ROW: f32 = 24.0;

/// How a file's row is drawn: indented under its folder, and with just its
/// name when its folders are shown above it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shown {
    pub indent: f32,
    pub name_only: bool,
}

impl Shown {
    /// A row with the file's whole path.
    pub const PATH: Shown = Shown {
        indent: 0.0,
        name_only: false,
    };
}

/// The folders shown above files. A folder with no files of its own and
/// just one folder inside shares a row with it ("game/assets/localisation"),
/// so a long path of single folders doesn't take a row each.
struct Folders {
    /// Folders that share a row with the one folder inside them.
    merged: HashSet<String>,
}

impl Folders {
    fn new<'a>(paths: impl Iterator<Item = &'a str>) -> Folders {
        // Each folder's folders and files directly inside it.
        let mut folders: HashMap<String, HashSet<&str>> = HashMap::new();
        let mut has_files: HashSet<String> = HashSet::new();
        for path in paths {
            let parts: Vec<&str> = path.split('/').collect();
            let dirs = &parts[..parts.len() - 1];
            for depth in 0..dirs.len() {
                let full = dirs[..=depth].join("/");
                let entry = folders.entry(full).or_default();
                if let Some(next) = dirs.get(depth + 1) {
                    entry.insert(next);
                }
            }
            if !dirs.is_empty() {
                has_files.insert(dirs.join("/"));
            }
        }
        let merged = folders
            .into_iter()
            .filter(|(full, inside)| inside.len() == 1 && !has_files.contains(full))
            .map(|(full, _)| full)
            .collect();
        Folders { merged }
    }

    /// The folder rows above a file, outermost first: each one's whole
    /// path, and the name shown.
    fn above(&self, path: &str) -> Vec<(String, String)> {
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop();
        let mut rows = Vec::new();
        let mut start = 0;
        for depth in 0..parts.len() {
            let full = parts[..=depth].join("/");
            if !self.merged.contains(&full) || depth + 1 == parts.len() {
                rows.push((full, parts[start..=depth].join("/")));
                start = depth + 1;
            }
        }
        rows
    }
}

/// How many rows `paths` take: one each, plus the folders in a tree.
pub fn rows<'a>(paths: impl Iterator<Item = &'a str>, tree: bool) -> usize {
    let mut paths: Vec<&str> = paths.collect();
    if !tree {
        return paths.len();
    }
    paths.sort_unstable();
    let shown = Folders::new(paths.iter().copied());
    let mut open: Vec<String> = Vec::new();
    let mut count = 0;
    for path in paths {
        let dirs = shown.above(path);
        let same = open
            .iter()
            .zip(&dirs)
            .take_while(|(a, (b, _))| *a == b)
            .count();
        open.truncate(same);
        for (full, _) in &dirs[same..] {
            open.push(full.clone());
            count += 1;
        }
        count += 1;
    }
    count
}

/// The order `items` are shown in: as given, or by path in a tree.
pub fn order<T>(items: &[T], path: impl Fn(&T) -> &str, tree: bool) -> Vec<&T> {
    let mut sorted: Vec<&T> = items.iter().collect();
    if tree {
        sorted.sort_by(|a, b| path(a).cmp(path(b)));
    }
    sorted
}

/// Draws `items` as paths, or as folders (which fold when clicked) with
/// the files inside them; `row` draws each file shown, and `folder` is
/// given each folder's row (its path, and the items inside it). `salt`
/// keeps different lists' folded folders apart.
pub fn show<T>(
    ui: &mut egui::Ui,
    salt: impl Hash + std::fmt::Debug,
    items: &[T],
    path: impl Fn(&T) -> &str,
    tree: bool,
    mut row: impl FnMut(&mut egui::Ui, &T, Shown),
    mut folder: impl FnMut(&egui::Response, &str, Vec<&T>),
) {
    if !tree {
        for item in items {
            row(ui, item, Shown::PATH);
        }
        return;
    }
    let id = egui::Id::new(&salt);
    let sorted = order(items, &path, true);
    let shown = Folders::new(sorted.iter().map(|i| path(i)));
    // The folders the last file was in, and the depth of a folded one
    // among them (whose contents are hidden).
    let mut open: Vec<String> = Vec::new();
    let mut folded_at: Option<usize> = None;
    for item in sorted {
        let dirs = shown.above(path(item));
        let same = open
            .iter()
            .zip(&dirs)
            .take_while(|(a, (b, _))| *a == b)
            .count();
        open.truncate(same);
        if folded_at.is_some_and(|d| d >= same) {
            folded_at = None;
        }
        for (depth, (full, name)) in dirs.iter().enumerate().skip(same) {
            open.push(full.clone());
            if folded_at.is_some() {
                continue;
            }
            let key = id.with(full);
            let folded = ui.data(|d| d.get_temp::<bool>(key).unwrap_or(false));
            let response = folder_row(ui, name, full, depth, folded);
            if response.clicked() {
                ui.data_mut(|d| d.insert_temp(key, !folded));
            }
            let prefix = format!("{full}/");
            let inside = items
                .iter()
                .filter(|i| path(i).starts_with(&prefix))
                .collect();
            folder(&response, full, inside);
            if folded {
                folded_at = Some(depth);
            }
        }
        if folded_at.is_none() {
            #[expect(clippy::cast_precision_loss, reason = "folder depth is small")]
            let indent = dirs.len() as f32 * INDENT;
            row(
                ui,
                item,
                Shown {
                    indent,
                    name_only: true,
                },
            );
        }
    }
}

fn folder_row(
    ui: &mut egui::Ui,
    name: &str,
    full: &str,
    depth: usize,
    folded: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), FOLDER_ROW), Sense::click());
    let label = format!("{full}/");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 0, theme::bg_hover());
    }
    #[expect(clippy::cast_precision_loss, reason = "folder depth is small")]
    let x = rect.left() + depth as f32 * INDENT;
    let caret = if folded {
        icon::CARET_RIGHT
    } else {
        icon::CARET_DOWN
    };
    let y = rect.center().y;
    painter.text(
        Pos2::new(x + 6.0, y),
        Align2::CENTER_CENTER,
        caret,
        theme::body(11.0),
        theme::muted(),
    );
    painter.text(
        Pos2::new(x + 20.0, y),
        Align2::CENTER_CENTER,
        if folded {
            icon::FOLDER_SIMPLE
        } else {
            icon::FOLDER_OPEN
        },
        theme::body(13.0),
        theme::muted(),
    );
    painter.text(
        Pos2::new(x + 31.0, y),
        Align2::LEFT_CENTER,
        name,
        theme::body(13.0),
        theme::muted(),
    );
    response.on_hover_text(full)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_rows_with_their_folders() {
        let paths = [
            "src/a.rs",
            "src/ui/b.rs",
            "src/ui/c.rs",
            "README.md",
            "docs/x.md",
        ];
        assert_eq!(rows(paths.into_iter(), false), 5);
        // README.md, docs/, x.md, src/, a.rs, ui/, b.rs, c.rs
        assert_eq!(rows(paths.into_iter(), true), 8);
    }

    #[test]
    fn single_folders_share_a_row() {
        let paths = [
            "game/assets/localisation/en.csv",
            "game/assets/localisation/fr.csv",
            "game/src/deep/er/main.rs",
            "game/src/lib.rs",
            "tools/x/y.py",
        ];
        let shown = Folders::new(paths.into_iter());
        let names = |path: &str| -> Vec<String> {
            shown
                .above(path)
                .into_iter()
                .map(|(_, name)| name)
                .collect()
        };
        // game/ holds two folders, so it stays; assets/ holds just one.
        assert_eq!(names(paths[0]), ["game", "assets/localisation"]);
        // src/ has a file of its own, so it stays.
        assert_eq!(names(paths[2]), ["game", "src", "deep/er"]);
        assert_eq!(names(paths[4]), ["tools/x"]);
        assert_eq!(
            shown.above(paths[2]).last().map(|(full, _)| full.as_str()),
            Some("game/src/deep/er")
        );
        // game/, assets/localisation/, 2 files, src/, deep/er/, main.rs,
        // lib.rs, tools/x/, y.py
        assert_eq!(rows(paths.into_iter(), true), 10);
        let issue = [
            "game/assets/localisation/a.txt",
            "game/assets/localisation/b.txt",
        ];
        assert_eq!(
            Folders::new(issue.into_iter()).above(issue[0])[0].1,
            "game/assets/localisation"
        );
        assert_eq!(rows(issue.into_iter(), true), 3);
    }
}
