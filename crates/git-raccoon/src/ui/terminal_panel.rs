//! The built-in terminal's panel, under the graph: a repository tab's
//! terminals, each in its own small tab, and what they're for.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use crate::term::{Start, Terminal};
use crate::theme;
use crate::ui::widgets;

/// A repository tab's terminals.
#[derive(Default)]
pub struct Terminals {
    pub list: Vec<(u64, Terminal)>,
    pub active: usize,
    /// The panel is showing.
    pub shown: bool,
    /// It takes the whole of the area under the toolbar.
    pub full: bool,
    /// The active one has the keyboard (as of the last frame).
    pub focused: bool,
    /// Give it the keyboard on the next frame.
    pub focus_next: bool,
    next: u64,
    /// Why the last one couldn't start.
    pub error: Option<String>,
}

impl Terminals {
    /// Starts another terminal, and makes it the one shown.
    pub fn open(&mut self, ctx: &egui::Context, start: &Start) {
        self.next += 1;
        match Terminal::start(ctx, start, self.next) {
            Ok(t) => {
                self.list.push((self.next, t));
                self.active = self.list.len() - 1;
                self.error = None;
                self.focus_next = true;
            }
            Err(e) => self.error = Some(e),
        }
        self.shown = true;
    }

    /// Ctrl+`: shows the panel (starting a terminal if there's none) and
    /// gives it the keyboard; with the keyboard already in it, hides it.
    pub fn toggle(&mut self, ctx: &egui::Context, start: &Start) {
        if self.shown && self.focused {
            self.shown = false;
            self.focused = false;
            return;
        }
        self.shown = true;
        self.focus_next = true;
        if self.list.is_empty() {
            self.open(ctx, start);
        }
    }

    /// The terminal in front, if any.
    pub fn current(&mut self) -> Option<&mut Terminal> {
        let i = self.active.min(self.list.len().checked_sub(1)?);
        self.list.get_mut(i).map(|(_, t)| t)
    }
}

/// What the panel asks for.
pub enum Action {
    /// Another terminal, in this tab.
    New,
    /// Start one again (the shell ended).
    Restart(usize),
    /// A word in the output was clicked (a SHA, a file, a link).
    Clicked(String),
}

/// Draws the panel along the bottom of `ui`: its tabs and header, then the
/// terminal in front. `who` says who it commits and signs in as.
pub fn show(ui: &mut egui::Ui, terms: &mut Terminals, folder: &str, who: &str) -> Option<Action> {
    let mut action = None;
    let full = terms.full;
    let mut panel = egui::Panel::bottom("terminal-panel")
        .resizable(!full)
        .default_size(280.0)
        .size_range(120.0..=f32::INFINITY)
        .frame(
            egui::Frame::new()
                .fill(theme::bg())
                .stroke(egui::Stroke::new(1.0, theme::border())),
        );
    if full {
        let h = ui.available_height();
        panel = panel.exact_size(h);
    }
    panel.show(ui, |ui| {
        // Its tabs, and what it's for.
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 3))
            .fill(theme::bg_raised())
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let mut close = None;
                    for (i, (_, t)) in terms.list.iter().enumerate() {
                        let on = i == terms.active;
                        let name = if t.ended.is_some() {
                            format!("{} (ended)", t.title)
                        } else {
                            t.title.clone()
                        };
                        let tab = ui.selectable_label(on, RichText::new(&name).size(12.0));
                        let label = format!("Terminal: {name}");
                        ui.ctx()
                            .accesskit_node_builder(tab.id, |n| n.set_label(label));
                        if tab.clicked() {
                            terms.active = i;
                            terms.focus_next = true;
                        }
                        if tab.middle_clicked() {
                            close = Some(i);
                        }
                        tab.context_menu(|ui| {
                            if ui.button("Close").clicked() {
                                close = Some(i);
                                ui.close();
                            }
                        });
                    }
                    if let Some(i) = close {
                        terms.list.remove(i);
                        terms.active = terms.active.min(terms.list.len().saturating_sub(1));
                        if terms.list.is_empty() {
                            terms.shown = false;
                        }
                    }
                    let add = widgets::icon_button(ui, icon::PLUS, "Another terminal");
                    if add.clicked() {
                        action = Some(Action::New);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if widgets::icon_button(ui, icon::X, "Hide the terminal (Ctrl+`)").clicked()
                        {
                            terms.shown = false;
                        }
                        let (glyph, tip) = if full {
                            (icon::ARROWS_IN_SIMPLE, "Back under the graph")
                        } else {
                            (icon::ARROWS_OUT_SIMPLE, "Take the whole window")
                        };
                        if widgets::icon_button(ui, glyph, tip).clicked() {
                            terms.full = !full;
                        }
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("{folder} · {who}"))
                                    .size(11.0)
                                    .color(theme::muted()),
                            )
                            .truncate(),
                        );
                    });
                });
            });
        if let Some(e) = &terms.error {
            ui.label(RichText::new(e).color(theme::danger()));
        }
        let focus_next = std::mem::take(&mut terms.focus_next);
        let active = terms.active;
        let Some((key, t)) = terms.list.get_mut(active) else {
            terms.focused = false;
            return;
        };
        let id = egui::Id::new(("terminal", *key));
        if focus_next {
            ui.memory_mut(|m| m.request_focus(id));
        }
        if let Some(code) = t.ended {
            // Said under what it printed, with a way to start again.
            ui.horizontal(|ui| {
                ui.add_space(8.0);
                let how = match code {
                    Some(0) | None => "The shell ended.".to_string(),
                    Some(c) => format!("The shell ended ({c})."),
                };
                ui.label(RichText::new(how).size(12.0).color(theme::muted()));
                if ui
                    .link(RichText::new("Start it again").size(12.0))
                    .clicked()
                {
                    action = Some(Action::Restart(active));
                }
            });
        }
        let shown = crate::term::view::show(ui, t, id);
        terms.focused = ui.memory(|m| m.has_focus(id));
        if let Some(word) = shown.clicked_word {
            action = Some(Action::Clicked(word));
        }
    });
    action
}
