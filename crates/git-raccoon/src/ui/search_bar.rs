//! The search box: a small panel over the top right of the history, while
//! searching (Ctrl+F, or the toolbar's Search).

use std::sync::Arc;

use eframe::egui::{self, Rect, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::search::{self, SearchField};

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
/// Room for the status, "12 of 340 · 75%" or "Can't read the date".
const STATUS_WIDTH: f32 = 118.0;
/// The filters menu's width.
const MENU_WIDTH: f32 = 300.0;

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

                        // The filters ("author:", "after:"…), to add one.
                        if filters_button(ui, &mut search.query, input_id()) {
                            search.focus = true;
                        }

                        // In a slot of its own width, so the buttons stay
                        // put as it changes ("3 matches · 40%").
                        let status = status(search, at);
                        let slot = egui::vec2(STATUS_WIDTH, ui.spacing().interact_size.y);
                        ui.allocate_ui_with_layout(
                            slot,
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                ui.set_min_width(STATUS_WIDTH);
                                if !status.is_empty() {
                                    let colour = if search.error.is_some() {
                                        theme::danger()
                                    } else {
                                        theme::muted()
                                    };
                                    let shown = if status.chars().count() > 40 {
                                        "Can't read the date".to_string()
                                    } else {
                                        status
                                    };
                                    let label =
                                        ui.label(RichText::new(shown).size(12.0).color(colour));
                                    if let Some(e) = &search.error {
                                        label.on_hover_text(e);
                                    }
                                }
                            },
                        );

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

/// The funnel button listing the filters ("author:", "after:"…); the one
/// picked is added to `query`, typed in the field `input`. Returns whether
/// one was (so the field can take the focus again).
pub fn filters_button(ui: &mut egui::Ui, query: &mut String, input: egui::Id) -> bool {
    let mut added = false;
    let filters = ui
        .menu_button(RichText::new(icon::FUNNEL).size(14.0), |ui| {
            ui.label(
                RichText::new("Narrow the search (all must match)")
                    .size(12.0)
                    .color(theme::muted()),
            );
            ui.set_max_width(MENU_WIDTH);
            for (key, what) in search::FILTERS {
                let mut text = egui::text::LayoutJob::default();
                text.append(
                    key,
                    0.0,
                    egui::TextFormat::simple(theme::mono(13.0), theme::accent()),
                );
                text.append(
                    what,
                    8.0,
                    egui::TextFormat::simple(theme::body(13.0), theme::text()),
                );
                let item = ui.add(
                    egui::Button::new(text)
                        .frame_when_inactive(false)
                        .min_size(egui::vec2(MENU_WIDTH, 24.0)),
                );
                ui.ctx()
                    .accesskit_node_builder(item.id, |n| n.set_label(*key));
                if item.clicked() {
                    add_filter(ui.ctx(), query, key, input);
                    added = true;
                    ui.close();
                }
            }
            ui.label(
                RichText::new(
                    "Dates: 2026-11-23, 2026-11, 2026, today, yesterday, or 3d, 2w, 6m, \
                     1y ago. Quote words with spaces: author:\"Ada L\"",
                )
                .size(11.0)
                .color(theme::muted()),
            );
        })
        .response
        .on_hover_text("Filters: by author, committer, message, SHA, file or date");
    ui.ctx()
        .accesskit_node_builder(filters.id, |n| n.set_label("Search filters"));
    added
}

/// Adds `key` ("author:") to the end of `query`, with the cursor after it.
fn add_filter(ctx: &egui::Context, query: &mut String, key: &str, input: egui::Id) {
    if !query.is_empty() && !query.ends_with(' ') {
        query.push(' ');
    }
    query.push_str(key);
    if let Some(mut state) = egui::TextEdit::load_state(ctx, input) {
        let end = egui::text::CCursor::new(query.chars().count());
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(end)));
        state.store(ctx, input);
    }
}

/// "3 of 40" (the selected match), "40 matches", "No matches", with how
/// far a running search has got; empty before anything is typed.
fn status(search: &Search, at: Option<usize>) -> String {
    if !search.is_active() {
        return String::new();
    }
    if let Some(e) = &search.error {
        return if e.starts_with("Can't read") {
            e.clone()
        } else {
            "Search failed".to_string()
        };
    }
    if search.parsed().is_ok_and(|q| q.is_empty()) {
        // Only a filter's name so far.
        return String::new();
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
        _ if search.rows.is_empty() => "Searching".to_string(),
        Some(i) => format!("{} of {count}", i + 1),
        None if search.rows.len() == 1 => "1 match".to_string(),
        None => format!("{count} matches"),
    };
    if search.running && search.total > 0 {
        status += &format!(" · {}%", search.searched * 100 / search.total);
    }
    status
}
