//! Signing in to a forge: GitHub, GitLab.com, or a self-managed GitLab;
//! with the browser where that's set up, or with a token.

use eframe::egui::{self, RichText};

use crate::forge::{Kind, oauth};
use crate::theme;
use crate::ui::widgets;

/// Where to sign in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    GitHub,
    GitLab,
    SelfManaged,
}

/// How far signing in has got.
#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    Choosing,
    /// Waiting for the code to show.
    Starting,
    /// The code is shown; waiting for the person to approve it.
    Approving(oauth::DeviceCode),
    /// Checking a token.
    Checking,
    Failed(String),
}

/// The sign-in window's state.
pub struct SignIn {
    pub place: Place,
    /// A self-managed GitLab's web address.
    pub server: String,
    pub token: String,
    /// Use a token even where the browser would do.
    pub with_token: bool,
    /// A self-managed GitLab: sign in with the browser, through the app
    /// registered on it ...
    pub with_app: bool,
    /// ... whose Application ID this is.
    pub app_id: String,
    pub stage: Stage,
}

impl Default for SignIn {
    fn default() -> Self {
        SignIn {
            place: Place::GitHub,
            server: "https://".into(),
            token: String::new(),
            with_token: false,
            with_app: false,
            app_id: String::new(),
            stage: Stage::Choosing,
        }
    }
}

impl SignIn {
    /// Signing in to a project's forge. `accounts` give the app already
    /// used on a self-managed server.
    pub fn for_project(
        project: &crate::forge::Project,
        accounts: &[crate::forge::Account],
    ) -> Self {
        let place = match project.base.as_str() {
            "https://github.com" => Place::GitHub,
            "https://gitlab.com" => Place::GitLab,
            _ => Place::SelfManaged,
        };
        let mut s = SignIn {
            place,
            server: project.base.clone(),
            ..SignIn::default()
        };
        s.remember_app(accounts);
        s
    }

    /// Fills in the app used before on this self-managed server, if any.
    pub fn remember_app(&mut self, accounts: &[crate::forge::Account]) {
        if self.place != Place::SelfManaged || !self.app_id.is_empty() {
            return;
        }
        let (_, base) = self.forge();
        if let Some(id) = accounts
            .iter()
            .filter(|a| a.base == base)
            .find_map(|a| a.client.clone())
        {
            self.app_id = id;
            self.with_app = true;
        }
    }

    /// The forge and its web address.
    pub fn forge(&self) -> (Kind, String) {
        match self.place {
            Place::GitHub => (Kind::GitHub, "https://github.com".into()),
            Place::GitLab => (Kind::GitLab, "https://gitlab.com".into()),
            Place::SelfManaged => (
                Kind::GitLab,
                self.server.trim().trim_end_matches('/').to_string(),
            ),
        }
    }

    /// Signing in with the browser, with this OAuth app (empty while a
    /// self-managed server's Application ID is still to be given).
    pub fn browser(&self) -> Option<String> {
        if self.place == Place::SelfManaged {
            return self.with_app.then(|| self.app_id.trim().to_string());
        }
        let (kind, base) = self.forge();
        oauth::client_id(kind, &base)
            .filter(|_| !self.with_token)
            .map(str::to_string)
    }

    /// Both ways of signing in can be offered here.
    fn has_both(&self) -> bool {
        let (kind, base) = self.forge();
        self.place == Place::SelfManaged || oauth::client_id(kind, &base).is_some()
    }

    /// The server's page for registering an app (a self-managed GitLab).
    pub fn apps_page(&self) -> String {
        format!("{}/-/user_settings/applications", self.forge().1)
    }

    /// The page that makes a token with the right permissions.
    pub fn token_page(&self) -> String {
        match self.forge() {
            (Kind::GitHub, _) => {
                "https://github.com/settings/tokens/new?scopes=repo,read:org&description=Git%20Raccoon".into()
            }
            (Kind::GitLab, base) => format!(
                "{base}/-/user_settings/personal_access_tokens?name=Git%20Raccoon&scopes=api"
            ),
        }
    }
}

pub enum Action {
    /// Start the browser sign-in.
    Browser,
    /// Check the token and sign in with it.
    Token,
    /// Open a page (the sign-in page, or the token page).
    Open(String),
    Close,
}

pub fn show(ctx: &egui::Context, s: &mut SignIn) -> Option<Action> {
    let mut action = None;
    let modal = egui::Modal::new(egui::Id::new("sign-in")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.label(RichText::new("Sign in").strong().size(16.0));
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "To see pull requests, merge requests and issues next to the graph, and to open new ones.",
            )
            .color(theme::muted()),
        );
        ui.add_space(10.0);
        let busy = matches!(s.stage, Stage::Starting | Stage::Approving(_) | Stage::Checking);
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                s.place = widgets::segmented(
                    ui,
                    s.place,
                    &[
                        (Place::GitHub, "GitHub"),
                        (Place::GitLab, "GitLab.com"),
                        (Place::SelfManaged, "Self-managed GitLab"),
                    ],
                );
            });
            if s.place == Place::SelfManaged {
                ui.add_space(6.0);
                let server = ui.add(
                    egui::TextEdit::singleline(&mut s.server)
                        .hint_text("https://gitlab.example.com")
                        .desired_width(f32::INFINITY),
                );
                ui.ctx()
                    .accesskit_node_builder(server.id, |n| n.set_label("GitLab server address"));
            }
        });
        ui.add_space(10.0);
        match &s.stage {
            Stage::Approving(code) => {
                ui.label("Enter this code on the page that opened, and approve Git Raccoon:");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&code.user_code).monospace().size(22.0).strong());
                    if ui.button("Copy").clicked() {
                        ui.ctx().copy_text(code.user_code.clone());
                    }
                    if ui.button("Open the page again").clicked() {
                        action = Some(Action::Open(code.page.clone()));
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Waiting for you to approve it…").color(theme::muted()));
                });
            }
            _ if s.place == Place::SelfManaged && s.with_app => {
                ui.label(
                    RichText::new(
                        "Signs in with the browser, through an application registered on the server. Register one once (Make an app…): give it any name and redirect URI, untick Confidential, tick the api scope, and paste its Application ID here.",
                    )
                    .color(theme::muted()),
                );
                ui.add_space(4.0);
                let ready_server = s.server.trim().len() > "https://".len();
                if ui
                    .add_enabled(ready_server, egui::Button::new("Make an app…"))
                    .clicked()
                {
                    action = Some(Action::Open(s.apps_page()));
                }
                ui.add_space(4.0);
                let id = ui.add(
                    egui::TextEdit::singleline(&mut s.app_id)
                        .hint_text("Application ID")
                        .desired_width(f32::INFINITY),
                );
                ui.ctx()
                    .accesskit_node_builder(id.id, |n| n.set_label("Application ID"));
            }
            _ if s.browser().is_some() => {
                ui.label(
                    RichText::new(format!(
                        "Signs in with your {} account in the browser.",
                        s.forge().0.name()
                    ))
                    .color(theme::muted()),
                );
            }
            _ => {
                let (kind, _) = s.forge();
                ui.label(
                    RichText::new(format!(
                        "Make a token on {} (the page opens with the permissions it needs), and paste it here. It's kept in your system's keychain.",
                        kind.name()
                    ))
                    .color(theme::muted()),
                );
                ui.add_space(4.0);
                let ready_server = s.place != Place::SelfManaged || s.server.trim().len() > "https://".len();
                if ui
                    .add_enabled(ready_server, egui::Button::new("Make a token…"))
                    .clicked()
                {
                    action = Some(Action::Open(s.token_page()));
                }
                ui.add_space(4.0);
                let token = ui.add(
                    egui::TextEdit::singleline(&mut s.token)
                        .password(true)
                        .hint_text("Token")
                        .desired_width(f32::INFINITY),
                );
                ui.ctx()
                    .accesskit_node_builder(token.id, |n| n.set_label("Access token"));
            }
        }
        if let Stage::Failed(why) = &s.stage {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Not signed in: {why}.")).color(theme::danger()));
        }
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let browser = s.browser();
            let (label, ready) = if let Some(id) = &browser {
                ("Sign in with the browser", !busy && !id.is_empty())
            } else {
                ("Sign in", !busy && !s.token.trim().is_empty())
            };
            let go = egui::Button::new(
                RichText::new(label)
                    .color(if ready { theme::on_accent() } else { theme::muted() })
                    .strong(),
            )
            .fill(if ready { theme::accent() } else { theme::bg_hover() });
            if ui.add_enabled(ready, go).clicked() {
                action = Some(if browser.is_some() {
                    Action::Browser
                } else {
                    Action::Token
                });
            }
            if busy && !matches!(s.stage, Stage::Approving(_)) {
                ui.spinner();
            }
            if ui.button("Cancel").clicked() {
                action = Some(Action::Close);
            }
            // The other way, where both are possible.
            if s.has_both() && !busy {
                let other = if s.browser().is_some() {
                    "Use a token instead"
                } else {
                    "Use the browser instead"
                };
                if ui.link(other).clicked() {
                    if s.place == Place::SelfManaged {
                        s.with_app = !s.with_app;
                    } else {
                        s.with_token = !s.with_token;
                    }
                }
            }
        });
    });
    if modal.should_close() && action.is_none() {
        action = Some(Action::Close);
    }
    action
}
