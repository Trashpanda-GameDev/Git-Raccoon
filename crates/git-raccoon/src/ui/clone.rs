//! The clone dialog: a URL and where to put it, how much (shallow,
//! sparse), and progress while cloning; with one of your accounts chosen,
//! a list of your repositories there to pick the URL from.

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::create::{self, CloneOptions};
use git_raccoon_core::sync::Progress;

use crate::forge::Account;
use crate::theme;
use crate::ui::repo_form::{self, FIELDS, LIST_HEIGHT, check, field, field_sized, label};

/// What has been filled in.
pub struct Form {
    /// The account whose repositories are offered (an index into the
    /// accounts); `None`: just a URL.
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
    /// Whether SSH works with a host, being found out, and the answer.
    ssh_check: Option<(String, crate::task::Task<bool>)>,
    ssh_works: Option<(String, bool)>,
    /// SSH or HTTPS was picked by hand: not switched for them.
    scheme_chosen: bool,
}

impl Form {
    /// A clone into `parent`, from a URL to start with.
    pub fn new(parent: String) -> Self {
        Form {
            account: None,
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
            ssh_check: None,
            ssh_works: None,
            scheme_chosen: false,
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
        .and_then(|repos| {
            repos
                .iter()
                .find(|r| r.clone_url == form.url || r.ssh_url == form.url)
        })
        .map(|r| r.path.clone());
    let shown = picked.unwrap_or_else(|| "Choose one of your repositories".into());
    let mut chosen = None;
    let combo = egui::ComboBox::from_id_salt("clone-repo")
        .selected_text(shown)
        .width(FIELDS)
        .height(LIST_HEIGHT)
        // Typing in its search field mustn't close it.
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            ui.set_min_height(LIST_HEIGHT);
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
                            form.url == repo.clone_url || form.url == repo.ssh_url,
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

/// SSH or HTTPS, for an address that can be either: SSH chosen when the
/// key already works with that host (unless picked by hand).
fn connect_with(ui: &mut egui::Ui, form: &mut Form) {
    use git_raccoon_core::ssh;
    let is_ssh = ssh::parse_url(&form.url).is_some();
    let other = if is_ssh {
        ssh::https_url(&form.url)
    } else {
        ssh::ssh_url(&form.url)
    };
    let Some(other) = other else { return };
    let host = ssh::parse_url(if is_ssh { &form.url } else { &other })
        .map(|u| u.host)
        .unwrap_or_default();
    // Does SSH work with it? Found out once per host, asking nobody.
    if let Some((h, task)) = &form.ssh_check
        && let Some(works) = task.poll()
    {
        form.ssh_works = Some((h.clone(), works));
        form.ssh_check = None;
    }
    let known = form
        .ssh_works
        .as_ref()
        .filter(|(h, _)| *h == host)
        .map(|(_, w)| *w);
    if known.is_none() && form.ssh_check.as_ref().is_none_or(|(h, _)| *h != host) {
        let h = host.clone();
        form.ssh_check = Some((
            host.clone(),
            crate::task::Task::spawn(ui.ctx(), move || {
                ssh::works_quietly(&ssh::Ssh::for_git(None), "git", &h)
            }),
        ));
    }
    if known == Some(true) && !is_ssh && !form.scheme_chosen {
        form.set_url(other.clone());
        return;
    }
    label(ui, "Connect with");
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let ssh_button = ui.selectable_label(is_ssh, "SSH");
            let https_button = ui.selectable_label(!is_ssh, "HTTPS");
            ui.ctx().accesskit_node_builder(ssh_button.id, |n| n.set_label("Connect with SSH"));
            ui.ctx().accesskit_node_builder(https_button.id, |n| n.set_label("Connect with HTTPS"));
            if (ssh_button.clicked() && !is_ssh) || (https_button.clicked() && is_ssh) {
                form.scheme_chosen = true;
                form.set_url(other.clone());
            }
        });
        match known {
            Some(true) => {
                ui.label(
                    RichText::new(format!("{}  Your SSH key works with {host}", icon::CHECK))
                        .size(12.0)
                        .color(theme::add()),
                );
            }
            Some(false) => {
                ui.label(
                    RichText::new(format!(
                        "SSH isn't set up for {host} yet (Settings › SSH): HTTPS signs in with your account."
                    ))
                    .size(12.0)
                    .color(theme::muted()),
                );
            }
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(format!("Checking SSH with {host}…")).size(12.0).color(theme::muted()));
                });
            }
        }
    });
    ui.end_row();
}

/// The form, for `accounts` signed in to.
fn details(ui: &mut egui::Ui, form: &mut Form, accounts: &[Account], action: &mut Option<Action>) {
    egui::Grid::new("clone-form")
        .num_columns(2)
        .spacing(egui::vec2(12.0, 10.0))
        .show(ui, |ui| {
            label(ui, "Account");
            if repo_form::account_picker(ui, &mut form.account, accounts, "Account to clone from") {
                form.repos = None;
                form.search.clear();
                form.error = None;
            }
            ui.end_row();
            // What an account adds: its repositories, to pick the URL from.
            if form.account.is_some() {
                label(ui, "Repository to clone");
                repository_picker(ui, form);
                ui.end_row();
            }
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
            connect_with(ui, form);
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
            repo_form::path(ui, &form.dest().display().to_string());
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
    // Signed out of since: just a URL.
    if form.account.is_some_and(|i| i >= accounts.len()) {
        form.account = None;
    }
    let modal = egui::Modal::new(egui::Id::new("clone")).show(ctx, |ui| {
        ui.set_width(repo_form::WIDTH);
        ui.label(RichText::new("Clone a repository").strong().size(15.0));
        ui.add_space(8.0);
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
                let go =
                    egui::Button::new(RichText::new("Clone").color(theme::on_accent()).strong())
                        .fill(theme::accent_fill());
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
