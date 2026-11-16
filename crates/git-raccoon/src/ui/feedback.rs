//! The feedback window: a bug report, feedback or an idea, continued on
//! GitHub as an issue.

use eframe::egui::{self, RichText};

use crate::feedback::Kind;
use crate::theme;
use crate::ui::widgets;

/// What's being written.
pub struct Form {
    pub kind: Kind,
    pub title: String,
    pub details: String,
}

impl Default for Form {
    fn default() -> Self {
        Form {
            kind: Kind::Feedback,
            title: String::new(),
            details: String::new(),
        }
    }
}

pub enum Action {
    /// Open the prefilled issue on GitHub.
    Send,
    Close,
}

pub fn show(ctx: &egui::Context, form: &mut Form) -> Option<Action> {
    let mut action = None;
    let modal = egui::Modal::new(egui::Id::new("feedback")).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.label(RichText::new("Send feedback").strong().size(16.0));
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Report a bug, say what you think, or suggest an idea. It continues on GitHub, where you can check it and post it (you'll need a GitHub account); it goes on the project's board.",
            )
            .color(theme::muted()),
        );
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
        if form.kind == Kind::Bug {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!("Sent with it: {}", crate::feedback::system()))
                    .size(11.0)
                    .color(theme::muted()),
            );
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let ready = !form.title.trim().is_empty();
            let send = egui::Button::new(
                RichText::new("Continue on GitHub")
                    .color(if ready { theme::on_accent() } else { theme::muted() })
                    .strong(),
            )
            .fill(if ready { theme::accent() } else { theme::bg_hover() });
            if ui
                .add_enabled(ready, send)
                .on_disabled_hover_text("Write a title first")
                .clicked()
            {
                action = Some(Action::Send);
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
