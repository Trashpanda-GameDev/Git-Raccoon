//! The settings window.

use eframe::egui::{self, RichText};

use crate::settings::{Settings, ThemeChoice, ThemeMode, ZOOMS};
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Close,
    ClearRecent,
    Licenses,
    /// Sign in to a forge.
    AddAccount,
    /// Sign out of this account (by its place in the list).
    SignOut(usize),
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
        ui.label(RichText::new("Theme").color(theme::muted()));
        ui.add_space(4.0);
        let system = ctx.system_theme();
        // Every look comes dark and light: which, or as the system is.
        ui.horizontal(|ui| {
            settings.theme_mode = widgets::segmented(
                ui,
                settings.theme_mode,
                &[
                    (ThemeMode::Dark, "Dark"),
                    (ThemeMode::Light, "Light"),
                    (ThemeMode::System, "System"),
                ],
            );
        });
        ui.add_space(6.0);
        let mode = settings.theme_mode;
        egui::Grid::new("themes")
            .spacing(egui::vec2(8.0, 8.0))
            .show(ui, |ui| {
                for (i, choice) in ThemeChoice::ALL.into_iter().enumerate() {
                    if swatch(ui, choice, mode, system, settings.theme == choice).clicked() {
                        settings.theme = choice;
                    }
                    if i % 3 == 2 {
                        ui.end_row();
                    }
                }
            });
        ui.add_space(10.0);
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
                settings.set_zoom(z);
            }
            // Zoomed to a size between these (Ctrl+scroll, Ctrl+plus/minus).
            if !labels.iter().any(|(p, _)| *p == now) {
                ui.label(RichText::new(format!("{now}%")).color(theme::accent()));
            }
            ui.label(
                RichText::new("Ctrl+scroll or Ctrl+plus/minus; Ctrl+0 resets")
                    .size(11.0)
                    .color(theme::muted()),
            );
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
        row(ui, "Updates", &mut |ui| {
            let mut check = !settings.no_update_checks;
            ui.checkbox(&mut check, "Check for updates automatically");
            settings.no_update_checks = !check;
        });
        row(ui, "Accounts", &mut |ui| {
            ui.vertical(|ui| {
                for (i, a) in settings.accounts.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}: {}", a.place(), a.login));
                        if ui.small_button("Sign out").clicked() {
                            action = Some(Action::SignOut(i));
                        }
                    });
                }
                if ui.button("Add account…").clicked() {
                    action = Some(Action::AddAccount);
                }
            });
        });
        row(ui, "About", &mut |ui| {
            if ui.button("Licenses…").clicked() {
                action = Some(Action::Licenses);
            }
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
            RichText::new(format!("Git Raccoon {}", env!("CARGO_PKG_VERSION")))
                .size(12.0)
                .color(theme::muted()),
        );
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

/// A small preview of a look in the chosen mode (half dark, half light
/// when following the system), to pick it.
fn swatch(
    ui: &mut egui::Ui,
    choice: ThemeChoice,
    mode: ThemeMode,
    system: Option<egui::Theme>,
    chosen: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(136.0, 64.0), egui::Sense::click());
    let family = choice.family();
    let name = family.name().to_string();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, chosen, &name)
    });
    let look = crate::app::look_for(choice, mode, system);
    let (bg, raised, text, accent, lanes) = look.preview();
    let painter = ui.painter();
    painter.rect_filled(rect, 6, bg);
    if mode == ThemeMode::System {
        // The other half in the other version.
        let (other_bg, ..) = family.look(!look.light).preview();
        let (_, right) = rect.split_left_right_at_fraction(0.5);
        painter.rect_filled(
            right,
            egui::CornerRadius {
                nw: 0,
                sw: 0,
                ne: 6,
                se: 6,
            },
            other_bg,
        );
    }
    let strip = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 18.0));
    painter.rect_filled(
        strip,
        egui::CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        },
        raised,
    );
    for (i, c) in lanes.iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "four dots")]
        let x = rect.left() + 14.0 + i as f32 * 12.0;
        painter.circle_filled(egui::pos2(x, strip.center().y), 4.0, *c);
    }
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.right() - 34.0, strip.center().y - 4.0),
            egui::vec2(22.0, 8.0),
        ),
        3,
        accent,
    );
    let label = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 26.0), rect.max);
    if mode == ThemeMode::System {
        painter.rect_filled(label.shrink2(egui::vec2(24.0, 3.0)), 4, bg);
    }
    painter.text(
        label.center(),
        egui::Align2::CENTER_CENTER,
        &name,
        theme::body(13.0),
        text,
    );
    let stroke = if chosen {
        egui::Stroke::new(2.0, theme::accent())
    } else if response.hovered() {
        egui::Stroke::new(1.0, theme::muted())
    } else {
        egui::Stroke::new(1.0, theme::border())
    };
    painter.rect_stroke(rect, 6, stroke, egui::StrokeKind::Inside);
    response
}
