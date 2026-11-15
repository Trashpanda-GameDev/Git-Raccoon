//! The modal dialogs: confirmations, and the small forms for stashes,
//! branches and tags.

use eframe::egui::{self, RichText};
use git_raccoon_core::branch;
use git_raccoon_core::refs::Refs;

use crate::repo_view::{Dialog, Op};
use crate::theme;

/// Shows the open dialog, if any. Returns the operation to run when the
/// user confirms; closes the dialog on confirm, cancel or Esc.
pub fn show(ctx: &egui::Context, open: &mut Option<Dialog>, refs: &Refs) -> Option<Op> {
    let current = open.as_mut()?;
    let mut answer: Option<bool> = None;
    let modal = egui::Modal::new(egui::Id::new("dialog")).show(ctx, |ui| {
        ui.set_width(420.0);
        let (button, danger, ready) = match current {
            Dialog::Confirm {
                title,
                body,
                button,
                danger,
                ..
            } => {
                heading(ui, title, Some(body));
                (button.clone(), *danger, true)
            }
            Dialog::Stash { message, untracked } => {
                heading(
                    ui,
                    "Stash changes",
                    Some(
                        "Puts all uncommitted changes aside, leaving a clean working tree. Apply or pop the stash later to get them back.",
                    ),
                );
                if field(ui, message, "Message (optional)", "Stash message", None) {
                    answer = Some(true);
                }
                ui.checkbox(untracked, "Include untracked files");
                ("Stash".into(), false, true)
            }
            Dialog::NewBranch {
                start_label,
                name,
                checkout,
                ..
            } => {
                heading(
                    ui,
                    "New branch",
                    Some(&format!("Starts at {start_label}.")),
                );
                let problem = branch_problem(name, refs, None);
                if field(ui, name, "Branch name", "Branch name", problem) && problem.is_none() {
                    answer = Some(true);
                }
                ui.checkbox(checkout, "Check it out");
                let label = if *checkout {
                    "Create and check out"
                } else {
                    "Create branch"
                };
                (label.into(), false, problem.is_none())
            }
            Dialog::RenameBranch { old, name } => {
                heading(ui, &format!("Rename {old}"), None);
                let problem = branch_problem(name, refs, Some(old));
                if field(ui, name, "New name", "New branch name", problem) && problem.is_none() {
                    answer = Some(true);
                }
                ("Rename".into(), false, problem.is_none() && name != old)
            }
            Dialog::NewTag {
                target_label,
                name,
                message,
                ..
            } => {
                heading(ui, "New tag", Some(&format!("Tags {target_label}.")));
                let problem = tag_problem(name, refs);
                if field(ui, name, "Tag name, e.g. v1.0.0", "Tag name", problem)
                    && problem.is_none()
                {
                    answer = Some(true);
                }
                ui.add_space(4.0);
                let notes = ui.add(
                    egui::TextEdit::multiline(message)
                        .hint_text("Message (optional; makes an annotated tag)")
                        .desired_rows(3)
                        .desired_width(f32::INFINITY),
                );
                ui.ctx()
                    .accesskit_node_builder(notes.id, |n| n.set_label("Tag message"));
                (
                    if message.trim().is_empty() {
                        "Create tag".into()
                    } else {
                        "Create annotated tag".into()
                    },
                    false,
                    problem.is_none(),
                )
            }
        };
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let (fill, text) = if !ready {
                (theme::BG_HOVER, theme::MUTED)
            } else if danger {
                (theme::DANGER.gamma_multiply(0.8), egui::Color32::WHITE)
            } else {
                (theme::ACCENT, theme::ON_ACCENT)
            };
            let go = egui::Button::new(RichText::new(button).color(text).strong()).fill(fill);
            if ui.add_enabled(ready, go).clicked() {
                answer = Some(true);
            }
            if ui.button("Cancel").clicked() {
                answer = Some(false);
            }
        });
    });
    if modal.should_close() && answer.is_none() {
        answer = Some(false);
    }
    match answer? {
        false => {
            *open = None;
            None
        }
        true => match open.take()? {
            Dialog::Confirm { op, .. } => Some(op),
            Dialog::Stash { message, untracked } => Some(Op::Stash { message, untracked }),
            Dialog::NewBranch {
                start,
                name,
                checkout,
                ..
            } => Some(Op::CreateBranch {
                name: name.trim().to_string(),
                start,
                checkout,
            }),
            Dialog::RenameBranch { old, name } => Some(Op::RenameBranch {
                old,
                new: name.trim().to_string(),
            }),
            Dialog::NewTag {
                target,
                name,
                message,
                ..
            } => Some(Op::CreateTag {
                name: name.trim().to_string(),
                target,
                message,
            }),
        },
    }
}

fn heading(ui: &mut egui::Ui, title: &str, body: Option<&str>) {
    ui.label(RichText::new(title).strong().size(15.0));
    if let Some(body) = body {
        ui.add_space(6.0);
        ui.label(RichText::new(body).color(theme::MUTED));
    }
    ui.add_space(8.0);
}

/// A one-line field, focused when the dialog opens, with a problem shown
/// under it. Returns whether Enter was pressed in it.
fn field(
    ui: &mut egui::Ui,
    text: &mut String,
    hint: &str,
    name: &str,
    problem: Option<&str>,
) -> bool {
    let response = ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
    ui.ctx()
        .accesskit_node_builder(response.id, |n| n.set_label(name));
    if ui.ctx().memory(|m| m.focused().is_none()) {
        response.request_focus();
    }
    if let Some(problem) = problem.filter(|p| !p.is_empty()) {
        ui.label(RichText::new(problem).size(12.0).color(theme::DANGER));
    }
    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
}

/// Why a branch name can't be used; empty for "nothing typed yet".
fn branch_problem(name: &str, refs: &Refs, renaming: Option<&str>) -> Option<&'static str> {
    let name = name.trim();
    if name.is_empty() {
        Some("")
    } else if !branch::is_valid_branch_name(name) {
        Some("Not a valid branch name. Avoid spaces, \"..\" and ~ ^ : ? * [ \\")
    } else if renaming != Some(name) && refs.local.iter().any(|b| b.name == name) {
        Some("A branch with this name already exists.")
    } else {
        None
    }
}

fn tag_problem(name: &str, refs: &Refs) -> Option<&'static str> {
    let name = name.trim();
    if name.is_empty() {
        Some("")
    } else if !branch::is_valid_tag_name(name) {
        Some("Not a valid tag name. Avoid spaces, \"..\" and ~ ^ : ? * [ \\")
    } else if refs.tags.iter().any(|t| t.name == name) {
        Some("A tag with this name already exists.")
    } else {
        None
    }
}
