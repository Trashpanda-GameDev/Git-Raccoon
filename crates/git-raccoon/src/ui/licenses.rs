//! The licenses: Git Raccoon's own, and those of everything it's built from.

use std::sync::OnceLock;

use eframe::egui::{self, RichText};

use crate::theme;
use crate::ui::widgets;

/// Which licenses are shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    GitRaccoon,
    Libraries,
    Syntaxes,
}

const NOTICES: &str = include_str!("../../assets/THIRD-PARTY-NOTICES.txt");
const MIT: &str = include_str!("../../../../LICENSE-MIT");
const APACHE: &str = include_str!("../../../../LICENSE-APACHE");

fn own() -> &'static [String] {
    static LINES: OnceLock<Vec<String>> = OnceLock::new();
    LINES.get_or_init(|| {
        let text = format!(
            "Git Raccoon {} is open source, licensed under either of the MIT license or the Apache License, Version 2.0, at your option.\n\nSource code: https://github.com/Trashpanda-GameDev/Git-Raccoon\n\n{MIT}\n\n{APACHE}",
            env!("CARGO_PKG_VERSION")
        );
        text.lines().map(str::to_string).collect()
    })
}

fn libraries() -> &'static [String] {
    static LINES: OnceLock<Vec<String>> = OnceLock::new();
    LINES.get_or_init(|| NOTICES.lines().map(str::to_string).collect())
}

fn syntaxes() -> &'static [String] {
    static LINES: OnceLock<Vec<String>> = OnceLock::new();
    LINES.get_or_init(|| {
        let mut text = String::from(
            "Syntax highlighting uses language definitions and color themes collected by the two-face crate (from the bat project). These are the ones whose licenses ask to be credited:\n\n",
        );
        text.push_str(&two_face::acknowledgement::listing().to_md());
        text.lines().map(str::to_string).collect()
    })
}

/// Shows the licenses window. Returns `false` once it's closed.
pub fn show(ctx: &egui::Context, tab: &mut Tab) -> bool {
    let mut open = true;
    let modal = egui::Modal::new(egui::Id::new("licenses")).show(ctx, |ui| {
        ui.set_width(640.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Licenses").strong().size(15.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, egui_phosphor::regular::X, "Close licenses").clicked() {
                    open = false;
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            *tab = widgets::segmented(
                ui,
                *tab,
                &[
                    (Tab::GitRaccoon, "Git Raccoon"),
                    (Tab::Libraries, "Libraries"),
                    (Tab::Syntaxes, "Syntax themes"),
                ],
            );
        });
        ui.add_space(8.0);
        let lines = match tab {
            Tab::GitRaccoon => own(),
            Tab::Libraries => libraries(),
            Tab::Syntaxes => syntaxes(),
        };
        egui::Frame::new()
            .fill(theme::bg())
            .stroke(egui::Stroke::new(1.0, theme::border()))
            .corner_radius(4)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // Rows exactly as tall as a line, with no gaps: the list
                // works out which lines are visible from this.
                let font = egui::FontId::monospace(11.0);
                let row = ui.fonts_mut(|f| f.row_height(&font));
                ui.spacing_mut().item_spacing.y = 0.0;
                // Only the visible lines are laid out: the notices are long.
                egui::ScrollArea::both()
                    .id_salt(*tab as u8)
                    .max_height(420.0)
                    .auto_shrink([false, false])
                    .show_rows(ui, row, lines.len(), |ui, range| {
                        for line in &lines[range] {
                            ui.add(
                                egui::Label::new(RichText::new(line).font(font.clone())).extend(),
                            );
                        }
                    });
            });
    });
    if modal.should_close() {
        open = false;
    }
    open
}
