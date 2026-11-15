//! The commit details pane on the right.

use eframe::egui::{self, Pos2, Rect, RichText, Sense, Stroke};
use egui_phosphor::regular as icon;
use git_raccoon_core::details::{ChangedFile, CommitDetails, Person};

use crate::format;
use crate::theme;
use crate::ui::file_list::{self, Shown};
use crate::ui::widgets;

pub enum Action {
    OpenFile(String),
    Select(String),
    StashApply,
    StashPop,
    StashDrop,
    /// Switch changed files between paths and folders.
    ToggleTree,
    Close,
}

/// How the changed files are shown.
#[derive(Clone, Copy)]
pub struct Files<'a> {
    /// The one whose diff is open.
    pub active: Option<&'a str>,
    /// As a tree of folders, rather than by path.
    pub tree: bool,
}

/// The button switching changed files between paths and a folder tree.
/// Returns whether it was clicked.
pub fn tree_toggle(ui: &mut egui::Ui, tree: bool) -> bool {
    let (glyph, name) = if tree {
        (icon::LIST, "Show paths")
    } else {
        (icon::TREE_STRUCTURE, "Show folders")
    };
    widgets::icon_button(ui, glyph, name).clicked()
}

pub fn show(
    ui: &mut egui::Ui,
    id: &str,
    details: Option<&Result<std::sync::Arc<CommitDetails>, String>>,
    loading: bool,
    files: Files<'_>,
    now: i64,
    // For a stash: its controls are shown (`busy` disables them).
    stash: Option<bool>,
) -> Option<Action> {
    let Files {
        active: active_file,
        tree,
    } = files;
    let mut action = None;
    ui.horizontal(|ui| {
        ui.set_height(36.0);
        ui.label(widgets::section_label(if stash.is_some() {
            "Stash"
        } else {
            "Commit"
        }));
        let mut text = egui::text::LayoutJob::default();
        text.append(
            format::short_sha(id),
            0.0,
            egui::TextFormat::simple(theme::mono(12.0), theme::text()),
        );
        text.append(
            icon::COPY,
            8.0,
            egui::TextFormat::simple(theme::body(12.0), theme::muted()),
        );
        let copy = ui
            .add(egui::Button::new(text))
            .on_hover_text("Copy full SHA");
        if copy.clicked() {
            ui.ctx().copy_text(id.to_string());
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icon::X, "Close (Esc)").clicked() {
                action = Some(Action::Close);
            }
            if loading {
                ui.spinner();
            }
        });
    });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );
    ui.add_space(8.0);
    if let Some(busy) = stash {
        ui.horizontal(|ui| {
            let buttons = [
                (
                    "Apply",
                    "Put these changes back into the working tree, keeping the stash",
                    Action::StashApply,
                ),
                (
                    "Pop",
                    "Apply, then remove the stash (it is kept if applying conflicts)",
                    Action::StashPop,
                ),
                ("Drop", "Delete the stash (asks first)", Action::StashDrop),
            ];
            for (label, tip, act) in buttons {
                if ui
                    .add_enabled(!busy, egui::Button::new(label))
                    .on_hover_text(tip)
                    .clicked()
                {
                    action = Some(act);
                }
            }
        });
        ui.add_space(4.0);
    }

    let d = match details {
        Some(Ok(d)) => d,
        Some(Err(e)) => {
            ui.colored_label(theme::danger(), e);
            return action;
        }
        None => {
            ui.label(RichText::new("Loading…").color(theme::muted()));
            return action;
        }
    };

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            if d.summary.is_empty() {
                ui.label(RichText::new("(no message)").color(theme::muted()));
            } else {
                ui.add(egui::Label::new(RichText::new(&d.summary).size(15.0).strong()).wrap());
            }
            for para in format::unwrap_paragraphs(&d.body) {
                ui.add(
                    egui::Label::new(RichText::new(para).color(theme::text().gamma_multiply(0.85)))
                        .wrap(),
                );
            }

            person(ui, &d.author, "authored", now);
            if d.committer.name != d.author.name || d.committer.email != d.author.email {
                person(ui, &d.committer, "committed", now);
            }

            if !d.parents.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(if d.parents.len() > 1 {
                            "Parents"
                        } else {
                            "Parent"
                        })
                        .color(theme::muted()),
                    );
                    for p in &d.parents {
                        let b = ui.add(egui::Button::new(
                            RichText::new(format::short_sha(p)).monospace().size(12.0),
                        ));
                        if b.on_hover_text(p).clicked() {
                            action = Some(Action::Select(p.clone()));
                        }
                    }
                    if d.parents.len() > 1 {
                        ui.label(RichText::new("· changes vs first parent").color(theme::muted()));
                    }
                });
            }

            ui.separator();
            ui.horizontal(|ui| {
                let n = d.files.len();
                ui.label(format!(
                    "{n} {} changed",
                    if n == 1 { "file" } else { "files" }
                ));
                ui.label(RichText::new(format!("+{}", d.additions)).color(theme::add()));
                ui.label(RichText::new(format!("−{}", d.deletions)).color(theme::del()));
                change_bar(ui, d.additions, d.deletions);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if tree_toggle(ui, tree) {
                        action = Some(Action::ToggleTree);
                    }
                });
            });

            ui.spacing_mut().item_spacing.y = 0.0;
            file_list::show(
                ui,
                "commit files",
                &d.files,
                |f| f.path.as_str(),
                tree,
                |ui, f, shown| {
                    if file_row_shown(ui, f, active_file == Some(f.path.as_str()), shown).clicked()
                    {
                        action = Some(Action::OpenFile(f.path.clone()));
                    }
                },
            );
        });
    action
}

fn person(ui: &mut egui::Ui, p: &Person, verb: &str, now: i64) {
    ui.horizontal(|ui| {
        widgets::avatar(ui, &p.name, &p.email);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new(&p.name).strong());
                ui.label(RichText::new(verb).color(theme::muted()));
            });
            ui.label(
                RichText::new(format!(
                    "{} · {}",
                    format::date_time(p.time),
                    format::age(p.time, now)
                ))
                .color(theme::muted()),
            )
            .on_hover_text(&p.email);
        });
    });
}

fn change_bar(ui: &mut egui::Ui, additions: usize, deletions: usize) {
    let total = additions + deletions;
    if total == 0 {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(5.0 * 9.0, 10.0), Sense::hover());
    let adds = (additions * 5 + total / 2) / total;
    for i in 0..5 {
        #[expect(clippy::cast_precision_loss, reason = "i < 5")]
        let x = rect.left() + i as f32 * 9.0;
        let r = Rect::from_min_size(Pos2::new(x, rect.center().y - 3.5), egui::vec2(7.0, 7.0));
        ui.painter()
            .rect_filled(r, 1, if i < adds { theme::add() } else { theme::del() });
    }
}

pub fn file_row(ui: &mut egui::Ui, f: &ChangedFile, active: bool) -> egui::Response {
    file_row_shown(ui, f, active, Shown::PATH)
}

/// A changed file's row, indented and with just its name in a folder tree.
pub fn file_row_shown(
    ui: &mut egui::Ui,
    f: &ChangedFile,
    active: bool,
    shown: Shown,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 26.0), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &f.path));
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, 0, theme::bg_selected());
    } else if response.hovered() {
        painter.rect_filled(rect, 0, theme::bg_hover());
    }
    let badge = Rect::from_min_size(
        Pos2::new(rect.left() + 2.0 + shown.indent, rect.center().y - 9.0),
        egui::vec2(18.0, 18.0),
    );
    widgets::status_badge(painter, badge, f.status);

    // Counts on the right.
    let mut right = rect.right() - 4.0;
    let mut count = |text: String, colour| {
        let g = painter.layout_no_wrap(text, theme::body(12.0), colour);
        right -= g.size().x;
        painter.galley(
            Pos2::new(right, rect.center().y - g.size().y / 2.0),
            g,
            colour,
        );
        right -= 6.0;
    };
    if f.binary {
        count("binary".into(), theme::muted());
    } else {
        if f.deletions > 0 {
            count(format!("−{}", f.deletions), theme::del());
        }
        if f.additions > 0 {
            count(format!("+{}", f.additions), theme::add());
        }
    }

    // Path: the folder (and old name) shrink first; the file name stays whole.
    let (dir, name) = format::split_path(&f.path);
    let prefix = match (&f.old_path, shown.name_only) {
        (Some(old), _) => format!(
            "{old} {} {}",
            icon::ARROW_RIGHT,
            if shown.name_only { "" } else { dir }
        ),
        (None, true) => String::new(),
        (None, false) => dir.to_string(),
    };
    let left = badge.right() + 8.0;
    let name_galley = painter.layout_no_wrap(name.to_string(), theme::body(13.0), theme::text());
    let room = (right - left - name_galley.size().x - 4.0).max(0.0);
    let prefix_rect = widgets::text_in(
        painter,
        Rect::from_min_max(
            Pos2::new(left, rect.top()),
            Pos2::new(left + room, rect.bottom()),
        ),
        &prefix,
        theme::body(13.0),
        theme::muted(),
    );
    let x = if prefix.is_empty() {
        left
    } else {
        prefix_rect.right()
    };
    painter.galley(
        Pos2::new(x, rect.center().y - name_galley.size().y / 2.0),
        name_galley,
        theme::text(),
    );
    response.on_hover_text(&f.path)
}
