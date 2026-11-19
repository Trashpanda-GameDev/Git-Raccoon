//! The feedback window: a bug report, feedback or an idea, continued on
//! GitHub as an issue.

use eframe::egui::{self, RichText};

use crate::feedback::{Kind, Route};
use crate::theme;
use crate::ui::widgets;

/// What's being written, and how sending it went.
pub struct Form {
    pub kind: Kind,
    pub title: String,
    pub details: String,
    /// How to reach them, when it's posted without their name.
    pub contact: String,
    pub status: Status,
    /// The window as it was when the form opened, and whether it goes too.
    pub screenshot: Option<std::sync::Arc<egui::ColorImage>>,
    pub attach: bool,
    preview: Option<egui::TextureHandle>,
}

impl Form {
    pub fn with_screenshot(screenshot: Option<std::sync::Arc<egui::ColorImage>>) -> Self {
        Form {
            screenshot,
            ..Form::default()
        }
    }

    /// The screenshot to send, if it's to go.
    pub fn attached(&self) -> Option<std::sync::Arc<egui::ColorImage>> {
        self.screenshot.clone().filter(|_| self.attach)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Writing,
    Sending,
    /// Posted, as this issue number.
    Sent(u64),
    Failed(String),
}

impl Default for Form {
    fn default() -> Self {
        Form {
            kind: Kind::Feedback,
            title: String::new(),
            details: String::new(),
            contact: String::new(),
            status: Status::Writing,
            screenshot: None,
            attach: false,
            preview: None,
        }
    }
}

pub enum Action {
    /// Post it (or, for builds that can't, open it on GitHub).
    Send,
    /// After a failure: open the prefilled issue on GitHub instead.
    OpenOnGitHub,
    Close,
}

pub fn show(ctx: &egui::Context, form: &mut Form, route: &Route) -> Option<Action> {
    let mut action = None;
    let posts = route.posts();
    let modal = egui::Modal::new(egui::Id::new("feedback")).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.label(RichText::new("Send feedback").strong().size(16.0));
        ui.add_space(4.0);
        if let Status::Sent(number) = form.status {
            ui.label(
                RichText::new(format!(
                    "Thanks! It's on the project board as issue #{number}."
                ))
                .color(theme::text()),
            );
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Done").clicked() {
                    action = Some(Action::Close);
                }
                ui.hyperlink_to(
                    "See it on GitHub",
                    format!("{}/{number}", crate::crash::ISSUES.trim_end_matches("/new")),
                );
            });
            return;
        }
        let about = match route {
            Route::AsYou(account) => format!(
                "Report a bug, say what you think, or suggest an idea. It's posted as an issue on Git Raccoon's GitHub page (publicly) as you, @{}, so you'll see replies there.",
                account.login
            ),
            Route::Bot => "Report a bug, say what you think, or suggest an idea. It's posted as an issue on Git Raccoon's GitHub page (publicly) by its feedback bot, without your name. Sign in to GitHub under Settings › Accounts to post as yourself and get replies.".to_string(),
            Route::OnGitHub => "Report a bug, say what you think, or suggest an idea. It continues on GitHub, where you can check it and post it (you'll need a GitHub account); it goes on the project's board.".to_string(),
        };
        ui.label(RichText::new(about).color(theme::muted()));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            form.kind = widgets::segmented(
                ui,
                form.kind,
                &[
                    (Kind::Bug, "Bug"),
                    (Kind::Feedback, "Feedback"),
                    (Kind::Idea, "Idea"),
                ],
            );
        });
        ui.add_space(8.0);
        let title = ui.add(
            egui::TextEdit::singleline(&mut form.title)
                .hint_text(form.kind.title_hint())
                .desired_width(f32::INFINITY),
        );
        ui.ctx()
            .accesskit_node_builder(title.id, |n| n.set_label("Feedback title"));
        ui.add_space(6.0);
        let details = ui.add(
            egui::TextEdit::multiline(&mut form.details)
                .hint_text(form.kind.details_hint())
                .desired_rows(6)
                .desired_width(f32::INFINITY),
        );
        ui.ctx()
            .accesskit_node_builder(details.id, |n| n.set_label("Feedback details"));
        if *route == Route::Bot {
            ui.add_space(6.0);
            let contact = ui.add(
                egui::TextEdit::singleline(&mut form.contact)
                    .hint_text("How to reach you, if you'd like a reply (optional; it's public)")
                    .desired_width(f32::INFINITY),
            );
            ui.ctx()
                .accesskit_node_builder(contact.id, |n| n.set_label("How to reach you"));
        }
        if form.kind == Kind::Bug {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("Sent with it: {}", crate::feedback::system()))
                    .size(11.0)
                    .color(theme::muted()),
            );
        }
        // A screenshot: sent along where this build can upload it, or
        // copied for pasting into the issue on GitHub.
        let can_send = route.attaches() || !posts;
        if let Some(shot) = form.screenshot.clone().filter(|_| can_send) {
            ui.add_space(6.0);
            let label = if posts {
                "Include a screenshot of the window"
            } else {
                "Copy a screenshot of the window, to paste into the issue"
            };
            ui.checkbox(&mut form.attach, label);
            if form.attach {
                let texture = form.preview.get_or_insert_with(|| {
                    ui.ctx().load_texture(
                        "feedback-screenshot",
                        crate::feedback::thumbnail(&shot, 1024),
                        egui::TextureOptions::LINEAR,
                    )
                });
                let [w, h] = shot.size;
                #[expect(clippy::cast_precision_loss, reason = "screen sizes")]
                let size = egui::vec2(240.0, 240.0 * h as f32 / (w as f32).max(1.0));
                ui.add(egui::Image::new((texture.id(), size)).corner_radius(4));
                ui.label(
                    RichText::new(
                        "It's public with the issue: check it shows nothing private (code, names, paths).",
                    )
                    .size(11.0)
                    .color(theme::muted()),
                );
            }
        }
        if let Status::Failed(why) = &form.status {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("It wasn't sent: {why}.")).color(theme::danger()),
            );
            if ui.button("Send it on GitHub instead").clicked() {
                action = Some(Action::OpenOnGitHub);
            }
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let sending = form.status == Status::Sending;
            let ready = !form.title.trim().is_empty() && !sending;
            let label = match (posts, sending) {
                (_, true) => "Sending…",
                (true, false) => "Send",
                (false, false) => "Continue on GitHub",
            };
            let send = egui::Button::new(
                RichText::new(label)
                    .color(if ready { theme::on_accent() } else { theme::muted() })
                    .strong(),
            )
            .fill(if ready { theme::accent_fill() } else { theme::bg_hover() });
            let send = ui.add_enabled(ready, send);
            if send.on_disabled_hover_text("Write a title first").clicked() {
                action = Some(Action::Send);
            }
            if sending {
                ui.spinner();
            }
            if ui.button("Cancel").clicked() {
                action = Some(Action::Close);
            }
        });
    });
    if modal.should_close() && action.is_none() {
        action = Some(Action::Close);
    }
    action
}
