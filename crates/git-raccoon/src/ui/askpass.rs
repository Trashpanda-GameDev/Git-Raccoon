//! The dialog for Git's and SSH's questions: passwords, key passphrases,
//! usernames and "trust this host?".

use eframe::egui::{self, RichText};
use git_raccoon_core::askpass::{Kind, Request};

use crate::theme;

/// A question being shown, and what has been typed so far.
pub struct Prompt {
    pub request: Request,
    pub input: String,
}

/// Shows the question. Returns the answer once given: `Some(None)` means
/// cancelled.
pub fn show(ctx: &egui::Context, prompt: &mut Prompt) -> Option<Option<String>> {
    let mut answer = None;
    let kind = prompt.request.kind();
    let modal = egui::Modal::new(egui::Id::new("askpass")).show(ctx, |ui| {
        ui.set_width(440.0);
        let title = match kind {
            Kind::YesNo => "Connect to this host?",
            _ => "Git needs to sign in",
        };
        ui.label(RichText::new(title).strong().size(15.0));
        ui.add_space(6.0);
        ui.label(RichText::new(prompt.request.prompt.trim()).color(theme::muted()));
        ui.add_space(10.0);
        match kind {
            Kind::YesNo => {
                ui.label(
                    RichText::new(
                        "Only say yes if you expected to connect to this host for the first time. The fingerprint can be checked against the one the host publishes.",
                    )
                    .size(12.0)
                    .color(theme::muted()),
                );
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.add(primary("Yes, connect")).clicked() {
                        answer = Some(Some("yes".to_string()));
                    }
                    if ui.button("No").clicked() {
                        answer = Some(Some("no".to_string()));
                    }
                });
            }
            Kind::Secret | Kind::Text => {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut prompt.input)
                        .password(kind == Kind::Secret)
                        .desired_width(f32::INFINITY),
                );
                ui.ctx().accesskit_node_builder(field.id, |n| {
                    n.set_label(if kind == Kind::Secret {
                        "Secret answer"
                    } else {
                        "Answer"
                    });
                });
                if ui.ctx().memory(|m| m.focused().is_none()) {
                    field.request_focus();
                }
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if kind == Kind::Secret {
                    ui.label(
                        RichText::new(
                            "Sent only to Git. If you use a credential helper, it may remember it.",
                        )
                        .size(12.0)
                        .color(theme::muted()),
                    );
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.add(primary("OK")).clicked() || enter {
                        answer = Some(Some(std::mem::take(&mut prompt.input)));
                    }
                    if ui.button("Cancel").clicked() {
                        answer = Some(None);
                    }
                });
            }
        }
    });
    if modal.should_close() && answer.is_none() {
        answer = Some(None);
    }
    answer
}

fn primary(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(theme::on_accent()).strong()).fill(theme::accent())
}
