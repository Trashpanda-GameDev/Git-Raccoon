//! The conflict editor as panes: each side's whole file on top, ready to
//! tick (a conflict at a time, or line by line), and the file as it will
//! be saved below.

use std::collections::BTreeMap;

use eframe::egui::{self, Color32, RichText, Stroke};
use git_raccoon_core::conflict::{self, Chunk};

use super::conflict::{Action, BASE, Names, OURS, THEIRS, tag, ticked_result};
use crate::repo_view::ConflictFile;
use crate::theme;

/// The code's size, and a row's height: the same as the file and diff
/// views'.
const CODE: f32 = theme::MONO_SIZE;
const ROW: f32 = crate::ui::diff::LINE;

/// One row of a pane.
enum Row<'a> {
    /// A line both sides have.
    Same { no: usize, text: &'a str },
    /// A line of conflict `conflict` (its `line`th on this side).
    Side {
        no: usize,
        text: &'a str,
        conflict: usize,
        line: usize,
    },
    /// A conflict where this side has nothing.
    Nothing { conflict: usize },
}

/// One row of the output.
enum OutRow<'a> {
    Same {
        no: usize,
        text: &'a str,
    },
    /// A line of a conflict's chosen result.
    Chosen {
        no: usize,
        text: &'a str,
    },
    /// A conflict with nothing chosen yet.
    Open {
        conflict: usize,
    },
}

/// A conflict's result, if one is chosen (ticked or typed).
pub fn chosen(open: &ConflictFile, index: usize) -> Option<&String> {
    open.results
        .get(&index)
        .filter(|t| !t.is_empty() || open.ticked.iter().any(|k| k.0 == index))
}

/// Every chosen result, by conflict.
pub fn all_chosen(open: &ConflictFile, total: usize) -> BTreeMap<usize, String> {
    (0..total)
        .filter_map(|i| chosen(open, i).map(|t| (i, t.clone())))
        .collect()
}

/// The panes for `chunks`. `scroll`: bring the conflict in focus into view.
pub fn show(
    ui: &mut egui::Ui,
    open: &mut ConflictFile,
    chunks: &[Chunk],
    names: &Names<'_>,
) -> Option<Action> {
    let action = None;
    let all: Vec<&conflict::Conflict> = conflict::conflicts(chunks).collect();
    let scroll = std::mem::take(&mut open.scroll);
    // Rows like the file view's, without gaps.
    let row_height = ROW;

    // The split between the sides and the output, draggable.
    let full = ui.available_height();
    let top = (full * open.pane_split).clamp(120.0, (full - 140.0).max(120.0));
    let mut changed = BTreeMap::new();
    ui.allocate_ui(egui::vec2(ui.available_width(), top), |ui| {
        ui.columns(2, |cols| {
            for (col, (column, side, colour)) in cols
                .iter_mut()
                .zip([(1u8, names.ours, OURS), (2u8, names.theirs, THEIRS)])
            {
                let rows = side_rows(chunks, column);
                side_pane(
                    col,
                    open,
                    &all,
                    (column, side, colour),
                    &rows,
                    (row_height, top),
                    scroll,
                    &mut changed,
                );
            }
        });
    });
    for (i, ()) in changed {
        if let Some(c) = all.get(i) {
            ticked_result(open, i, c);
        }
    }

    // The handle.
    let (handle, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 8.0), egui::Sense::drag());
    let hot = response.hovered() || response.dragged();
    ui.painter().hline(
        handle.x_range(),
        handle.center().y,
        Stroke::new(if hot { 2.0 } else { 1.0 }, theme::border()),
    );
    if hot {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if response.dragged() {
        open.pane_split = ((top + response.drag_delta().y) / full).clamp(0.2, 0.8);
    }

    // The output.
    let out = output_rows(open, chunks);
    let resolved = (0..all.len())
        .filter(|i| chosen(open, *i).is_some())
        .count();
    let current = open.current;
    let mut go = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Output").strong());
        ui.label(
            RichText::new(format!(
                "{resolved} of {} conflicts chosen · the file as Save writes it",
                all.len()
            ))
            .size(12.0)
            .color(theme::muted()),
        );
        // Where you are, and the way to the others.
        let total = all.len();
        ui.add_space(12.0);
        ui.label(
            RichText::new(format!("conflict {} of {total}", current + 1))
                .size(12.0)
                .color(theme::accent()),
        );
        for (glyph, label, step) in [
            (
                egui_phosphor::regular::CARET_UP,
                "Previous conflict",
                total.saturating_sub(1),
            ),
            (egui_phosphor::regular::CARET_DOWN, "Next conflict", 1),
        ] {
            let b = ui
                .add_enabled(total > 1, egui::Button::new(glyph).small())
                .on_hover_text(if step == 1 {
                    "Next conflict (Ctrl+↓)"
                } else {
                    "Previous conflict (Ctrl+↑)"
                });
            ui.ctx()
                .accesskit_node_builder(b.id, |n| n.set_label(label));
            if b.clicked() && total > 0 {
                go = Some((current + step) % total);
                ui.ctx().request_repaint();
            }
        }
    });
    let target = all
        .get(open.current)
        .and_then(|_| {
            out.iter().position(|r| match r {
                OutRow::Open { conflict } => *conflict == current,
                _ => false,
            })
        })
        .or_else(|| chosen_row(&out, chunks, open, current));
    let mut area = egui::ScrollArea::both()
        .id_salt("conflict-output")
        .auto_shrink([false, false]);
    if scroll && let Some(r) = target {
        area = area.vertical_scroll_offset((r as f32 - 3.0).max(0.0) * row_height);
    }
    let width = out.len().to_string().len();
    crate::ui::widgets::gapless_rows(ui, area, row_height, out.len(), |ui, range| {
        for row in &out[range] {
            let (no, text, fill, colour) = match row {
                OutRow::Same { no, text } => (Some(*no), (*text).to_string(), None, theme::text()),
                OutRow::Chosen { no, text } => (
                    Some(*no),
                    (*text).to_string(),
                    Some(BASE.gamma_multiply(0.18)),
                    theme::text(),
                ),
                OutRow::Open { conflict } => (
                    None,
                    format!(
                        "conflict {}: nothing chosen yet (tick it above); it stays a conflict",
                        conflict + 1
                    ),
                    Some(theme::danger().gamma_multiply(0.15)),
                    theme::danger(),
                ),
            };
            line(ui, row_height, fill, |ui| {
                gutter(ui, None, no, width);
                // The file's lines in its colours; a note about an open
                // conflict in its own.
                let text: egui::WidgetText = if no.is_some() {
                    crate::highlight::line(&open.file, &text, CODE, false).into()
                } else {
                    RichText::new(text)
                        .monospace()
                        .size(CODE)
                        .color(colour)
                        .into()
                };
                ui.add(egui::Label::new(text).wrap_mode(egui::TextWrapMode::Extend));
            });
        }
    });
    // Scrolled to on the next frame.
    if let Some(i) = go {
        open.current = i;
        open.scroll = true;
    }
    action
}

/// The output row where conflict `index`'s chosen result starts.
fn chosen_row(
    out: &[OutRow<'_>],
    chunks: &[Chunk],
    open: &ConflictFile,
    index: usize,
) -> Option<usize> {
    // Count rows up to that conflict.
    let mut r = 0;
    let mut n = 0;
    for chunk in chunks {
        match chunk {
            Chunk::Same(t) => r += t.lines().count(),
            Chunk::Conflict(_) => {
                if n == index {
                    return (r < out.len()).then_some(r);
                }
                r += chosen(open, n).map_or(1, |t| t.lines().count());
                n += 1;
            }
        }
    }
    None
}

/// A side's rows: the agreed lines and its version of each conflict.
fn side_rows(chunks: &[Chunk], column: u8) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
    let mut no = 0;
    let mut n = 0;
    for chunk in chunks {
        match chunk {
            Chunk::Same(t) => {
                for text in t.lines() {
                    no += 1;
                    rows.push(Row::Same { no, text });
                }
            }
            Chunk::Conflict(c) => {
                let text = if column == 1 { &c.ours } else { &c.theirs };
                if text.is_empty() {
                    rows.push(Row::Nothing { conflict: n });
                }
                for (line, text) in text.lines().enumerate() {
                    no += 1;
                    rows.push(Row::Side {
                        no,
                        text,
                        conflict: n,
                        line,
                    });
                }
                n += 1;
            }
        }
    }
    rows
}

/// The output's rows: agreed lines, each conflict's chosen result, or a
/// note where nothing is chosen yet.
fn output_rows<'a>(open: &'a ConflictFile, chunks: &'a [Chunk]) -> Vec<OutRow<'a>> {
    let mut rows = Vec::new();
    let mut no = 0;
    let mut n = 0;
    for chunk in chunks {
        match chunk {
            Chunk::Same(t) => {
                for text in t.lines() {
                    no += 1;
                    rows.push(OutRow::Same { no, text });
                }
            }
            Chunk::Conflict(_) => {
                match chosen(open, n) {
                    Some(t) => {
                        for text in t.lines() {
                            no += 1;
                            rows.push(OutRow::Chosen { no, text });
                        }
                    }
                    None => rows.push(OutRow::Open { conflict: n }),
                }
                n += 1;
            }
        }
    }
    rows
}

/// One side's pane: who it is, then its whole file.
#[expect(
    clippy::too_many_arguments,
    reason = "the pane's parts, kept apart for borrowing"
)]
fn side_pane(
    ui: &mut egui::Ui,
    open: &mut ConflictFile,
    all: &[&conflict::Conflict],
    (column, side, colour): (u8, &git_raccoon_core::merge::Side, Color32),
    rows: &[Row<'_>],
    (row_height, height): (f32, f32),
    scroll: bool,
    changed: &mut BTreeMap<usize, ()>,
) {
    let lines_of = |i: usize| -> usize {
        all.get(i).map_or(0, |c| {
            if column == 1 { &c.ours } else { &c.theirs }
                .lines()
                .count()
        })
    };
    let whole = |open: &ConflictFile, i: usize| -> bool {
        let n = lines_of(i);
        n > 0 && (0..n).all(|l| open.ticked.contains(&(i, column, l)))
    };
    // Header: tick every conflict's side at once, and whose it is.
    ui.horizontal(|ui| {
        let mut on = (0..all.len()).all(|i| lines_of(i) == 0 || whole(open, i))
            && (0..all.len()).any(|i| lines_of(i) > 0);
        let all_box = ui.checkbox(&mut on, "");
        let label = format!("Take {} in every conflict", side.name);
        ui.ctx()
            .accesskit_node_builder(all_box.id, |n| n.set_label(label.as_str()));
        if all_box.changed() {
            for i in 0..all.len() {
                for l in 0..lines_of(i) {
                    if on {
                        open.ticked.insert((i, column, l));
                    } else {
                        open.ticked.remove(&(i, column, l));
                    }
                }
                changed.insert(i, ());
            }
        }
        tag(ui, &side.name, column == 1);
        if let Some(id) = &side.commit {
            ui.label(
                RichText::new(crate::format::short_sha(id))
                    .monospace()
                    .size(CODE)
                    .color(theme::muted()),
            );
        }
        if let Some(summary) = &side.summary {
            ui.add(egui::Label::new(RichText::new(summary).color(theme::muted())).truncate());
        }
    });
    let header = ui.min_rect().height();
    let target = rows.iter().position(|r| match r {
        Row::Side { conflict, .. } | Row::Nothing { conflict } => *conflict == open.current,
        Row::Same { .. } => false,
    });
    let mut area = egui::ScrollArea::both()
        .id_salt(("conflict-pane", column))
        .max_height((height - header - 4.0).max(60.0))
        .auto_shrink([false, false]);
    if scroll && let Some(r) = target {
        area = area.vertical_scroll_offset((r as f32 - 3.0).max(0.0) * row_height);
    }
    let width = rows.len().to_string().len();
    crate::ui::widgets::gapless_rows(ui, area, row_height, rows.len(), |ui, range| {
        for row in &rows[range] {
            match row {
                Row::Same { no, text } => {
                    line(ui, row_height, None, |ui| {
                        gutter(ui, Some(None), Some(*no), width);
                        ui.add(
                            egui::Label::new(crate::highlight::line(&open.file, text, CODE, true))
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                }
                Row::Nothing { conflict } => {
                    let focused = *conflict == open.current;
                    line(
                        ui,
                        row_height,
                        Some(colour.gamma_multiply(if focused { 0.2 } else { 0.1 })),
                        |ui| {
                            gutter(ui, Some(None), None, width);
                            ui.label(
                                RichText::new(format!(
                                    "(nothing here in conflict {})",
                                    conflict + 1
                                ))
                                .italics()
                                .color(colour),
                            );
                        },
                    );
                }
                Row::Side {
                    no,
                    text,
                    conflict,
                    line: l,
                } => {
                    let key = (*conflict, column, *l);
                    let on = open.ticked.contains(&key);
                    let focused = *conflict == open.current;
                    let fill = colour.gamma_multiply(match (on, focused) {
                        (true, _) => 0.35,
                        (false, true) => 0.2,
                        (false, false) => 0.1,
                    });
                    let mut clicked_hunk = None;
                    let mut clicked_line = false;
                    let response = line(ui, row_height, Some(fill), |ui| {
                        // The conflict's box, on its first line.
                        let hunk = (*l == 0).then(|| whole(open, *conflict));
                        if let Some(r) = gutter(ui, Some(hunk), Some(*no), width) {
                            let label = format!("Take {}'s conflict {}", side.name, conflict + 1);
                            ui.ctx()
                                .accesskit_node_builder(r.id, |n| n.set_label(label.as_str()));
                            if r.changed() {
                                clicked_hunk = Some(!hunk.unwrap_or(false));
                            }
                        }
                        let text = ui
                            .add(
                                egui::Button::new(crate::highlight::line(
                                    &open.file, text, CODE, false,
                                ))
                                .frame(false)
                                .wrap_mode(egui::TextWrapMode::Extend),
                            )
                            .on_hover_text("Click to take this line");
                        let label = format!(
                            "Take line {} of {} in conflict {}",
                            l + 1,
                            side.name,
                            conflict + 1
                        );
                        ui.ctx()
                            .accesskit_node_builder(text.id, |n| n.set_label(label.as_str()));
                        clicked_line = text.clicked();
                    });
                    drop(response);
                    if let Some(take) = clicked_hunk {
                        for l in 0..lines_of(*conflict) {
                            if take {
                                open.ticked.insert((*conflict, column, l));
                            } else {
                                open.ticked.remove(&(*conflict, column, l));
                            }
                        }
                        open.current = *conflict;
                        changed.insert(*conflict, ());
                    } else if clicked_line {
                        if on {
                            open.ticked.remove(&key);
                        } else {
                            open.ticked.insert(key);
                        }
                        open.current = *conflict;
                        changed.insert(*conflict, ());
                    }
                }
            }
        }
    });
}

/// A row of fixed height, filled behind if asked.
fn line(
    ui: &mut egui::Ui,
    height: f32,
    fill: Option<Color32>,
    add: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let behind = ui.painter().add(egui::Shape::Noop);
    let width = ui.available_width();
    // A horizontal row is at least the interact size tall: that's the
    // row's height here, so rows match the file view's.
    let response = ui
        .scope(|ui| {
            let spacing = ui.spacing_mut();
            spacing.interact_size.y = height;
            spacing.button_padding.y = 0.0;
            spacing.item_spacing.x = 6.0;
            ui.horizontal(|ui| {
                ui.set_min_size(egui::vec2(width, height));
                ui.set_max_height(height);
                add(ui);
            })
            .response
        })
        .inner;
    if let Some(fill) = fill {
        ui.painter().set(
            behind,
            egui::epaint::RectShape::filled(response.rect, 0, fill),
        );
    }
    response
}

/// The left edge of a row: a box (`Some(Some(on))`), room for one
/// (`Some(None)`) or none, and the line number. Returns the box's response.
fn gutter(
    ui: &mut egui::Ui,
    tick: Option<Option<bool>>,
    no: Option<usize>,
    width: usize,
) -> Option<egui::Response> {
    let mut out = None;
    match tick {
        Some(Some(mut on)) => {
            let r = ui.checkbox(&mut on, "");
            out = Some(r);
        }
        Some(None) => {
            ui.add_space(ui.spacing().icon_width);
        }
        None => {}
    }
    let no = no.map_or_else(|| " ".repeat(width), |n| format!("{n:>width$}"));
    ui.label(
        RichText::new(no)
            .monospace()
            .size(CODE)
            .color(theme::muted()),
    );
    out
}
