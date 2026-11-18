//! The tab bar: one tab per open repository (or welcome screen), and +.

use eframe::egui::{self, Align2, Painter, Rect, Sense, Stroke};
use egui_phosphor::regular as icon;

use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Switch(usize),
    Close(usize),
    New,
    /// A tab dropped in another place: move it from one place to another.
    Move {
        from: usize,
        to: usize,
    },
}

const HEIGHT: f32 = 32.0;
const MAX_WIDTH: f32 = 220.0;
const MIN_WIDTH: f32 = 90.0;

/// `tabs` are each tab's lasting id (so a tab dragged to a new place is
/// still the one being dragged) and name.
///
/// A dragged tab follows the pointer, see-through, and the others slide
/// apart where it would land; dropping it moves it there.
pub fn show(ui: &mut egui::Ui, tabs: &[(u64, String)], active: usize) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // Tabs share the width (less room for +), each between MIN and MAX.
        #[expect(clippy::cast_precision_loss, reason = "a handful of tabs")]
        let count = tabs.len().max(1) as f32;
        let width = ((ui.available_width() - 40.0) / count).clamp(MIN_WIDTH, MAX_WIDTH);
        let (bar, _) = ui.allocate_exact_size(egui::vec2(width * count, HEIGHT), Sense::hover());

        // The tab being dragged, and how far from its left edge it was taken hold of.
        let drag_id = ui.id().with("dragged-tab");
        let mut dragged: Option<(u64, f32)> = ui.data(|d| d.get_temp(drag_id));
        let pointer = ui.ctx().pointer_interact_pos();
        let from = dragged.and_then(|(id, _)| tabs.iter().position(|(t, _)| *t == id));
        let floating_left = dragged.zip(pointer).map(|((_, grab), p)| p.x - grab);
        // Where it would land: the place its left edge is nearest.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "rounded and kept at zero or more"
        )]
        let to = from.and(floating_left).map(|left| {
            (((left - bar.left()) / width).round().max(0.0) as usize).min(tabs.len() - 1)
        });

        for (i, (id, name)) in tabs.iter().enumerate() {
            let chosen = i == active;
            let is_dragged = from == Some(i);
            // Its place in the row: with the dragged tab taken out, and a
            // gap where it would land.
            let place = match (from, to) {
                (Some(f), Some(t)) if !is_dragged => {
                    let without = if i > f { i - 1 } else { i };
                    if without >= t { without + 1 } else { without }
                }
                _ => i,
            };
            #[expect(clippy::cast_precision_loss, reason = "a handful of tabs")]
            let resting = bar.left() + place as f32 * width;
            let x_id = ui.id().with(("tab-x", id));
            let x = match floating_left.filter(|_| is_dragged) {
                // Under the pointer; dropped, it slides from there.
                Some(left) => ui.ctx().animate_value_with_time(x_id, left, 0.0),
                None => ui
                    .ctx()
                    .animate_value_with_time(x_id, resting, widgets::DRAG_SLIDE),
            };
            let rect = Rect::from_min_size(egui::pos2(x, bar.top()), egui::vec2(width, HEIGHT));

            let response = ui.interact(rect, ui.id().with(("tab", id)), Sense::click_and_drag());
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, chosen, name)
            });
            // Held where it was pressed (a drag only starts once the pointer moves).
            if response.drag_started()
                && let Some(p) = ui.input(|i| i.pointer.press_origin())
            {
                dragged = Some((*id, p.x - rect.left()));
            }
            if response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            if response.drag_stopped() {
                if let (Some(from), Some(to)) = (from, to)
                    && from != to
                {
                    action = Some(Action::Move { from, to });
                }
                dragged = None;
            }

            // The dragged tab is drawn above everything, see-through.
            let painter = if is_dragged {
                let mut p = ui
                    .ctx()
                    .layer_painter(egui::LayerId::new(egui::Order::Tooltip, drag_id));
                p.multiply_opacity(widgets::DRAG_OPACITY);
                p
            } else {
                ui.painter().clone()
            };
            let hovered = response.hovered() && from.is_none();
            if chosen || is_dragged {
                draw_raised(&painter, rect, chosen);
            } else if hovered {
                painter.rect_filled(rect.shrink2(egui::vec2(2.0, 4.0)), 4, theme::bg_hover());
            }
            // A divider, between tabs that aren't raised (not while dragging,
            // when tabs are on the move).
            if from.is_none() && i + 1 < tabs.len() && !chosen && i + 1 != active {
                painter.vline(
                    rect.right(),
                    rect.y_range().shrink(9.0),
                    Stroke::new(1.0, theme::border()),
                );
            }

            // Close button at the right.
            let close_rect = Rect::from_center_size(
                egui::pos2(rect.right() - 16.0, rect.center().y),
                egui::vec2(20.0, 20.0),
            );
            let close = ui.interact(close_rect, ui.id().with(("close-tab", i)), Sense::click());
            let close_name = format!("Close tab {name}");
            close.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &close_name)
            });
            if close.hovered() && from.is_none() {
                painter.rect_filled(close_rect, 4, theme::bg_selected());
            }
            if chosen || hovered || close.hovered() {
                painter.text(
                    close_rect.center(),
                    Align2::CENTER_CENTER,
                    icon::X,
                    theme::body(12.0),
                    theme::muted(),
                );
            }
            draw_name(&painter, rect, close_rect, name, chosen);

            if from.is_none() {
                if close.clicked() || response.middle_clicked() {
                    action = Some(Action::Close(i));
                } else if response.clicked() || response.drag_started() {
                    action = Some(Action::Switch(i));
                }
            }
            // No tip while one is on the move.
            if from.is_none() {
                response.on_hover_text(name);
            }
        }
        ui.data_mut(|d| match dragged {
            Some(drag) => {
                d.insert_temp(drag_id, drag);
            }
            None => d.remove::<(u64, f32)>(drag_id),
        });

        ui.add_space(6.0);
        if widgets::icon_button(ui, icon::PLUS, "New tab (Ctrl+T)").clicked() {
            action = Some(Action::New);
        }
    });
    action
}

/// The raised look of the tab being shown (and of one being dragged).
fn draw_raised(painter: &Painter, rect: Rect, chosen: bool) {
    painter.rect_filled(
        rect,
        egui::CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        },
        theme::bg_raised(),
    );
    if chosen {
        painter.hline(
            rect.x_range().shrink(6.0),
            rect.top() + 1.0,
            Stroke::new(2.0, theme::accent()),
        );
    }
}

/// The tab's icon and name, up to its close button.
fn draw_name(painter: &Painter, rect: Rect, close: Rect, name: &str, chosen: bool) {
    let glyph = if name == "New tab" {
        icon::HOUSE
    } else {
        icon::GIT_BRANCH
    };
    painter.text(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        Align2::CENTER_CENTER,
        glyph,
        theme::body(13.0),
        if chosen {
            theme::accent()
        } else {
            theme::muted()
        },
    );
    let label = Rect::from_min_max(
        egui::pos2(rect.left() + 30.0, rect.top()),
        egui::pos2(close.left() - 4.0, rect.bottom()),
    );
    widgets::text_in(
        painter,
        label,
        name,
        theme::body(13.0),
        if chosen {
            theme::text()
        } else {
            theme::muted()
        },
    );
}
