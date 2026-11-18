//! The commit list with its graph. Only the rows on screen are drawn.

use crate::settings::Column;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, epaint::CubicBezierShape};
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
    fn new(width: f32, lanes: usize, order: &[Column]) -> Self {
        let narrow = width < NARROW;
        Columns {
            refs: if narrow { 140.0 } else { 200.0 },
            graph: graph_column_width(lanes),
            author: if narrow { 0.0 } else { 140.0 },
            date: 110.0,
            sha: if narrow { 0.0 } else { 72.0 },
            order: Column::sanitise(order),
        }
    }

    fn message(&self, width: f32) -> f32 {
        (width - self.refs - self.graph - self.author - self.date - self.sha).max(120.0)
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

/// The column headings. A heading can be dragged left or right to move
/// its column; returns the new order when one is dropped. Right-clicking
/// one offers the usual order back.
fn headings(ui: &mut egui::Ui, header: Rect, cols: &Columns, width: f32) -> Option<Vec<Column>> {
    let mut moved = None;
    let pointer = ui.ctx().pointer_interact_pos();
    let mut dragging: Option<(Column, Rect)> = None;
    for &column in &cols.order {
        let w = cols.w(column, width);
        if w <= 0.0 {
            continue;
        }
        let cell = Rect::from_min_size(
            Pos2::new(header.left() + cols.x(column, width), header.top()),
            egui::vec2(w, header.height()),
        );
        let response = ui.interact(
            cell,
            ui.id().with(("heading", column.title())),
            Sense::click_and_drag(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, column.title())
        });
        let response = response
            .on_hover_cursor(egui::CursorIcon::Grab)
            .on_hover_text("Drag to move this column");
        response.context_menu(|ui| {
            if ui.button("Put the columns back in order").clicked() {
                moved = Some(Column::ALL.to_vec());
                ui.close();
            }
        });
        if response.dragged() {
            dragging = Some((column, cell));
        } else {
            draw_heading(ui.painter(), cell, column.title(), theme::muted());
        }
        if response.drag_stopped()
            && let Some(p) = pointer
        {
            // Where it lands: before the first column whose middle is
            // right of the pointer.
            let mut order: Vec<Column> = cols
                .order
                .iter()
                .copied()
                .filter(|c| *c != column)
                .collect();
            let at = order
                .iter()
                .position(|c| header.left() + cols.x(*c, width) + cols.w(*c, width) / 2.0 > p.x)
                .unwrap_or(order.len());
            order.insert(at, column);
            if order != cols.order {
                moved = Some(order);
            }
        }
    }
    // The heading being moved follows the pointer, with a line where it
    // would land.
    if let (Some((column, cell)), Some(p)) = (dragging, pointer) {
        let others: Vec<Column> = cols
            .order
            .iter()
            .copied()
            .filter(|c| *c != column)
            .collect();
        let landing = others
            .iter()
            .find(|c| header.left() + cols.x(**c, width) + cols.w(**c, width) / 2.0 > p.x)
            .map_or(header.right(), |c| header.left() + cols.x(*c, width));
        let painter = ui.painter();
        painter.vline(landing, header.y_range(), Stroke::new(2.0, theme::accent()));
        let ghost = cell.translate(egui::vec2(p.x - cell.center().x, 0.0));
        painter.rect_filled(ghost, 4, theme::bg_hover());
        draw_heading(painter, ghost, column.title(), theme::text());
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    ui.painter().hline(
        header.x_range(),
        header.bottom(),
        Stroke::new(1.0, theme::border()),
    );
    moved
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
    /// Open a page on the forge (from a commit's context menu).
    OpenWeb(String),
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
) -> Option<Click> {
    let total = repo.history.total();
    let width = ui.available_width();
    let cols = Columns::new(width, repo.history.width(), order);
    // A graph wider than its column scrolls sideways.
    let scroll_id = graph_scroll_id(repo);
    let spare = (lanes_width(repo.history.width()) - cols.graph).max(0.0);
    let mut graph_x: f32 = ui
        .data(|d| d.get_temp::<f32>(scroll_id))
        .unwrap_or(0.0)
        .clamp(0.0, spare);

    // Column headings: drag one to move its column.
    let (header, _) = ui.allocate_exact_size(egui::vec2(width, 28.0), Sense::hover());
    if let Some(moved) = headings(ui, header, &cols, width) {
        *order = moved;
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
    if overlay.is_some_and(|(_, cell, area)| {
        !pointer.is_some_and(|p| area.expand(2.0).contains(p) || cell.contains(p))
    }) {
        overlay = None;
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
                let (id, label) = (c.id.clone(), commit_label(&c.id, &c.summary));
                let web = repo.web.commit(&c.id);
                let stash = c.refs.iter().any(|r| r.kind == RefKind::Stash);
                if stash {
                    response.context_menu(|ui| {
                        for (glyph, text, which) in [
                            (icon::ARROW_SQUARE_DOWN, "Apply", 0),
                            (icon::STACK_MINUS, "Pop", 1),
                            (icon::TRASH, "Delete…", 2),
                        ] {
                            if ui.button(format!("{glyph}  {text}")).clicked() {
                                let id = id.clone();
                                clicked = Some(match which {
                                    0 => Click::StashApply(id),
                                    1 => Click::StashPop(id),
                                    _ => Click::StashDrop(id),
                                });
                                ui.close();
                            }
                        }
                    });
                } else {
                    response.context_menu(|ui| {
                        if let Some((text, url)) = &web {
                            if ui
                                .button(format!("{}  {text}", icon::ARROW_SQUARE_OUT))
                                .clicked()
                            {
                                clicked = Some(Click::OpenWeb(url.clone()));
                                ui.close();
                            }
                            if ui
                                .button(format!("{}  Copy its link", icon::LINK))
                                .clicked()
                            {
                                ui.ctx().copy_text(url.clone());
                                ui.close();
                            }
                            ui.separator();
                        }
                        let on_branch = head.is_some();
                        let target = head.as_deref().unwrap_or("HEAD");
                        let items = [
                            (icon::GIT_BRANCH, "Create branch here…".to_string(), 0, true),
                            (icon::TAG, "Create tag here…".to_string(), 1, true),
                            (
                                icon::SIGN_OUT,
                                "Check out this commit…".to_string(),
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
                                clicked = Some(match which {
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
                    });
                }
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
                if (a.x - b.x).abs() < 0.5 {
                    if dotted {
                        graph.extend(egui::Shape::dotted_line(&[a, b], stroke.color, 6.0, 1.2));
                    } else {
                        graph.line_segment([a, b], stroke);
                    }
                } else {
                    let mid = (a.y + b.y) / 2.0;
                    let curve = CubicBezierShape::from_points_stroke(
                        [a, Pos2::new(a.x, mid), Pos2::new(b.x, mid), b],
                        false,
                        Color32::TRANSPARENT,
                        stroke,
                    );
                    if dotted {
                        let points = curve.flatten(Some(0.5));
                        graph.extend(egui::Shape::dotted_line(&points, stroke.color, 6.0, 1.2));
                    } else {
                        graph.add(curve);
                    }
                }
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
                && overlay.is_none()
                && pointer.is_some_and(|p| {
                    Rect::from_x_y_ranges(refs_cell.x_range(), top..=top + ROW_HEIGHT).contains(p)
                })
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
                let (area, action) = all_labels(ui.ctx(), overlay_id, cell, row, c);
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
) -> (Rect, Option<Click>) {
    const GAP: f32 = 4.0;
    const PAD: f32 = 4.0;
    let colour = theme::lane_color(c.column);
    let mut action = None;
    let top = cell.center().y - 10.0 - PAD;
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(Pos2::new(cell.left() - PAD, top))
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(theme::bg())
                .stroke(Stroke::new(1.0, theme::border()))
                .corner_radius(6)
                .inner_margin(PAD)
                .shadow(egui::Shadow {
                    offset: [0, 4],
                    blur: 12,
                    spread: 0,
                    color: Color32::from_black_alpha(90),
                })
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

/// "a1b2c3d Fix the thing", for dialogs about a commit.
fn commit_label(id: &str, summary: &str) -> String {
    let summary: String = summary.chars().take(50).collect();
    format!("{} \u{201c}{summary}\u{201d}", &id[..7.min(id.len())])
}
