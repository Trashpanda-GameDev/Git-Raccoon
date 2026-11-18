//! The new-repository dialog: where it goes down the side (this computer
//! only, or a forge: made there, and cloned unless asked not to), and what
//! it starts with on the right (its first branch's name, a .gitignore and a
//! licence from the forge's templates, Git LFS).

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::{branch, create};

use crate::forge::Account;
use crate::forge::templates::{Template, Templates};
use crate::theme;
use crate::ui::side_form::{self, FIELDS, LIST_HEIGHT, check, field, label, place_row};
use crate::ui::sign_in;

/// Where a new repository goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// On this computer only.
    Here,
    /// On a forge (and, cloned, here too).
    Forge(sign_in::Place),
}

impl Place {
    const ALL: [Place; 4] = [
        Place::Here,
        Place::Forge(sign_in::Place::GitHub),
        Place::Forge(sign_in::Place::GitLab),
        Place::Forge(sign_in::Place::SelfManaged),
    ];

    fn name(self) -> &'static str {
        match self {
            Place::Here => "Only on this computer",
            Place::Forge(forge) => side_form::forge_name(forge),
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Place::Here => icon::DESKTOP,
            Place::Forge(forge) => side_form::forge_glyph(forge),
        }
    }

    /// Whether `account` is on this place.
    fn has(self, account: &Account) -> bool {
        match self {
            Place::Here => false,
            Place::Forge(forge) => side_form::forge_has(forge, account),
        }
    }
}

/// What has been filled in.
pub struct Form {
    pub place: Place,
    /// The account it's made on (an index into the accounts).
    pub account: Option<usize>,
    pub name: String,
    pub description: String,
    pub private: bool,
    /// Clone it once it's made on the forge.
    pub clone: bool,
    /// The folder its own folder goes in.
    pub parent: String,
    /// Its first branch's name (empty: as Git names it).
    pub branch: String,
    pub gitignore: Option<Template>,
    pub license: Option<Template>,
    pub lfs: bool,
    /// Whether Git LFS is installed (else it can't be set up).
    pub lfs_installed: bool,
    /// A template picker has been opened: the templates are wanted (they're
    /// only fetched then).
    pub want_templates: bool,
    /// The templates on offer (`None` while loading), and whose they are.
    pub templates: Option<Result<Templates, String>>,
    pub templates_from: Option<TemplatesFrom>,
    /// While it's being made: what's happening.
    pub progress: Option<String>,
    pub error: Option<String>,
    /// Made on a forge without a copy here: what was made.
    pub done: Option<String>,
}

/// Whose templates: GitHub's own (`None`), or an account's forge.
pub type TemplatesFrom = Option<usize>;

impl Form {
    pub fn new(parent: String, lfs_installed: bool) -> Self {
        Form {
            place: Place::Here,
            account: None,
            name: String::new(),
            description: String::new(),
            private: true,
            clone: true,
            parent,
            branch: "main".into(),
            gitignore: None,
            license: None,
            lfs: false,
            lfs_installed,
            want_templates: false,
            templates: None,
            templates_from: None,
            progress: None,
            error: None,
            done: None,
        }
    }

    /// The folder it goes in here.
    pub fn dest(&self) -> PathBuf {
        PathBuf::from(self.parent.trim()).join(self.name.trim())
    }

    /// Whether there'll be a copy on this computer (what starts it, a
    /// branch, templates and LFS, are for that copy).
    pub fn here_too(&self) -> bool {
        self.place == Place::Here || self.clone
    }

    /// Where it's going, to name it.
    pub fn place_name(&self) -> &'static str {
        self.place.name()
    }

    /// Whose templates to offer: the account's forge, or GitHub's.
    pub fn templates_wanted(&self) -> TemplatesFrom {
        match self.place {
            Place::Here => None,
            Place::Forge(_) => self.account,
        }
    }
}

pub enum Action {
    Browse,
    /// Sign in to this place first.
    SignIn(sign_in::Place),
    Create,
    Cancel,
    /// Stop it while it's being made.
    Stop,
}

/// A template picker: "None" or one of `offered` (`None` while they load,
/// or why they couldn't be had); `id` is its name for screen readers.
/// Returns whether it's open.
fn template_picker(
    ui: &mut egui::Ui,
    id: &str,
    chosen: &mut Option<Template>,
    offered: Option<Result<&[Template], &str>>,
) -> bool {
    let shown = chosen.as_ref().map_or("None", |t| t.name.as_str());
    let combo = egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .width(FIELDS)
        .height(LIST_HEIGHT)
        .show_ui(ui, |ui| {
            // (Its list is fetched once it's first opened.)
            ui.set_min_height(LIST_HEIGHT);
            ui.selectable_value(chosen, None, "None");
            match offered {
                Some(Ok(offered)) => {
                    for t in offered {
                        ui.selectable_value(chosen, Some(t.clone()), &t.name);
                    }
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(e).color(theme::danger()));
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("Loading…").color(theme::muted()));
                    });
                }
            }
        });
    ui.ctx()
        .accesskit_node_builder(combo.response.id, |n| n.set_label(id));
    combo.inner.is_some()
}

/// The form on the right, for `accounts` on the place chosen.
fn details(ui: &mut egui::Ui, form: &mut Form, accounts: &[Account], action: &mut Option<Action>) {
    let place = form.place;
    let forge = matches!(place, Place::Forge(_));
    let here_too = form.here_too();
    egui::Grid::new("new-repo-form")
        .num_columns(2)
        .spacing(egui::vec2(12.0, 10.0))
        .show(ui, |ui| {
            if forge {
                label(ui, "Account");
                let name = |i: usize| accounts.get(i).map_or("", |a| a.login.as_str());
                let combo = egui::ComboBox::from_id_salt("new-repo-account")
                    .selected_text(form.account.map_or("", name))
                    .width(FIELDS)
                    .show_ui(ui, |ui| {
                        for (i, a) in accounts.iter().enumerate().filter(|(_, a)| place.has(a)) {
                            ui.selectable_value(&mut form.account, Some(i), &a.login);
                        }
                    });
                ui.ctx().accesskit_node_builder(combo.response.id, |n| {
                    n.set_label("Account to make it on");
                });
                ui.end_row();
            }
            label(ui, "Name");
            field(ui, &mut form.name, "New repository name", "my-project");
            ui.end_row();
            if forge {
                label(ui, "Description");
                field(
                    ui,
                    &mut form.description,
                    "New repository description",
                    "Optional",
                );
                ui.end_row();
                label(ui, "Access");
                let combo = egui::ComboBox::from_id_salt("new-repo-access")
                    .selected_text(if form.private { "Private" } else { "Public" })
                    .width(FIELDS)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut form.private, false, "Public");
                        ui.selectable_value(&mut form.private, true, "Private");
                    });
                ui.ctx()
                    .accesskit_node_builder(combo.response.id, |n| n.set_label("Who can see it"));
                ui.end_row();
                label(ui, "Clone after creating");
                check(ui, &mut form.clone, "Clone after creating");
                ui.end_row();
            }
            // What follows is for the copy on this computer.
            label(ui, if forge { "Where to clone to" } else { "Where" });
            ui.add_enabled_ui(here_too, |ui| {
                ui.horizontal(|ui| {
                    let path = ui.add(
                        egui::TextEdit::singleline(&mut form.parent)
                            .hint_text("A folder")
                            .desired_width(FIELDS - 84.0),
                    );
                    ui.ctx()
                        .accesskit_node_builder(path.id, |n| n.set_label("New repository folder"));
                    if ui.button("Browse…").clicked() {
                        *action = Some(Action::Browse);
                    }
                });
            });
            ui.end_row();
            label(ui, "Full path");
            ui.add_enabled_ui(here_too, |ui| {
                side_form::path(ui, &form.dest().display().to_string());
            });
            ui.end_row();
            label(ui, "Default branch name");
            ui.add_enabled_ui(here_too, |ui| {
                field(ui, &mut form.branch, "First branch name", "main");
            });
            ui.end_row();
            let offered = |list: fn(&Templates) -> &[Template]| {
                form.templates
                    .as_ref()
                    .map(|t| t.as_ref().map(list).map_err(String::as_str))
            };
            label(ui, ".gitignore template");
            ui.add_enabled_ui(here_too, |ui| {
                let open = template_picker(
                    ui,
                    "Choose a .gitignore template",
                    &mut form.gitignore,
                    offered(|t| &t.gitignores),
                );
                form.want_templates |= open;
            });
            ui.end_row();
            label(ui, "License");
            ui.add_enabled_ui(here_too, |ui| {
                let open = template_picker(
                    ui,
                    "Choose a licence",
                    &mut form.license,
                    offered(|t| &t.licenses),
                );
                form.want_templates |= open;
            });
            ui.end_row();
            label(ui, "Initialize with LFS");
            ui.add_enabled_ui(here_too && form.lfs_installed, |ui| {
                check(ui, &mut form.lfs, "Initialize with LFS")
                    .on_disabled_hover_text("Git LFS isn't installed");
            });
            ui.end_row();
        });
    // Templates: loading, or why there aren't any.
    match &form.templates {
        None if here_too && form.want_templates => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new("Getting the templates…")
                        .size(12.0)
                        .color(theme::muted()),
                );
            });
        }
        Some(Err(e)) if here_too => {
            ui.label(
                RichText::new(format!("No templates: {e}"))
                    .size(12.0)
                    .color(theme::muted()),
            );
        }
        _ => {}
    }
}

/// Why it can't be made yet, if it can't.
fn problem(form: &Form) -> Option<&'static str> {
    let name = form.name.trim();
    let branch_name = form.branch.trim();
    if name.is_empty() {
        Some("")
    } else if name.contains(['/', '\\']) {
        Some("A name without slashes.")
    } else if matches!(form.place, Place::Forge(_)) && form.account.is_none() {
        Some("Choose the account to make it on.")
    } else if form.here_too() && form.parent.trim().is_empty() {
        Some("")
    } else if form.here_too() && !create::can_clone_into(&form.dest()) {
        Some("That folder is already there and isn't empty.")
    } else if form.here_too()
        && !branch_name.is_empty()
        && !branch::is_valid_branch_name(branch_name)
    {
        Some("Not a valid branch name.")
    } else {
        None
    }
}

/// Shows the dialog; `accounts` are the ones signed in to.
pub fn show(ctx: &egui::Context, form: &mut Form, accounts: &[Account]) -> Option<Action> {
    let mut action = None;
    let making = form.progress.is_some();
    // Signed in since (from here): that account.
    if form.account.is_none() {
        form.account = accounts.iter().position(|a| form.place.has(a));
    }
    let modal = egui::Modal::new(egui::Id::new("new-repo")).show(ctx, |ui| {
        ui.set_width(760.0);
        ui.label(RichText::new("New repository").strong().size(15.0));
        ui.add_space(8.0);
        ui.horizontal_top(|ui| {
            // Where it goes.
            ui.add_enabled_ui(!making && form.done.is_none(), |ui| {
                ui.vertical(|ui| {
                    ui.set_width(side_form::PLACES);
                    for place in Place::ALL {
                        let row = place_row(ui, place.glyph(), place.name(), form.place == place);
                        if row.clicked() && form.place != place {
                            form.place = place;
                            form.account = accounts.iter().position(|a| place.has(a));
                            form.error = None;
                        }
                    }
                });
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_min_height(430.0);
                if let Some(done) = &form.done {
                    ui.label(done);
                    ui.add_space(12.0);
                    if ui.button("Close").clicked() {
                        action = Some(Action::Cancel);
                    }
                    return;
                }
                let signed_in = match form.place {
                    Place::Here => true,
                    place => accounts.iter().any(|a| place.has(a)),
                };
                if let (false, Place::Forge(place)) = (signed_in, form.place) {
                    ui.label(format!(
                        "Sign in to {} to make repositories there.",
                        form.place.name()
                    ));
                    ui.add_space(8.0);
                    if ui.button("Sign in…").clicked() {
                        action = Some(Action::SignIn(place));
                    }
                    return;
                }
                ui.add_enabled_ui(!making, |ui| details(ui, form, accounts, &mut action));
                ui.add_space(12.0);
                if let Some(e) = &form.error {
                    ui.label(RichText::new(e).size(12.0).color(theme::danger()));
                    ui.add_space(4.0);
                }
                let problem = problem(form);
                if let Some(p) = problem.filter(|p| !p.is_empty()) {
                    ui.label(RichText::new(p).size(12.0).color(theme::danger()));
                    ui.add_space(4.0);
                }
                ui.horizontal(|ui| {
                    if let Some(doing) = &form.progress {
                        ui.spinner();
                        ui.label(RichText::new(doing).size(12.0).color(theme::muted()));
                        let stop = ui.button("Cancel");
                        ui.ctx()
                            .accesskit_node_builder(stop.id, |n| n.set_label("Stop making it"));
                        if stop.clicked() {
                            action = Some(Action::Stop);
                        }
                        return;
                    }
                    let label = match (form.place, form.clone) {
                        (Place::Here, _) | (Place::Forge(_), false) => "Create repository",
                        (Place::Forge(_), true) => "Create repository and clone",
                    };
                    let go =
                        egui::Button::new(RichText::new(label).color(theme::on_accent()).strong())
                            .fill(theme::accent());
                    if ui.add_enabled(problem.is_none(), go).clicked() {
                        action = Some(Action::Create);
                    }
                    if ui.button("Cancel").clicked() {
                        action = Some(Action::Cancel);
                    }
                });
            });
        });
    });
    if modal.should_close() && action.is_none() {
        action = Some(if making { Action::Stop } else { Action::Cancel });
    }
    action
}
