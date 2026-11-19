//! The commit list with its graph. Only the rows on screen are drawn.

use crate::settings::{Column, LineStyle};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, epaint::CubicBezierShape};
use egui_phosphor::regular as icon;
use git_raccoon_core::repo::{GraphCommit, RefKind, RefLabel};

use crate::format;
use crate::repo_view::RepoView;
use crate::theme;
use crate::ui::widgets;

pub const ROW_HEIGHT: f32 = 28.0;
const LANE_WIDTH: f32 = 18.0;
const GRAPH_PADDING: f32 = 12.0;
const DOT_RADIUS: f32 = 6.0;
/// The length of the dashes (and gaps) in the working changes' line.
const WIP_DASH: f32 = 3.0;
/// The working changes' summary field grows with what's written, from
/// this...
const WIP_FIELD_MIN: f32 = 140.0;
/// ...to this (or the room there is), then scrolls.
const WIP_FIELD_MAX: f32 = 520.0;
const WIP_HINT: &str = "Write a summary";
/// Very wide graphs are clipped to this.
const MAX_GRAPH_COLUMN: f32 = 320.0;
/// Rows drawn above and below the visible ones.
const OVERSCAN: usize = 6;
/// How long after the list last scrolled hovering labels spreads them out.
const SCROLL_SETTLE: f32 = 0.3;
/// The commit message column is never narrower than this.
const MIN_MESSAGE: f32 = 120.0;
/// Below this width, the author and SHA columns are dropped.
const NARROW: f32 = 820.0;

#[expect(clippy::cast_precision_loss, reason = "lane counts are small")]
fn lane_x(left: f32, lane: usize) -> f32 {
    left + GRAPH_PADDING + lane as f32 * LANE_WIDTH
}

/// How wide all the lanes are, which can be more than the graph's column.
#[expect(clippy::cast_precision_loss, reason = "lane counts are small")]
fn lanes_width(lanes: usize) -> f32 {
    // As the column measures them, before it's held to its widest.
    lanes.max(1) as f32 * LANE_WIDTH + GRAPH_PADDING
}

/// How far the graph is scrolled sideways (when its lanes are wider than
/// its column), kept per repository.
fn graph_scroll_id(repo: &RepoView) -> egui::Id {
    egui::Id::new(("graph-x", repo.path()))
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
    /// The order they're in (every column, once).
    order: Vec<Column>,
}

impl Columns {
    /// `widths`: the widths the user gave columns; the rest are the defaults.
    fn new(width: f32, lanes: usize, order: &[Column], widths: &[(Column, f32)]) -> Self {
        let narrow = width < NARROW;
        let given = |column: Column, default: f32| {
            widths
                .iter()
                .find(|(c, _)| *c == column)
                .map_or(default, |(_, w)| *w)
        };
        Columns {
            refs: given(Column::Refs, if narrow { 140.0 } else { 200.0 }),
            graph: given(Column::Graph, graph_column_width(lanes)),
            author: if narrow {
                0.0
            } else {
                given(Column::Author, 140.0)
            },
            date: given(Column::Date, 110.0),
            sha: if narrow {
                0.0
            } else {
                given(Column::Sha, 72.0)
            },
            order: Column::sanitise(order),
        }
    }

    fn message(&self, width: f32) -> f32 {
        (width - self.refs - self.graph - self.author - self.date - self.sha).max(MIN_MESSAGE)
    }

    /// How much wider a column can get before the message column would be
    /// narrower than its heading.
    fn room(&self, width: f32, painter: &egui::Painter) -> f32 {
        (self.message(width) - min_width(painter, Column::Message)).max(0.0)
    }

    /// The order with `column` moved to where it would land if dropped with
    /// the pointer at `pointer` (in a list starting at `start`): before the
    /// first of the others whose middle is right of the pointer, however
    /// wide the moved column is.
    fn dropped_at(&self, column: Column, pointer: f32, start: f32, width: f32) -> Vec<Column> {
        let mut order: Vec<Column> = self
            .order
            .iter()
            .copied()
            .filter(|c| *c != column)
            .collect();
        let mut x = start;
        let mut at = order.len();
        for (i, c) in order.iter().enumerate() {
            let w = self.w(*c, width);
            if x + w / 2.0 > pointer {
                at = i;
                break;
            }
            x += w;
        }
        order.insert(at, column);
        order
    }

    /// A column's width, in a list `width` wide.
    fn w(&self, column: Column, width: f32) -> f32 {
        match column {
            Column::Refs => self.refs,
            Column::Graph => self.graph,
            Column::Message => self.message(width),
            Column::Author => self.author,
            Column::Date => self.date,
            Column::Sha => self.sha,
        }
    }

    /// Where a column starts.
    fn x(&self, column: Column, width: f32) -> f32 {
        self.order
            .iter()
            .take_while(|c| **c != column)
            .map(|c| self.w(*c, width))
            .sum()
    }

    /// The message column and the text columns right after it, as one
    /// stretch (for the working-changes row's wide text).
    fn message_run(&self, width: f32) -> (f32, f32) {
        let start = self
            .order
            .iter()
            .position(|c| *c == Column::Message)
            .unwrap_or(0);
        let run: f32 = self.order[start..]
            .iter()
            .take_while(|c| !matches!(c, Column::Refs | Column::Graph))
            .map(|c| self.w(*c, width))
            .sum();
        (self.x(Column::Message, width), run)
    }
}

/// The narrowest a column gets: room for its heading.
fn min_width(painter: &egui::Painter, column: Column) -> f32 {
    let title = column.title().to_uppercase();
    painter
        .layout_no_wrap(title, theme::body(11.0), theme::muted())
        .size()
        .x
        + 20.0
}

/// Where the heading being moved is kept: its column, and how far from its
/// left edge it was taken hold of.
fn moving_id(ui: &egui::Ui) -> egui::Id {
    ui.id().with("moving-heading")
}

/// The heading being moved, if any, and where its left edge is (it follows
/// the pointer).
fn moving(ui: &egui::Ui) -> Option<(Column, f32)> {
    let (column, grab) = ui.data(|d| d.get_temp::<(Column, f32)>(moving_id(ui)))?;
    let pointer = ui.ctx().pointer_interact_pos()?;
    Some((column, pointer.x - grab))
}

/// What was done to the headings.
enum Heading {
    /// A column was dropped in a new place, or the usual order asked for.
    Moved(Vec<Column>),
    /// A column's edge was dragged.
    Resized(Column, f32),
    /// A column's edge was double-clicked: fit it to what it shows.
    Fit(Column),
    /// The usual widths asked for.
    ResetWidths,
}

/// The column headings, in `cols`' order (while one is being moved, the
/// order it would have if dropped now: the others slide aside, and it
/// follows the pointer). A heading can be dragged left or right to move
/// its column, and its right edge to make the column wider or narrower;
/// double-clicking the edge fits the column to what it shows. Right-
/// clicking one offers the usual order and widths back.
fn headings(
    ui: &mut egui::Ui,
    header: Rect,
    cols: &Columns,
    width: f32,
    moved: Option<(Column, f32)>,
) -> Option<Heading> {
    let mut action = None;
    let state = moving_id(ui);
    for &column in &cols.order {
        let w = cols.w(column, width);
        if w <= 0.0 {
            continue;
        }
        let is_moving = moved.is_some_and(|(c, _)| c == column);
        let x_id = ui.id().with(("heading-x", column.title()));
        let x = match moved.filter(|_| is_moving) {
            // Under the pointer; dropped, it slides from there.
            Some((_, left)) => ui.ctx().animate_value_with_time(x_id, left, 0.0),
            None => ui.ctx().animate_value_with_time(
                x_id,
                header.left() + cols.x(column, width),
                widgets::DRAG_SLIDE,
            ),
        };
        let cell = Rect::from_min_size(Pos2::new(x, header.top()), egui::vec2(w, header.height()));
        let response = ui.interact(
            cell,
            ui.id().with(("heading", column.title())),
            Sense::click_and_drag(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, column.title())
        });
        // An open hand over it, a closed one while it's held.
        if response.hovered() && moved.is_none() {
            crate::ui::cursors::grab(ui.ctx());
        }
        // No tip while one is on the move.
        let response = if moved.is_none() {
            response.on_hover_text("Drag to move this column")
        } else {
            response
        };
        response.context_menu(|ui| {
            if ui.button("Put the columns back in order").clicked() {
                action = Some(Heading::Moved(Column::ALL.to_vec()));
                ui.close();
            }
            if ui.button("Put the column widths back").clicked() {
                action = Some(Heading::ResetWidths);
                ui.close();
            }
        });
        // Held where it was pressed (a drag only starts once the pointer moves).
        if response.drag_started()
            && let Some(p) = ui.input(|i| i.pointer.press_origin())
        {
            ui.data_mut(|d| d.insert_temp(state, (column, p.x - cell.left())));
        }
        if response.dragged() {
            crate::ui::cursors::grabbing(ui.ctx());
        }
        if response.drag_stopped() {
            ui.data_mut(|d| d.remove::<(Column, f32)>(state));
            action = Some(Heading::Moved(cols.order.clone()));
        }
        if is_moving {
            // Above everything, see-through.
            let mut painter = ui
                .ctx()
                .layer_painter(egui::LayerId::new(egui::Order::Tooltip, state));
            painter.multiply_opacity(widgets::DRAG_OPACITY);
            painter.rect(
                cell,
                4,
                theme::bg_raised(),
                Stroke::new(1.0, theme::border()),
                egui::StrokeKind::Inside,
            );
            draw_heading(&painter, cell, column.title(), theme::text());
        } else {
            draw_heading(ui.painter(), cell, column.title(), theme::muted());
        }
    }
    if moved.is_none()
        && let Some(edge) = column_edges(ui, header, cols, width)
    {
        action = Some(edge);
    }
    ui.painter().hline(
        header.x_range(),
        header.bottom(),
        Stroke::new(1.0, theme::border()),
    );
    action
}

/// The edges between headings: drag one to resize the column it belongs
/// to, double-click it to fit that column. The commit message takes the
/// room that's left, so the edge on its right resizes the column after it.
fn column_edges(ui: &mut egui::Ui, header: Rect, cols: &Columns, width: f32) -> Option<Heading> {
    let mut action = None;
    let shown: Vec<Column> = cols
        .order
        .iter()
        .copied()
        .filter(|c| cols.w(*c, width) > 0.0)
        .collect();
    for (i, &left) in shown.iter().enumerate() {
        let (column, grows) = match (left, shown.get(i + 1)) {
            (Column::Message, Some(&right)) => (right, -1.0),
            (Column::Message, None) => continue,
            _ => (left, 1.0),
        };
        let x = header.left() + cols.x(left, width) + cols.w(left, width);
        let rect = Rect::from_center_size(
            Pos2::new(x, header.center().y),
            egui::vec2(8.0, header.height()),
        );
        let response = ui.interact(
            rect,
            ui.id().with(("column-edge", left.title())),
            Sense::click_and_drag(),
        );
        let name = if column == left {
            format!("Resize {}", column.title())
        } else {
            format!("Resize {} from its left", column.title())
        };
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
        let response = response
            .on_hover_cursor(egui::CursorIcon::ResizeColumn)
            .on_hover_text("Drag to resize; double-click to fit");
        let active = response.hovered() || response.dragged();
        ui.painter().vline(
            x,
            header.y_range().shrink(if active { 2.0 } else { 7.0 }),
            Stroke::new(
                if active { 2.0 } else { 1.0 },
                if active {
                    theme::accent()
                } else {
                    theme::border()
                },
            ),
        );
        // The width it had when the drag began: the edge then stays under
        // the pointer (a drag only starts once the pointer has moved a bit).
        let start = ui.id().with(("column-edge-start", left.title()));
        if response.drag_started() {
            ui.data_mut(|d| d.insert_temp(start, cols.w(column, width)));
        }
        if response.double_clicked() {
            action = Some(Heading::Fit(column));
        } else if let Some(moved) = response.total_drag_delta() {
            let now = cols.w(column, width);
            let began = ui.data(|d| d.get_temp::<f32>(start)).unwrap_or(now);
            let wanted = began + moved.x * grows;
            let widest = now + cols.room(width, ui.painter());
            let w = wanted.min(widest).max(min_width(ui.painter(), column));
            if (w - now).abs() > f32::EPSILON {
                action = Some(Heading::Resized(column, w));
            }
        }
    }
    action
}

fn set_width(widths: &mut Vec<(Column, f32)>, column: Column, w: f32) {
    widths.retain(|(c, _)| *c != column);
    widths.push((column, w));
}

/// How wide `column` needs to be for the widest of what it shows in the
/// rows on screen (a long history isn't read all through for this).
fn fitted_width(
    ui: &egui::Ui,
    repo: &mut RepoView,
    column: Column,
    scroll: &Scroll,
    now: i64,
) -> f32 {
    if column == Column::Graph {
        return lanes_width(repo.history.width());
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "scroll offsets are non-negative and bounded"
    )]
    let rows = {
        let first = (scroll.offset / ROW_HEIGHT).floor() as usize;
        let last = (((scroll.offset + scroll.height) / ROW_HEIGHT).ceil() as usize + 1)
            .min(repo.history.total());
        first.min(last)..last
    };
    repo.rows.ensure(rows.clone());
    let painter = ui.painter();
    let measure = |text: String, font: egui::FontId| {
        painter.layout_no_wrap(text, font, theme::text()).size().x
    };
    let widest = rows
        .filter_map(|row| repo.rows.get(row))
        .map(|c| match column {
            Column::Refs => labels(c).first().map_or(0.0, |(r, on_remote)| {
                // A label at its whole length, and "+N" after it.
                let pill =
                    measure(ref_text(r, *on_remote), egui::FontId::proportional(12.0)) + 14.0;
                if labels(c).len() > 1 {
                    pill + 38.0
                } else {
                    pill
                }
            }),
            Column::Author => measure(c.author.clone(), theme::body(theme::FONT_SIZE)),
            Column::Date => measure(format::age(c.time, now), theme::body(theme::FONT_SIZE)),
            Column::Sha => measure(format::short_sha(&c.id).to_string(), theme::mono(12.0)),
            // The message takes what's left; the graph is measured above.
            Column::Message | Column::Graph => 0.0,
        })
        .fold(0.0, f32::max);
    // The cell's padding on each side.
    widest + 16.0
}

/// A thin scrollbar along the bottom of the graph's heading, for a graph
/// wider than its column. Returns where the graph is scrolled to.
fn graph_scrollbar(
    ui: &mut egui::Ui,
    header: Rect,
    cols: &Columns,
    width: f32,
    graph_x: f32,
    spare: f32,
) -> f32 {
    let left = header.left() + cols.x(Column::Graph, width) + 4.0;
    let track = Rect::from_min_max(
        Pos2::new(left, header.bottom() - 7.0),
        Pos2::new(left + cols.graph - 8.0, header.bottom() - 2.0),
    );
    let response = ui.interact(
        track.expand2(egui::vec2(0.0, 3.0)),
        ui.id().with("graph-scrollbar"),
        Sense::click_and_drag(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Slider, true, "Scroll the graph sideways")
    });
    let content = cols.graph + spare;
    let thumb_w = (track.width() * cols.graph / content).max(16.0);
    let travel = (track.width() - thumb_w).max(1.0);
    let mut x = graph_x;
    if response.dragged() {
        x += response.drag_delta().x * spare / travel;
    } else if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        x = (p.x - track.left() - thumb_w / 2.0) / travel * spare;
    }
    let x = x.clamp(0.0, spare);
    let painter = ui.painter();
    painter.rect_filled(track, 3, theme::bg_hover());
    let thumb = Rect::from_min_size(
        Pos2::new(track.left() + x / spare * travel, track.top()),
        egui::vec2(thumb_w, track.height()),
    );
    let colour = if response.hovered() || response.dragged() {
        theme::text()
    } else {
        theme::muted()
    };
    painter.rect_filled(thumb, 3, colour);
    x
}

fn draw_heading(painter: &egui::Painter, cell: Rect, text: &str, colour: Color32) {
    let r = Rect::from_min_size(
        Pos2::new(cell.left() + 8.0, cell.top()),
        egui::vec2(cell.width() - 12.0, cell.height()),
    );
    widgets::text_in(painter, r, &text.to_uppercase(), theme::body(11.0), colour);
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

/// What was clicked in the history.
pub enum Click {
    Row(usize),
    Working,
    /// What a menu in the sidebar does, asked for here: from a branch's or
    /// tag's label, or opening the commit's page on the forge.
    Branch(crate::ui::sidebar::Action),
    /// From a commit's context menu; `label` describes the commit.
    NewBranch {
        id: String,
        label: String,
    },
    NewTag {
        id: String,
        label: String,
    },
    CheckoutDetached {
        id: String,
        label: String,
    },
    CherryPick {
        id: String,
    },
    Revert {
        id: String,
    },
    Reset {
        id: String,
        label: String,
    },
    InteractiveRebase {
        id: String,
        label: String,
    },
    /// A branch label double-clicked: check it out.
    Checkout(String),
    /// A remote branch label double-clicked ("origin/main").
    CheckoutRemote(String),
    /// A branch label dragged onto a local branch's label: merge it there.
    Merge {
        source: String,
        target: String,
    },
    /// From a stash's context menu (its commit id).
    StashApply(String),
    StashPop(String),
    StashDrop(String),
}

/// A line from `a` down to `b` in the graph, bending between lanes as
/// `style` says (dotted for a stash's).
pub fn branch_line(
    painter: &egui::Painter,
    a: Pos2,
    b: Pos2,
    style: LineStyle,
    stroke: Stroke,
    dotted: bool,
) {
    let points: Vec<Pos2> = if (a.x - b.x).abs() < 0.5 {
        vec![a, b]
    } else {
        let mid = (a.y + b.y) / 2.0;
        match style {
            LineStyle::Curved => {
                let curve = CubicBezierShape::from_points_stroke(
                    [a, Pos2::new(a.x, mid), Pos2::new(b.x, mid), b],
                    false,
                    Color32::TRANSPARENT,
                    stroke,
                );
                if !dotted {
                    painter.add(curve);
                    return;
                }
                curve.flatten(Some(0.5))
            }
            LineStyle::Straight => vec![a, b],
            LineStyle::Angled => vec![a, Pos2::new(a.x, mid), Pos2::new(b.x, mid), b],
        }
    };
    if dotted {
        painter.extend(egui::Shape::dotted_line(&points, stroke.color, 6.0, 1.2));
    } else {
        painter.add(egui::Shape::line(points, stroke));
    }
}

/// Whether a row is a stash.
fn is_stash(c: &GraphCommit) -> bool {
    c.refs.iter().any(|r| r.kind == RefKind::Stash)
}

/// The branch a label stands for when dragged: a local or remote branch.
fn draggable(r: &RefLabel) -> Option<&str> {
    matches!(r.kind, RefKind::Local | RefKind::Remote).then_some(r.name.as_str())
}

/// Makes a label a drag source (a branch) and a drop target (a local
/// branch). Returns the merge asked for by a drop on it.
fn drag_and_drop(
    painter: &egui::Painter,
    response: &egui::Response,
    r: &RefLabel,
) -> Option<Click> {
    if let Some(name) = draggable(r) {
        crate::ui::sidebar::drag_source(response, name);
    }
    if r.kind != RefKind::Local {
        return None;
    }
    crate::ui::sidebar::dropped_branch(painter, response, &r.name).map(|source| Click::Merge {
        source,
        target: r.name.clone(),
    })
}

/// What double-clicking a label on `c` does.
fn label_click(c: &GraphCommit, r: &RefLabel) -> Option<Click> {
    match r.kind {
        RefKind::Local if !r.is_head => Some(Click::Checkout(r.name.clone())),
        RefKind::Remote => Some(Click::CheckoutRemote(r.name.clone())),
        RefKind::Tag => Some(Click::CheckoutDetached {
            id: c.id.clone(),
            label: format!("tag {}", r.name),
        }),
        RefKind::Local | RefKind::Head | RefKind::Stash => None,
    }
}

/// Shows the history; returns what was clicked, if anything.
pub fn show(
    ui: &mut egui::Ui,
    repo: &mut RepoView,
    scroll: &mut Scroll,
    now: i64,
    order: &mut Vec<Column>,
    widths: &mut Vec<(Column, f32)>,
    lines: LineStyle,
) -> Option<Click> {
    let total = repo.history.total();
    let width = ui.available_width();
    let mut cols = Columns::new(width, repo.history.width(), order, widths);
    // A graph wider than its column scrolls sideways.
    let scroll_id = graph_scroll_id(repo);
    let spare = (lanes_width(repo.history.width()) - cols.graph).max(0.0);
    let mut graph_x: f32 = ui
        .data(|d| d.get_temp::<f32>(scroll_id))
        .unwrap_or(0.0)
        .clamp(0.0, spare);

    // Column headings: drag one to move its column, or its edge to resize it.
    let (header, _) = ui.allocate_exact_size(egui::vec2(width, 28.0), Sense::hover());
    // While one is moved, everything is shown in the order it would have
    // if dropped now.
    let moved = moving(ui);
    if let (Some((column, _)), Some(pointer)) = (moved, ui.ctx().pointer_interact_pos()) {
        cols.order = cols.dropped_at(column, pointer.x, header.left(), width);
    }
    match headings(ui, header, &cols, width, moved) {
        Some(Heading::Moved(new)) => *order = new,
        Some(Heading::Resized(column, w)) => set_width(widths, column, w),
        Some(Heading::Fit(column)) => {
            let fit = fitted_width(ui, repo, column, scroll, now);
            let w = fit
                .min(cols.w(column, width) + cols.room(width, ui.painter()))
                .max(min_width(ui.painter(), column));
            set_width(widths, column, w);
        }
        Some(Heading::ResetWidths) => widths.clear(),
        None => {}
    }
    if spare > 0.0 {
        graph_x = graph_scrollbar(ui, header, &cols, width, graph_x, spare);
    }

    // The working changes sit on HEAD: a dashed line runs down to it,
    // until the list is scrolled past it.
    let wip = repo.status.as_ref().is_some_and(|s| !s.is_clean());
    let to_head = repo
        .history
        .head_row()
        .filter(|&row| row_top(row) + ROW_HEIGHT / 2.0 >= scroll.offset);
    if working_row(ui, repo, &cols, width, graph_x, to_head.is_some()) {
        return Some(Click::Working);
    }

    if total == 0 {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(
                egui::RichText::new("This repository has no commits yet.").color(theme::muted()),
            )
        });
        return None;
    }

    let mut area = egui::ScrollArea::vertical()
        .id_salt("history")
        .auto_shrink([false, false]);
    let jumped = repo.scroll_to;
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
    let head = repo.head_branch().map(str::to_string);
    let searching = repo.search.is_active();
    let mut clicked = None;
    let mut double_click = None;
    let pointer = ui.ctx().pointer_hover_pos();
    let overlay_id = ui.id().with("all labels");
    // The row whose labels are spread out, its label cell, and the area
    // they cover: kept while the pointer is over them, so they can be reached.
    let mut overlay: Option<(usize, Rect, Rect)> = ui.ctx().data(|d| d.get_temp(overlay_id));
    // (Or while one of their menus is open: it's outside them.)
    if overlay.is_some_and(|(_, cell, area)| {
        !pointer.is_some_and(|p| area.expand(2.0).contains(p) || cell.contains(p))
    }) && !egui::Popup::is_any_open(ui.ctx())
    {
        overlay = None;
    }
    // Not while the list scrolls (or just after): spread out under the
    // pointer, the labels would catch the wheel and stop it.
    let since_scroll = ui.input(|i| i.time_since_last_scroll());
    let scrolling = ui.input(egui::InputState::is_scrolling) || since_scroll < SCROLL_SETTLE;
    if scrolling {
        // Still on top this frame, they have the wheel's first turn: pass
        // it on to the list.
        let dy = ui.input(|i| i.smooth_scroll_delta.y);
        if jumped.is_none()
            && dy != 0.0
            && overlay.is_some_and(|(_, _, labels)| pointer.is_some_and(|p| labels.contains(p)))
        {
            area = area.vertical_scroll_offset((scroll.offset - dy).max(0.0));
        }
        overlay = None;
        // Look again once it has settled, in case the pointer rests on labels.
        ui.ctx()
            .request_repaint_after_secs(SCROLL_SETTLE - since_scroll.min(SCROLL_SETTLE));
    }
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
                ui.painter().rect_filled(rect, 0, theme::bg_selected());
                let lane = commit.map_or(0, |c| c.column);
                ui.painter().rect_filled(
                    Rect::from_min_size(rect.min, egui::vec2(3.0, ROW_HEIGHT)),
                    0,
                    theme::lane_color(lane),
                );
            } else if response.hovered() {
                ui.painter().rect_filled(rect, 0, theme::bg_hover());
            }
            if response.clicked() && commit.is_some() {
                clicked = Some(Click::Row(row));
            }
            if response.double_clicked() {
                double_click = response.interact_pointer_pos().map(|p| (row, p));
            }
            if let Some(c) = commit {
                let web = repo.web.commit(&c.id);
                response.context_menu(|ui| {
                    if let Some(action) = commit_menu(ui, c, web.as_ref(), head.as_deref(), None) {
                        clicked = Some(action);
                    }
                });
            }
        }

        // The graph: edges from each row to the next, then the dots.
        let column_left = origin.x + cols.x(Column::Graph, full);
        let clip = Rect::from_x_y_ranges(
            column_left..=column_left + cols.graph,
            ui.clip_rect().y_range(),
        );
        // Sideways: the horizontal wheel (or Shift and the wheel) over the
        // graph moves it, rather than the list.
        if spare > 0.0 && ui.rect_contains_pointer(clip) {
            let dx = ui.input(|i| i.smooth_scroll_delta.x);
            if dx != 0.0 {
                graph_x = (graph_x - dx).clamp(0.0, spare);
                ui.ctx().request_repaint();
            }
        }
        // A jump to a commit brings its lane into view.
        if let Some(row) = jumped
            && let Some(c) = repo.rows.get(row)
        {
            let at = lane_x(0.0, c.column);
            if at - graph_x < GRAPH_PADDING || at - graph_x > cols.graph - GRAPH_PADDING {
                graph_x = (at - cols.graph / 2.0).clamp(0.0, spare);
            }
        }
        ui.data_mut(|d| d.insert_temp(scroll_id, graph_x));
        let graph_left = column_left - graph_x;
        let graph = ui.painter().with_clip_rect(clip);
        let centre = |row: usize| origin.y + row_top(row) + ROW_HEIGHT / 2.0;
        for row in first.saturating_sub(1)..last {
            let Some(c) = repo.rows.get(row) else {
                continue;
            };
            let stash = is_stash(c);
            for e in &c.edges {
                let (a, b) = (
                    Pos2::new(lane_x(graph_left, usize::from(e.from)), centre(row)),
                    Pos2::new(lane_x(graph_left, usize::from(e.to)), centre(row + 1)),
                );
                let stroke = Stroke::new(2.0, theme::lane_color(usize::from(e.lane)));
                // A stash's line to its commit is dotted.
                let dotted = stash && usize::from(e.from) == c.column;
                branch_line(&graph, a, b, lines, stroke, dotted);
            }
        }
        if wip
            && let Some(row) = to_head
            && let Some(c) = repo.rows.get(row)
        {
            // From the top of the list (where the WIP row's line ends) to
            // HEAD's ring; dashes start on the pattern so they scroll with it.
            let x = lane_x(graph_left, c.column);
            let period = WIP_DASH * 2.0;
            let top = origin.y + (viewport.min.y / period).floor() * period;
            let end = centre(row) - DOT_RADIUS - 3.0;
            if end > top {
                graph.extend(egui::Shape::dashed_line(
                    &[Pos2::new(x, top), Pos2::new(x, end)],
                    Stroke::new(2.0, theme::muted()),
                    WIP_DASH,
                    WIP_DASH,
                ));
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
            if is_stash(c) {
                // A stash: its icon, not a dot.
                graph.circle_filled(p, DOT_RADIUS + 1.0, theme::bg());
                graph.text(
                    p,
                    egui::Align2::CENTER_CENTER,
                    icon::STACK,
                    theme::body(15.0),
                    colour,
                );
            } else if c.parents.len() > 1 {
                graph.circle_filled(p, DOT_RADIUS - 2.0, colour);
            } else {
                graph.circle_filled(p, DOT_RADIUS, theme::bg());
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
                    Pos2::new(origin.x + cols.x(Column::Message, full) + 8.0, top + 9.0),
                    egui::vec2(240.0, 10.0),
                );
                painter.rect_filled(bar, 4, theme::bg_hover());
                continue;
            };
            let dim = searching && !repo.search.matches.contains(&row);
            let (text, muted) = if dim {
                (
                    theme::muted().gamma_multiply(0.6),
                    theme::muted().gamma_multiply(0.5),
                )
            } else {
                (theme::text(), theme::muted())
            };
            let refs_cell = cell(cols.x(Column::Refs, full), cols.refs);
            // More than one label, or one cut short: hovering spreads them
            // out, at their full length.
            let cut_short = labels(c).first().is_some_and(|(r, on_remote)| {
                widgets::pill_elided(painter, &ref_text(r, *on_remote), refs_cell.width())
            });
            if (labels(c).len() > 1 || cut_short)
                && !scrolling
                && overlay.is_none()
                && pointer.is_some_and(|p| {
                    Rect::from_x_y_ranges(refs_cell.x_range(), top..=top + ROW_HEIGHT).contains(p)
                        // The list itself, not a menu or window over it.
                        && ui.ctx().layer_id_at(p) == Some(ui.layer_id())
                })
                && !egui::Popup::is_any_open(ui.ctx())
                && ui.clip_rect().contains(refs_cell.center())
            {
                overlay = Some((row, refs_cell, Rect::NOTHING));
            }
            // While they're spread out, "+N" is hidden.
            let spread = overlay.is_some_and(|(r, _, _)| r == row);
            let pills = ref_pills(painter, refs_cell, c, dim, spread);
            // A label can be dragged onto another branch to merge, and
            // double-clicked to check out; a click selects the commit.
            if let (Some(rect), Some((r, _))) = (pills.first(), labels(c).first()) {
                let response =
                    ui.interact(*rect, ui.id().with(("label", row)), Sense::click_and_drag());
                // Right-clicked, the commit's menu, with the label's own first.
                let web = repo.web.commit(&c.id);
                response.context_menu(|ui| {
                    if let Some(action) = commit_menu(ui, c, web.as_ref(), head.as_deref(), Some(r))
                    {
                        clicked = Some(action);
                    }
                });
                if let Some(merge) = drag_and_drop(painter, &response, r) {
                    clicked = Some(merge);
                } else if response.double_clicked() {
                    if let Some(action) = label_click(c, r) {
                        clicked = Some(action);
                    }
                } else if response.clicked() {
                    clicked = Some(Click::Row(row));
                }
            }
            // Double-clicking a label checks it out.
            if let Some((_, p)) = double_click.filter(|(r, _)| *r == row)
                && let Some(i) = pills.iter().position(|rect| rect.contains(p))
                && let Some(action) = labels(c).get(i).and_then(|(r, _)| label_click(c, r))
            {
                clicked = Some(action);
            }
            widgets::text_in(
                painter,
                cell(cols.x(Column::Message, full), cols.message(full)),
                &c.summary,
                theme::body(theme::FONT_SIZE),
                text,
            );
            if cols.author > 0.0 {
                widgets::text_in(
                    painter,
                    cell(cols.x(Column::Author, full), cols.author),
                    &c.author,
                    theme::body(theme::FONT_SIZE),
                    muted,
                );
            }
            widgets::text_in(
                painter,
                cell(cols.x(Column::Date, full), cols.date),
                &format::age(c.time, now),
                theme::body(theme::FONT_SIZE),
                muted,
            );
            if cols.sha > 0.0 {
                widgets::text_in(
                    painter,
                    cell(cols.x(Column::Sha, full), cols.sha),
                    format::short_sha(&c.id),
                    theme::mono(12.0),
                    muted,
                );
            }
        }

        if let Some((row, cell, _)) = overlay {
            if let Some(c) = repo.rows.get(row) {
                let web = repo.web.commit(&c.id);
                let (area, action) = all_labels(
                    ui.ctx(),
                    overlay_id,
                    cell,
                    row,
                    c,
                    (web.as_ref(), head.as_deref()),
                );
                overlay = Some((row, cell, area));
                if action.is_some() {
                    clicked = action;
                }
            } else {
                overlay = None;
            }
        }
    });
    ui.ctx().data_mut(|d| match overlay {
        Some(o) => {
            d.insert_temp(overlay_id, o);
        }
        None => d.remove::<(usize, Rect, Rect)>(overlay_id),
    });
    scroll.offset = output.state.offset.y;
    scroll.height = output.inner_rect.height();
    clicked
}

/// The "Working changes" row above the commits, shown while there are
/// uncommitted changes, with the commit summary to write in. `to_head`:
/// its dashed line goes on down (towards HEAD, below). Returns whether it
/// was clicked.
fn working_row(
    ui: &mut egui::Ui,
    repo: &mut RepoView,
    cols: &Columns,
    width: f32,
    graph_x: f32,
    to_head: bool,
) -> bool {
    let Some(status) = repo.status.as_ref().filter(|s| !s.is_clean()) else {
        return false;
    };
    let (staged, unstaged) = (status.staged.len(), status.unstaged.len());
    // Sit in HEAD's lane, so the row reads as "on top of" the checked-out commit.
    let lane = repo.history.head_row().map_or(0, |row| {
        repo.rows.ensure(row..row + 1);
        repo.rows.get(row).map_or(0, |c| c.column)
    });

    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Working changes")
    });
    // Cloned: the summary field below needs `ui` again.
    let painter = ui.painter().clone();
    if repo.working_selected {
        painter.rect_filled(rect, 0, theme::bg_selected());
        painter.rect_filled(
            Rect::from_min_size(rect.min, egui::vec2(3.0, ROW_HEIGHT)),
            0,
            theme::muted(),
        );
    } else if response.hovered() {
        painter.rect_filled(rect, 0, theme::bg_hover());
    }

    let cell = |x: f32, w: f32| {
        Rect::from_min_size(
            Pos2::new(rect.left() + x + 8.0, rect.top()),
            egui::vec2((w - 16.0).max(0.0), ROW_HEIGHT),
        )
    };
    widgets::pill(
        &painter,
        Pos2::new(
            rect.left() + cols.x(Column::Refs, width) + 8.0,
            rect.center().y,
        ),
        cols.refs - 16.0,
        "// WIP",
        theme::muted(),
        false,
    );

    // A hollow, dashed node with a dashed line down towards HEAD (kept in
    // the graph's column, scrolled with it).
    let column = Rect::from_x_y_ranges(
        rect.left() + cols.x(Column::Graph, width)
            ..=rect.left() + cols.x(Column::Graph, width) + cols.graph,
        rect.y_range(),
    );
    let graph = painter.with_clip_rect(column.intersect(painter.clip_rect()));
    let centre = Pos2::new(lane_x(column.left() - graph_x, lane), rect.center().y);
    let dashed = Stroke::new(2.0, theme::muted());
    if to_head {
        graph.extend(egui::Shape::dashed_line(
            &[
                Pos2::new(centre.x, centre.y + DOT_RADIUS),
                Pos2::new(centre.x, rect.bottom()),
            ],
            dashed,
            WIP_DASH,
            WIP_DASH,
        ));
    }
    graph.circle_filled(centre, DOT_RADIUS, theme::bg());
    let points: Vec<Pos2> = (0..=24)
        .map(|i| {
            #[expect(clippy::cast_precision_loss, reason = "i <= 24")]
            let a = i as f32 / 24.0 * std::f32::consts::TAU;
            centre + egui::vec2(a.cos(), a.sin()) * (DOT_RADIUS - 1.0)
        })
        .collect();
    graph.extend(egui::Shape::dashed_line(&points, dashed, 2.5, 2.0));

    // The commit summary (the same text as under Commit, and the stash's
    // title), then the counts right after it in the same (wide) column.
    let (start, run) = cols.message_run(width);
    let message = cell(start, run);
    let mut parts = Vec::new();
    if unstaged > 0 {
        parts.push(format!("{unstaged} unstaged"));
    }
    if staged > 0 {
        parts.push(format!("{staged} staged"));
    }
    let counts = painter.layout_no_wrap(parts.join(" · "), theme::body(12.0), theme::muted());
    // Only as wide as what's written, so the counts stay close by.
    let font = theme::body(theme::FONT_SIZE);
    let written = match repo.draft.summary.as_str() {
        "" => WIP_HINT,
        s => s,
    };
    let text = painter.layout_no_wrap(written.to_string(), font.clone(), theme::text());
    let room = message.width() - counts.size().x - 12.0;
    let field_width = (text.size().x + 24.0)
        .clamp(WIP_FIELD_MIN, WIP_FIELD_MAX)
        .min(room)
        .max(0.0);
    let field_right = message.left() + field_width;
    let field = Rect::from_min_max(
        Pos2::new(message.left() - 4.0, message.top() + 3.0),
        Pos2::new(field_right, message.bottom() - 3.0),
    );
    let committing = repo.is_committing();
    let summary = ui.put(
        field,
        egui::TextEdit::singleline(&mut repo.draft.summary)
            // Kept while the row's selection redraws what's around it.
            .id(egui::Id::new("wip-summary"))
            .hint_text(WIP_HINT)
            .font(font)
            .vertical_align(egui::Align::Center)
            .interactive(!committing),
    );
    ui.ctx()
        .accesskit_node_builder(summary.id, |n| n.set_label("WIP summary"));
    painter.galley(
        Pos2::new(
            field_right + 12.0,
            message.center().y - counts.size().y / 2.0,
        ),
        counts,
        theme::muted(),
    );
    painter.hline(
        rect.x_range(),
        rect.bottom(),
        Stroke::new(1.0, theme::border()),
    );
    // Writing the summary shows the changes being committed.
    response.clicked() || summary.gained_focus()
}

/// The labels to show for a commit. A local branch and its remote copy
/// ("main" and "origin/main") on the same commit are one label, marked as
/// being on the remote too (`true`).
fn labels(c: &GraphCommit) -> Vec<(&RefLabel, bool)> {
    let remote_copy = |local: &RefLabel, r: &RefLabel| {
        r.kind == RefKind::Remote
            && r.name
                .split_once('/')
                .is_some_and(|(_, rest)| rest == local.name)
    };
    let locals: Vec<&RefLabel> = c.refs.iter().filter(|r| r.kind == RefKind::Local).collect();
    c.refs
        .iter()
        .filter(|r| !locals.iter().any(|l| remote_copy(l, r)))
        .map(|r| {
            let on_remote =
                r.kind == RefKind::Local && c.refs.iter().any(|other| remote_copy(r, other));
            (r, on_remote)
        })
        .collect()
}

/// A label's text: its name after an icon for its kind, and a cloud after
/// it when the remote has it there too.
fn ref_text(r: &RefLabel, on_remote: bool) -> String {
    let glyph = match (r.kind, r.is_head) {
        (RefKind::Local, true) => icon::CHECK,
        (RefKind::Local, false) => "",
        (RefKind::Remote, _) => icon::CLOUD,
        (RefKind::Tag, _) => icon::TAG,
        (RefKind::Head, _) => icon::DIAMOND,
        (RefKind::Stash, _) => icon::STACK,
    };
    let mut text = if glyph.is_empty() {
        r.name.clone()
    } else {
        format!("{glyph} {}", r.name)
    };
    if on_remote {
        text.push_str("  ");
        text.push_str(icon::CLOUD);
    }
    text
}

/// Draws the first label, and "+N" for the others unless they're `spread`
/// out. Returns where the
/// first label is (as the rect of `labels(c)[0]`).
fn ref_pills(
    painter: &egui::Painter,
    cell: Rect,
    c: &GraphCommit,
    dim: bool,
    spread: bool,
) -> Vec<Rect> {
    let shown = labels(c);
    let Some(&(first, on_remote)) = shown.first() else {
        return Vec::new();
    };
    let mut colour = theme::lane_color(c.column);
    if dim {
        colour = colour.gamma_multiply(0.45);
    }
    let more = if spread { 0 } else { shown.len() - 1 };
    let reserve = if more > 0 { 34.0 } else { 0.0 };
    let left = Pos2::new(cell.left(), cell.center().y);
    let rect = widgets::pill(
        painter,
        left,
        cell.width() - reserve,
        &ref_text(first, on_remote),
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
    vec![rect]
}

/// How wide a label grows when they're spread out: room for long names.
const MAX_LABEL: f32 = 640.0;

/// Every label of a commit, one above the other, over the rows below:
/// shown while the pointer is over them. A double-click checks one out; a
/// click selects the commit. Returns the area they cover, and what was
/// clicked.
fn all_labels(
    ctx: &egui::Context,
    id: egui::Id,
    cell: Rect,
    row: usize,
    c: &GraphCommit,
    // The commit's page on the forge, and the checked-out branch: for the
    // right-click menu.
    (web, head): (Option<&(String, String)>, Option<&str>),
) -> (Rect, Option<Click>) {
    const GAP: f32 = 4.0;
    const PAD: f32 = 4.0;
    let colour = theme::lane_color(c.column);
    let mut action = None;
    let top = cell.center().y - 10.0 - PAD;
    let area = egui::Area::new(id)
        // Over the list, but under menus, tooltips and windows.
        .order(egui::Order::Middle)
        .fixed_pos(Pos2::new(cell.left() - PAD, top))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(theme::bg())
                .stroke(Stroke::new(1.0, theme::border()))
                .corner_radius(6)
                .inner_margin(PAD)
                .shadow(widgets::POPUP_SHADOW)
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = GAP;
                    for (r, on_remote) in labels(c) {
                        let text = ref_text(r, on_remote);
                        // Measure, then claim the space and draw into it.
                        let width = ui
                            .painter()
                            .layout_no_wrap(
                                text.clone(),
                                egui::FontId::proportional(12.0),
                                theme::text(),
                            )
                            .size()
                            .x
                            + 14.0;
                        let width = width.min(MAX_LABEL);
                        let (rect, response) = ui
                            .allocate_exact_size(egui::vec2(width, 20.0), Sense::click_and_drag());
                        let what = label_click(c, r);
                        let name = r.name.clone();
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name)
                        });
                        if response.hovered() {
                            ui.painter()
                                .rect_filled(rect.expand(2.0), 5, theme::bg_hover());
                        }
                        widgets::pill(
                            ui.painter(),
                            Pos2::new(rect.left(), rect.center().y),
                            MAX_LABEL,
                            &text,
                            colour,
                            r.is_head,
                        );
                        let mut tips = Vec::new();
                        if on_remote {
                            let copies: Vec<&str> = c
                                .refs
                                .iter()
                                .filter(|o| {
                                    o.kind == RefKind::Remote
                                        && o.name.split_once('/').is_some_and(|(_, b)| b == r.name)
                                })
                                .map(|o| o.name.as_str())
                                .collect();
                            tips.push(format!("{} is here too", copies.join(" and ")));
                        }
                        // The hint only where there are several to choose
                        // from: one label grown to its whole name is
                        // left as it is.
                        if what.is_some() && labels(c).len() > 1 {
                            tips.push("Double-click to check out".to_string());
                        }
                        let tip = tips.join(". ");
                        let response = if tip.is_empty() {
                            response
                        } else {
                            response.on_hover_text(tip)
                        };
                        response.context_menu(|ui| {
                            if let Some(chosen) = commit_menu(ui, c, web, head, Some(r)) {
                                action = Some(chosen);
                            }
                        });
                        if let Some(merge) = drag_and_drop(ui.painter(), &response, r) {
                            action = Some(merge);
                        } else if response.double_clicked() {
                            action = what;
                        } else if response.clicked() {
                            action = Some(Click::Row(row));
                        }
                    }
                });
        });
    (area.response.rect, action)
}

/// The right-click menu of a commit, or of one of its labels (`label`):
/// that label's checkout first, then what can be done with the commit (a
/// stash: apply, pop or delete it). `web` is the commit's page on the
/// forge; `head` the checked-out branch.
fn commit_menu(
    ui: &mut egui::Ui,
    c: &GraphCommit,
    web: Option<&(String, String)>,
    head: Option<&str>,
    label: Option<&RefLabel>,
) -> Option<Click> {
    let mut chosen = None;
    let id = c.id.clone();
    if c.refs.iter().any(|r| r.kind == RefKind::Stash) {
        for (glyph, text, which) in [
            (icon::ARROW_SQUARE_DOWN, "Apply", 0),
            (icon::STACK_MINUS, "Pop", 1),
            (icon::TRASH, "Delete…", 2),
        ] {
            if ui.button(format!("{glyph}  {text}")).clicked() {
                chosen = Some(match which {
                    0 => Click::StashApply(id.clone()),
                    1 => Click::StashPop(id.clone()),
                    _ => Click::StashDrop(id.clone()),
                });
                ui.close();
            }
        }
        return chosen;
    }
    // A branch's or tag's label: what its menu in the sidebar has, under
    // its name ("Create branch here" is in the commit's part below).
    if let Some(r) = label {
        use crate::ui::sidebar;
        let items = match r.kind {
            RefKind::Local => sidebar::local_branch_items(&r.name, &c.id, r.is_head, head),
            RefKind::Remote => sidebar::remote_branch_items(&r.name, &c.id, head),
            RefKind::Tag => sidebar::tag_items(&r.name, &c.id),
            RefKind::Head | RefKind::Stash => Vec::new(),
        };
        let items: Vec<_> = items
            .into_iter()
            .filter(|(_, _, a)| !matches!(a, sidebar::Action::NewBranch { .. }))
            .collect();
        if !items.is_empty() {
            ui.label(RichText::new(&r.name).size(12.0).color(theme::muted()));
            let mut action = None;
            sidebar::menu(ui, &mut action, items);
            chosen = action.map(Click::Branch);
            ui.separator();
        }
    }
    let mut opened = None;
    crate::ui::sidebar::web_items(ui, &mut opened, web);
    if let Some(open) = opened {
        chosen = Some(Click::Branch(open));
    }
    let on_branch = head.is_some();
    let target = head.unwrap_or("HEAD");
    let label = commit_label(&c.id, &c.summary);
    let items = [
        (icon::GIT_BRANCH, "Create branch here…".to_string(), 0, true),
        (icon::TAG, "Create tag here…".to_string(), 1, true),
        (
            icon::SIGN_OUT,
            "Check out this commit only (detached)…".to_string(),
            2,
            true,
        ),
        (
            icon::ARROW_BEND_DOWN_RIGHT,
            format!("Cherry-pick onto {target}"),
            3,
            on_branch,
        ),
        (
            icon::ARROW_COUNTER_CLOCKWISE,
            "Revert this commit".to_string(),
            4,
            on_branch,
        ),
        (
            icon::CLOCK_COUNTER_CLOCKWISE,
            format!("Reset {target} to here…"),
            5,
            on_branch,
        ),
        (
            icon::LIST_NUMBERS,
            "Interactive rebase from here…".to_string(),
            6,
            on_branch,
        ),
    ];
    for (glyph, text, which, enabled) in items {
        if which == 3 {
            ui.separator();
        }
        if ui
            .add_enabled(enabled, egui::Button::new(format!("{glyph}  {text}")))
            .clicked()
        {
            let (id, label) = (id.clone(), label.clone());
            chosen = Some(match which {
                0 => Click::NewBranch { id, label },
                1 => Click::NewTag { id, label },
                2 => Click::CheckoutDetached { id, label },
                3 => Click::CherryPick { id },
                4 => Click::Revert { id },
                5 => Click::Reset { id, label },
                _ => Click::InteractiveRebase { id, label },
            });
            ui.close();
        }
    }
    chosen
}

/// "a1b2c3d Fix the thing", for dialogs about a commit.
fn commit_label(id: &str, summary: &str) -> String {
    let summary: String = summary.chars().take(50).collect();
    format!("{} \u{201c}{summary}\u{201d}", &id[..7.min(id.len())])
}
