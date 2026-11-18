//! The clone dialog: where from (a URL, or one of your repositories on an
//! account you're signed in to), where to, and progress while cloning. (New
//! repositories have a dialog of their own.)

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use git_raccoon_core::create;
use git_raccoon_core::sync::Progress;

use crate::theme;

/// What has been filled in.
#[derive(Default)]
pub struct Form {
    /// The account being picked from (an index into the accounts), if any.
    pub account: Option<usize>,
    /// Its repositories, once loaded (`None` while loading).
    pub repos: Option<Result<Vec<crate::forge::accounts::Repo>, String>>,
    /// Narrows the list.
    pub search: String,
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
    /// Load the repositories of the account picked.
    LoadRepos,
    Clone,
    Browse,
    Cancel,
    /// Stop the clone in progress.
    Stop,
}

/// Picking one of an account's repositories.
fn from_account(
    ui: &mut egui::Ui,
    form: &mut Form,
    accounts: &[crate::forge::Account],
    action: &mut Option<Action>,
) {
    use egui_phosphor::regular as icon;
    let name = |a: &crate::forge::Account| format!("{} · {}", a.place(), a.login);
    ui.label(RichText::new("From").size(12.0).color(theme::muted()));
    let chosen = form
        .account
        .and_then(|i| accounts.get(i))
        .map_or_else(|| "A URL".to_string(), name);
    let before = form.account;
    let combo = egui::ComboBox::from_id_salt("clone-from")
        .selected_text(chosen)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut form.account, None, "A URL");
            for (i, a) in accounts.iter().enumerate() {
                ui.selectable_value(&mut form.account, Some(i), name(a));
            }
        });
    ui.ctx()
        .accesskit_node_builder(combo.response.id, |n| n.set_label("Clone from"));
    if form.account != before {
        form.repos = None;
        form.search.clear();
        if form.account.is_some() {
            *action = Some(Action::LoadRepos);
        }
    }
    if form.account.and_then(|i| accounts.get(i)).is_none() {
        ui.add_space(4.0);
        return;
    }
    ui.add_space(6.0);
    let search = ui.add(
        egui::TextEdit::singleline(&mut form.search)
            .hint_text(format!("{}  Find a repository…", icon::MAGNIFYING_GLASS))
            .desired_width(f32::INFINITY),
    );
    ui.ctx()
        .accesskit_node_builder(search.id, |n| n.set_label("Find a repository"));
    ui.add_space(2.0);
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, theme::border()))
        .corner_radius(4)
        .inner_margin(4)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("clone-repos")
                .max_height(180.0)
                .min_scrolled_height(180.0)
                .auto_shrink([false, false])
                .show(ui, |ui| match &form.repos {
                    None => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(RichText::new("Loading…").color(theme::muted()));
                        });
                    }
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(theme::danger()));
                    }
                    Some(Ok(repos)) => {
                        let wanted = form.search.trim().to_lowercase();
                        let mut shown = 0;
                        for repo in repos.iter().filter(|r| {
                            wanted.is_empty() || r.path.to_lowercase().contains(&wanted)
                        }) {
                            shown += 1;
                            let picked = form.url == repo.clone_url;
                            let lock = if repo.private {
                                icon::LOCK_SIMPLE
                            } else {
                                icon::BOOK_BOOKMARK
                            };
                            let row = ui.selectable_label(picked, format!("{lock}  {}", repo.path));
                            let row = if repo.description.is_empty() {
                                row
                            } else {
                                row.on_hover_text(&repo.description)
                            };
                            if row.clicked() {
                                form.url = repo.clone_url.clone();
                                if !form.name_edited {
                                    form.name = create::folder_name(&form.url);
                                }
                            }
                        }
                        if shown == 0 {
                            ui.label(RichText::new("No repositories").color(theme::muted()));
                        }
                    }
                });
        });
    ui.add_space(8.0);
}

/// Shows the dialog. `progress` is `Some` while cloning; `accounts` are
/// the ones signed in to, to pick a repository from.
pub fn show(
    ctx: &egui::Context,
    form: &mut Form,
    progress: Option<Option<Progress>>,
    accounts: &[crate::forge::Account],
) -> Option<Action> {
    let mut action = None;
    let cloning = progress.is_some();
    let modal = egui::Modal::new(egui::Id::new("clone")).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.label(RichText::new("Clone a repository").strong().size(15.0));
        ui.add_space(8.0);
        ui.add_enabled_ui(!cloning, |ui| {
            if !accounts.is_empty() {
                from_account(ui, form, accounts, &mut action);
            }
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
