//! The settings window.

use eframe::egui::{self, RichText};

use crate::settings::{Settings, ThemeChoice, ZOOMS};
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Close,
    ClearRecent,
}

pub fn show(ctx: &egui::Context, settings: &mut Settings) -> Option<Action> {
    let mut action = None;
    let modal = egui::Modal::new(egui::Id::new("settings")).show(ctx, |ui| {
        ui.set_width(440.0);
        ui.label(RichText::new("Settings").strong().size(15.0));
        ui.add_space(10.0);
        let row = |ui: &mut egui::Ui, name: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            ui.horizontal(|ui| {
                ui.allocate_ui(egui::vec2(150.0, 24.0), |ui| {
                    ui.label(RichText::new(name).color(theme::muted()));
                });
                add(ui);
            });
            ui.add_space(6.0);
        };
        row(ui, "Theme", &mut |ui| {
            settings.theme = widgets::segmented(
                ui,
                settings.theme,
                &[
                    (ThemeChoice::System, "System"),
                    (ThemeChoice::Dark, "Dark"),
                    (ThemeChoice::Light, "Light"),
                ],
            );
        });
        row(ui, "Interface size", &mut |ui| {
            let current = settings.zoom.unwrap_or(1.0);
            let options: Vec<(u32, String)> = ZOOMS
                .iter()
                .map(|z| {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "a small percentage"
                    )]
                    let percent = (z * 100.0).round() as u32;
                    (percent, format!("{percent}%"))
                })
                .collect();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a small percentage"
            )]
            let now = (current * 100.0).round() as u32;
            let labels: Vec<(u32, &str)> = options.iter().map(|(p, l)| (*p, l.as_str())).collect();
            let chosen = widgets::segmented(ui, now, &labels);
            if chosen != now {
                #[expect(clippy::cast_precision_loss, reason = "a small percentage")]
                let z = chosen as f32 / 100.0;
                settings.zoom = Some(z).filter(|z| (z - 1.0).abs() > f32::EPSILON);
            }
        });
        row(ui, "Diffs", &mut |ui| {
            settings.split_diff = widgets::segmented(
                ui,
                settings.split_diff,
                &[(false, "Unified"), (true, "Side by side")],
            );
        });
        row(ui, "Sidebar", &mut |ui| {
            let mut shown = !settings.hide_sidebar;
            ui.checkbox(&mut shown, "Show the sidebar");
            settings.hide_sidebar = !shown;
        });
        row(ui, "Recent repositories", &mut |ui| {
            let n = settings.recent.len();
            if ui
                .add_enabled(n > 0, egui::Button::new(format!("Clear the list ({n})")))
                .clicked()
            {
                action = Some(Action::ClearRecent);
            }
        });
        ui.add_space(6.0);
        ui.label(
            RichText::new("Changes apply straight away and are remembered.")
                .size(12.0)
                .color(theme::muted()),
        );
        ui.add_space(8.0);
        if ui.button("Done").clicked() {
            action = Some(Action::Close);
        }
    });
    if modal.should_close() && action.is_none() {
        action = Some(Action::Close);
    }
    action
}
