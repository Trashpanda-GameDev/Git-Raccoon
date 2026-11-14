//! The right-hand pane when the working changes are selected: the staged
//! and unstaged files, and the commit message box.

use eframe::egui::{self, RichText, Stroke};
use egui_phosphor::regular as icon;
use git_raccoon_core::details::ChangedFile;
use git_raccoon_core::working::{Side, WorkingFile, WorkingStatus};

use crate::repo_view::CommitDraft;
use crate::theme;
use crate::ui::{details, widgets};

/// Summary lengths past these get an amber, then red, counter.
const SUMMARY_SOFT: usize = 50;
const SUMMARY_HARD: usize = 72;

pub enum Action {
    Open(Side, String),
    Stage(String),
    Unstage(String),
    StageAll,
    UnstageAll,
    SetAmend(bool),
    Commit,
    Close,
}

pub struct State<'a> {
    pub status: Option<&'a WorkingStatus>,
    pub loading: bool,
    /// A staging operation is running: file buttons are disabled.
    pub staging: bool,
    pub committing: bool,
    pub active: Option<(Side, &'a str)>,
    pub draft: &'a mut CommitDraft,
}

pub fn show(ui: &mut egui::Ui, st: State<'_>) -> Option<Action> {
    let mut action = None;
    let State {
        status,
        loading,
        staging,
        committing,
        active,
        draft,
    } = st;
    ui.horizontal(|ui| {
        ui.set_height(36.0);
        ui.label(widgets::section_label("Working changes"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icon::X, "Close (Esc)").clicked() {
                action = Some(Action::Close);
            }
            if loading || staging || committing {
                ui.spinner();
            }
        });
    });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::BORDER),
    );

    let Some(status) = status else {
        ui.add_space(8.0);
        ui.label(RichText::new("Loading…").color(theme::MUTED));
        return action;
    };

    // The commit box sits at the bottom; the file lists fill the rest.
    let staged = status.staged.len();
    egui::Panel::bottom("commit-box")
        .frame(
            egui::Frame::new()
                .fill(theme::BG_RAISED)
                .inner_margin(egui::Margin::symmetric(0, 10)),
        )
        .show(ui, |ui| {
            if let Some(a) = commit_box(ui, draft, staged, committing) {
                action = Some(a);
            }
        });

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(8.0);
            if status.is_clean() {
                ui.label(RichText::new("No uncommitted changes.").color(theme::MUTED));
                return;
            }
            for side in [Side::Unstaged, Side::Staged] {
                if let Some(a) = file_list(ui, status, side, staging, active) {
                    action = Some(a);
                }
            }
        });
    action
}

fn file_list(
    ui: &mut egui::Ui,
    status: &WorkingStatus,
    side: Side,
    busy: bool,
    active: Option<(Side, &str)>,
) -> Option<Action> {
    let mut action = None;
    let files = status.files(side);
    let (title, all_label, all_glyph, all_action) = match side {
        Side::Unstaged => (
            "Unstaged files",
            "Stage all",
            icon::PLUS_CIRCLE,
            Action::StageAll,
        ),
        Side::Staged => (
            "Staged files",
            "Unstage all",
            icon::MINUS_CIRCLE,
            Action::UnstageAll,
        ),
    };
    ui.horizontal(|ui| {
        ui.label(widgets::section_label(title));
        ui.label(
            RichText::new(files.len().to_string())
                .size(11.0)
                .color(theme::MUTED),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let button =
                egui::Button::new(RichText::new(format!("{all_glyph}  {all_label}")).size(12.0));
            if ui.add_enabled(!busy && !files.is_empty(), button).clicked() {
                action = Some(all_action);
            }
        });
    });
    ui.spacing_mut().item_spacing.y = 0.0;
    if files.is_empty() {
        ui.label(RichText::new("None").size(12.0).color(theme::MUTED));
    }
    for f in files {
        let is_active = active.is_some_and(|(s, p)| s == side && p == f.path);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let row_width = ui.available_width() - 26.0;
            let row = ui
                .allocate_ui(egui::vec2(row_width, 26.0), |ui| {
                    details::file_row(ui, &as_changed(f), is_active)
                })
                .inner;
            if row.clicked() {
                action = Some(Action::Open(side, f.path.clone()));
            }
            let (glyph, verb) = match side {
                Side::Unstaged => (icon::PLUS, "Stage"),
                Side::Staged => (icon::MINUS, "Unstage"),
            };
            if widgets::named_icon_button(ui, glyph, &format!("{verb} {}", f.path), !busy).clicked()
            {
                action = Some(match side {
                    Side::Unstaged => Action::Stage(f.path.clone()),
                    Side::Staged => Action::Unstage(f.path.clone()),
                });
            }
        });
    }
    ui.add_space(14.0);
    ui.spacing_mut().item_spacing.y = 6.0;
    action
}

fn commit_box(
    ui: &mut egui::Ui,
    draft: &mut CommitDraft,
    staged: usize,
    committing: bool,
) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.label(widgets::section_label(if draft.amend {
            "Amend"
        } else {
            "Commit"
        }));
        // How long the summary is, against the usual 50/72 guides.
        let n = draft.summary.chars().count();
        let colour = if n > SUMMARY_HARD {
            theme::DANGER
        } else if n > SUMMARY_SOFT {
            theme::MODIFIED
        } else {
            theme::MUTED
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(n.to_string()).size(11.0).color(colour))
                .on_hover_text(
                    "Summary length. Aim for 50 characters or fewer; Git tools cut lines past 72.",
                );
        });
    });

    let width = ui.available_width();
    let summary = ui.add_enabled(
        !committing,
        egui::TextEdit::singleline(&mut draft.summary)
            .hint_text("Summary (required)")
            .desired_width(width),
    );
    ui.ctx()
        .accesskit_node_builder(summary.id, |n| n.set_label("Commit summary"));
    let body = ui.add_enabled(
        !committing,
        egui::TextEdit::multiline(&mut draft.body)
            .hint_text("Description")
            .desired_rows(4)
            .desired_width(width),
    );
    ui.ctx()
        .accesskit_node_builder(body.id, |n| n.set_label("Commit description"));

    let mut amend = draft.amend;
    if ui
        .add_enabled(
            !committing,
            egui::Checkbox::new(&mut amend, "Amend last commit"),
        )
        .changed()
    {
        action = Some(Action::SetAmend(amend));
    }
    if draft.amend && draft.head_pushed {
        ui.label(
            RichText::new(format!(
                "{}  This commit is already pushed. Amending rewrites it, and you'll need to force-push.",
                icon::WARNING
            ))
            .size(12.0)
            .color(theme::MODIFIED),
        );
    }

    let can_commit = !committing && !draft.summary.trim().is_empty() && (draft.amend || staged > 0);
    let label = if committing {
        "Committing…".to_string()
    } else if draft.amend {
        "Amend commit".to_string()
    } else {
        format!(
            "Commit {staged} {}",
            if staged == 1 { "file" } else { "files" }
        )
    };
    let button = egui::Button::new(
        RichText::new(&label)
            .color(if can_commit {
                theme::ON_ACCENT
            } else {
                theme::MUTED
            })
            .strong(),
    )
    .fill(if can_commit {
        theme::ACCENT
    } else {
        theme::BG_HOVER
    })
    .min_size(egui::vec2(width, 30.0));
    let clicked = ui.add_enabled(can_commit, button).on_disabled_hover_text(
        if draft.summary.trim().is_empty() {
            "Write a summary first."
        } else {
            "Stage some changes first."
        },
    );
    // Ctrl/Cmd+Enter in either field commits.
    let shortcut = (summary.has_focus() || body.has_focus())
        && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
    if can_commit && (clicked.clicked() || shortcut) {
        action = Some(Action::Commit);
    }
    action
}

/// The file row widget is shared with commit details; line counts aren't
/// known for working files, so they are left out.
fn as_changed(f: &WorkingFile) -> ChangedFile {
    ChangedFile {
        path: f.path.clone(),
        old_path: f.old_path.clone(),
        status: f.status,
        additions: 0,
        deletions: 0,
        binary: false,
    }
}
