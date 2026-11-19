//! The dialog after a crash: what happened, the report itself, and the
//! choice of sending it, copying it or throwing it away.

use eframe::egui::{self, RichText};

use crate::crash::Report;
use crate::theme;

pub enum Action {
    Send,
    Copy,
    Discard,
}

pub fn show(ctx: &egui::Context, report: &Report, more: usize) -> Option<Action> {
    let mut action = None;
    egui::Modal::new(egui::Id::new("crash")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.label(RichText::new("Git Raccoon ran into a problem").strong().size(15.0));
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "It wrote this report, which hasn't been sent anywhere. Sending it helps fix the bug: it opens a GitHub issue you can read and add to before posting. It holds the version, the system and where in the code it happened; check it for anything you'd rather not share.",
            )
            .color(theme::muted()),
        );
        ui.add_space(8.0);
        egui::Frame::new()
            .fill(theme::bg())
            .stroke(egui::Stroke::new(1.0, theme::border()))
            .corner_radius(4)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::both().max_height(220.0).show(ui, |ui| {
                    let text = ui.label(RichText::new(&report.text).monospace().size(11.0));
                    ui.ctx()
                        .accesskit_node_builder(text.id, |n| n.set_label("Crash report"));
                });
            });
        if more > 0 {
            ui.label(
                RichText::new(format!("{more} more after this one."))
                    .size(12.0)
                    .color(theme::muted()),
            );
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            let send = egui::Button::new(
                RichText::new("Report on GitHub…").color(theme::on_accent()).strong(),
            )
            .fill(theme::accent_fill());
            if ui.add(send).clicked() {
                action = Some(Action::Send);
            }
            if ui.button("Copy report").clicked() {
                action = Some(Action::Copy);
            }
            if ui.button("Don't send").clicked() {
                action = Some(Action::Discard);
            }
        });
    });
    action
}
