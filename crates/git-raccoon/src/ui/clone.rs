//! The clone dialog: where from, where to, and progress while cloning.

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use git_raccoon_core::create;
use git_raccoon_core::sync::Progress;

use crate::theme;

/// What has been filled in.
#[derive(Default)]
pub struct Form {
    pub url: String,
    /// The folder the clone goes in.
    pub parent: String,
    /// The clone's own folder name; follows the URL until edited.
    pub name: String,
    pub name_edited: bool,
    /// Why the last attempt failed.
    pub error: Option<String>,
}

impl Form {
    pub fn dest(&self) -> PathBuf {
        PathBuf::from(self.parent.trim()).join(self.name.trim())
    }
}

pub enum Action {
    Clone,
    Browse,
    Cancel,
    /// Stop the clone in progress.
    Stop,
}

/// Shows the dialog. `progress` is `Some` while cloning.
pub fn show(
    ctx: &egui::Context,
    form: &mut Form,
    progress: Option<Option<Progress>>,
) -> Option<Action> {
    let mut action = None;
    let cloning = progress.is_some();
    let modal = egui::Modal::new(egui::Id::new("clone")).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.label(RichText::new("Clone a repository").strong().size(15.0));
        ui.add_space(8.0);
        ui.add_enabled_ui(!cloning, |ui| {
            let url = labelled(
                ui,
                "URL",
                "Repository URL",
                &mut form.url,
                "https://github.com/owner/repo.git",
            );
            if url.changed() && !form.name_edited {
                form.name = create::folder_name(&form.url);
            }
            ui.label(RichText::new("Folder").size(12.0).color(theme::muted()));
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut form.parent)
                        .hint_text("Where to put it")
                        .desired_width(ui.available_width() - 84.0),
                );
                ui.ctx()
                    .accesskit_node_builder(field.id, |n| n.set_label("Clone folder"));
                if ui.button("Browse…").clicked() {
                    action = Some(Action::Browse);
                }
            });
            ui.add_space(4.0);
            if labelled(ui, "Name", "Clone name", &mut form.name, "Folder name").changed() {
                form.name_edited = true;
            }
        });
        let ready = !form.url.trim().is_empty()
            && !form.parent.trim().is_empty()
            && !form.name.trim().is_empty();
        let free = !ready || create::can_clone_into(&form.dest());
        if ready {
            let text = if free {
                format!("Clones into {}", form.dest().display())
            } else {
                format!("{} already exists and isn't empty.", form.dest().display())
            };
            ui.label(RichText::new(text).size(12.0).color(if free {
                theme::muted()
            } else {
                theme::danger()
            }));
        }
        if let Some(e) = &form.error {
            ui.add_space(4.0);
            ui.label(RichText::new(e).size(12.0).color(theme::danger()));
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if let Some(progress) = &progress {
                ui.spinner();
                let text = match progress {
                    Some(Progress {
                        phase,
                        percent: Some(p),
                    }) => format!("{phase} {p}%"),
                    Some(p) => p.phase.clone(),
                    None => "Cloning…".into(),
                };
                ui.label(RichText::new(text).size(12.0).color(theme::muted()));
                if let Some(Progress {
                    percent: Some(p), ..
                }) = progress
                {
                    ui.add(
                        egui::ProgressBar::new(f32::from(*p) / 100.0)
                            .desired_width(100.0)
                            .desired_height(4.0),
                    );
                }
                let stop = ui.button("Cancel");
                ui.ctx()
                    .accesskit_node_builder(stop.id, |n| n.set_label("Cancel clone"));
                if stop.clicked() {
                    action = Some(Action::Stop);
                }
            } else {
                let go =
                    egui::Button::new(RichText::new("Clone").color(theme::on_accent()).strong())
                        .fill(theme::accent());
                if ui.add_enabled(ready && free, go).clicked() {
                    action = Some(Action::Clone);
                }
                if ui.button("Cancel").clicked() {
                    action = Some(Action::Cancel);
                }
            }
        });
    });
    if modal.should_close() && action.is_none() {
        action = Some(if cloning {
            Action::Stop
        } else {
            Action::Cancel
        });
    }
    action
}

/// A field with its name above it (and as its accessible name).
fn labelled(
    ui: &mut egui::Ui,
    name: &str,
    accessible: &str,
    text: &mut String,
    hint: &str,
) -> egui::Response {
    ui.label(RichText::new(name).size(12.0).color(theme::muted()));
    let response = ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
    ui.ctx()
        .accesskit_node_builder(response.id, |n| n.set_label(accessible));
    ui.add_space(4.0);
    response
}
