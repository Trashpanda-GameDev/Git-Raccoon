//! A file's past, shown with its diff: its history (each commit that
//! changed it), down the side, and its blame (who last changed each line),
//! beside the lines.

use std::sync::Arc;

use eframe::egui::{self, Color32, RichText, Sense, Stroke};
use git_raccoon_core::file_history::{Blame, Change};
use git_raccoon_core::search::{Query, SearchField};

use crate::format;
use crate::theme;
use crate::ui::widgets;

/// The blame column's width.
pub const GUTTER: f32 = 230.0;

fn message(ui: &mut egui::Ui, text: &str, colour: Color32) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(RichText::new(text).color(colour));
    });
}

/// The file's history, as a list down the left of `ui`: each commit that
/// changed it. Returns the one clicked (its id, and the file's path then).
pub fn history(
    ui: &mut egui::Ui,
    list: Option<&Result<Arc<Vec<Change>>, String>>,
    file: &str,
    selected: Option<&str>,
    now: i64,
) -> Option<(String, String)> {
    let mut action = None;
    egui::Panel::left("file-history-list")
        .resizable(true)
        .default_size(280.0)
        .size_range(200.0..=520.0)
        .frame(egui::Frame::new().fill(theme::bg_raised()))
        .show(ui, |ui| {
            let changes = match list {
                None => return message(ui, "Loading…", theme::muted()),
                Some(Err(e)) => return message(ui, e, theme::danger()),
                Some(Ok(c)) => c,
            };
            if changes.is_empty() {
                return message(ui, "No commits changed this file.", theme::muted());
            }
            // Narrowed by what's typed in the search box at the top.
            let query = search_box(ui);
            let shown: Vec<&Change> = match &query {
                Ok(q) if !q.is_empty() => changes.iter().filter(|c| q.matches_change(c)).collect(),
                _ => changes.iter().collect(),
            };
            match &query {
                Err(e) => {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new("Can't read the date")
                                .size(12.0)
                                .color(theme::danger()),
                        )
                        .on_hover_text(e);
                    });
                }
                Ok(q) if !q.is_empty() => {
                    let text = match shown.len() {
                        0 => "No matches".to_string(),
                        n => format!("{n} of {}", changes.len()),
                    };
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(RichText::new(text).size(12.0).color(theme::muted()));
                    });
                }
                Ok(_) => {}
            }
            ui.add_space(4.0);
            let area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            widgets::gapless_rows(ui, area, 44.0, shown.len(), |ui, range| {
                for c in &shown[range] {
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
                    widgets::text_in(painter, top, &c.summary, theme::body(13.0), theme::text());
                    let mut detail = format!(
                        "{}  ·  {}  ·  {}",
                        format::short_sha(&c.id),
                        c.author,
                        format::age(c.time, now)
                    );
                    if c.path != file {
                        detail.push_str(&format!("  ·  as {}", c.path));
                    }
                    widgets::text_in(painter, bottom, &detail, theme::body(11.0), theme::muted());
                    if response.clicked() {
                        action = Some((c.id.clone(), c.path.clone()));
                    }
                }
            });
        });
    action
}

pub fn search_id() -> egui::Id {
    egui::Id::new("file-history-search")
}

/// The search box over a file's history, with the filters' button; what's
/// typed is kept while the app runs, for each file's history alike.
fn search_box(ui: &mut egui::Ui) -> Result<Query, String> {
    let key = search_id().with("query");
    let mut query: String = ui.data(|d| d.get_temp(key)).unwrap_or_default();
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.label(
            RichText::new(egui_phosphor::regular::MAGNIFYING_GLASS)
                .color(theme::muted())
                .size(14.0),
        );
        let field = ui.add(
            egui::TextEdit::singleline(&mut query)
                .id(search_id())
                .hint_text("Find in this file's history")
                .desired_width(ui.available_width() - 44.0),
        );
        ui.ctx()
            .accesskit_node_builder(field.id, |n| n.set_label("Search this file's history"));
        if (field.has_focus() || field.lost_focus())
            && ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
            query.clear();
        }
        if crate::ui::search_bar::filters_button(ui, &mut query, search_id()) {
            ui.memory_mut(|m| m.request_focus(search_id()));
        }
    });
    ui.data_mut(|d| d.insert_temp(key, query.clone()));
    Query::parse(&query, SearchField::Text)
}

/// The oldest and newest commit times in a blame, for its age stripes.
pub fn ages(blame: &Blame) -> (i64, i64) {
    let times = blame.commits.iter().map(|c| c.time).filter(|t| *t > 0);
    (times.clone().min().unwrap_or(0), times.max().unwrap_or(1))
}

/// One line's blame in `rect`: a stripe as strong as the commit is new,
/// and who and when where a run of lines from one commit starts. Returns
/// the commit to go to when clicked.
pub fn blame_cell(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    blame: &Blame,
    line: Option<usize>,
    starts_run: bool,
    (oldest, newest): (i64, i64),
    now: i64,
) -> Option<String> {
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0, theme::bg_raised());
    painter.vline(
        rect.right(),
        rect.y_range(),
        Stroke::new(1.0, theme::border()),
    );
    let n = line?;
    let line = blame.lines.get(n)?;
    let commit = &blame.commits[line.commit];
    #[expect(clippy::cast_precision_loss, reason = "a colour weight")]
    let recency = if commit.time == 0 {
        1.0
    } else {
        (commit.time - oldest) as f32 / ((newest - oldest).max(1)) as f32
    };
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
        0,
        theme::accent().gamma_multiply(0.25 + 0.75 * recency),
    );
    if !starts_run {
        return None;
    }
    let id = egui::Id::new(("blame", n));
    let response = ui.interact(rect, id, Sense::click());
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
        &painter,
        rect.shrink2(egui::vec2(10.0, 0.0)),
        &who,
        theme::body(11.0),
        if response.hovered() {
            theme::text()
        } else {
            theme::muted()
        },
    );
    let id = commit.id.as_ref()?;
    let label = format!(
        "Commit {} by {}: {}",
        format::short_sha(id),
        commit.author,
        commit.summary
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!(
            "{}\n{}",
            commit.summary,
            format::date_time(commit.time)
        ));
    response.clicked().then(|| id.clone())
}
