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
    // Some(Some(i)): button i; Some(None): cancelled.
    let mut answer: Option<Option<usize>> = None;
    let modal = egui::Modal::new(egui::Id::new("dialog")).show(ctx, |ui| {
        ui.set_width(420.0);
        let single = match current {
            Dialog::Choice { title, body, .. } => {
                heading(ui, title, Some(body));
                (String::new(), false, true)
            }
            Dialog::Worktree {
                folder,
                branch,
                new,
            } => {
                heading(
                    ui,
                    "Add worktree",
                    Some("A second working folder for this repository, with another branch checked out, so you can work on two branches at once."),
                );
                let f = ui.add(
                    egui::TextEdit::singleline(folder)
                        .hint_text("Folder")
                        .desired_width(f32::INFINITY),
                );
                ui.ctx()
                    .accesskit_node_builder(f.id, |n| n.set_label("Worktree folder"));
                ui.add_space(4.0);
                ui.checkbox(new, "Create a new branch (from the current commit)");
                let problem = if *new {
                    branch_problem(branch, refs, None)
                } else if branch.trim().is_empty() {
                    Some("")
                } else if !refs.local.iter().any(|b| b.name == branch.trim()) {
                    Some("There's no local branch with this name.")
                } else if refs.local.iter().any(|b| b.name == branch.trim() && b.is_head) {
                    Some("This branch is checked out here; pick another, or a new one.")
                } else {
                    None
                };
                let hint = if *new { "New branch name" } else { "Existing branch" };
                if field(ui, branch, hint, "Worktree branch", problem) && problem.is_none() {
                    answer = Some(Some(0));
                }
                let ready = problem.is_none() && !folder.trim().is_empty();
                ("Create worktree".into(), false, ready)
            }
            Dialog::Remote {
                original,
                name,
                url,
            } => {
                let adding = original.is_none();
                heading(
                    ui,
                    &original
                        .as_ref()
                        .map_or_else(|| "Add remote".to_string(), |o| format!("Edit remote {o}")),
                    adding.then_some("It's fetched once added, so its branches show up."),
                );
                let problem = remote_problem(name, refs, original.as_deref());
                let mut enter = field(ui, name, "Name, e.g. upstream", "Remote name", problem);
                ui.add_space(4.0);
                let url_field = ui.add(
                    egui::TextEdit::singleline(url)
                        .hint_text("URL, e.g. https://github.com/owner/repo.git")
                        .desired_width(f32::INFINITY),
                );
                ui.ctx()
                    .accesskit_node_builder(url_field.id, |n| n.set_label("Remote URL"));
                enter |= url_field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let ready = problem.is_none() && !url.trim().is_empty();
                if enter && ready {
                    answer = Some(Some(0));
                }
                (
                    if adding { "Add and fetch" } else { "Save" }.into(),
                    false,
                    ready,
                )
            }
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
                    answer = Some(Some(0));
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
                    answer = Some(Some(0));
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
                    answer = Some(Some(0));
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
                    answer = Some(Some(0));
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
        let buttons: Vec<(String, bool, bool)> = match current {
            Dialog::Choice { choices, .. } => choices
                .iter()
                .map(|(label, danger, _)| (label.clone(), *danger, true))
                .collect(),
            _ => vec![single],
        };
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            for (i, (button, danger, ready)) in buttons.into_iter().enumerate() {
                let (fill, text) = if !ready {
                    (theme::bg_hover(), theme::muted())
                } else if danger {
                    (theme::danger().gamma_multiply(0.8), egui::Color32::WHITE)
                } else if i == 0 {
                    (theme::accent(), theme::on_accent())
                } else {
                    (theme::bg_hover(), theme::text())
                };
                let go = egui::Button::new(RichText::new(button).color(text).strong()).fill(fill);
                if ui.add_enabled(ready, go).clicked() {
                    answer = Some(Some(i));
                }
            }
            if ui.button("Cancel").clicked() {
                answer = Some(None);
            }
        });
    });
    if modal.should_close() && answer.is_none() {
        answer = Some(None);
    }
    match answer? {
        None => {
            *open = None;
            None
        }
        Some(chosen) => match open.take()? {
            Dialog::Choice { choices, .. } => choices.into_iter().nth(chosen).map(|c| c.2),
            Dialog::Worktree {
                folder,
                branch,
                new,
            } => Some(Op::AddWorktree {
                folder: folder.trim().to_string(),
                branch: branch.trim().to_string(),
                new,
            }),
            Dialog::Remote {
                original,
                name,
                url,
            } => {
                let (name, url) = (name.trim().to_string(), url.trim().to_string());
                match original {
                    None => Some(Op::Then(vec![
                        Op::AddRemote {
                            name: name.clone(),
                            url,
                        },
                        Op::FetchRemote(name),
                    ])),
                    Some(old) => {
                        let current = refs
                            .remotes
                            .iter()
                            .find(|r| r.name == old)
                            .and_then(|r| r.url.clone());
                        let mut ops = Vec::new();
                        if name != old {
                            ops.push(Op::RenameRemote {
                                old,
                                new: name.clone(),
                            });
                        }
                        if current.as_deref() != Some(url.as_str()) {
                            ops.push(Op::SetRemoteUrl { name, url });
                        }
                        (!ops.is_empty()).then_some(Op::Then(ops))
                    }
                }
            }
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
        ui.label(RichText::new(body).color(theme::muted()));
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
        ui.label(RichText::new(problem).size(12.0).color(theme::danger()));
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

fn remote_problem(name: &str, refs: &Refs, editing: Option<&str>) -> Option<&'static str> {
    let name = name.trim();
    if name.is_empty() {
        Some("")
    } else if !git_raccoon_core::remote::is_valid_name(name) {
        Some("Not a valid remote name. Avoid spaces and ~ ^ : ? * [ \\")
    } else if editing != Some(name) && refs.remotes.iter().any(|r| r.name == name) {
        Some("A remote with this name already exists.")
    } else {
        None
    }
}
