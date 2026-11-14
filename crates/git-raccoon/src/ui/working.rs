//! The right-hand pane when the working changes are selected.

use eframe::egui::{self, RichText, Stroke};
use egui_phosphor::regular as icon;
use git_raccoon_core::details::ChangedFile;
use git_raccoon_core::working::{Side, WorkingFile, WorkingStatus};

use crate::theme;
use crate::ui::{details, widgets};

pub enum Action {
    Open(Side, String),
    Stage(String),
    Unstage(String),
    StageAll,
    UnstageAll,
    Close,
}

/// `busy`: a staging operation is running, so the buttons are disabled.
pub fn show(
    ui: &mut egui::Ui,
    status: Option<&WorkingStatus>,
    loading: bool,
    busy: bool,
    active: Option<(Side, &str)>,
) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.set_height(36.0);
        ui.label(widgets::section_label("Working changes"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icon::X, "Close (Esc)").clicked() {
                action = Some(Action::Close);
            }
            if loading || busy {
                ui.spinner();
            }
        });
    });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::BORDER),
    );
    ui.add_space(8.0);

    let Some(status) = status else {
        ui.label(RichText::new("Loading…").color(theme::MUTED));
        return action;
    };
    if status.is_clean() {
        ui.label(RichText::new("No uncommitted changes.").color(theme::MUTED));
        return action;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for side in [Side::Unstaged, Side::Staged] {
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
                    ui.label(widgets::section_label(title));
                    ui.label(
                        RichText::new(files.len().to_string())
                            .size(11.0)
                            .color(theme::MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let button = egui::Button::new(
                            RichText::new(format!("{all_glyph}  {all_label}")).size(12.0),
                        );
                        if ui.add_enabled(!busy && !files.is_empty(), button).clicked() {
                            action = Some(all_action);
                        }
                    });
                });
                ui.spacing_mut().item_spacing.y = 0.0;
                if files.is_empty() {
                    ui.label(RichText::new("None").size(12.0).color(theme::MUTED));
                }
                for f in files {
                    let is_active = active.is_some_and(|(s, p)| s == side && p == f.path);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let row_width = ui.available_width() - 26.0;
                        let row = ui
                            .allocate_ui(egui::vec2(row_width, 26.0), |ui| {
                                details::file_row(ui, &as_changed(f), is_active)
                            })
                            .inner;
                        if row.clicked() {
                            action = Some(Action::Open(side, f.path.clone()));
                        }
                        let (glyph, verb) = match side {
                            Side::Unstaged => (icon::PLUS, "Stage"),
                            Side::Staged => (icon::MINUS, "Unstage"),
                        };
                        let name = format!("{verb} {}", f.path);
                        if widgets::named_icon_button(ui, glyph, &name, !busy).clicked() {
                            action = Some(match side {
                                Side::Unstaged => Action::Stage(f.path.clone()),
                                Side::Staged => Action::Unstage(f.path.clone()),
                            });
                        }
                    });
                }
                ui.add_space(14.0);
                ui.spacing_mut().item_spacing.y = 6.0;
            }
        });
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
