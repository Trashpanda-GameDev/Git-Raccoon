//! A file's diff, shown over the history when a file is opened.

use std::collections::BTreeSet;
use std::sync::Arc;

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke};
use egui_phosphor::regular as icon;
use git_raccoon_core::details::{DiffLine, LineKind};
use git_raccoon_core::working::Side as WorkSide;

use crate::format;
use crate::highlight::{RenderedDiff, Row, SplitRow};
use crate::images::{ImagePair, Preview, Side};
use crate::theme;
use crate::ui::widgets;

const LINE: f32 = 20.0;
const NUMBER: f32 = 52.0;
const MARKER: f32 = 18.0;
const IMAGE_BOX: f32 = 220.0;

pub enum Action {
    Close,
    /// Show the file's history, or its blame.
    History,
    Blame,
    SetSplit(bool),
    /// Show the whole file, or just the changes.
    SetWhole(bool),
    /// Stage (or unstage) a whole hunk.
    StageHunk(usize),
    /// Pick or unpick a line; `extend` picks the range from the last pick.
    ToggleLine {
        hunk: usize,
        line: usize,
        extend: bool,
    },
    StagePicked,
    ClearPicked,
}

/// For a working file's diff: which side it is, and the lines picked.
pub struct Staging<'a> {
    pub side: WorkSide,
    pub picked: &'a BTreeSet<(usize, usize)>,
    /// A staging operation is running.
    pub busy: bool,
}

impl Staging<'_> {
    fn verb(&self) -> &'static str {
        match self.side {
            WorkSide::Unstaged => "Stage",
            WorkSide::Staged => "Unstage",
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "one call site; each argument is a distinct piece of what the view shows"
)]
pub fn show(
    ui: &mut egui::Ui,
    file: &str,
    side_label: Option<&str>,
    diff: Option<&Result<Arc<RenderedDiff>, String>>,
    loading: bool,
    images: Option<&Result<Arc<ImagePair>, String>>,
    // Side by side, and the whole file.
    (split, whole): (bool, bool),
    staging: Option<&Staging<'_>>,
    // Whether to offer the file's History and Blame.
    past: bool,
) -> Option<Action> {
    let mut action = None;
    let ready = diff.and_then(|d| d.as_ref().ok());

    // Title bar.
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin::symmetric(12, 0))
        .show(ui, |ui| {
            ui.set_height(36.0);
            ui.horizontal_centered(|ui| {
                if let Some(d) = ready {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), Sense::hover());
                    widgets::status_badge(ui.painter(), rect, d.diff.status);
                }
                let title = match ready.and_then(|d| d.diff.old_path.as_deref()) {
                    Some(old) => format!("{old}  {}  {file}", icon::ARROW_RIGHT),
                    None => file.to_string(),
                };
                ui.label(RichText::new(title).strong());
                if let Some(label) = side_label {
                    ui.label(RichText::new(label).size(12.0).color(theme::muted()));
                }
                if loading {
                    ui.spinner();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, icon::X, "Back to the graph (Esc)").clicked() {
                        action = Some(Action::Close);
                    }
                    ui.add_space(4.0);
                    if past {
                        let buttons = [
                            (
                                icon::USER_LIST,
                                "Blame",
                                "Who last changed each line",
                                Action::Blame,
                            ),
                            (
                                icon::CLOCK_COUNTER_CLOCKWISE,
                                "History",
                                "Every commit that changed this file",
                                Action::History,
                            ),
                        ];
                        for (glyph, name, tip, act) in buttons {
                            let b = ui.button(RichText::new(format!("{glyph}  {name}")).size(12.0));
                            ui.ctx().accesskit_node_builder(b.id, |n| n.set_label(name));
                            if b.on_hover_text(tip).clicked() {
                                action = Some(act);
                            }
                        }
                    }
                    if let Some(st) = staging
                        && !st.picked.is_empty()
                    {
                        let n = st.picked.len();
                        let label = format!(
                            "{} {n} {}",
                            st.verb(),
                            if n == 1 { "line" } else { "lines" }
                        );
                        let stage = egui::Button::new(
                            RichText::new(label).color(theme::on_accent()).strong(),
                        )
                        .fill(theme::accent_fill());
                        if ui.add_enabled(!st.busy, stage).clicked() {
                            action = Some(Action::StagePicked);
                        }
                        if ui.button("Clear").clicked() {
                            action = Some(Action::ClearPicked);
                        }
                        ui.add_space(8.0);
                    }
                    let chosen =
                        widgets::segmented(ui, split, &[(true, "Split"), (false, "Unified")]);
                    if chosen != split {
                        action = Some(Action::SetSplit(chosen));
                    }
                    let chosen =
                        widgets::segmented(ui, whole, &[(true, "Whole file"), (false, "Changes")]);
                    if chosen != whole {
                        action = Some(Action::SetWhole(chosen));
                    }
                });
            });
        });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );

    if let Some(Ok(pair)) = images {
        image_preview(ui, pair);
    } else if let Some(Err(e)) = images {
        ui.colored_label(theme::danger(), format!("Could not load images: {e}"));
    }

    match diff {
        None => {
            ui.add_space(24.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Loading diff…").color(theme::muted()))
            });
        }
        Some(Err(e)) => {
            ui.add_space(12.0);
            ui.colored_label(theme::danger(), e);
        }
        Some(Ok(d)) => {
            if let Some(a) = body(ui, d, split, images.is_some(), staging) {
                action = Some(a);
            }
        }
    }
    action
}

fn notice(ui: &mut egui::Ui, text: &str) {
    ui.add_space(24.0);
    ui.vertical_centered(|ui| ui.label(RichText::new(text).color(theme::muted())));
}

fn body(
    ui: &mut egui::Ui,
    d: &RenderedDiff,
    split: bool,
    has_images: bool,
    staging: Option<&Staging<'_>>,
) -> Option<Action> {
    // Large files kept in Git LFS: the repository only has small pointers.
    if let Some((before, after)) = git_raccoon_core::modules::pointer_change(&d.diff) {
        let size = |p: &Option<git_raccoon_core::modules::Pointer>| {
            p.as_ref().map_or_else(
                || "none".to_string(),
                |p| crate::format::bytes(usize::try_from(p.size).unwrap_or(usize::MAX)),
            )
        };
        notice(
            ui,
            &format!(
                "Stored with Git LFS. Size before: {}, after: {}. The repository keeps a small pointer; the file itself is on the LFS server.",
                size(&before),
                size(&after)
            ),
        );
        return None;
    }
    if d.diff.binary {
        if !has_images {
            notice(ui, "Binary file not shown.");
        }
        return None;
    }
    if d.diff.hunks.is_empty() {
        notice(ui, "No content changes (mode or rename only).");
        return None;
    }
    let mut action = None;

    let char_width = ui
        .painter()
        .layout_no_wrap("M".repeat(20), theme::mono(theme::MONO_SIZE), theme::text())
        .size()
        .x
        / 20.0;
    #[expect(clippy::cast_precision_loss, reason = "line lengths are small")]
    let code_width = d.widest as f32 * char_width + 24.0;
    let rows = if split {
        d.split.len()
    } else {
        d.unified.len()
    };
    let available = ui.available_width();
    // Unified, the whole view scrolls sideways. Side by side, each half
    // keeps half the window, and the code in each scrolls sideways on its
    // own, with its own scrollbar (under its numbers, which stay put).
    let width = if split {
        available
    } else {
        (2.0 * NUMBER + MARKER + code_width).max(available)
    };
    let area = if split {
        egui::ScrollArea::vertical()
    } else {
        egui::ScrollArea::both()
    };
    let sideways_id = egui::Id::new("diff-split-sideways");
    let sideways = ui
        .data(|d| d.get_temp::<f32>(sideways_id))
        .unwrap_or_default();

    // Where the last line drawn ends, if it's on screen.
    let mut end = None;
    let mut area = area.id_salt(("diff", split)).auto_shrink([false, false]);
    // With the whole file, a newly opened one starts a few lines above its
    // first change (not when the same one reloads, as working files do).
    let seen_id = egui::Id::new("diff-whole-opened");
    let this = (d.diff.path.clone(), split, d.changes < d.diff.hunks.len());
    if ui.data(|m| m.get_temp::<(String, bool, bool)>(seen_id)) != Some(this.clone()) {
        let first = if split {
            d.split.iter().position(|r| match r {
                SplitRow::Header(_) => true,
                SplitRow::Pair(a, b) => a.or(*b).is_some_and(|(h, _)| h < d.changes),
            })
        } else {
            d.unified.iter().position(|r| match r {
                Row::Header(_) => true,
                Row::Line(h, _) => *h < d.changes,
            })
        };
        #[expect(clippy::cast_precision_loss, reason = "row counts are small")]
        let offset = first.unwrap_or(0).saturating_sub(3) as f32 * LINE;
        area = area.vertical_scroll_offset(offset);
        ui.data_mut(|m| m.insert_temp(seen_id, this));
    }
    let output = widgets::gapless_rows(ui, area, LINE, rows, |ui, range| {
        let start_y = ui.cursor().top();
        let left = ui.max_rect().left();
        #[expect(clippy::cast_precision_loss, reason = "visible row count is small")]
        ui.allocate_exact_size(egui::vec2(width, LINE * range.len() as f32), Sense::hover());
        let painter = ui.painter().clone();
        let painter = &painter;
        let shift = ui.input(|i| i.modifiers.shift);
        // A click on a changed line's numbers picks it (working diffs only).
        let gutter = |ui: &mut egui::Ui, rect: Rect, h: usize, l: usize| -> Option<Action> {
            let st = staging?;
            let line = &d.diff.hunks[h].lines[l];
            if !matches!(line.kind, LineKind::Add | LineKind::Delete) {
                return None;
            }
            let response = ui.interact(rect, ui.id().with(("pick", h, l)), Sense::click());
            let name = format!("Pick line: {}", line.content.trim());
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Checkbox, true, &name));
            let picked = st.picked.contains(&(h, l));
            if picked {
                painter.rect_filled(rect, 0, theme::accent().gamma_multiply(0.35));
                painter.text(
                    Pos2::new(rect.left() + 9.0, rect.center().y),
                    egui::Align2::CENTER_CENTER,
                    icon::CHECK,
                    theme::body(11.0),
                    theme::text(),
                );
            } else if response.hovered() {
                painter.rect_filled(rect, 0, theme::accent().gamma_multiply(0.15));
            }
            response
                .on_hover_text("Pick this line to stage it on its own (Shift: a range)")
                .clicked()
                .then_some(Action::ToggleLine {
                    hunk: h,
                    line: l,
                    extend: shift,
                })
        };
        if range.end == rows {
            #[expect(clippy::cast_precision_loss, reason = "visible row count is small")]
            let drawn = start_y + range.len() as f32 * LINE;
            end = Some(drawn);
        }
        for (n, row) in range.enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "visible row count is small")]
            let top = start_y + n as f32 * LINE;
            let rect = Rect::from_min_size(Pos2::new(left, top), egui::vec2(width, LINE));
            if split {
                match d.split[row] {
                    SplitRow::Header(h) => {
                        header(painter, rect, &d.diff.hunks[h].header);
                        if let Some(a) = hunk_button(ui, rect, h, staging) {
                            action = Some(a);
                        }
                    }
                    SplitRow::Pair(a, b) => {
                        let half = width / 2.0;
                        let l = Rect::from_min_size(rect.min, egui::vec2(half, LINE));
                        let r = Rect::from_min_size(
                            Pos2::new(rect.left() + half, top),
                            egui::vec2(half, LINE),
                        );
                        side(painter, l, d, a, false, sideways);
                        side(painter, r, d, b, true, sideways);
                        // Deletions are picked on the left, additions on the right.
                        let deleted =
                            a.filter(|&(h, i)| d.diff.hunks[h].lines[i].kind == LineKind::Delete);
                        let added =
                            b.filter(|&(h, i)| d.diff.hunks[h].lines[i].kind == LineKind::Add);
                        for (at, half_rect) in [(deleted, l), (added, r)] {
                            if let Some((h, i)) = at
                                && let Some(picked) = gutter(
                                    ui,
                                    Rect::from_min_size(half_rect.min, egui::vec2(NUMBER, LINE)),
                                    h,
                                    i,
                                )
                            {
                                action = Some(picked);
                            }
                        }
                        painter.vline(l.right(), l.y_range(), Stroke::new(1.0, theme::border()));
                    }
                }
            } else {
                match d.unified[row] {
                    Row::Header(h) => {
                        header(painter, rect, &d.diff.hunks[h].header);
                        if let Some(a) = hunk_button(ui, rect, h, staging) {
                            action = Some(a);
                        }
                    }
                    Row::Line(h, l) => {
                        unified_line(painter, rect, &d.diff.hunks[h].lines[l], &d.jobs[h][l]);
                        if let Some(a) = gutter(
                            ui,
                            Rect::from_min_size(rect.min, egui::vec2(2.0 * NUMBER, LINE)),
                            h,
                            l,
                        ) {
                            action = Some(a);
                        }
                    }
                }
            }
        }
    });
    // Side by side, the two halves go on down to the bottom of the view,
    // past the last line.
    let view = output.inner_rect;
    // Each half's code has a scrollbar of its own, and the two move
    // together: either one (or the wheel over either half) scrolls both.
    if split {
        let half = view.width() / 2.0;
        let code = |i: f32| {
            Rect::from_x_y_ranges(
                view.left() + i * half + NUMBER + MARKER..=view.left() + (i + 1.0) * half,
                view.y_range(),
            )
        };
        let mut x = sideways;
        for which in [0u8, 1] {
            x = widgets::sideways_scrollbar(
                ui,
                sideways_id.with(which),
                (code(f32::from(which)), view),
                x,
                code_width,
            );
        }
        ui.data_mut(|d| d.insert_temp(sideways_id, x));
        if x != sideways {
            // The bar drawn first shows where they are next frame.
            ui.ctx().request_repaint();
        }
    }
    if split && let Some(end) = end.filter(|&end| end < view.bottom()) {
        ui.painter().with_clip_rect(view).vline(
            view.left() + width / 2.0,
            end.max(view.top())..=view.bottom(),
            Stroke::new(1.0, theme::border()),
        );
    }
    if d.diff.truncated {
        notice(
            ui,
            "This diff is too large; only the first 20,000 lines are shown.",
        );
    }
    action
}

/// "Stage hunk" / "Unstage hunk" at the right of a hunk header.
fn hunk_button(
    ui: &mut egui::Ui,
    header: Rect,
    hunk: usize,
    staging: Option<&Staging<'_>>,
) -> Option<Action> {
    let st = staging?;
    let label = format!("{} hunk", st.verb());
    let rect = Rect::from_min_size(
        Pos2::new(
            header.right().min(ui.clip_rect().right()) - 118.0,
            header.top() + 1.0,
        ),
        egui::vec2(110.0, LINE - 2.0),
    );
    let button = egui::Button::new(RichText::new(label).size(11.0))
        .fill(theme::bg_raised())
        .stroke(Stroke::new(1.0, theme::border()))
        .small();
    // Buttons default to a 24 px minimum height; keep this one inside the row.
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().interact_size.y = LINE - 4.0;
            ui.put(rect.shrink2(egui::vec2(0.0, 1.0)), button)
        })
        .inner;
    (response.clicked() && !st.busy).then_some(Action::StageHunk(hunk))
}

fn backgrounds(kind: LineKind) -> (Color32, Color32) {
    match kind {
        LineKind::Add => (theme::add_bg_strong(), theme::add_bg()),
        LineKind::Delete => (theme::del_bg_strong(), theme::del_bg()),
        LineKind::Context | LineKind::NoEol => (Color32::TRANSPARENT, Color32::TRANSPARENT),
    }
}

fn marker(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Add => "+",
        LineKind::Delete => "−",
        LineKind::NoEol => "\\",
        LineKind::Context => " ",
    }
}

fn header(painter: &egui::Painter, rect: Rect, text: &str) {
    painter.rect_filled(rect, 0, theme::hunk_bg());
    painter.hline(
        rect.x_range(),
        rect.top(),
        Stroke::new(1.0, theme::border()),
    );
    let r = rect.shrink2(egui::vec2(12.0, 0.0));
    widgets::text_in(painter, r, text, theme::mono(12.0), theme::hunk());
}

fn number(painter: &egui::Painter, rect: Rect, n: Option<u32>) {
    if let Some(n) = n {
        painter.text(
            Pos2::new(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            n.to_string(),
            theme::mono(12.0),
            theme::muted(),
        );
    }
}

/// A line's marker and its code, scrolled `shift` to the left (under the
/// marker, which stays put).
fn code(
    painter: &egui::Painter,
    rect: Rect,
    line: &DiffLine,
    job: &egui::text::LayoutJob,
    shift: f32,
) {
    painter.text(
        Pos2::new(rect.left() + MARKER / 2.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        marker(line.kind),
        theme::mono(12.0),
        theme::muted(),
    );
    let galley = painter.layout_job(job.clone());
    let text = Rect::from_min_max(Pos2::new(rect.left() + MARKER, rect.top()), rect.max);
    let clipped = painter.with_clip_rect(text.intersect(painter.clip_rect()));
    clipped.galley(
        Pos2::new(text.left() - shift, rect.center().y - galley.size().y / 2.0),
        galley,
        theme::text(),
    );
}

fn unified_line(painter: &egui::Painter, rect: Rect, line: &DiffLine, job: &egui::text::LayoutJob) {
    let (gutter_bg, code_bg) = backgrounds(line.kind);
    let gutter = Rect::from_min_size(rect.min, egui::vec2(2.0 * NUMBER, LINE));
    let body = Rect::from_min_max(Pos2::new(gutter.right(), rect.top()), rect.max);
    painter.rect_filled(gutter, 0, gutter_bg);
    painter.rect_filled(body, 0, code_bg);
    number(
        painter,
        Rect::from_min_size(rect.min, egui::vec2(NUMBER, LINE)),
        line.old_line,
    );
    number(
        painter,
        Rect::from_min_size(
            Pos2::new(rect.left() + NUMBER, rect.top()),
            egui::vec2(NUMBER, LINE),
        ),
        line.new_line,
    );
    code(painter, body, line, job, 0.0);
}

/// One half of a side-by-side row; its code is `shift` scrolled sideways.
fn side(
    painter: &egui::Painter,
    rect: Rect,
    d: &RenderedDiff,
    at: Option<(usize, usize)>,
    new: bool,
    shift: f32,
) {
    let Some((h, l)) = at else {
        // Nothing on this side: a faint filler.
        painter.rect_filled(rect, 0, Color32::from_white_alpha(4));
        return;
    };
    let line = &d.diff.hunks[h].lines[l];
    let (gutter_bg, code_bg) = backgrounds(line.kind);
    let gutter = Rect::from_min_size(rect.min, egui::vec2(NUMBER, LINE));
    let body = Rect::from_min_max(Pos2::new(gutter.right(), rect.top()), rect.max);
    painter.rect_filled(gutter, 0, gutter_bg);
    painter.rect_filled(body, 0, code_bg);
    number(
        painter,
        gutter,
        if new { line.new_line } else { line.old_line },
    );
    code(painter, body, line, &d.jobs[h][l], shift);
}

fn image_preview(ui: &mut egui::Ui, pair: &ImagePair) {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            let gap = 16.0;
            let w = ((ui.available_width() - gap) / 2.0).max(120.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                image_side(ui, "Before", pair.old.as_ref(), "New file", w);
                image_side(ui, "After", pair.new.as_ref(), "Deleted", w);
            });
        });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );
}

fn image_side(ui: &mut egui::Ui, label: &str, side: Option<&Side>, missing: &str, width: f32) {
    ui.vertical(|ui| {
        ui.set_width(width);
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).strong().size(12.0));
            if let Some(s) = side {
                let dims = match &s.preview {
                    Preview::Image { width, height, .. } => format!("{width} × {height} · "),
                    _ => String::new(),
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{dims}{}", format::bytes(s.size)))
                            .size(12.0)
                            .color(theme::muted()),
                    );
                });
            }
        });
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, IMAGE_BOX), Sense::hover());
        let painter = ui.painter();
        checkerboard(painter, rect);
        painter.rect_stroke(
            rect,
            6,
            Stroke::new(1.0, theme::border()),
            egui::StrokeKind::Inside,
        );
        let say = |text: &str| {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                text,
                theme::body(12.0),
                theme::muted(),
            );
        };
        match side.map(|s| &s.preview) {
            None => say(missing),
            Some(Preview::TooLarge) => say("Too large to preview"),
            Some(Preview::Failed(_)) => say("Can't display this image"),
            Some(Preview::Image { texture, .. }) => {
                let size = texture.size_vec2();
                let fit = rect.shrink(12.0);
                // Never upscale small images; shrink large ones to fit.
                let scale = (fit.width() / size.x).min(fit.height() / size.y).min(1.0);
                let shown = Rect::from_center_size(rect.center(), size * scale);
                painter.image(
                    texture.id(),
                    shown,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }
    });
}

/// Shows transparency.
fn checkerboard(painter: &egui::Painter, rect: Rect) {
    painter.rect_filled(rect, 6, theme::bg_raised());
    let cell = 8.0;
    let clipped = painter.with_clip_rect(rect.shrink(1.0).intersect(painter.clip_rect()));
    let mut y = rect.top();
    let mut row = 0;
    while y < rect.bottom() {
        let mut x = rect.left() + if row % 2 == 0 { 0.0 } else { cell };
        while x < rect.right() {
            clipped.rect_filled(
                Rect::from_min_size(Pos2::new(x, y), egui::vec2(cell, cell)),
                0,
                theme::bg_hover(),
            );
            x += 2.0 * cell;
        }
        y += cell;
        row += 1;
    }
}
