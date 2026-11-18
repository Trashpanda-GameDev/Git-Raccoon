//! The search box: a small panel over the top right of the history, while
//! searching (Ctrl+F, or the toolbar's Search).

use std::sync::Arc;

use eframe::egui::{self, Rect, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::search::SearchField;

use crate::repo_view::RepoView;
use crate::search::Search;
use crate::theme;
use crate::ui::widgets;

pub fn input_id() -> egui::Id {
    egui::Id::new("search-input")
}

/// How far below the top of the history the box sits: under the column
/// headings, so they can still be used.
const BELOW_HEADINGS: f32 = 32.0;
/// How far in from the history's right edge.
const MARGIN: f32 = 12.0;

/// Shows the search box over the top right of `list` (the history's area).
pub fn show(ctx: &egui::Context, list: Rect, repo: &mut RepoView) {
    let history = Arc::clone(&repo.history);
    // Which of the matches is selected, to show "3 of 40".
    let at = repo
        .selected_row()
        .and_then(|row| repo.search.rows.iter().position(|&r| r == row));
    let search = &mut repo.search;
    let mut step = None;
    egui::Area::new(egui::Id::new("search-box"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(egui::pos2(
            list.right() - MARGIN,
            list.top() + BELOW_HEADINGS,
        ))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(theme::bg_raised())
                .stroke(egui::Stroke::new(1.0, theme::border()))
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(8, 6))
                .shadow(widgets::POPUP_SHADOW)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(icon::MAGNIFYING_GLASS)
                                .color(theme::muted())
                                .size(15.0),
                        );
                        let hint = match search.field {
                            SearchField::Text => "Find commit",
                            SearchField::Path => "Find a changed file",
                        };
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut search.query)
                                .id(input_id())
                                .hint_text(hint)
                                .desired_width(200.0),
                        );
                        // A name for screen readers (the hint text isn't one).
                        ui.ctx().accesskit_node_builder(response.id, |node| {
                            node.set_label("Search commits");
                        });
                        // Just opened: ready to type in.
                        if std::mem::take(&mut search.focus) {
                            response.request_focus();
                        }
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
                                search.close();
                                response.surrender_focus();
                            } else if enter {
                                step = Some(!shift);
                                response.request_focus();
                            }
                        }

                        // Messages, authors and SHAs, or (toggled) changed files.
                        let paths = search.field == SearchField::Path;
                        let toggle = ui
                            .add(egui::Button::selectable(
                                paths,
                                RichText::new(icon::FILE).size(14.0),
                            ))
                            .on_hover_text("Search changed file paths instead");
                        ui.ctx().accesskit_node_builder(toggle.id, |node| {
                            node.set_label("Search file paths");
                        });
                        if toggle.clicked() {
                            search.field = if paths {
                                SearchField::Text
                            } else {
                                SearchField::Path
                            };
                            search.changed(ui.ctx(), &history);
                        }

                        let status = status(search, at);
                        if !status.is_empty() {
                            let label =
                                ui.label(RichText::new(status).size(12.0).color(theme::muted()));
                            if let Some(e) = &search.error {
                                label.on_hover_text(e);
                            }
                        }

                        ui.add_enabled_ui(!search.rows.is_empty(), |ui| {
                            if widgets::icon_button(
                                ui,
                                icon::ARROW_UP,
                                "Previous match (Shift+Enter)",
                            )
                            .clicked()
                            {
                                step = Some(false);
                            }
                            if widgets::icon_button(ui, icon::ARROW_DOWN, "Next match (Enter)")
                                .clicked()
                            {
                                step = Some(true);
                            }
                        });
                        if widgets::icon_button(ui, icon::X, "Close search (Esc)").clicked() {
                            search.close();
                        }
                    });
                });
        });
    if let Some(forward) = step {
        repo.step_match(forward);
    }
}

/// "3 of 40" (the selected match), "40 matches", "No matches", with how
/// far a running search has got; empty before anything is typed.
fn status(search: &Search, at: Option<usize>) -> String {
    if !search.is_active() {
        return String::new();
    }
    if search.error.is_some() {
        return "Search failed".to_string();
    }
    if search.rows.is_empty() && !search.running {
        return "No matches".to_string();
    }
    let count = format!(
        "{}{}",
        search.rows.len(),
        if search.truncated { "+" } else { "" }
    );
    let mut status = match at {
        Some(i) => format!("{} of {count}", i + 1),
        None if search.rows.len() == 1 => "1 match".to_string(),
        None => format!("{count} matches"),
    };
    if search.running && search.total > 0 {
        status += &format!(" · {}%", search.searched * 100 / search.total);
    }
    status
}
