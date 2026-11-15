//! Resolving a conflicted file: each conflict with its two versions and a
//! choice of ours, theirs or both; whole-file choices; or editing the text.

use eframe::egui::{self, Color32, RichText, Stroke};
use egui_phosphor::regular as icon;
use git_raccoon_core::conflict::{self, Chunk, Pick};

use crate::repo_view::ConflictFile;
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Pick(usize, Pick),
    /// Take one side's whole file.
    TakeSide {
        ours: bool,
    },
    MarkResolved,
    StartEdit,
    SaveEdit,
    CancelEdit,
    Close,
}

const OURS: Color32 = Color32::from_rgb(0x3f, 0xb9, 0x8a);
const THEIRS: Color32 = Color32::from_rgb(0x5a, 0x9b, 0xf0);

pub fn show(ui: &mut egui::Ui, open: &mut ConflictFile, busy: bool) -> Option<Action> {
    let mut action = None;
    let chunks = open.text.as_ref().ok().and_then(|t| conflict::parse(t));
    let remaining = chunks.as_ref().map_or(0, |c| {
        c.iter().filter(|c| matches!(c, Chunk::Conflict(_))).count()
    });

    // Header: the file, what's left, and whole-file actions.
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(12, 0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(40.0);
                ui.label(RichText::new(icon::WARNING).color(theme::danger()));
                ui.label(RichText::new(&open.file).strong());
                let status = if open.edit.is_some() {
                    "editing".to_string()
                } else {
                    match remaining {
                        0 => "no conflicts left".to_string(),
                        1 => "1 conflict".to_string(),
                        n => format!("{n} conflicts"),
                    }
                };
                ui.label(RichText::new(status).size(12.0).color(theme::muted()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, icon::X, "Close (Esc)").clicked() {
                        action = Some(Action::Close);
                    }
                    ui.add_enabled_ui(!busy, |ui| {
                        if open.edit.is_some() {
                            if ui.button("Cancel").clicked() {
                                action = Some(Action::CancelEdit);
                            }
                            if ui.add(primary("Save")).clicked() {
                                action = Some(Action::SaveEdit);
                            }
                            return;
                        }
                        let resolved = ui
                            .add_enabled(
                                remaining == 0,
                                primary_if("Mark resolved", remaining == 0),
                            )
                            .on_disabled_hover_text("Resolve every conflict first")
                            .on_hover_text("Stage the file as it is now");
                        if resolved.clicked() {
                            action = Some(Action::MarkResolved);
                        }
                        if ui
                            .button(format!("{}  Edit", icon::PENCIL_SIMPLE))
                            .on_hover_text("Write the result yourself")
                            .clicked()
                        {
                            action = Some(Action::StartEdit);
                        }
                        if ui
                            .button("Use theirs for the file")
                            .on_hover_text("Take the incoming version of the whole file")
                            .clicked()
                        {
                            action = Some(Action::TakeSide { ours: false });
                        }
                        if ui
                            .button("Use ours for the file")
                            .on_hover_text("Keep the current branch's version of the whole file")
                            .clicked()
                        {
                            action = Some(Action::TakeSide { ours: true });
                        }
                    });
                });
            });
        });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );

    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    if let Some(edit) = &mut open.edit {
                        let editor = ui.add(
                            egui::TextEdit::multiline(edit)
                                .code_editor()
                                .desired_width(f32::INFINITY)
                                .desired_rows(30),
                        );
                        ui.ctx()
                            .accesskit_node_builder(editor.id, |n| n.set_label("Resolved text"));
                        return;
                    }
                    match (&open.text, &chunks) {
                        (Err(e), _) => {
                            ui.label(RichText::new(e).color(theme::danger()));
                        }
                        (Ok(_), None) => {
                            ui.label(
                                RichText::new(
                                    "No conflict markers in this file. It may be binary, or deleted on one side: pick a side for the whole file, or edit it and mark it resolved.",
                                )
                                .color(theme::muted()),
                            );
                        }
                        (Ok(_), Some(chunks)) => {
                            if let Some(a) = chunk_list(ui, chunks, busy) {
                                action = Some(a);
                            }
                        }
                    }
                });
        });
    action
}

fn chunk_list(ui: &mut egui::Ui, chunks: &[Chunk], busy: bool) -> Option<Action> {
    let mut action = None;
    let mut n = 0;
    for chunk in chunks {
        match chunk {
            Chunk::Same(text) => {
                // Long agreed stretches are trimmed to their edges.
                let lines: Vec<&str> = text.lines().collect();
                let shown: Vec<&str> = if lines.len() > 8 {
                    let mut v = lines[..3].to_vec();
                    v.push("⋯");
                    v.extend_from_slice(&lines[lines.len() - 3..]);
                    v
                } else {
                    lines
                };
                for line in shown {
                    ui.label(RichText::new(line).monospace().color(theme::muted()));
                }
            }
            Chunk::Conflict(c) => {
                let index = n;
                n += 1;
                egui::Frame::new()
                    .stroke(Stroke::new(1.0, theme::border()))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("Conflict {}", index + 1))
                                    .strong()
                                    .size(12.0),
                            );
                            ui.add_enabled_ui(!busy, |ui| {
                                let picks = [
                                    ("Use ours", Pick::Ours),
                                    ("Use theirs", Pick::Theirs),
                                    ("Use both", Pick::Both),
                                ];
                                for (label, pick) in picks {
                                    let b = ui.button(label);
                                    let name = format!("{label} for conflict {}", index + 1);
                                    ui.ctx().accesskit_node_builder(b.id, |n| n.set_label(name));
                                    if b.clicked() {
                                        action = Some(Action::Pick(index, pick));
                                    }
                                }
                            });
                        });
                        side(ui, &format!("Ours ({})", c.ours_label), &c.ours, OURS);
                        if let Some(base) = &c.base {
                            side(ui, "Before both changes", base, theme::muted());
                        }
                        side(
                            ui,
                            &format!("Theirs ({})", c.theirs_label),
                            &c.theirs,
                            THEIRS,
                        );
                    });
                ui.add_space(6.0);
            }
        }
    }
    action
}

fn side(ui: &mut egui::Ui, title: &str, text: &str, colour: Color32) {
    ui.add_space(4.0);
    ui.label(RichText::new(title).size(11.0).color(colour));
    egui::Frame::new()
        .fill(colour.gamma_multiply(0.12))
        .inner_margin(egui::Margin::symmetric(6, 4))
        .corner_radius(4)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if text.is_empty() {
                ui.label(RichText::new("(nothing)").italics().color(theme::muted()));
            }
            for line in text.lines() {
                ui.label(RichText::new(line).monospace().color(theme::text()));
            }
        });
}

/// A primary button, greyed out unless `enabled`.
fn primary_if(label: &str, enabled: bool) -> egui::Button<'_> {
    if enabled {
        primary(label)
    } else {
        egui::Button::new(RichText::new(label).color(theme::muted())).fill(theme::bg_hover())
    }
}

fn primary(label: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(theme::on_accent()).strong()).fill(theme::accent())
}
