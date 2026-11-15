//! Planning an interactive rebase: the commits newest first, each with
//! what to do with it, reordered by dragging (or with the arrows).

use eframe::egui::{self, Color32, RichText, Sense, Stroke};
use egui_phosphor::regular as icon;
use git_raccoon_core::interactive;

use crate::repo_view::{PlanAction, Planner};
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Start,
    Cancel,
}

/// What's carried while dragging a commit: its row.
#[derive(Clone, Copy)]
struct DragRow(usize);

fn colour(action: PlanAction) -> Color32 {
    match action {
        PlanAction::Pick => theme::TEXT,
        PlanAction::Reword => theme::ACCENT,
        PlanAction::Squash | PlanAction::Fixup => theme::MODIFIED,
        PlanAction::Drop => theme::DANGER,
    }
}

pub fn show(
    ui: &mut egui::Ui,
    planner: &mut Planner,
    original: &[String],
    busy: bool,
) -> Option<Action> {
    let mut action = None;
    let problem = interactive::check(&planner.steps());
    let changed = planner.changes_anything(original);

    // Header.
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::LIST_NUMBERS).color(theme::ACCENT));
                ui.label(RichText::new("Interactive rebase").strong().size(15.0));
                ui.label(
                    RichText::new(format!(
                        "{} commits after {}",
                        planner.rows.len(),
                        planner.base_label
                    ))
                    .color(theme::MUTED),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, icon::X, "Close (Esc)").clicked() {
                        action = Some(Action::Cancel);
                    }
                    if ui.button("Cancel").clicked() {
                        action = Some(Action::Cancel);
                    }
                    let ready = !busy && changed && problem.is_none();
                    let label = RichText::new("Start rebase");
                    let go = if !ready {
                        egui::Button::new(label.color(theme::MUTED)).fill(theme::BG_HOVER)
                    } else if planner.pushed {
                        egui::Button::new(label.color(Color32::WHITE).strong())
                            .fill(theme::DANGER.gamma_multiply(0.8))
                    } else {
                        egui::Button::new(label.color(theme::ON_ACCENT).strong())
                            .fill(theme::ACCENT)
                    };
                    let tip = if !changed {
                        "Change something first"
                    } else {
                        problem.unwrap_or("Rewrite the commits as planned")
                    };
                    if ui
                        .add_enabled(ready, go)
                        .on_hover_text(tip)
                        .on_disabled_hover_text(tip)
                        .clicked()
                    {
                        action = Some(Action::Start);
                    }
                });
            });
            ui.label(
                RichText::new(
                    "Newest at the top. Drag commits to reorder them. Squash and fixup combine a commit with the one below it. The rewritten commits are new commits; if a step conflicts, you'll resolve it and continue.",
                )
                .size(12.0)
                .color(theme::MUTED),
            );
            if planner.pushed {
                ui.label(
                    RichText::new(format!(
                        "{}  Some of these commits are already pushed: afterwards you'll need to force-push.",
                        icon::WARNING
                    ))
                    .size(12.0)
                    .color(theme::MODIFIED),
                );
            }
            if let Some(p) = problem {
                ui.label(RichText::new(p).size(12.0).color(theme::DANGER));
            }
        });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::BORDER),
    );

    let mut moved: Option<(usize, usize)> = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    let count = planner.rows.len();
                    for i in 0..count {
                        if let Some(m) = row(ui, planner, i, count) {
                            moved = Some(m);
                        }
                        ui.add_space(4.0);
                    }
                    // The base, for orientation.
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "{}  {} (stays as it is)",
                            icon::ANCHOR,
                            planner.base_label
                        ))
                        .color(theme::MUTED),
                    );
                });
        });
    if let Some((from, to)) = moved
        && from != to
        && to < planner.rows.len()
    {
        let r = planner.rows.remove(from);
        planner.rows.insert(to, r);
    }
    drag_label(ui.ctx(), planner);
    action
}

/// One commit. Returns a move (from, to) if one was asked for.
fn row(ui: &mut egui::Ui, planner: &mut Planner, i: usize, count: usize) -> Option<(usize, usize)> {
    let mut moved = None;
    let frame = egui::Frame::new()
        .fill(theme::BG_RAISED)
        .stroke(Stroke::new(1.0, theme::BORDER))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(8, 6));
    let summary = planner.rows[i].summary.clone();
    let shown = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            // Drag handle.
            let (handle, response) = ui.allocate_exact_size(egui::vec2(18.0, 22.0), Sense::drag());
            ui.painter().text(
                handle.center(),
                egui::Align2::CENTER_CENTER,
                icon::DOTS_SIX_VERTICAL,
                theme::body(14.0),
                if response.hovered() {
                    theme::TEXT
                } else {
                    theme::MUTED
                },
            );
            response.dnd_set_drag_payload(DragRow(i));
            let response = response.on_hover_cursor(egui::CursorIcon::Grab);
            let name = format!("Drag {summary}");
            ui.ctx()
                .accesskit_node_builder(response.id, |n| n.set_label(name));

            // Up/down, for the keyboard (and precise moves).
            if widgets::named_icon_button(ui, icon::ARROW_UP, &format!("Move {summary} up"), i > 0)
                .clicked()
            {
                moved = Some((i, i - 1));
            }
            if widgets::named_icon_button(
                ui,
                icon::ARROW_DOWN,
                &format!("Move {summary} down"),
                i + 1 < count,
            )
            .clicked()
            {
                moved = Some((i, i + 1));
            }

            // What to do with it.
            let r = &mut planner.rows[i];
            for a in PlanAction::ALL {
                let selected = r.action == a;
                let text = RichText::new(a.name()).size(12.0).color(if selected {
                    colour(a)
                } else {
                    theme::MUTED
                });
                let b = ui.selectable_label(selected, text).on_hover_text(a.help());
                let name = format!("{} {summary}", a.name());
                ui.ctx().accesskit_node_builder(b.id, |n| n.set_label(name));
                if b.clicked() {
                    r.action = a;
                }
            }
            ui.add_space(8.0);
            let mut text = RichText::new(&r.summary);
            if r.action == PlanAction::Drop {
                text = text.strikethrough().color(theme::MUTED);
            }
            ui.label(text);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(&r.id[..7.min(r.id.len())])
                        .monospace()
                        .color(theme::MUTED),
                );
            });
        });
        let r = &mut planner.rows[i];
        match r.action {
            PlanAction::Reword => {
                let field = ui.add(
                    egui::TextEdit::multiline(&mut r.new_message)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
                let name = format!("Message for {summary}");
                ui.ctx()
                    .accesskit_node_builder(field.id, |n| n.set_label(name));
            }
            PlanAction::Squash | PlanAction::Fixup => {
                ui.label(
                    RichText::new(format!(
                        "{}  combined into the commit below",
                        icon::ARROW_ELBOW_DOWN_LEFT
                    ))
                    .size(12.0)
                    .color(theme::MODIFIED),
                );
            }
            _ => {}
        }
    });

    // Dropping a dragged commit here puts it in this place.
    let target = ui.interact(
        shown.response.rect,
        ui.id().with(("plan-row", i)),
        Sense::hover(),
    );
    if let Some(dragged) = target.dnd_hover_payload::<DragRow>()
        && dragged.0 != i
    {
        // A line where it will go: above when moving up, below when moving down.
        let y = if dragged.0 > i {
            target.rect.top() - 2.0
        } else {
            target.rect.bottom() + 2.0
        };
        ui.painter()
            .hline(target.rect.x_range(), y, Stroke::new(2.0, theme::ACCENT));
    }
    if let Some(dragged) = target.dnd_release_payload::<DragRow>() {
        moved = Some((dragged.0, i));
    }
    moved
}

/// While a commit is dragged, its summary follows the pointer.
fn drag_label(ctx: &egui::Context, planner: &Planner) {
    let (Some(dragged), Some(pos)) = (
        egui::DragAndDrop::payload::<DragRow>(ctx),
        ctx.pointer_interact_pos(),
    ) else {
        return;
    };
    let Some(r) = planner.rows.get(dragged.0) else {
        return;
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("drag-commit"),
    ));
    let galley = painter.layout_no_wrap(r.summary.clone(), theme::body(12.0), theme::ON_ACCENT);
    let rect = egui::Rect::from_min_size(pos + egui::vec2(14.0, 10.0), galley.size())
        .expand2(egui::vec2(8.0, 4.0));
    painter.rect_filled(rect, 6, theme::ACCENT);
    painter.galley(rect.min + egui::vec2(8.0, 4.0), galley, theme::ON_ACCENT);
}
