//! The bar that asks where to push a branch that isn't on a remote yet,
//! filled in with the same name on "origin" (or the only remote).

use eframe::egui::{self, RichText};
use git_raccoon_core::refs::Refs;

use crate::repo_view::PushTarget;
use crate::theme;
use crate::ui::dialog::branch_name_problem;

pub enum Answer {
    Push,
    Cancel,
}

/// Shows the question and the remote and branch to push to. Returns the
/// answer once given.
pub fn show(ui: &mut egui::Ui, target: &mut PushTarget, refs: &Refs) -> Option<Answer> {
    let mut answer = None;
    ui.horizontal(|ui| {
        ui.label(format!(
            "Where should {} push to and pull from?",
            target.branch
        ));
        if refs.remotes.len() > 1 {
            let combo = egui::ComboBox::from_id_salt("push-remote")
                .selected_text(target.remote.as_str())
                .show_ui(ui, |ui| {
                    for r in &refs.remotes {
                        ui.selectable_value(&mut target.remote, r.name.clone(), &r.name);
                    }
                });
            ui.ctx()
                .accesskit_node_builder(combo.response.id, |n| n.set_label("Push to remote"));
        } else {
            ui.label(RichText::new(&target.remote).strong());
        }
        ui.label("/");
        let name = target.name.trim().to_string();
        let problem = branch_name_problem(&name);
        let exists = refs
            .remotes
            .iter()
            .filter(|r| r.name == target.remote)
            .flat_map(|r| &r.branches)
            .any(|b| b.name == name);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Cancel").clicked() {
                answer = Some(Answer::Cancel);
            }
            let label = RichText::new(format!("Push to {}/{name}", target.remote));
            let go = if problem.is_none() {
                egui::Button::new(label.color(theme::on_accent()).strong()).fill(theme::accent())
            } else {
                egui::Button::new(label.color(theme::muted())).fill(theme::bg_hover())
            };
            if ui.add_enabled(problem.is_none(), go).clicked() {
                answer = Some(Answer::Push);
            }
            // Why it can't be pushed there, or what pushing there does.
            if let Some(problem) = problem.filter(|p| !p.is_empty()) {
                ui.label(RichText::new(problem).size(12.0).color(theme::danger()));
            } else if exists {
                ui.label(
                    RichText::new("Already there: your commits go on top.")
                        .size(12.0)
                        .color(theme::muted()),
                );
            }
            let field = ui.add(
                egui::TextEdit::singleline(&mut target.name)
                    .hint_text("Branch name")
                    .desired_width(f32::INFINITY),
            );
            ui.ctx()
                .accesskit_node_builder(field.id, |n| n.set_label("Remote branch name"));
            if field.lost_focus()
                && problem.is_none()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
            {
                answer = Some(Answer::Push);
            }
        });
    });
    answer
}
