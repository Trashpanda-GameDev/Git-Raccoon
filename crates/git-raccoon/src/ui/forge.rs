//! A pull/merge request's or issue's details (the right-hand pane), and
//! the window for opening a new pull/merge request.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use crate::forge::{Kind, Review};
use crate::forge_view::{ForgeView, Opened};
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    /// Check out a request's branch.
    CheckOut(u64),
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
        Some(Opened::Request(n)) => {
            heading(
                ui,
                &format!("{} {}", kind.request(), kind.number(n)),
                &mut action,
            );
            let Some(r) = forge.detailed(n).or_else(|| forge.request(n)) else {
                ui.label(RichText::new("It isn't open any more.").color(theme::muted()));
                return action;
            };
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&r.title).strong().size(15.0));
                        if r.draft {
                            ui.label(RichText::new("Draft").size(11.0).color(theme::muted()));
                        }
                    });
                    ui.add_space(4.0);
                    let fork = if r.from_fork { " (from a fork)" } else { "" };
                    ui.label(
                        RichText::new(format!(
                            "{} wants to merge {} into {}{fork} · {}",
                            r.author,
                            r.source,
                            r.target,
                            crate::format::age(r.created, now)
                        ))
                        .color(theme::muted()),
                    );
                    ui.add_space(8.0);
                    match forge.detailed(n) {
                        Some(d) => {
                            // Reviews.
                            let (glyph, colour, text) = match d.review {
                                Review::Approved => {
                                    (icon::CHECK_CIRCLE, theme::add(), "Approved".to_string())
                                }
                                Review::ChangesRequested => (
                                    icon::WARNING_CIRCLE,
                                    theme::modified(),
                                    "Changes requested".to_string(),
                                ),
                                Review::None => {
                                    (icon::CIRCLE, theme::muted(), "No reviews yet".to_string())
                                }
                            };
                            let who: Vec<&str> =
                                d.reviews.iter().map(|(w, _)| w.as_str()).collect();
                            let text = if who.is_empty() {
                                text
                            } else {
                                format!("{text}: {}", who.join(", "))
                            };
                            ui.label(RichText::new(format!("{glyph}  {text}")).color(colour));
                            // Checks.
                            let c = d.checks;
                            if c.is_empty() {
                                ui.label(
                                    RichText::new(format!("{}  No checks", icon::CIRCLE))
                                        .color(theme::muted()),
                                );
                            } else {
                                ui.horizontal(|ui| {
                                    if c.failed > 0 {
                                        ui.label(
                                            RichText::new(format!(
                                                "{}  {} failed",
                                                icon::X_CIRCLE,
                                                c.failed
                                            ))
                                            .color(theme::danger()),
                                        );
                                    }
                                    if c.pending > 0 {
                                        ui.label(
                                            RichText::new(format!(
                                                "{}  {} running",
                                                icon::CLOCK,
                                                c.pending
                                            ))
                                            .color(theme::modified()),
                                        );
                                    }
                                    if c.passed > 0 {
                                        ui.label(
                                            RichText::new(format!(
                                                "{}  {} passed",
                                                icon::CHECK_CIRCLE,
                                                c.passed
                                            ))
                                            .color(theme::add()),
                                        );
                                    }
                                });
                            }
                            ui.label(
                                RichText::new(format!(
                                    "{}  {} comments",
                                    icon::CHAT_CIRCLE,
                                    d.comments
                                ))
                                .color(theme::muted()),
                            );
                        }
                        None => match &forge.details {
                            Some((m, Err(e))) if *m == n => {
                                ui.label(RichText::new(e).color(theme::danger()));
                            }
                            _ => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(
                                        RichText::new("Reviews and checks…").color(theme::muted()),
                                    );
                                });
                            }
                        },
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        let out = ui.button(format!("{}  Check out", icon::SIGN_OUT));
                        let tip = if r.from_fork {
                            "Fetch it into a branch of its own, and check that out".to_string()
                        } else {
                            format!("Check out {} (and bring it up to the remote's)", r.source)
                        };
                        if out.on_hover_text(tip).clicked() {
                            action = Some(Action::CheckOut(n));
                        }
                        if ui
                            .button(format!("{}  Open on {site}", icon::ARROW_SQUARE_OUT))
                            .clicked()
                        {
                            action = Some(Action::Open(r.url.clone()));
                        }
                    });
                    ui.separator();
                    body(ui, &r.body);
                });
        }
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
        None => {}
    }
    action
}

/// The window for opening a new pull/merge request.
pub struct NewForm {
    pub kind: Kind,
    pub source: String,
    pub target: String,
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
            let target = ui.add(egui::TextEdit::singleline(&mut f.target).desired_width(160.0));
            ui.ctx()
                .accesskit_node_builder(target.id, |n| n.set_label("Target branch"));
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
