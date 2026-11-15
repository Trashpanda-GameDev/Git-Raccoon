//! The tab bar: one tab per open repository (or welcome screen), and +.

use eframe::egui::{self, Align2, Rect, Sense, Stroke};
use egui_phosphor::regular as icon;

use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Switch(usize),
    Close(usize),
    New,
    /// A tab dragged past its neighbour: move it from one place to another.
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
pub fn show(ui: &mut egui::Ui, tabs: &[(u64, String)], active: usize) -> Option<Action> {
    let mut action = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        // Tabs share the width (less room for +), each between MIN and MAX.
        #[expect(clippy::cast_precision_loss, reason = "a handful of tabs")]
        let width =
            ((ui.available_width() - 40.0) / tabs.len().max(1) as f32).clamp(MIN_WIDTH, MAX_WIDTH);
        for (i, (id, name)) in tabs.iter().enumerate() {
            let chosen = i == active;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(width, HEIGHT), Sense::hover());
            let response = ui.interact(rect, ui.id().with(("tab", id)), Sense::click_and_drag());
            // Dragged past the middle of a neighbour: take its place.
            if response.dragged()
                && let Some(x) = response.interact_pointer_pos().map(|p| p.x)
            {
                if x < rect.left() - width / 2.0 && i > 0 {
                    action = Some(Action::Move { from: i, to: i - 1 });
                } else if x > rect.right() + width / 2.0 && i + 1 < tabs.len() {
                    action = Some(Action::Move { from: i, to: i + 1 });
                }
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            }
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, true, chosen, name)
            });
            let painter = ui.painter();
            if chosen {
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
                painter.hline(
                    rect.x_range().shrink(6.0),
                    rect.top() + 1.0,
                    Stroke::new(2.0, theme::accent()),
                );
            } else if response.hovered() {
                painter.rect_filled(rect.shrink2(egui::vec2(2.0, 4.0)), 4, theme::bg_hover());
            }
            if i + 1 < tabs.len() && !chosen && i + 1 != active {
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
            let painter = ui.painter();
            let show_close = chosen || response.hovered() || close.hovered();
            if close.hovered() {
                painter.rect_filled(close_rect, 4, theme::bg_selected());
            }
            if show_close {
                painter.text(
                    close_rect.center(),
                    Align2::CENTER_CENTER,
                    icon::X,
                    theme::body(12.0),
                    theme::muted(),
                );
            }
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
                egui::pos2(close_rect.left() - 4.0, rect.bottom()),
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
            if close.clicked() || response.middle_clicked() {
                action = Some(Action::Close(i));
            } else if response.clicked() || response.drag_started() {
                action = Some(Action::Switch(i));
            }
            response.on_hover_text(name);
        }
        ui.add_space(6.0);
        if widgets::icon_button(ui, icon::PLUS, "New tab (Ctrl+T)").clicked() {
            action = Some(Action::New);
        }
    });
    action
}
