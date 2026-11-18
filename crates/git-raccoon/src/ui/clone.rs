//! The clone dialog: where from down the side (a URL, or a forge you're
//! signed in to, to pick one of your repositories there), and beside it
//! where to, how much (shallow, sparse), and progress while cloning.

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::create::{self, CloneOptions};
use git_raccoon_core::sync::Progress;

use crate::forge::Account;
use crate::theme;
use crate::ui::side_form::{self, FIELDS, check, field, field_sized, label, place_row};
use crate::ui::sign_in;

/// Where a clone comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    Url,
    Forge(sign_in::Place),
}

impl Place {
    const ALL: [Place; 4] = [
        Place::Url,
        Place::Forge(side_form::FORGES[0]),
        Place::Forge(side_form::FORGES[1]),
        Place::Forge(side_form::FORGES[2]),
    ];

    fn name(self) -> &'static str {
        match self {
            Place::Url => "Clone with URL",
            Place::Forge(forge) => side_form::forge_name(forge),
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Place::Url => icon::GLOBE,
            Place::Forge(forge) => side_form::forge_glyph(forge),
        }
    }

    fn has(self, account: &Account) -> bool {
        match self {
            Place::Url => false,
            Place::Forge(forge) => side_form::forge_has(forge, account),
        }
    }
}

/// What has been filled in.
pub struct Form {
    pub place: Place,
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
    /// Only the latest commit's history.
    pub shallow: bool,
    /// Check out only some folders: these (space-separated; none, only the
    /// top-level files).
    pub sparse: bool,
    pub folders: String,
    /// Why the last attempt failed.
    pub error: Option<String>,
}

impl Form {
    /// A clone into `parent`, from the first forge with an account (else a URL).
    pub fn new(parent: String, accounts: &[Account]) -> Self {
        let first = accounts.first();
        let place = Place::ALL
            .into_iter()
            .find(|p| first.is_some_and(|a| p.has(a)))
            .unwrap_or(Place::Url);
        Form {
            place,
            account: first.map(|_| 0),
            repos: None,
            search: String::new(),
            url: String::new(),
            parent,
            name: String::new(),
            name_edited: false,
            shallow: false,
            sparse: false,
            folders: String::new(),
            error: None,
        }
    }

    pub fn dest(&self) -> PathBuf {
        PathBuf::from(self.parent.trim()).join(self.name.trim())
    }

    /// How much of it to clone.
    pub fn options(&self) -> CloneOptions {
        CloneOptions {
            shallow: self.shallow,
            sparse: self
                .sparse
                .then(|| self.folders.split_whitespace().map(String::from).collect()),
        }
    }

    fn set_url(&mut self, url: String) {
        self.url = url;
        if !self.name_edited {
            self.name = create::folder_name(&self.url);
        }
    }
}

pub enum Action {
    Clone,
    Browse,
    /// Sign in to this forge first.
    SignIn(sign_in::Place),
    Cancel,
    /// Stop the clone in progress.
    Stop,
}

/// One of the account's repositories, to pick: a dropdown with a field to
/// narrow it down.
fn repository_picker(ui: &mut egui::Ui, form: &mut Form) {
    let picked = form
        .repos
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .and_then(|repos| repos.iter().find(|r| r.clone_url == form.url))
        .map(|r| r.path.clone());
    let shown = picked.unwrap_or_else(|| "Choose one of your repositories".into());
    let mut chosen = None;
    let combo = egui::ComboBox::from_id_salt("clone-repo")
        .selected_text(shown)
        .width(FIELDS)
        .height(280.0)
        // Typing in its search field mustn't close it.
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            let search = field_sized(
                ui,
                &mut form.search,
                "Find a repository",
                &format!("{}  Find a repository…", icon::MAGNIFYING_GLASS),
                FIELDS - 16.0,
            );
            if !search.has_focus() && form.search.is_empty() {
                search.request_focus();
            }
            match &form.repos {
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
                    for repo in repos
                        .iter()
                        .filter(|r| wanted.is_empty() || r.path.to_lowercase().contains(&wanted))
                    {
                        shown += 1;
                        let lock = if repo.private {
                            icon::LOCK_SIMPLE
                        } else {
                            icon::BOOK_BOOKMARK
                        };
                        let row = ui.selectable_label(
                            form.url == repo.clone_url,
                            format!("{lock}  {}", repo.path),
                        );
                        let row = if repo.description.is_empty() {
                            row
                        } else {
                            row.on_hover_text(&repo.description)
                        };
                        if row.clicked() {
                            chosen = Some(repo.clone_url.clone());
                            ui.close();
                        }
                    }
                    if shown == 0 {
                        ui.label(RichText::new("No repositories").color(theme::muted()));
                    }
                }
            }
        });
    ui.ctx()
        .accesskit_node_builder(combo.response.id, |n| n.set_label("Repository to clone"));
    if let Some(url) = chosen {
        form.set_url(url);
    }
}

/// The form beside the places.
fn details(ui: &mut egui::Ui, form: &mut Form, accounts: &[Account], action: &mut Option<Action>) {
    let place = form.place;
    egui::Grid::new("clone-form")
        .num_columns(2)
        .spacing(egui::vec2(12.0, 10.0))
        .show(ui, |ui| {
            label(ui, "Where to clone to");
            ui.horizontal(|ui| {
                field_sized(
                    ui,
                    &mut form.parent,
                    "Clone folder",
                    "Where to put it",
                    FIELDS - 84.0,
                );
                if ui.button("Browse…").clicked() {
                    *action = Some(Action::Browse);
                }
            });
            ui.end_row();
            match place {
                Place::Url => {
                    label(ui, "URL");
                    let before = form.url.clone();
                    field(
                        ui,
                        &mut form.url,
                        "Repository URL",
                        "https://github.com/owner/repo.git",
                    );
                    if form.url != before {
                        let url = form.url.clone();
                        form.set_url(url);
                    }
                    ui.end_row();
                }
                Place::Forge(_) => {
                    // An account to pick from, when there are several there.
                    if accounts.iter().filter(|a| place.has(a)).count() > 1 {
                        label(ui, "Account");
                        let before = form.account;
                        let login = |i: usize| accounts.get(i).map_or("", |a| a.login.as_str());
                        let combo = egui::ComboBox::from_id_salt("clone-account")
                            .selected_text(form.account.map_or("", login))
                            .width(FIELDS)
                            .show_ui(ui, |ui| {
                                for (i, a) in
                                    accounts.iter().enumerate().filter(|(_, a)| place.has(a))
                                {
                                    ui.selectable_value(&mut form.account, Some(i), &a.login);
                                }
                            });
                        ui.ctx().accesskit_node_builder(combo.response.id, |n| {
                            n.set_label("Account to clone from");
                        });
                        if form.account != before {
                            form.repos = None;
                            form.search.clear();
                        }
                        ui.end_row();
                    }
                    label(ui, "Repository to clone");
                    repository_picker(ui, form);
                    ui.end_row();
                }
            }
            label(ui, "Folder name");
            if field(
                ui,
                &mut form.name,
                "Clone name",
                "Named after the repository",
            )
            .changed()
            {
                form.name_edited = true;
            }
            ui.end_row();
            label(ui, "Full path");
            side_form::path(ui, &form.dest().display().to_string());
            ui.end_row();
            label(ui, "Shallow clone");
            check(ui, &mut form.shallow, "Shallow clone")
                .on_hover_text("Only the latest commit, not the history before it");
            ui.end_row();
            label(ui, "Sparse checkout");
            check(ui, &mut form.sparse, "Sparse checkout")
                .on_hover_text("Only some of its folders in the working tree");
            ui.end_row();
            if form.sparse {
                label(ui, "Folders to include");
                field(
                    ui,
                    &mut form.folders,
                    "Folders to include",
                    "src docs (none: the top-level files only)",
                );
                ui.end_row();
            }
        });
}

/// Shows the dialog. `progress` is `Some` while cloning; `accounts` are
/// the ones signed in to, to pick a repository from.
pub fn show(
    ctx: &egui::Context,
    form: &mut Form,
    progress: Option<Option<Progress>>,
    accounts: &[Account],
) -> Option<Action> {
    let mut action = None;
    let cloning = progress.is_some();
    // Signed in since (from here): that account.
    if form.account.is_none() {
        form.account = accounts.iter().position(|a| form.place.has(a));
    }
    let modal = egui::Modal::new(egui::Id::new("clone")).show(ctx, |ui| {
        ui.set_width(760.0);
        ui.label(RichText::new("Clone a repository").strong().size(15.0));
        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            // Where from.
            ui.add_enabled_ui(!cloning, |ui| {
                ui.vertical(|ui| {
                    ui.set_width(side_form::PLACES);
                    for place in Place::ALL {
                        let row = place_row(ui, place.glyph(), place.name(), form.place == place);
                        if row.clicked() && form.place != place {
                            form.place = place;
                            form.account = accounts.iter().position(|a| place.has(a));
                            form.repos = None;
                            form.search.clear();
                            form.error = None;
                        }
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_min_height(380.0);
                if let Place::Forge(forge) = form.place
                    && form.account.is_none()
                {
                    ui.label(format!(
                        "Sign in to {} to clone your repositories there.",
                        form.place.name()
                    ));
                    ui.add_space(8.0);
                    if ui.button("Sign in…").clicked() {
                        action = Some(Action::SignIn(forge));
                    }
                    return;
                }
                ui.add_enabled_ui(!cloning, |ui| details(ui, form, accounts, &mut action));
                ui.add_space(12.0);
                let ready = !form.url.trim().is_empty()
                    && !form.parent.trim().is_empty()
                    && !form.name.trim().is_empty();
                let free = !ready || create::can_clone_into(&form.dest());
                if !free {
                    ui.label(
                        RichText::new(format!(
                            "{} already exists and isn't empty.",
                            form.dest().display()
                        ))
                        .size(12.0)
                        .color(theme::danger()),
                    );
                }
                if let Some(e) = &form.error {
                    ui.label(RichText::new(e).size(12.0).color(theme::danger()));
                }
                ui.add_space(4.0);
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
                        let go = egui::Button::new(
                            RichText::new("Clone").color(theme::on_accent()).strong(),
                        )
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
