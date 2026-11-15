//! Changed files, shown either by their paths or as a tree of folders.

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

/// The folders above a file, outermost first.
fn folders(path: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = path.split('/').collect();
    parts.pop();
    parts
}

/// How many rows `paths` take: one each, plus the folders in a tree.
pub fn rows<'a>(paths: impl Iterator<Item = &'a str>, tree: bool) -> usize {
    let mut paths: Vec<&str> = paths.collect();
    if !tree {
        return paths.len();
    }
    paths.sort_unstable();
    let mut open: Vec<&str> = Vec::new();
    let mut count = 0;
    for path in paths {
        let dirs = folders(path);
        let same = open.iter().zip(&dirs).take_while(|(a, b)| a == b).count();
        open.truncate(same);
        for d in &dirs[same..] {
            open.push(d);
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
    // The folders the last file was in, and the depth of a folded one
    // among them (whose contents are hidden).
    let mut open: Vec<&str> = Vec::new();
    let mut folded_at: Option<usize> = None;
    for item in sorted {
        let dirs = folders(path(item));
        let same = open.iter().zip(&dirs).take_while(|(a, b)| a == b).count();
        open.truncate(same);
        if folded_at.is_some_and(|d| d >= same) {
            folded_at = None;
        }
        for (depth, name) in dirs.iter().enumerate().skip(same) {
            open.push(name);
            if folded_at.is_some() {
                continue;
            }
            let full = dirs[..=depth].join("/");
            let key = id.with(&full);
            let folded = ui.data(|d| d.get_temp::<bool>(key).unwrap_or(false));
            let response = folder_row(ui, name, &full, depth, folded);
            if response.clicked() {
                ui.data_mut(|d| d.insert_temp(key, !folded));
            }
            let prefix = format!("{full}/");
            let inside = items
                .iter()
                .filter(|i| path(i).starts_with(&prefix))
                .collect();
            folder(&response, &full, inside);
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
}
