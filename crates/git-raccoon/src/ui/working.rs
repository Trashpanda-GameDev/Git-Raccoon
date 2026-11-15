//! The right-hand pane when the working changes are selected: the staged
//! and unstaged files, and the commit message box.

use eframe::egui::{self, RichText, Stroke};
use egui_phosphor::regular as icon;
use git_raccoon_core::details::ChangedFile;
use git_raccoon_core::working::{Side, WorkingFile, WorkingStatus};

use crate::repo_view::CommitDraft;
use crate::theme;
use crate::ui::file_list;
use crate::ui::{details, widgets};

/// Summary lengths past these get an amber, then red, counter.
const SUMMARY_SOFT: usize = 50;
const SUMMARY_HARD: usize = 72;

/// The commit summary field, to focus it from a shortcut.
pub fn summary_id() -> egui::Id {
    egui::Id::new("commit-summary")
}

pub enum Action {
    Open(Side, String),
    /// Open a conflicted file for resolving.
    OpenConflict(String),
    Stage(String),
    Unstage(String),
    StageAll,
    UnstageAll,
    SetAmend(bool),
    Commit,
    /// Discard unstaged changes (after asking).
    Discard(Vec<String>),
    /// Open the stash dialog.
    Stash,
    /// Discard every change, staged or not (after asking).
    DiscardAll,
    /// Switch files between paths and folders.
    ToggleTree,
    Close,
}

pub struct State<'a> {
    pub status: Option<&'a WorkingStatus>,
    pub loading: bool,
    /// A staging operation is running: file buttons are disabled.
    pub staging: bool,
    pub committing: bool,
    pub active: Option<(Side, &'a str)>,
    /// The conflicted file open for resolving.
    pub active_conflict: Option<&'a str>,
    pub draft: &'a mut CommitDraft,
    /// Files shown as a tree of folders.
    pub tree: bool,
}

pub fn show(ui: &mut egui::Ui, st: State<'_>) -> Option<Action> {
    let mut action = None;
    let State {
        status,
        loading,
        staging,
        committing,
        active,
        active_conflict,
        draft,
        tree,
    } = st;
    ui.horizontal(|ui| {
        ui.set_height(36.0);
        ui.label(widgets::section_label("Working changes"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icon::X, "Close (Esc)").clicked() {
                action = Some(Action::Close);
            }
            let has_changes = status.is_some_and(|s| !s.is_clean());
            let stash =
                egui::Button::new(RichText::new(format!("{}  Stash", icon::STACK)).size(12.0));
            let stash = ui.add_enabled(has_changes && !staging && !committing, stash);
            ui.ctx()
                .accesskit_node_builder(stash.id, |n| n.set_label("Stash changes"));
            if stash
                .on_hover_text("Put uncommitted changes aside in a stash: all of them, or just the staged ones")
                .clicked()
            {
                action = Some(Action::Stash);
            }
            let conflicts = status.is_some_and(|s| !s.conflicted.is_empty());
            if widgets::named_icon_button(
                ui,
                icon::TRASH,
                "Discard all changes",
                has_changes && !conflicts && !staging && !committing,
            )
            .clicked()
            {
                action = Some(Action::DiscardAll);
            }
            if details::tree_toggle(ui, tree) {
                action = Some(Action::ToggleTree);
            }
            if loading || staging || committing {
                ui.spinner();
            }
        });
    });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );

    let Some(status) = status else {
        ui.add_space(8.0);
        ui.label(RichText::new("Loading…").color(theme::muted()));
        return action;
    };

    // The commit box sits at the bottom; the file lists fill the rest.
    let staged = status.staged.len();
    egui::Panel::bottom("commit-box")
        .frame(
            egui::Frame::new()
                .fill(theme::bg_raised())
                .inner_margin(egui::Margin::symmetric(0, 10)),
        )
        .show(ui, |ui| {
            if let Some(a) = commit_box(ui, draft, staged, committing) {
                action = Some(a);
            }
        });

    ui.add_space(8.0);
    if status.is_clean() {
        ui.label(RichText::new("No uncommitted changes.").color(theme::muted()));
        return action;
    }
    // Conflicts first, scrolling within a third of the height at most.
    egui::ScrollArea::vertical()
        .id_salt("conflicted")
        .max_height(ui.available_height() / 3.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            if !status.conflicted.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(widgets::section_label("Conflicted files").color(theme::danger()));
                    ui.label(
                        RichText::new(status.conflicted.len().to_string())
                            .size(11.0)
                            .color(theme::muted()),
                    );
                });
                ui.spacing_mut().item_spacing.y = 0.0;
                for f in &status.conflicted {
                    let row = details::file_row(
                        ui,
                        &as_changed(f),
                        active_conflict == Some(f.path.as_str()),
                    );
                    if row.clicked() {
                        action = Some(Action::OpenConflict(f.path.clone()));
                    }
                }
                ui.add_space(14.0);
                ui.spacing_mut().item_spacing.y = 6.0;
            }
        });
    // Unstaged and staged files share the rest, each list scrolling on its
    // own, so staged files are in view however many unstaged ones there are.
    // A folded list gives its room to the other.
    let folded_id = |side: Side| egui::Id::new(("list folded", side == Side::Staged));
    let folded = |side: Side| ui.data(|d| d.get_temp::<bool>(folded_id(side)).unwrap_or(false));
    let (unstaged_folded, staged_folded) = (folded(Side::Unstaged), folded(Side::Staged));
    let need = |side: Side, folded: bool| {
        if folded {
            0.0
        } else {
            list_height(file_list::rows(
                status.files(side).iter().map(|f| f.path.as_str()),
                tree,
            ))
        }
    };
    let (unstaged, staged) = split(
        ui.available_height() - 2.0 * LIST_CHROME,
        need(Side::Unstaged, unstaged_folded),
        need(Side::Staged, staged_folded),
    );
    for (side, height, folded) in [
        (Side::Unstaged, unstaged, unstaged_folded),
        (Side::Staged, staged, staged_folded),
    ] {
        let list = List {
            side,
            busy: staging,
            active,
            height,
            folded,
            tree,
        };
        match file_list(ui, status, &list) {
            Some(ListAction::Fold) => {
                ui.data_mut(|d| d.insert_temp(folded_id(side), !folded));
            }
            Some(ListAction::Do(a)) => action = Some(a),
            None => {}
        }
    }
    action
}

/// How one of the two file lists is shown.
struct List<'a> {
    side: Side,
    /// Staging is running: buttons are disabled.
    busy: bool,
    active: Option<(Side, &'a str)>,
    height: f32,
    folded: bool,
    tree: bool,
}

enum ListAction {
    /// Its heading was clicked: fold or unfold it.
    Fold,
    Do(Action),
}

const FILE_ROW: f32 = 26.0;
/// A list's heading and the space after it.
const LIST_CHROME: f32 = 50.0;

/// The height a list of `files` needs ("None" when empty).
#[expect(
    clippy::cast_precision_loss,
    reason = "file counts are far below f32 limits"
)]
fn list_height(files: usize) -> f32 {
    (files as f32 * FILE_ROW).max(20.0)
}

/// Shares `space` between two lists: each gets what it needs if both fit;
/// otherwise a short one keeps its height and the other gets the rest, or
/// two long ones get half each.
fn split(space: f32, first: f32, second: f32) -> (f32, f32) {
    let space = space.max(2.0 * FILE_ROW);
    let half = space / 2.0;
    if first + second <= space {
        (first, second)
    } else if second <= half {
        (space - second, second)
    } else if first <= half {
        (first, space - first)
    } else {
        (half, half)
    }
}

fn file_list(ui: &mut egui::Ui, status: &WorkingStatus, list: &List<'_>) -> Option<ListAction> {
    let &List {
        side,
        busy,
        active,
        height,
        folded,
        tree,
    } = list;
    let mut action = None;
    let mut fold = false;
    let files = status.files(side);
    let (title, all_label, all_glyph, all_action) = match side {
        Side::Unstaged => (
            "Unstaged files",
            "Stage all",
            icon::PLUS_CIRCLE,
            Action::StageAll,
        ),
        Side::Staged => (
            "Staged files",
            "Unstage all",
            icon::MINUS_CIRCLE,
            Action::UnstageAll,
        ),
    };
    ui.horizontal(|ui| {
        // The heading folds and unfolds the list.
        let caret = if folded {
            icon::CARET_RIGHT
        } else {
            icon::CARET_DOWN
        };
        let heading = ui.add(
            egui::Label::new(
                RichText::new(format!("{caret} {} {}", title.to_uppercase(), files.len()))
                    .size(11.0)
                    .strong()
                    .color(theme::muted()),
            )
            .sense(egui::Sense::click()),
        );
        let name = if folded {
            format!("Show {}", title.to_lowercase())
        } else {
            format!("Hide {}", title.to_lowercase())
        };
        heading.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
        if heading.on_hover_text(&name).clicked() {
            fold = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let button =
                egui::Button::new(RichText::new(format!("{all_glyph}  {all_label}")).size(12.0));
            if ui.add_enabled(!busy && !files.is_empty(), button).clicked() {
                action = Some(all_action);
            }
            if side == Side::Unstaged
                && widgets::named_icon_button(
                    ui,
                    icon::ARROW_COUNTER_CLOCKWISE,
                    "Discard all unstaged changes",
                    !busy && !files.is_empty(),
                )
                .clicked()
            {
                action = Some(Action::Discard(
                    files.iter().map(|f| f.path.clone()).collect(),
                ));
            }
        });
    });
    ui.spacing_mut().item_spacing.y = 0.0;
    if files.is_empty() && !folded {
        ui.label(RichText::new("None").size(12.0).color(theme::muted()));
    }
    if folded {
        ui.add_space(8.0);
        ui.spacing_mut().item_spacing.y = 6.0;
        return if fold {
            Some(ListAction::Fold)
        } else {
            action.map(ListAction::Do)
        };
    }
    egui::ScrollArea::vertical()
        .id_salt(("files", side == Side::Staged))
        .max_height(height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            file_list::show(
                ui,
                ("tree", side == Side::Staged),
                files,
                |f| f.path.as_str(),
                tree,
                |ui, f, shown| {
                    let is_active = active.is_some_and(|(s, p)| s == side && p == f.path);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        // Room for the buttons: stage/unstage, plus discard when unstaged.
                        let buttons = if side == Side::Unstaged { 2.0 } else { 1.0 };
                        let row_width = ui.available_width() - 26.0 * buttons;
                        let row = ui
                            .allocate_ui(egui::vec2(row_width, 26.0), |ui| {
                                details::file_row_shown(ui, &as_changed(f), is_active, shown)
                            })
                            .inner;
                        if row.clicked() {
                            action = Some(Action::Open(side, f.path.clone()));
                        }
                        let (glyph, verb) = match side {
                            Side::Unstaged => (icon::PLUS, "Stage"),
                            Side::Staged => (icon::MINUS, "Unstage"),
                        };
                        if side == Side::Unstaged
                            && widgets::named_icon_button(
                                ui,
                                icon::ARROW_COUNTER_CLOCKWISE,
                                &format!("Discard changes to {}", f.path),
                                !busy,
                            )
                            .clicked()
                        {
                            action = Some(Action::Discard(vec![f.path.clone()]));
                        }
                        if widgets::named_icon_button(
                            ui,
                            glyph,
                            &format!("{verb} {}", f.path),
                            !busy,
                        )
                        .clicked()
                        {
                            action = Some(match side {
                                Side::Unstaged => Action::Stage(f.path.clone()),
                                Side::Staged => Action::Unstage(f.path.clone()),
                            });
                        }
                    });
                },
            );
        });
    ui.add_space(14.0);
    ui.spacing_mut().item_spacing.y = 6.0;
    if fold {
        Some(ListAction::Fold)
    } else {
        action.map(ListAction::Do)
    }
}

fn commit_box(
    ui: &mut egui::Ui,
    draft: &mut CommitDraft,
    staged: usize,
    committing: bool,
) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(widgets::section_label(if draft.amend {
            "Amend"
        } else {
            "Commit"
        }));
        // How long the summary is, against the usual 50/72 guides.
        let n = draft.summary.chars().count();
        let colour = if n > SUMMARY_HARD {
            theme::danger()
        } else if n > SUMMARY_SOFT {
            theme::modified()
        } else {
            theme::muted()
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(n.to_string()).size(11.0).color(colour))
                .on_hover_text(
                    "Summary length. Aim for 50 characters or fewer; Git tools cut lines past 72.",
                );
        });
    });

    let width = ui.available_width();
    let summary = ui.add_enabled(
        !committing,
        egui::TextEdit::singleline(&mut draft.summary)
            .id(summary_id())
            .hint_text("Summary (required)")
            .desired_width(width),
    );
    ui.ctx()
        .accesskit_node_builder(summary.id, |n| n.set_label("Commit summary"));
    let body = ui.add_enabled(
        !committing,
        egui::TextEdit::multiline(&mut draft.body)
            .hint_text("Description")
            .desired_rows(4)
            .desired_width(width),
    );
    ui.ctx()
        .accesskit_node_builder(body.id, |n| n.set_label("Commit description"));

    let mut amend = draft.amend;
    if ui
        .add_enabled(
            !committing,
            egui::Checkbox::new(&mut amend, "Amend last commit"),
        )
        .changed()
    {
        action = Some(Action::SetAmend(amend));
    }
    if draft.amend && draft.head_pushed {
        ui.label(
            RichText::new(format!(
                "{}  This commit is already pushed. Amending rewrites it, and you'll need to force-push.",
                icon::WARNING
            ))
            .size(12.0)
            .color(theme::modified()),
        );
    }

    let can_commit = !committing && !draft.summary.trim().is_empty() && (draft.amend || staged > 0);
    let label = if committing {
        "Committing…".to_string()
    } else if draft.amend {
        "Amend commit".to_string()
    } else {
        format!(
            "Commit {staged} {}",
            if staged == 1 { "file" } else { "files" }
        )
    };
    let button = egui::Button::new(
        RichText::new(&label)
            .color(if can_commit {
                theme::on_accent()
            } else {
                theme::muted()
            })
            .strong(),
    )
    .fill(if can_commit {
        theme::accent()
    } else {
        theme::bg_hover()
    })
    .min_size(egui::vec2(width, 30.0));
    let clicked = ui.add_enabled(can_commit, button).on_disabled_hover_text(
        if draft.summary.trim().is_empty() {
            "Write a summary first."
        } else {
            "Stage some changes first."
        },
    );
    // Ctrl/Cmd+Enter in either field commits.
    let shortcut = (summary.has_focus() || body.has_focus())
        && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    if can_commit && (clicked.clicked() || shortcut) {
        action = Some(Action::Commit);
    }
    action
}

/// The file row widget is shared with commit details; line counts aren't
/// known for working files, so they are left out.
fn as_changed(f: &WorkingFile) -> ChangedFile {
    ChangedFile {
        path: f.path.clone(),
        old_path: f.old_path.clone(),
        status: f.status,
        additions: 0,
        deletions: 0,
        binary: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_the_height_between_the_lists() {
        // Both fit: each as tall as it needs.
        assert_eq!(split(400.0, 100.0, 50.0), (100.0, 50.0));
        // Many unstaged, a few staged: the staged list keeps its height.
        assert_eq!(split(400.0, 2000.0, 52.0), (348.0, 52.0));
        // And the other way round.
        assert_eq!(split(400.0, 26.0, 2000.0), (26.0, 374.0));
        // Both long: half each.
        assert_eq!(split(400.0, 2000.0, 2000.0), (200.0, 200.0));
    }
}
