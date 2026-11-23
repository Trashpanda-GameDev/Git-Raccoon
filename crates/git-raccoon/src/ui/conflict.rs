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
