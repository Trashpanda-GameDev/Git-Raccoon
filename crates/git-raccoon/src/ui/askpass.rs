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
        // A host SSH hasn't seen: checked against the keys its owner
        // publishes, where the app knows them.
        if let Some(q) = git_raccoon_core::ssh::host_question(&prompt.request.prompt) {
            answer = host_trust(ui, &q);
            return;
        }
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

/// "Trust this host?", with what the app knows about its key.
fn host_trust(
    ui: &mut egui::Ui,
    q: &git_raccoon_core::ssh::HostQuestion,
) -> Option<Option<String>> {
    use git_raccoon_core::ssh::{HostCheck, check_host_key};
    let mut answer = None;
    let check = check_host_key(&q.host, &q.fingerprint);
    ui.label(
        RichText::new(format!("First connection to {}", q.host))
            .strong()
            .size(15.0),
    );
    ui.add_space(4.0);
    let who = match &check {
        HostCheck::Published(f) | HostCheck::NotPublished(f) => (*f).to_string(),
        HostCheck::Unknown => q.host.clone(),
    };
    ui.label(
        RichText::new(format!(
            "SSH hasn't connected to this host before, so it checks it's really {who}."
        ))
        .color(theme::muted()),
    );
    ui.add_space(10.0);
    let (colour, title, more) = match &check {
        HostCheck::Published(f) => (
            theme::add(),
            format!(
                "{}  This is {f}'s published key",
                egui_phosphor::regular::CHECK
            ),
            None,
        ),
        HostCheck::NotPublished(f) => (
            theme::danger(),
            format!(
                "{}  This is not a key {f} publishes",
                egui_phosphor::regular::WARNING
            ),
            Some(format!(
                "Something between you and {f} may be pretending to be it (a network you don't trust, say). Don't trust it: cancel, and try from another network."
            )),
        ),
        HostCheck::Unknown => (
            theme::border(),
            "Compare this fingerprint with the one the host publishes".to_string(),
            Some(format!(
                "Git Raccoon doesn't know {}'s keys. Its documentation, or whoever runs it, can tell you its fingerprint; trust it only if they match.",
                q.host
            )),
        ),
    };
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, colour))
        .corner_radius(6)
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let strong = if matches!(check, HostCheck::Unknown) {
                theme::text()
            } else {
                colour
            };
            ui.label(RichText::new(title).strong().color(strong));
            ui.label(
                RichText::new(format!("{} · {}", q.kind, q.fingerprint))
                    .monospace()
                    .size(12.0)
                    .color(theme::muted()),
            );
            if let Some(m) = more {
                ui.add_space(4.0);
                ui.label(RichText::new(m).size(12.0).color(
                    if matches!(check, HostCheck::NotPublished(_)) {
                        theme::danger()
                    } else {
                        theme::muted()
                    },
                ));
            }
        });
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        let trust = format!("Trust {}", q.host);
        let b = if matches!(check, HostCheck::NotPublished(_)) {
            ui.button(format!("{trust} anyway"))
        } else {
            ui.add(primary(&trust))
        };
        if b.clicked() {
            answer = Some(Some("yes".to_string()));
        }
        if ui.button("Cancel").clicked() {
            answer = Some(Some("no".to_string()));
        }
    });
    answer
}

fn primary(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(theme::on_accent()).strong())
        .fill(theme::accent_fill())
}
