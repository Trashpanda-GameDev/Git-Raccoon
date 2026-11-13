//! The commit list with its graph. Only the rows on screen are drawn.

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, epaint::CubicBezierShape};
use egui_phosphor::regular as icon;
use git_raccoon_core::repo::{GraphCommit, RefKind};

use crate::format;
use crate::repo_view::RepoView;
use crate::theme;
use crate::ui::widgets;

pub const ROW_HEIGHT: f32 = 28.0;
const LANE_WIDTH: f32 = 18.0;
const GRAPH_PADDING: f32 = 12.0;
const DOT_RADIUS: f32 = 6.0;
/// Very wide graphs are clipped to this.
const MAX_GRAPH_COLUMN: f32 = 320.0;
/// Rows drawn above and below the visible ones.
const OVERSCAN: usize = 6;
/// Below this width, the author and SHA columns are dropped.
const NARROW: f32 = 820.0;

#[expect(clippy::cast_precision_loss, reason = "lane counts are small")]
fn lane_x(left: f32, lane: usize) -> f32 {
    left + GRAPH_PADDING + lane as f32 * LANE_WIDTH
}

#[expect(clippy::cast_precision_loss, reason = "lane counts are small")]
pub fn graph_column_width(lanes: usize) -> f32 {
    (lanes.max(1) as f32 * LANE_WIDTH + GRAPH_PADDING).clamp(64.0, MAX_GRAPH_COLUMN)
}

struct Columns {
    refs: f32,
    graph: f32,
    author: f32,
    date: f32,
    sha: f32,
}

impl Columns {
    fn new(width: f32, lanes: usize) -> Self {
        let narrow = width < NARROW;
        Columns {
            refs: if narrow { 140.0 } else { 200.0 },
            graph: graph_column_width(lanes),
            author: if narrow { 0.0 } else { 140.0 },
            date: 110.0,
            sha: if narrow { 0.0 } else { 72.0 },
        }
    }

    fn message(&self, width: f32) -> f32 {
        (width - self.refs - self.graph - self.author - self.date - self.sha).max(120.0)
    }
}

pub struct Scroll {
    offset: f32,
    height: f32,
}

impl Default for Scroll {
    fn default() -> Self {
        Scroll {
            offset: 0.0,
            height: 600.0,
        }
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "row counts stay far below f32 precision limits for positions"
)]
fn row_top(row: usize) -> f32 {
    row as f32 * ROW_HEIGHT
}

/// Shows the history; returns the row clicked, if any.
pub fn show(
    ui: &mut egui::Ui,
    repo: &mut RepoView,
    scroll: &mut Scroll,
    now: i64,
) -> Option<usize> {
    let total = repo.history.total();
    let width = ui.available_width();
    let cols = Columns::new(width, repo.history.width());

    // Column headings.
    let (header, _) = ui.allocate_exact_size(egui::vec2(width, 28.0), Sense::hover());
    let painter = ui.painter();
    let heading = |x: f32, w: f32, text: &str| {
        if w > 0.0 {
            let r = Rect::from_min_size(
                Pos2::new(header.left() + x + 8.0, header.top()),
                egui::vec2(w - 12.0, header.height()),
            );
            widgets::text_in(
                painter,
                r,
                &text.to_uppercase(),
                theme::body(11.0),
                theme::MUTED,
            );
        }
    };
    let mut x = 0.0;
    for (w, text) in [
        (cols.refs, "Branch / Tag"),
        (cols.graph, "Graph"),
        (cols.message(width), "Commit message"),
        (cols.author, "Author"),
        (cols.date, "Date"),
        (cols.sha, "SHA"),
    ] {
        heading(x, w, text);
        x += w;
    }
    painter.hline(
        header.x_range(),
        header.bottom(),
        Stroke::new(1.0, theme::BORDER),
    );

    if total == 0 {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("This repository has no commits yet.").color(theme::MUTED))
        });
        return None;
    }

    let mut area = egui::ScrollArea::vertical()
        .id_salt("history")
        .auto_shrink([false, false]);
    if let Some(row) = repo.scroll_to.take() {
        let top = row_top(row);
        let target = if top < scroll.offset {
            Some(top)
        } else if top + ROW_HEIGHT > scroll.offset + scroll.height {
            // Far jumps land mid-screen; single steps just reveal the row.
            let far = top > scroll.offset + scroll.height + ROW_HEIGHT * 2.0;
            Some(if far {
                top - scroll.height / 2.0
            } else {
                top + ROW_HEIGHT - scroll.height
            })
        } else {
            None
        };
        if let Some(t) = target {
            area = area.vertical_scroll_offset(t.max(0.0));
        }
    }

    let selected = repo.selected.as_ref().map(|s| s.id.clone());
    let searching = repo.search.is_active();
    let mut clicked = None;
    let output = area.show_viewport(ui, |ui, viewport| {
        ui.set_min_height(row_top(total));
        let origin = ui.max_rect().min;
        let full = ui.max_rect().width();

        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "viewport offsets are non-negative and bounded"
        )]
        let (first, last) = (
            ((viewport.min.y / ROW_HEIGHT).floor() as usize).saturating_sub(OVERSCAN),
            (((viewport.max.y / ROW_HEIGHT).ceil() as usize) + OVERSCAN).min(total),
        );
        repo.rows.ensure(first.saturating_sub(1)..last);

        // Row backgrounds and clicks first, so the graph draws on top.
        for row in first..last {
            let rect = Rect::from_min_size(
                Pos2::new(origin.x, origin.y + row_top(row)),
                egui::vec2(full, ROW_HEIGHT),
            );
            let response = ui.interact(rect, ui.id().with(("row", row)), Sense::click());
            let commit = repo.rows.get(row);
            if let Some(c) = commit {
                let label = c.summary.clone();
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label)
                });
            }
            let is_selected = commit.is_some_and(|c| selected.as_deref() == Some(c.id.as_str()));
            if is_selected {
                ui.painter().rect_filled(rect, 0, theme::BG_SELECTED);
                let lane = commit.map_or(0, |c| c.column);
                ui.painter().rect_filled(
                    Rect::from_min_size(rect.min, egui::vec2(3.0, ROW_HEIGHT)),
                    0,
                    theme::lane_color(lane),
                );
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 0, theme::BG_HOVER);
            }
            if response.clicked() && commit.is_some() {
                clicked = Some(row);
            }
        }

        // The graph: edges from each row to the next, then the dots.
        let graph_left = origin.x + cols.refs;
        let clip = Rect::from_x_y_ranges(
            graph_left..=graph_left + cols.graph,
            ui.clip_rect().y_range(),
        );
        let graph = ui.painter().with_clip_rect(clip);
        let centre = |row: usize| origin.y + row_top(row) + ROW_HEIGHT / 2.0;
        for row in first.saturating_sub(1)..last {
            let Some(c) = repo.rows.get(row) else {
                continue;
            };
            for e in &c.edges {
                let (a, b) = (
                    Pos2::new(lane_x(graph_left, e.from), centre(row)),
                    Pos2::new(lane_x(graph_left, e.to), centre(row + 1)),
                );
                let stroke = Stroke::new(2.0, theme::lane_color(e.lane));
                if (a.x - b.x).abs() < 0.5 {
                    graph.line_segment([a, b], stroke);
                } else {
                    let mid = (a.y + b.y) / 2.0;
                    graph.add(CubicBezierShape::from_points_stroke(
                        [a, Pos2::new(a.x, mid), Pos2::new(b.x, mid), b],
                        false,
                        Color32::TRANSPARENT,
                        stroke,
                    ));
                }
            }
        }
        for row in first..last {
            let Some(c) = repo.rows.get(row) else {
                continue;
            };
            let p = Pos2::new(lane_x(graph_left, c.column), centre(row));
            let colour = theme::lane_color(c.column);
            let is_head = c.refs.iter().any(|r| r.is_head);
            if is_head || selected.as_deref() == Some(c.id.as_str()) {
                graph.circle_stroke(p, DOT_RADIUS + 3.0, Stroke::new(2.0, colour));
            }
            if c.parents.len() > 1 {
                graph.circle_filled(p, DOT_RADIUS - 2.0, colour);
            } else {
                graph.circle_filled(p, DOT_RADIUS, theme::BG);
                graph.circle_stroke(p, DOT_RADIUS - 1.5, Stroke::new(3.0, colour));
            }
        }

        // Text cells.
        let painter = ui.painter();
        for row in first..last {
            let top = origin.y + row_top(row);
            let cell = |x: f32, w: f32| {
                Rect::from_min_size(
                    Pos2::new(origin.x + x + 8.0, top),
                    egui::vec2((w - 16.0).max(0.0), ROW_HEIGHT),
                )
            };
            let Some(c) = repo.rows.get(row) else {
                let bar = Rect::from_min_size(
                    Pos2::new(origin.x + cols.refs + cols.graph + 8.0, top + 9.0),
                    egui::vec2(240.0, 10.0),
                );
                painter.rect_filled(bar, 4, theme::BG_HOVER);
                continue;
            };
            let dim = searching && !repo.search.matches.contains(&row);
            let (text, muted) = if dim {
                (
                    theme::MUTED.gamma_multiply(0.6),
                    theme::MUTED.gamma_multiply(0.5),
                )
            } else {
                (theme::TEXT, theme::MUTED)
            };
            ref_pills(painter, cell(0.0, cols.refs), c, dim);
            let mut x = cols.refs + cols.graph;
            widgets::text_in(
                painter,
                cell(x, cols.message(full)),
                &c.summary,
                theme::body(theme::FONT_SIZE),
                text,
            );
            x += cols.message(full);
            if cols.author > 0.0 {
                widgets::text_in(
                    painter,
                    cell(x, cols.author),
                    &c.author,
                    theme::body(theme::FONT_SIZE),
                    muted,
                );
                x += cols.author;
            }
            widgets::text_in(
                painter,
                cell(x, cols.date),
                &format::age(c.time, now),
                theme::body(theme::FONT_SIZE),
                muted,
            );
            x += cols.date;
            if cols.sha > 0.0 {
                widgets::text_in(
                    painter,
                    cell(x, cols.sha),
                    format::short_sha(&c.id),
                    theme::mono(12.0),
                    muted,
                );
            }
        }
    });
    scroll.offset = output.state.offset.y;
    scroll.height = output.inner_rect.height();
    clicked
}

fn ref_pills(painter: &egui::Painter, cell: Rect, c: &GraphCommit, dim: bool) {
    let Some(first) = c.refs.first() else { return };
    let mut colour = theme::lane_color(c.column);
    if dim {
        colour = colour.gamma_multiply(0.45);
    }
    let glyph = match (first.kind, first.is_head) {
        (RefKind::Local, true) => icon::CHECK,
        (RefKind::Local, false) => "",
        (RefKind::Remote, _) => icon::CLOUD,
        (RefKind::Tag, _) => icon::TAG,
        (RefKind::Head, _) => icon::DIAMOND,
    };
    let label = if glyph.is_empty() {
        first.name.clone()
    } else {
        format!("{glyph} {}", first.name)
    };
    let more = c.refs.len() - 1;
    let reserve = if more > 0 { 34.0 } else { 0.0 };
    let left = Pos2::new(cell.left(), cell.center().y);
    let rect = widgets::pill(
        painter,
        left,
        cell.width() - reserve,
        &label,
        colour,
        first.is_head,
    );
    if more > 0 {
        widgets::pill(
            painter,
            Pos2::new(rect.right() + 4.0, cell.center().y),
            40.0,
            &format!("+{more}"),
            colour,
            false,
        );
    }
}
