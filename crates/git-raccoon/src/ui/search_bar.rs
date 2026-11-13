//! The search bar above the history.

use std::sync::Arc;

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::search::SearchField;

use crate::repo_view::RepoView;
use crate::theme;
use crate::ui::widgets;

pub fn input_id() -> egui::Id {
    egui::Id::new("search-input")
}

pub fn show(ui: &mut egui::Ui, repo: &mut RepoView) {
    let history = Arc::clone(&repo.history);
    let search = &mut repo.search;
    let mut step = None;
    ui.horizontal_centered(|ui| {
        ui.label(
            RichText::new(icon::MAGNIFYING_GLASS)
                .color(theme::MUTED)
                .size(15.0),
        );
        let hint = match search.field {
            SearchField::Text => "Search messages, authors, SHAs…  (Ctrl+F)",
            SearchField::Path => "Search changed file paths…",
        };
        let status_width = 190.0;
        let edit = egui::TextEdit::singleline(&mut search.query)
            .id(input_id())
            .hint_text(hint)
            .frame(egui::Frame::NONE)
            .desired_width((ui.available_width() - status_width - 220.0).max(80.0));
        let response = ui.add(edit);
        // A name for screen readers (the hint text isn't one).
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_label("Search commits"));
        if response.changed() {
            search.changed(ui.ctx(), &history);
        }
        if response.has_focus() || response.lost_focus() {
            let (enter, shift, escape) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.modifiers.shift,
                    i.key_pressed(egui::Key::Escape),
                )
            });
            if escape {
                search.clear();
                response.surrender_focus();
            } else if enter {
                step = Some(!shift);
                response.request_focus();
            }
        }

        let field = widgets::segmented(
            ui,
            search.field,
            &[(SearchField::Text, "Message"), (SearchField::Path, "File")],
        );
        if field != search.field {
            search.field = field;
            search.changed(ui.ctx(), &history);
        }

        if search.is_active() {
            let count = format!(
                "{}{}",
                search.rows.len(),
                if search.truncated { "+" } else { "" }
            );
            let mut status = if let Some(e) = &search.error {
                format!("Search failed: {e}")
            } else if search.rows.is_empty() && !search.running {
                "No matches".to_string()
            } else {
                let noun = if search.rows.len() == 1 {
                    "match"
                } else {
                    "matches"
                };
                format!("{count} {noun}")
            };
            if search.running && search.total > 0 {
                status += &format!(" · {}%", search.searched * 100 / search.total);
            }
            ui.label(RichText::new(status).size(12.0).color(theme::MUTED));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if search.is_active()
                && widgets::icon_button(ui, icon::X, "Clear search (Esc)").clicked()
            {
                search.clear();
            }
            ui.add_enabled_ui(!search.rows.is_empty(), |ui| {
                if widgets::icon_button(ui, icon::ARROW_DOWN, "Next match (Enter)").clicked() {
                    step = Some(true);
                }
                if widgets::icon_button(ui, icon::ARROW_UP, "Previous match (Shift+Enter)")
                    .clicked()
                {
                    step = Some(false);
                }
            });
        });
    });
    if let Some(forward) = step {
        repo.step_match(forward);
    }
}
