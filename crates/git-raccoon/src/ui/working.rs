//! The right-hand pane when the working changes are selected.

use eframe::egui::{self, RichText, Stroke};
use egui_phosphor::regular as icon;
use git_raccoon_core::details::ChangedFile;
use git_raccoon_core::working::{Side, WorkingFile, WorkingStatus};

use crate::theme;
use crate::ui::{details, widgets};

pub enum Action {
    Open(Side, String),
    Close,
}

pub fn show(
    ui: &mut egui::Ui,
    status: Option<&WorkingStatus>,
    loading: bool,
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
            if loading {
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
            for (side, title) in [
                (Side::Unstaged, "Unstaged files"),
                (Side::Staged, "Staged files"),
            ] {
                let files = status.files(side);
                ui.horizontal(|ui| {
                    ui.label(widgets::section_label(title));
                    ui.label(
                        RichText::new(files.len().to_string())
                            .size(11.0)
                            .color(theme::MUTED),
                    );
                });
                ui.spacing_mut().item_spacing.y = 0.0;
                if files.is_empty() {
                    ui.label(RichText::new("None").size(12.0).color(theme::MUTED));
                }
                for f in files {
                    let is_active = active.is_some_and(|(s, p)| s == side && p == f.path);
                    if details::file_row(ui, &as_changed(f), is_active).clicked() {
                        action = Some(Action::Open(side, f.path.clone()));
                    }
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
