//! A file's past: its history (each commit that changed it) or its blame
//! (who last changed each line).

use eframe::egui::{self, Color32, RichText, Sense, Stroke};
use egui_phosphor::regular as icon;

use crate::format;
use crate::repo_view::{FileMode, FileView};
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Close,
    Mode(FileMode),
    /// Show this commit's change to the file (at `path`, its name then).
    Pick {
        id: String,
        path: String,
    },
    /// Select this commit.
    GoTo(String),
}

const ROW: f32 = 20.0;
const GUTTER: f32 = 300.0;

/// Shows the header, and the history list or the blame. In history mode,
/// what's left of `ui` is for the picked commit's diff.
pub fn show(
    ui: &mut egui::Ui,
    view: &FileView,
    selected: Option<&str>,
    now: i64,
) -> Option<Action> {
    let mut action = None;
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin::symmetric(12, 0))
        .show(ui, |ui| {
            ui.set_height(36.0);
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new(&view.file).strong());
                let at = view.at.as_deref().map_or_else(
                    || "working copy".to_string(),
                    |id| format!("as of {}", format::short_sha(id)),
                );
                if view.mode == FileMode::Blame {
                    ui.label(RichText::new(at).size(12.0).color(theme::muted()));
                }
                if view.is_loading() {
                    ui.spinner();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, icon::X, "Close (Esc)").clicked() {
                        action = Some(Action::Close);
                    }
                    let mode = widgets::segmented(
                        ui,
                        view.mode,
                        &[(FileMode::Blame, "Blame"), (FileMode::History, "History")],
                    );
                    if mode != view.mode {
                        action = Some(Action::Mode(mode));
                    }
                });
            });
        });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );
    let inner = match view.mode {
        FileMode::History => history(ui, view, selected, now),
        FileMode::Blame => blame(ui, view, now),
    };
    action.or(inner)
}

fn message(ui: &mut egui::Ui, text: &str, colour: Color32) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(RichText::new(text).color(colour));
    });
}

fn history(ui: &mut egui::Ui, view: &FileView, selected: Option<&str>, now: i64) -> Option<Action> {
    let mut action = None;
    egui::Panel::left("file-history-list")
        .resizable(true)
        .default_size(360.0)
        .frame(egui::Frame::new().fill(theme::bg_raised()))
        .show(ui, |ui| {
            let changes = match &view.history {
                None => return message(ui, "Loading…", theme::muted()),
                Some(Err(e)) => return message(ui, e, theme::danger()),
                Some(Ok(c)) => c,
            };
            if changes.is_empty() {
                return message(ui, "No commits changed this file.", theme::muted());
            }
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_rows(ui, 44.0, changes.len(), |ui, range| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for c in &changes[range] {
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 44.0),
                            Sense::click(),
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &c.summary)
                        });
                        let painter = ui.painter();
                        if selected == Some(c.id.as_str()) {
                            painter.rect_filled(rect, 0, theme::bg_selected());
                        } else if response.hovered() {
                            painter.rect_filled(rect, 0, theme::bg_hover());
                        }
                        let inner = rect.shrink2(egui::vec2(12.0, 4.0));
                        let top = egui::Rect::from_min_max(
                            inner.min,
                            egui::pos2(inner.max.x, inner.center().y),
                        );
                        let bottom = egui::Rect::from_min_max(
                            egui::pos2(inner.min.x, inner.center().y),
                            inner.max,
                        );
                        widgets::text_in(
                            painter,
                            top,
                            &c.summary,
                            theme::body(13.0),
                            theme::text(),
                        );
                        let mut detail = format!(
                            "{}  ·  {}  ·  {}",
                            format::short_sha(&c.id),
                            c.author,
                            format::age(c.time, now)
                        );
                        if c.path != view.file {
                            detail.push_str(&format!("  ·  as {}", c.path));
                        }
                        widgets::text_in(
                            painter,
                            bottom,
                            &detail,
                            theme::body(11.0),
                            theme::muted(),
                        );
                        if response.clicked() {
                            action = Some(Action::Pick {
                                id: c.id.clone(),
                                path: c.path.clone(),
                            });
                        }
                    }
                });
        });
    action
}

fn blame(ui: &mut egui::Ui, view: &FileView, now: i64) -> Option<Action> {
    let mut action = None;
    let blame = match &view.blame {
        None => {
            message(ui, "Loading…", theme::muted());
            return None;
        }
        Some(Err(e)) => {
            message(ui, e, theme::danger());
            return None;
        }
        Some(Ok(b)) => b,
    };
    if blame.lines.is_empty() {
        message(ui, "This file is empty.", theme::muted());
        return None;
    }
    // Newer commits get a stronger stripe.
    let times: Vec<i64> = blame
        .commits
        .iter()
        .map(|c| c.time)
        .filter(|t| *t > 0)
        .collect();
    let (oldest, newest) = (
        times.iter().copied().min().unwrap_or(0),
        times.iter().copied().max().unwrap_or(1),
    );
    let digits = blame.lines.len().to_string().len();
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show_rows(ui, ROW, blame.lines.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for n in range {
                let line = &blame.lines[n];
                let commit = &blame.commits[line.commit];
                let width = ui.available_width().max(GUTTER + 400.0);
                let (rect, _) = ui.allocate_exact_size(egui::vec2(width, ROW), Sense::hover());
                let gutter = egui::Rect::from_min_size(rect.min, egui::vec2(GUTTER, ROW));
                let response = ui.interact(gutter, ui.id().with(("blame", n)), Sense::click());
                let painter = ui.painter();
                painter.rect_filled(gutter, 0, theme::bg_raised());
                // The age stripe.
                #[expect(clippy::cast_precision_loss, reason = "a colour weight")]
                let recency = if commit.time == 0 {
                    1.0
                } else {
                    (commit.time - oldest) as f32 / ((newest - oldest).max(1)) as f32
                };
                painter.rect_filled(
                    egui::Rect::from_min_size(gutter.min, egui::vec2(3.0, ROW)),
                    0,
                    theme::accent().gamma_multiply(0.25 + 0.75 * recency),
                );
                if line.starts_run {
                    painter.hline(
                        rect.x_range(),
                        rect.top(),
                        Stroke::new(1.0, theme::border()),
                    );
                    let who = match &commit.id {
                        Some(id) => format!(
                            "{}  {}  {}",
                            format::short_sha(id),
                            commit.author,
                            format::age(commit.time, now)
                        ),
                        None => "Not committed yet".to_string(),
                    };
                    widgets::text_in(
                        painter,
                        gutter.shrink2(egui::vec2(10.0, 0.0)),
                        &who,
                        theme::body(11.0),
                        if response.hovered() {
                            theme::text()
                        } else {
                            theme::muted()
                        },
                    );
                    if let Some(id) = &commit.id {
                        let label = format!(
                            "Commit {} by {}: {}",
                            format::short_sha(id),
                            commit.author,
                            commit.summary
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label)
                        });
                        if response.clicked() {
                            action = Some(Action::GoTo(id.clone()));
                        }
                        let tip = format!("{}\n{}", commit.summary, format::date_time(commit.time));
                        response.on_hover_text(tip);
                    }
                }
                let number = format!("{:>digits$}", n + 1);
                painter.text(
                    egui::pos2(gutter.right() + 8.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    number,
                    egui::FontId::monospace(12.0),
                    theme::muted(),
                );
                #[expect(clippy::cast_precision_loss, reason = "a few digits")]
                let text_x = gutter.right() + 20.0 + digits as f32 * 8.0;
                painter.text(
                    egui::pos2(text_x, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    line.text.replace('\t', "    "),
                    egui::FontId::monospace(12.0),
                    theme::text(),
                );
            }
        });
    action
}
