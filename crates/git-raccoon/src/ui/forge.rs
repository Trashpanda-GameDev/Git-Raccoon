//! A pull/merge request's or issue's details (the right-hand pane), and
//! the window for opening a new pull/merge request.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use crate::forge::Kind;
use crate::forge_view::{ForgeView, Opened};
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Open(String),
    Close,
}

fn heading(ui: &mut egui::Ui, text: &str, action: &mut Option<Action>) {
    ui.horizontal(|ui| {
        ui.set_height(36.0);
        ui.label(widgets::section_label(text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icon::X, "Close (Esc)").clicked() {
                *action = Some(Action::Close);
            }
        });
    });
}

fn body(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    if text.trim().is_empty() {
        ui.label(RichText::new("No description.").color(theme::muted()));
    } else {
        ui.label(RichText::new(text.trim()).size(13.0));
    }
}

/// Shows what's open from the forge.
pub fn show(ui: &mut egui::Ui, forge: &ForgeView, now: i64) -> Option<Action> {
    let mut action = None;
    let kind = forge.project.kind;
    let site = kind.name();
    match forge.open {
        Some(Opened::Issue(n)) => {
            heading(ui, &format!("Issue #{n}"), &mut action);
            let Some(i) = forge.issue(n) else {
                ui.label(RichText::new("It isn't open any more.").color(theme::muted()));
                return action;
            };
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(RichText::new(&i.title).strong().size(15.0));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "Opened by {} · {} · {} comments",
                            i.author,
                            crate::format::age(i.created, now),
                            i.comments
                        ))
                        .color(theme::muted()),
                    );
                    if !i.labels.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for l in &i.labels {
                                ui.label(RichText::new(l).size(11.0).color(theme::accent()));
                            }
                        });
                    }
                    ui.add_space(8.0);
                    if ui
                        .button(format!("{}  Open on {site}", icon::ARROW_SQUARE_OUT))
                        .clicked()
                    {
                        action = Some(Action::Open(i.url.clone()));
                    }
                    ui.separator();
                    body(ui, &i.body);
                });
        }
        // Requests have a view of their own, in place of the graph.
        Some(Opened::Request(_)) | None => {}
    }
    action
}

/// The window for opening a new pull/merge request.
pub struct NewForm {
    pub kind: Kind,
    pub source: String,
    pub target: String,
    /// The remote's branches it can go into (the default one first).
    pub targets: Vec<String>,
    pub title: String,
    pub body: String,
    pub draft: bool,
    /// The branch isn't on the remote as it is here (push it first).
    pub unpushed: bool,
    pub sending: bool,
    pub error: Option<String>,
}

pub enum NewAction {
    Push,
    Create,
    Close,
}

pub fn new_request(ctx: &egui::Context, f: &mut NewForm) -> Option<NewAction> {
    let mut action = None;
    let modal = egui::Modal::new(egui::Id::new("new-request")).show(ctx, |ui| {
        ui.set_width(480.0);
        ui.label(
            RichText::new(format!("New {}", f.kind.request().to_lowercase()))
                .strong()
                .size(16.0),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}  into", f.source)).color(theme::muted()));
            if f.targets.is_empty() {
                // No branches known for the remote (not fetched yet): type one.
                let target = ui.add(egui::TextEdit::singleline(&mut f.target).desired_width(200.0));
                ui.ctx()
                    .accesskit_node_builder(target.id, |n| n.set_label("Target branch"));
            } else {
                let combo = egui::ComboBox::from_id_salt("request-target")
                    .selected_text(f.target.as_str())
                    .width(220.0)
                    .height(320.0)
                    .show_ui(ui, |ui| {
                        for t in &f.targets {
                            ui.selectable_value(&mut f.target, t.clone(), t);
                        }
                    });
                ui.ctx()
                    .accesskit_node_builder(combo.response.id, |n| n.set_label("Target branch"));
            }
        });
        ui.add_space(6.0);
        let title = ui.add(
            egui::TextEdit::singleline(&mut f.title)
                .hint_text("Title")
                .desired_width(f32::INFINITY),
        );
        ui.ctx()
            .accesskit_node_builder(title.id, |n| n.set_label("Request title"));
        ui.add_space(4.0);
        let body = ui.add(
            egui::TextEdit::multiline(&mut f.body)
                .hint_text("What it changes, and why")
                .desired_rows(6)
                .desired_width(f32::INFINITY),
        );
        ui.ctx()
            .accesskit_node_builder(body.id, |n| n.set_label("Request description"));
        ui.checkbox(&mut f.draft, "Draft (not ready for review yet)");
        if f.unpushed {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "{} has commits the remote doesn't have yet.",
                        f.source
                    ))
                    .color(theme::modified()),
                );
                if ui.button("Push").clicked() {
                    action = Some(NewAction::Push);
                }
            });
        }
        if let Some(e) = &f.error {
            ui.add_space(6.0);
            ui.label(RichText::new(format!("It wasn't opened: {e}.")).color(theme::danger()));
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let ready = !f.unpushed
                && !f.sending
                && !f.title.trim().is_empty()
                && !f.target.trim().is_empty();
            let label = if f.sending { "Opening…" } else { "Open it" };
            let go = egui::Button::new(
                RichText::new(label)
                    .color(if ready {
                        theme::on_accent()
                    } else {
                        theme::muted()
                    })
                    .strong(),
            )
            .fill(if ready {
                theme::accent_fill()
            } else {
                theme::bg_hover()
            });
            if ui.add_enabled(ready, go).clicked() {
                action = Some(NewAction::Create);
            }
            if f.sending {
                ui.spinner();
            }
            if ui.button("Cancel").clicked() {
                action = Some(NewAction::Close);
            }
        });
    });
    if modal.should_close() && action.is_none() {
        action = Some(NewAction::Close);
    }
    action
}
