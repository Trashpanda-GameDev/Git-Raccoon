//! The screen shown when no repository is open.

use eframe::egui::{self, Align, Layout, RichText, Sense, Stroke, StrokeKind};

use crate::settings::RecentRepo;
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Browse,
    Clone,
    Init,
    Open(String),
    Forget(String),
}

pub fn show(
    ui: &mut egui::Ui,
    logo: &egui::TextureHandle,
    recent: &[RecentRepo],
    now: i64,
) -> Option<Action> {
    let mut action = None;
    let width = 560.0_f32.min(ui.available_width() - 32.0);
    ui.vertical_centered(|ui| {
        let content_height = 240.0 + 52.0 * recent.len().min(8) as f32;
        ui.add_space(((ui.available_height() - content_height) / 2.0).max(24.0));
        ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(96.0, 96.0)));
        ui.add_space(8.0);
        ui.label(RichText::new("Git Raccoon").size(22.0).strong());
        ui.add_space(12.0);
        if widgets::primary_button(ui, "Open a repository…").clicked() {
            action = Some(Action::Browse);
        }
        ui.add_space(4.0);
        // Centred pair of secondary actions.
        let w = 300.0;
        ui.allocate_ui_with_layout(
            egui::vec2(w, 28.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                ui.add_space((w - 270.0) / 2.0);
                if ui.button("Clone a repository…").clicked() {
                    action = Some(Action::Clone);
                }
                if ui.button("New repository…").clicked() {
                    action = Some(Action::Init);
                }
            },
        );
        if recent.is_empty() {
            return;
        }
        ui.add_space(20.0);
        ui.allocate_ui_with_layout(egui::vec2(width, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.label(widgets::section_label("Recent"));
            egui::Frame::new()
                .fill(theme::BG_RAISED)
                .stroke(Stroke::new(1.0, theme::BORDER))
                .corner_radius(6)
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for repo in recent {
                        if let Some(a) = recent_row(ui, repo, now) {
                            action = Some(a);
                        }
                    }
                });
        });
    });
    action
}

/// One recent repo: name and path, age, and a remove button.
pub fn recent_row(ui: &mut egui::Ui, repo: &RecentRepo, now: i64) -> Option<Action> {
    let mut action = None;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 50.0), Sense::click());
    let remove_rect = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 20.0, rect.center().y),
        egui::vec2(24.0, 24.0),
    );
    let remove = ui.interact(
        remove_rect,
        ui.id().with(("forget", &repo.path)),
        Sense::click(),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, repo.name()));
    remove.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("Remove {} from recent", repo.name()),
        )
    });
    let painter = ui.painter();
    if response.hovered() || remove.hovered() {
        painter.rect_filled(rect, 0, theme::BG_HOVER);
    }
    let age = crate::format::age(repo.opened_at, now);
    let age_galley = painter.layout_no_wrap(age, theme::body(12.0), theme::MUTED);
    let text_right = rect.right() - 44.0 - age_galley.size().x - 12.0;
    let left = rect.left() + 12.0;
    widgets::text_in(
        painter,
        egui::Rect::from_min_max(
            egui::pos2(left, rect.top() + 6.0),
            egui::pos2(text_right, rect.top() + 26.0),
        ),
        repo.name(),
        theme::body(theme::FONT_SIZE).clone(),
        theme::TEXT,
    );
    widgets::text_in(
        painter,
        egui::Rect::from_min_max(
            egui::pos2(left, rect.top() + 26.0),
            egui::pos2(text_right, rect.bottom() - 6.0),
        ),
        &repo.path,
        theme::body(12.0),
        theme::MUTED,
    );
    painter.galley(
        egui::pos2(
            rect.right() - 44.0 - age_galley.size().x,
            rect.center().y - age_galley.size().y / 2.0,
        ),
        age_galley,
        theme::MUTED,
    );
    let x_colour = if remove.hovered() {
        theme::TEXT
    } else {
        theme::MUTED
    };
    painter.text(
        remove_rect.center(),
        egui::Align2::CENTER_CENTER,
        widgets::ICON_CLOSE,
        theme::body(14.0),
        x_colour,
    );
    if remove.hovered() {
        painter.rect_stroke(
            remove_rect,
            4,
            egui::Stroke::new(1.0, theme::BORDER),
            StrokeKind::Inside,
        );
    }
    if remove.on_hover_text("Remove from recent").clicked() {
        action = Some(Action::Forget(repo.path.clone()));
    } else if response.on_hover_text(&repo.path).clicked() {
        action = Some(Action::Open(repo.path.clone()));
    }
    action
}
