//! The command palette: type to find any action, arrows to choose, Enter
//! to run. Each command shows its shortcut, so it doubles as the list of
//! keyboard shortcuts.

use eframe::egui::{self, Key, Modifiers, RichText};
use egui_phosphor::regular as icon;

use crate::commands::{self, Command, Entry};
use crate::theme;

/// What's typed and which line is chosen.
#[derive(Default)]
pub struct State {
    pub query: String,
    pub selected: usize,
}

pub enum Outcome {
    Run(Command),
    Close,
}

/// How many lines show at once.
const LINES: usize = 12;
const LINE: f32 = 30.0;

pub fn show(ctx: &egui::Context, state: &mut State, entries: &[Entry]) -> Option<Outcome> {
    let mut outcome = None;
    let matches = commands::filter(entries, &state.query);
    state.selected = state.selected.min(matches.len().saturating_sub(1));

    // The list's keys, taken before the text field sees them.
    let (down, up, enter) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::Enter),
        )
    });
    if down && state.selected + 1 < matches.len() {
        state.selected += 1;
    }
    if up {
        state.selected = state.selected.saturating_sub(1);
    }
    if enter
        && let Some(e) = matches.get(state.selected)
        && e.enabled
    {
        outcome = Some(Outcome::Run(e.command.clone()));
    }

    let id = egui::Id::new("palette");
    let area =
        egui::Modal::default_area(id).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 80.0));
    let modal = egui::Modal::new(id).area(area).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(theme::MUTED));
            let field = ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .hint_text("Type a command or a branch name…")
                    .frame(egui::Frame::NONE)
                    .desired_width(f32::INFINITY),
            );
            ui.ctx()
                .accesskit_node_builder(field.id, |n| n.set_label("Command"));
            field.request_focus();
            if field.changed() {
                state.selected = 0;
            }
        });
        ui.separator();
        if matches.is_empty() {
            ui.label(RichText::new("No matching commands").color(theme::MUTED));
            return;
        }
        egui::ScrollArea::vertical()
            .max_height(LINE * LINES as f32)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, e) in matches.iter().enumerate() {
                    let chosen = i == state.selected;
                    let (rect, response) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), LINE),
                        egui::Sense::click(),
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, e.enabled, &e.title)
                    });
                    if chosen {
                        ui.painter().rect_filled(rect, 4, theme::BG_SELECTED);
                        response.scroll_to_me(None);
                    } else if response.hovered() {
                        ui.painter().rect_filled(rect, 4, theme::BG_HOVER);
                    }
                    let text_colour = if e.enabled { theme::TEXT } else { theme::MUTED };
                    crate::ui::widgets::text_in(
                        ui.painter(),
                        rect.shrink2(egui::vec2(10.0, 0.0)),
                        &e.title,
                        theme::body(13.0),
                        text_colour,
                    );
                    if let Some(s) = commands::shortcut(&e.command) {
                        ui.painter().text(
                            rect.right_center() - egui::vec2(10.0, 0.0),
                            egui::Align2::RIGHT_CENTER,
                            ui.ctx().format_shortcut(&s),
                            theme::body(12.0),
                            theme::MUTED,
                        );
                    }
                    if response.clicked() && e.enabled {
                        outcome = Some(Outcome::Run(e.command.clone()));
                    }
                }
            });
    });
    if outcome.is_none() && modal.should_close() {
        outcome = Some(Outcome::Close);
    }
    outcome
}
