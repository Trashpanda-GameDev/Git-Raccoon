//! Resolving a conflicted file: each conflict with its two versions and a
//! choice of ours, theirs or both; whole-file choices; or editing the text.

use eframe::egui::{self, Color32, RichText, Stroke};
use egui_phosphor::regular as icon;
use git_raccoon_core::conflict::{self, Chunk, ConflictedFile, Kind, Pick};
use git_raccoon_core::merge::{self, InProgress};

use crate::repo_view::ConflictFile;
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Pick(usize, Pick),
    /// Write this as one conflict's result.
    UseText(usize, String),
    /// Take one side's whole file.
    TakeSide {
        ours: bool,
    },
    MarkResolved,
    StartEdit,
    SaveEdit,
    CancelEdit,
    Close,
    /// Mark these commits (newest first) in the graph: one side's, behind
    /// the conflict in focus.
    ShowCommits {
        ids: Vec<String>,
        side: String,
    },
    /// Open the pull/merge request a commit came in with.
    OpenRequest(u64),
}

const OURS: Color32 = Color32::from_rgb(0x3f, 0xb9, 0x8a);
const THEIRS: Color32 = Color32::from_rgb(0x5a, 0x9b, 0xf0);
const BASE: Color32 = Color32::from_rgb(0xb3, 0x9d, 0xdb);

/// The names of the two sides: what the files had ("ours"), and what's
/// coming in ("theirs"), each with the commit's message if known.
pub struct Names<'a> {
    pub ours: &'a merge::Side,
    pub theirs: &'a merge::Side,
}

pub fn show(
    ui: &mut egui::Ui,
    open: &mut ConflictFile,
    names: &Names<'_>,
    busy: bool,
) -> Option<Action> {
    let mut action = None;
    let total = open
        .chunks
        .as_ref()
        .map_or(0, |c| conflict::conflicts(c).count());
    let (ours, theirs) = (names.ours.name.as_str(), names.theirs.name.as_str());

    // Ctrl+↓ / Ctrl+↑: the next or previous conflict (not while typing).
    if total > 0 && !ui.ctx().text_edit_focused() {
        let (down, up) = ui.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowDown),
                i.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowUp),
            )
        });
        if down || up {
            open.current = if down {
                (open.current + 1) % total
            } else {
                (open.current + total - 1) % total
            };
            open.scroll = true;
        }
    }

    // Header: the file, where you are, and whole-file actions.
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(12, 0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(40.0);
                ui.label(RichText::new(icon::WARNING).color(theme::danger()));
                ui.label(RichText::new(&open.file).strong().monospace());
                if open.edit.is_some() {
                    ui.label(RichText::new("editing").size(12.0).color(theme::muted()));
                } else if total == 0 {
                    ui.label(
                        RichText::new("no conflicts left")
                            .size(12.0)
                            .color(theme::muted()),
                    );
                } else {
                    ui.label(
                        RichText::new(format!("conflict {} of {total}", open.current + 1))
                            .size(12.0)
                            .color(theme::muted()),
                    );
                    let previous = ui
                        .add_enabled(total > 1, egui::Button::new(icon::CARET_UP).small())
                        .on_hover_text("Previous conflict (Ctrl+↑)");
                    ui.ctx()
                        .accesskit_node_builder(previous.id, |n| n.set_label("Previous conflict"));
                    if previous.clicked() {
                        open.current = (open.current + total - 1) % total;
                        open.scroll = true;
                    }
                    let next = ui
                        .add_enabled(total > 1, egui::Button::new(icon::CARET_DOWN).small())
                        .on_hover_text("Next conflict (Ctrl+↓)");
                    ui.ctx()
                        .accesskit_node_builder(next.id, |n| n.set_label("Next conflict"));
                    if next.clicked() {
                        open.current = (open.current + 1) % total;
                        open.scroll = true;
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, icon::X, "Back to all the conflicts (Esc)")
                        .clicked()
                    {
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
                            .add_enabled(total == 0, primary_if("Mark resolved", total == 0))
                            .on_disabled_hover_text("Resolve every conflict first")
                            .on_hover_text("Stage the file as it is now");
                        if resolved.clicked() {
                            action = Some(Action::MarkResolved);
                        }
                        if total > 0 {
                            let why = ui
                                .selectable_label(
                                    open.why_open,
                                    format!("{}  Why?", icon::QUESTION),
                                )
                                .on_hover_text("The commits on each side that changed these lines");
                            ui.ctx().accesskit_node_builder(why.id, |n| {
                                n.set_label("Why it happened");
                            });
                            if why.clicked() {
                                open.why_open = !open.why_open;
                            }
                        }
                        if ui
                            .button(format!("{}  Edit", icon::PENCIL_SIMPLE))
                            .on_hover_text("Write the whole file yourself")
                            .clicked()
                        {
                            action = Some(Action::StartEdit);
                        }
                        // One side's version of the whole file.
                        ui.menu_button(format!("Whole file  {}", icon::CARET_DOWN), |ui| {
                            for (name, side) in [(ours, true), (theirs, false)] {
                                if ui
                                    .button(format!("Use {name}'s file"))
                                    .on_hover_text(format!(
                                        "Take {name}'s version of the whole file"
                                    ))
                                    .clicked()
                                {
                                    action = Some(Action::TakeSide { ours: side });
                                    ui.close();
                                }
                            }
                        });
                    });
                });
            });
        });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );

    egui::ScrollArea::vertical()
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
                    if let Err(e) = &open.text {
                        ui.label(RichText::new(e).color(theme::danger()));
                        return;
                    }
                    let Some(chunks) = open.chunks.clone() else {
                        ui.label(
                            RichText::new(
                                "No conflict markers in this file. It may be binary, or deleted on one side: pick a side for the whole file, or edit it and mark it resolved.",
                            )
                            .color(theme::muted()),
                        );
                        return;
                    };
                    if let Some(a) = chunk_list(ui, open, &chunks, names, busy) {
                        action = Some(a);
                    }
                });
        });
    action
}

fn chunk_list(
    ui: &mut egui::Ui,
    open: &mut ConflictFile,
    chunks: &[Chunk],
    names: &Names<'_>,
    busy: bool,
) -> Option<Action> {
    let mut action = None;
    let total = conflict::conflicts(chunks).count();
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
                let focused = index == open.current;
                let card = egui::Frame::new()
                    .stroke(Stroke::new(
                        if focused { 1.5 } else { 1.0 },
                        if focused {
                            theme::accent()
                        } else {
                            theme::border()
                        },
                    ))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if focused && open.why_open {
                            if let Some(a) = why_panel(ui, open.why.get(&index), names) {
                                action = Some(a);
                            }
                            ui.add_space(8.0);
                        }
                        if let Some(a) = conflict_card(ui, open, c, (index, total), names, busy) {
                            action = Some(a);
                        }
                    })
                    .response;
                if card.contains_pointer() && ui.input(|i| i.pointer.any_pressed()) {
                    open.current = index;
                }
                if focused && open.scroll {
                    open.scroll = false;
                    card.scroll_to_me(Some(egui::Align::TOP));
                }
                ui.add_space(8.0);
            }
        }
    }
    action
}

/// One conflict: its base and both sides, each line ready to tick, the
/// result they make (editable), and quick picks.
fn conflict_card(
    ui: &mut egui::Ui,
    open: &mut ConflictFile,
    c: &conflict::Conflict,
    (index, total): (usize, usize),
    names: &Names<'_>,
    busy: bool,
) -> Option<Action> {
    let mut action = None;
    let (ours, theirs) = (names.ours.name.as_str(), names.theirs.name.as_str());
    let number = index + 1;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("Conflict {number} of {total}"))
                .strong()
                .size(12.0),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                let both = ui.menu_button(format!("Both  {}", icon::CARET_DOWN), |ui| {
                    if ui.button(format!("{ours} first, then {theirs}")).clicked() {
                        action = Some(Action::Pick(index, Pick::Both));
                        ui.close();
                    }
                    if ui.button(format!("{theirs} first, then {ours}")).clicked() {
                        action = Some(Action::UseText(index, format!("{}{}", c.theirs, c.ours)));
                        ui.close();
                    }
                });
                let name = format!("Both sides for conflict {number}");
                ui.ctx()
                    .accesskit_node_builder(both.response.id, |n| n.set_label(name));
                for (name, pick, colour) in
                    [(theirs, Pick::Theirs, THEIRS), (ours, Pick::Ours, OURS)]
                {
                    let b = ui.add(
                        egui::Button::new(RichText::new(format!("Take {name}")).color(colour))
                            .stroke(Stroke::new(1.0, colour.gamma_multiply(0.5))),
                    );
                    let label = format!("Take {name} for conflict {number}");
                    ui.ctx()
                        .accesskit_node_builder(b.id, |n| n.set_label(label));
                    if b.clicked() {
                        action = Some(Action::Pick(index, pick));
                    }
                }
            });
        });
    });
    ui.add_space(6.0);

    // The three versions side by side; every line can be ticked.
    let mut changed = false;
    let base = c.base.as_ref().map(|b| {
        (
            0u8,
            "Before".to_string(),
            Some("before either changed it".to_string()),
            b.as_str(),
            BASE,
        )
    });
    let columns: Vec<(u8, String, Option<String>, &str, Color32)> = base
        .into_iter()
        .chain([
            (
                1,
                ours.to_string(),
                names.ours.summary.clone(),
                c.ours.as_str(),
                OURS,
            ),
            (
                2,
                theirs.to_string(),
                names.theirs.summary.clone(),
                c.theirs.as_str(),
                THEIRS,
            ),
        ])
        .collect();
    ui.columns(columns.len(), |cols| {
        for (col, (column, title, about, text, colour)) in cols.iter_mut().zip(&columns) {
            col.horizontal_wrapped(|ui| {
                egui::Frame::new()
                    .fill(colour.gamma_multiply(0.15))
                    .corner_radius(4)
                    .inner_margin(egui::Margin::symmetric(6, 1))
                    .show(ui, |ui| {
                        ui.label(RichText::new(title).size(12.0).color(*colour));
                    });
                if let Some(about) = about {
                    ui.label(RichText::new(about).size(11.0).color(theme::muted()));
                }
            });
            egui::Frame::new()
                .fill(colour.gamma_multiply(0.08))
                .corner_radius(4)
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(col, |ui| {
                    ui.set_width(ui.available_width());
                    if text.is_empty() {
                        ui.label(RichText::new("(nothing)").italics().color(theme::muted()));
                    }
                    for (line_no, line) in text.lines().enumerate() {
                        let key = (index, *column, line_no);
                        let mut on = open.ticked.contains(&key);
                        let tick = ui.checkbox(&mut on, RichText::new(line).monospace());
                        let label =
                            format!("Take line {} of {title} in conflict {number}", line_no + 1);
                        ui.ctx()
                            .accesskit_node_builder(tick.id, |n| n.set_label(label));
                        if tick.changed() {
                            if on {
                                open.ticked.insert(key);
                            } else {
                                open.ticked.remove(&key);
                            }
                            changed = true;
                        }
                    }
                });
        }
    });

    // The result: the ticked lines (yours, then theirs), or typed.
    if changed {
        let mut text = String::new();
        for column in [1u8, 2, 0] {
            let source = match column {
                0 => c.base.as_deref().unwrap_or_default(),
                1 => c.ours.as_str(),
                _ => c.theirs.as_str(),
            };
            for (line_no, line) in source.lines().enumerate() {
                if open.ticked.contains(&(index, column, line_no)) {
                    text.push_str(line);
                    text.push('\n');
                }
            }
        }
        open.results.insert(index, text);
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Result").strong().size(12.0));
        ui.label(
            RichText::new("tick lines above, or write it here")
                .size(11.0)
                .color(theme::muted()),
        );
    });
    let ticked_any = open.ticked.iter().any(|t| t.0 == index);
    let result = open.results.entry(index).or_default();
    let field = ui.add(
        egui::TextEdit::multiline(result)
            .code_editor()
            .desired_width(f32::INFINITY)
            .desired_rows(2)
            .hint_text("Nothing yet"),
    );
    ui.ctx().accesskit_node_builder(field.id, |n| {
        n.set_label(format!("Result of conflict {number}"))
    });
    let ready = ticked_any || !result.is_empty();
    let use_it = ui
        .add_enabled(!busy && ready, primary_if("Use this result", ready))
        .on_disabled_hover_text("Tick some lines, or write the result");
    let label = format!("Use this result for conflict {number}");
    ui.ctx()
        .accesskit_node_builder(use_it.id, |n| n.set_label(label));
    if use_it.clicked() {
        action = Some(Action::UseText(index, result.clone()));
    }
    action
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
    egui::Button::new(RichText::new(label).color(theme::on_accent()).strong())
        .fill(theme::accent_fill())
}

/// What the overview asks for.
pub enum OverviewAction {
    /// Open a file to resolve it.
    Open(String),
    /// Resolve a file by taking one side's whole version (or its deletion).
    TakeSide {
        file: String,
        ours: bool,
    },
    MarkResolved(String),
    /// Show a resolved file's result (its staged changes).
    Review(String),
    /// Bring a resolved file's conflict back.
    Unresolve(String),
    Continue,
}

/// What's in progress, in words: "Rebasing feature/login onto main", and
/// "commit 3 of 5: “Remember the last user”".
pub fn describe(state: &merge::State) -> (String, Option<String>) {
    let (ours, theirs) = (&state.ours, &state.theirs);
    let quoted = |s: &merge::Side| {
        s.summary
            .as_ref()
            .map_or_else(|| s.name.clone(), |m| format!("“{m}”"))
    };
    match state.what {
        InProgress::Rebase => (
            format!("Rebasing {} onto {}", theirs.name, ours.name),
            Some(match state.step {
                Some((n, of)) => format!("commit {n} of {of}: {}", quoted(theirs)),
                None => format!("applying {}", quoted(theirs)),
            }),
        ),
        InProgress::Merge => (format!("Merging {} into {}", theirs.name, ours.name), None),
        InProgress::CherryPick => (
            format!("Copying {} onto {}", quoted(theirs), ours.name),
            None,
        ),
        InProgress::Revert => {
            let undone = merge::Side {
                name: theirs.name.trim_start_matches("reverting ").to_string(),
                ..theirs.clone()
            };
            (
                format!("Reverting {} on {}", quoted(&undone), ours.name),
                None,
            )
        }
        InProgress::Other => (
            "A Git operation is in progress".to_string(),
            state.detail.clone(),
        ),
    }
}

/// What each side is, for the overview's key: (ours, theirs).
fn side_words(state: &merge::State) -> (&'static str, &'static str) {
    match state.what {
        InProgress::Rebase => ("what you're rebasing onto", "your commit being applied"),
        InProgress::Merge => ("your branch", "the branch being merged in"),
        InProgress::CherryPick => ("your branch", "the commit being copied"),
        InProgress::Revert => ("your branch", "undoing that commit"),
        InProgress::Other => ("what the files had", "what's coming in"),
    }
}

/// A side's name as a coloured tag.
pub fn tag(ui: &mut egui::Ui, name: &str, ours: bool) {
    let colour = if ours { OURS } else { THEIRS };
    egui::Frame::new()
        .fill(colour.gamma_multiply(0.15))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(name).size(12.0).color(colour));
        });
}

/// Every file of what's in progress, and how far along resolving it is,
/// in place of the history.
pub fn overview(
    ui: &mut egui::Ui,
    state: &merge::State,
    files: &[ConflictedFile],
    resolved: &[String],
    busy: bool,
) -> Option<OverviewAction> {
    let mut action = None;
    let (ours, theirs) = (state.ours.name.as_str(), state.theirs.name.as_str());
    let total = files.len() + resolved.len();
    let left: usize = files.iter().map(|f| f.conflicts.max(1)).sum();
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(20, 16))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Conflicts").strong().size(16.0));
                        #[expect(clippy::cast_precision_loss, reason = "file counts are small")]
                        let done = resolved.len() as f32 / total.max(1) as f32;
                        // A track, filled as far as it's got (a bar at 0 is still a bar).
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(160.0, 6.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 3, theme::bg_hover());
                        let mut filled = rect;
                        filled.set_width(rect.width() * done);
                        ui.painter().rect_filled(filled, 3, theme::accent());
                        let s = if total == 1 { "" } else { "s" };
                        ui.label(
                            RichText::new(format!(
                                "{} of {total} file{s} resolved",
                                resolved.len()
                            ))
                            .color(theme::muted()),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(first) = next_file(files)
                                && ui.add_enabled(!busy, primary("Next conflict")).clicked()
                            {
                                action = Some(OverviewAction::Open(first));
                            }
                        });
                    });
                    ui.add_space(12.0);
                    egui::Frame::new()
                        .stroke(Stroke::new(1.0, theme::border()))
                        .corner_radius(8)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            for f in files {
                                if let Some(a) = file_row(ui, f, (ours, theirs), busy) {
                                    action = Some(a);
                                }
                            }
                            for file in resolved {
                                if let Some(a) = resolved_row(ui, file, busy) {
                                    action = Some(a);
                                }
                            }
                        });
                    ui.add_space(12.0);
                    if files.is_empty() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("{}  Everything is resolved.", icon::CHECK))
                                    .color(theme::add()),
                            );
                            let noun = state.what.noun();
                            if ui
                                .add_enabled(!busy, primary(&format!("Continue the {noun}")))
                                .clicked()
                            {
                                action = Some(OverviewAction::Continue);
                            }
                        });
                    } else {
                        let s = if left == 1 { "" } else { "s" };
                        ui.label(
                            RichText::new(format!("{left} conflict{s} left."))
                                .color(theme::muted()),
                        );
                    }
                    ui.add_space(8.0);
                    let (our_words, their_words) = side_words(state);
                    ui.horizontal_wrapped(|ui| {
                        tag(ui, ours, true);
                        ui.label(RichText::new(format!("= {our_words}")).color(theme::muted()));
                        ui.add_space(12.0);
                        tag(ui, theirs, false);
                        ui.label(RichText::new(format!("= {their_words}")).color(theme::muted()));
                    });
                });
        });
    action
}

/// The first file to resolve: one with conflicts in its text, else any.
fn next_file(files: &[ConflictedFile]) -> Option<String> {
    files
        .iter()
        .find(|f| f.conflicts > 0)
        .or(files.first())
        .map(|f| f.path.clone())
}

fn row(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(add);
        });
    ui.painter().hline(
        ui.min_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );
}

fn chip(ui: &mut egui::Ui, text: &str, colour: Color32) {
    egui::Frame::new()
        .fill(colour.gamma_multiply(0.15))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(colour));
        });
}

fn file_row(
    ui: &mut egui::Ui,
    f: &ConflictedFile,
    (ours, theirs): (&str, &str),
    busy: bool,
) -> Option<OverviewAction> {
    let mut action = None;
    let path = f.path.clone();
    let take = |ours: bool| OverviewAction::TakeSide {
        file: path.clone(),
        ours,
    };
    row(ui, |ui| {
        let (glyph, colour) = match f.kind {
            Kind::DeletedOnOurs | Kind::DeletedOnTheirs => (icon::DIAMOND, theme::modified()),
            _ => (icon::WARNING, theme::danger()),
        };
        ui.label(RichText::new(glyph).color(colour));
        ui.label(RichText::new(&f.path).monospace());
        let s = if f.conflicts == 1 { "" } else { "s" };
        let (badge, note) = match f.kind {
            Kind::DeletedOnOurs => (format!("deleted on {ours}"), format!("{theirs} changed it")),
            Kind::DeletedOnTheirs => (format!("deleted on {theirs}"), format!("{ours} changed it")),
            _ if f.binary => ("binary".into(), "pick one side's version".into()),
            _ if f.conflicts == 0 => (
                "no conflicts left".into(),
                "its text is resolved: mark it resolved".into(),
            ),
            Kind::BothAdded => (
                format!("{} conflict{s}", f.conflicts),
                "added on both sides".into(),
            ),
            Kind::BothChanged => (
                format!("{} conflict{s}", f.conflicts),
                "both sides changed it".into(),
            ),
            Kind::Other => (
                format!("{} conflict{s}", f.conflicts),
                "they conflict in another way (a rename, say)".into(),
            ),
        };
        chip(ui, &badge, colour);
        ui.label(RichText::new(note).size(12.0).color(theme::muted()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(!busy, |ui| match f.kind {
                Kind::DeletedOnOurs => {
                    if ui.button("Delete it").clicked() {
                        action = Some(take(true));
                    }
                    if ui.button(format!("Keep {theirs}'s")).clicked() {
                        action = Some(take(false));
                    }
                }
                Kind::DeletedOnTheirs => {
                    if ui.button("Delete it").clicked() {
                        action = Some(take(false));
                    }
                    if ui.button(format!("Keep {ours}'s")).clicked() {
                        action = Some(take(true));
                    }
                }
                _ if f.binary => {
                    if ui.button(format!("Keep {theirs}'s")).clicked() {
                        action = Some(take(false));
                    }
                    if ui.button(format!("Keep {ours}'s")).clicked() {
                        action = Some(take(true));
                    }
                }
                _ if f.conflicts == 0 => {
                    if ui.add(primary("Mark resolved")).clicked() {
                        action = Some(OverviewAction::MarkResolved(path.clone()));
                    }
                    if ui.button("Open").clicked() {
                        action = Some(OverviewAction::Open(path.clone()));
                    }
                }
                _ => {
                    let open = ui.button("Resolve…");
                    let name = format!("Resolve {}", f.path);
                    ui.ctx()
                        .accesskit_node_builder(open.id, |n| n.set_label(name));
                    if open.clicked() {
                        action = Some(OverviewAction::Open(path.clone()));
                    }
                }
            });
        });
    });
    action
}

fn resolved_row(ui: &mut egui::Ui, file: &str, busy: bool) -> Option<OverviewAction> {
    let mut action = None;
    row(ui, |ui| {
        ui.label(RichText::new(icon::CHECK).color(theme::add()));
        ui.label(RichText::new(file).monospace());
        chip(ui, "resolved", theme::add());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                let undo = ui
                    .button("Undo")
                    .on_hover_text("Bring its conflict back, as it was");
                let name = format!("Bring back the conflict in {file}");
                ui.ctx()
                    .accesskit_node_builder(undo.id, |n| n.set_label(name));
                if undo.clicked() {
                    action = Some(OverviewAction::Unresolve(file.to_string()));
                }
                let review = ui.button("Review").on_hover_text("Its resolved changes");
                let name = format!("Review {file}");
                ui.ctx()
                    .accesskit_node_builder(review.id, |n| n.set_label(name));
                if review.clicked() {
                    action = Some(OverviewAction::Review(file.to_string()));
                }
            });
        });
    });
    action
}

/// Why a conflict happened: each side's commits that changed its lines,
/// and a hint when it's clear what happened.
fn why_panel(
    ui: &mut egui::Ui,
    why: Option<&Result<git_raccoon_core::why::Why, String>>,
    names: &Names<'_>,
) -> Option<Action> {
    let why = match why {
        Some(Ok(w)) => w,
        Some(Err(e)) => {
            ui.label(RichText::new(format!("Couldn't tell why: {e}")).color(theme::muted()));
            return None;
        }
        None => return None,
    };
    let mut action = None;
    let now = chrono::Utc::now().timestamp();
    ui.columns(2, |cols| {
        for (col, ours) in cols.iter_mut().zip([true, false]) {
            let (side, changes) = if ours {
                (names.ours, &why.ours)
            } else {
                (names.theirs, &why.theirs)
            };
            egui::Frame::new()
                .fill(theme::bg_raised())
                .stroke(Stroke::new(1.0, theme::border()))
                .corner_radius(6)
                .inner_margin(egui::Margin::same(10))
                .show(col, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        tag(ui, &side.name, ours);
                        let what = if !ours && why.applying {
                            "the commit being applied".to_string()
                        } else {
                            match changes.len() {
                                0 => "no commits found for these lines".into(),
                                1 => "changed these lines in 1 commit".into(),
                                n => format!("changed these lines in {n} commits"),
                            }
                        };
                        ui.label(RichText::new(what).strong());
                    });
                    for c in changes {
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(crate::format::short_sha(&c.id))
                                    .monospace()
                                    .color(theme::muted()),
                            );
                            ui.add(egui::Label::new(&c.summary).truncate())
                                .on_hover_text(&c.summary);
                        });
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                c.author,
                                crate::format::age(c.time, now)
                            ))
                            .size(12.0)
                            .color(theme::muted()),
                        );
                        if let Some(first) = c.body.split("\n\n").next().filter(|b| !b.is_empty()) {
                            let mut text = first.split_whitespace().collect::<Vec<_>>().join(" ");
                            if text.chars().count() > 200 {
                                text = text.chars().take(200).collect::<String>() + "…";
                            }
                            ui.label(
                                RichText::new(format!("\u{201c}{text}\u{201d}"))
                                    .italics()
                                    .color(theme::muted()),
                            );
                        }
                    }
                    if changes.is_empty() {
                        return;
                    }
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        let text = if changes.len() == 1 {
                            "Show this commit"
                        } else {
                            "Show these commits"
                        };
                        let show = ui.button(text).on_hover_text("Mark them in the graph");
                        let label = format!("Show {}'s commits", side.name);
                        ui.ctx()
                            .accesskit_node_builder(show.id, |n| n.set_label(label.as_str()));
                        if show.clicked() {
                            action = Some(Action::ShowCommits {
                                ids: changes.iter().map(|c| c.id.clone()).collect(),
                                side: side.name.clone(),
                            });
                        }
                        let mut requests: Vec<u64> =
                            changes.iter().filter_map(|c| c.request).collect();
                        requests.dedup();
                        for n in requests {
                            if ui
                                .button(format!("#{n}  {}", icon::ARROW_UP_RIGHT))
                                .on_hover_text("The request it came in with")
                                .clicked()
                            {
                                action = Some(Action::OpenRequest(n));
                            }
                        }
                    });
                });
        }
    });
    if let Some(hint) = &why.hint {
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(theme::bg_raised())
            .stroke(Stroke::new(1.0, theme::border()))
            .corner_radius(6)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(icon::LIGHTBULB).color(theme::accent()));
                    ui.label(hint);
                });
            });
    }
    action
}
