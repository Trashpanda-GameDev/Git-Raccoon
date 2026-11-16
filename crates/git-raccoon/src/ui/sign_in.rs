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
    pub stage: Stage,
}

impl Default for SignIn {
    fn default() -> Self {
        SignIn {
            place: Place::GitHub,
            server: "https://".into(),
            token: String::new(),
            with_token: false,
            stage: Stage::Choosing,
        }
    }
}

impl SignIn {
    /// Signing in to a project's forge.
    pub fn for_project(project: &crate::forge::Project) -> Self {
        let place = match project.base.as_str() {
            "https://github.com" => Place::GitHub,
            "https://gitlab.com" => Place::GitLab,
            _ => Place::SelfManaged,
        };
        SignIn {
            place,
            server: project.base.clone(),
            ..SignIn::default()
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

    /// The browser can be used here (an OAuth app is set up for it).
    pub fn browser(&self) -> Option<&'static str> {
        let (kind, base) = self.forge();
        oauth::client_id(kind, &base).filter(|_| !self.with_token)
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
            let browser = s.browser().is_some();
            let (label, ready) = if browser {
                ("Sign in with the browser", !busy)
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
                action = Some(if browser { Action::Browser } else { Action::Token });
            }
            if busy && !matches!(s.stage, Stage::Approving(_)) {
                ui.spinner();
            }
            if ui.button("Cancel").clicked() {
                action = Some(Action::Close);
            }
            // The other way, where both are possible.
            let (kind, base) = s.forge();
            if oauth::client_id(kind, &base).is_some() && !busy {
                let other = if s.with_token {
                    "Use the browser instead"
                } else {
                    "Use a token instead"
                };
                if ui.link(other).clicked() {
                    s.with_token = !s.with_token;
                }
            }
        });
    });
    if modal.should_close() && action.is_none() {
        action = Some(Action::Close);
    }
    action
}
