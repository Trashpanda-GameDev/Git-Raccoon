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
use crate::repo_view::Select;
use crate::theme;
use crate::ui::file_view::{self, GUTTER};
use crate::ui::widgets;
use git_raccoon_core::file_history::{Blame, Change};

pub(crate) const LINE: f32 = 20.0;
pub(crate) const NUMBER: f32 = 52.0;
/// The +/− before a line's code.
pub(crate) const MARKER: f32 = 18.0;
const IMAGE_BOX: f32 = 220.0;

pub enum Action {
    Close,
    /// Open the file's two versions in the diff tool, or the file in the
    /// editor (at the line looked at).
    OpenInDiffTool,
    OpenInEditor,
    /// Show or hide the file's history beside the diff, or its blame.
    ToggleHistory,
    ToggleBlame,
    SetSplit(bool),
    /// Show the whole file, or just the changes.
    SetWhole(bool),
    /// Stage (or unstage) a whole hunk.
    StageHunk(usize),
    /// A click (or drag) on a line, picking it.
    Select {
        at: (usize, usize),
        how: Select,
    },
    SelectAll,
    CopyPicked,
    StagePicked,
    /// From the history: show this commit's change to the file (at `path`,
    /// its name then).
    Pick {
        id: String,
        path: String,
    },
    /// Go to a commit (from the blame).
    GoTo(String),
}

/// The lines picked, and for a working file's diff, which side it is.
pub struct Lines<'a> {
    pub picked: &'a BTreeSet<(usize, usize)>,
    /// The working side, and whether staging is running.
    pub staging: Option<(WorkSide, bool)>,
}

impl Lines<'_> {
    fn verb(&self) -> Option<&'static str> {
        match self.staging?.0 {
            WorkSide::Unstaged => Some("Stage"),
            WorkSide::Staged => Some("Unstage"),
        }
    }

    fn busy(&self) -> bool {
        self.staging.is_some_and(|(_, busy)| busy)
    }
}

/// Something loaded in the background, or the reason it couldn't be.
type Loaded<'a, T> = Option<&'a Result<Arc<T>, String>>;

/// The file's history and blame, when offered.
pub struct Past<'a> {
    /// Shown, and their contents (`None` while loading).
    pub history: Option<Loaded<'a, Vec<Change>>>,
    pub blame: Option<Loaded<'a, Blame>>,
    /// The file the history is of, and the commit selected.
    pub history_file: &'a str,
    pub selected: Option<&'a str>,
    pub now: i64,
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
    lines: &Lines<'_>,
    // The file's history and blame, if offered.
    past: Option<&Past<'_>>,
    // The diff tool and editor chosen, by name.
    open_with: (Option<&str>, Option<&str>),
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
                    if let Some(a) = open_with_buttons(ui, open_with) {
                        action = Some(a);
                    }
                    if let Some(past) = past {
                        let buttons = [
                            (
                                icon::USER_LIST,
                                "Blame",
                                "Who last changed each line",
                                past.blame.is_some(),
                                Action::ToggleBlame,
                            ),
                            (
                                icon::CLOCK_COUNTER_CLOCKWISE,
                                "History",
                                "Every commit that changed this file, beside it",
                                past.history.is_some(),
                                Action::ToggleHistory,
                            ),
                        ];
                        for (glyph, name, tip, on, act) in buttons {
                            let b = ui.add(
                                egui::Button::new(
                                    RichText::new(format!("{glyph}  {name}")).size(12.0),
                                )
                                .selected(on),
                            );
                            ui.ctx().accesskit_node_builder(b.id, |n| {
                                n.set_label(name);
                                n.set_toggled(if on {
                                    egui::accesskit::Toggled::True
                                } else {
                                    egui::accesskit::Toggled::False
                                });
                            });
                            if b.on_hover_text(tip).clicked() {
                                action = Some(act);
                            }
                        }
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

    // The history, down the left.
    if let Some(past) = past
        && let Some(list) = past.history
        && let Some((id, path)) =
            crate::ui::file_view::history(ui, list, past.history_file, past.selected, past.now)
    {
        action = Some(Action::Pick { id, path });
    }
    let blame = past.and_then(|p| p.blame).map(|b| {
        b.and_then(|b| b.as_ref().ok())
            .map(|b| (b.as_ref(), past.map_or(0, |p| p.now)))
    });
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
            if let Some(a) = body(ui, d, split, images.is_some(), lines, blame) {
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

/// How many of the picked lines are changes (that staging would take).
fn changed(d: &RenderedDiff, picked: &BTreeSet<(usize, usize)>) -> usize {
    picked
        .iter()
        .filter(|&&(h, l)| {
            h < d.changes
                && matches!(
                    d.diff.hunks[h].lines[l].kind,
                    LineKind::Add | LineKind::Delete
                )
        })
        .count()
}

/// The line shown in a row (for a side-by-side row, the new side's, or
/// the old side's when it has none).
fn line_at(d: &RenderedDiff, split: bool, row: usize) -> Option<(usize, usize)> {
    if split {
        match d.split.get(row)? {
            SplitRow::Pair(a, b) => b.or(*a),
            SplitRow::Header(_) => None,
        }
    } else {
        match d.unified.get(row)? {
            Row::Line(h, l) => Some((*h, *l)),
            Row::Header(_) => None,
        }
    }
}

/// A line's own space: a click picks it (Ctrl adds, Shift picks the range
/// from the last one), as does dragging over lines; right-click for a menu.
fn line_input(
    ui: &mut egui::Ui,
    rect: Rect,
    d: &RenderedDiff,
    (at, salt): ((usize, usize), u8),
    lines: &Lines<'_>,
    menu: &mut Option<Action>,
) -> Option<Action> {
    let response = ui.interact(
        rect,
        ui.id().with(("line", at, salt)),
        Sense::click_and_drag(),
    );
    let picked = lines.picked.contains(&at);
    let content = &d.diff.hunks[at.0].lines[at.1].content;
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            true,
            picked,
            format!("Select line: {}", content.trim()),
        )
    });
    let mods = ui.input(|i| i.modifiers);
    let how = if mods.shift {
        Select::Range
    } else if mods.command {
        Select::Toggle
    } else {
        Select::Only
    };
    response.context_menu(|ui| {
        if ui.button("Copy").clicked() {
            *menu = Some(Action::CopyPicked);
            ui.close();
        }
        if let Some(verb) = lines.verb()
            && changed(d, lines.picked) > 0
            && ui
                .add_enabled(
                    !lines.busy(),
                    egui::Button::new(format!("{verb} selection")),
                )
                .clicked()
        {
            *menu = Some(Action::StagePicked);
            ui.close();
        }
        if ui.button("Select all lines").clicked() {
            *menu = Some(Action::SelectAll);
            ui.close();
        }
    });
    if response.drag_started() {
        ui.data_mut(|m| m.insert_temp(egui::Id::new("diff-line-drag"), true));
        let how = if how == Select::Range {
            how
        } else {
            Select::Only
        };
        return Some(Action::Select { at, how });
    }
    if response.clicked() {
        return Some(Action::Select { at, how });
    }
    // Right-click outside the picked lines picks that one, for its menu.
    (response.secondary_clicked() && !picked).then_some(Action::Select {
        at,
        how: Select::Only,
    })
}

fn body(
    ui: &mut egui::Ui,
    d: &RenderedDiff,
    split: bool,
    has_images: bool,
    lines: &Lines<'_>,
    // The blame (once loaded), and the time now.
    blame: Option<Option<(&Blame, i64)>>,
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
    // The blame's column, at the left.
    let lead = if blame.is_some() { GUTTER } else { 0.0 };
    let blamed = blame.flatten().map(|(b, now)| (b, file_view::ages(b), now));
    let width = if split {
        available
    } else {
        (lead + 2.0 * NUMBER + MARKER + code_width).max(available)
    };
    let area = if split {
        egui::ScrollArea::vertical()
    } else {
        egui::ScrollArea::both()
    };
    let sideways_id = egui::Id::new("diff-split-sideways");
    let sideways = sideways_offset(ui, sideways_id);

    let share = split_share(ui.ctx());
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
        let mut menu = None;
        // The blame's commit on the row above, for where a run starts.
        let mut above: Option<usize> = None;
        if range.end == rows {
            #[expect(clippy::cast_precision_loss, reason = "visible row count is small")]
            let drawn = start_y + range.len() as f32 * LINE;
            end = Some(drawn);
        }
        for (n, row) in range.clone().enumerate() {
            #[expect(clippy::cast_precision_loss, reason = "visible row count is small")]
            let top = start_y + n as f32 * LINE;
            let full = Rect::from_min_size(Pos2::new(left, top), egui::vec2(width, LINE));
            // The blame, for the new side's line.
            if blame.is_some() {
                let cell = Rect::from_min_size(full.min, egui::vec2(lead, LINE));
                let new_line = line_at(d, split, row)
                    .and_then(|(h, l)| d.diff.hunks[h].lines[l].new_line)
                    .and_then(|n| usize::try_from(n).ok())
                    .and_then(|n| n.checked_sub(1));
                match blamed {
                    Some((b, ages, now)) => {
                        let commit = new_line.and_then(|n| b.lines.get(n)).map(|l| l.commit);
                        let starts = commit.is_some() && commit != above;
                        if commit.is_some() {
                            above = commit;
                        }
                        if let Some(id) =
                            file_view::blame_cell(ui, cell, b, new_line, starts, ages, now)
                        {
                            action = Some(Action::GoTo(id));
                        }
                    }
                    None => {
                        painter.rect_filled(cell, 0, theme::bg_raised());
                    }
                }
            }
            let rect = Rect::from_min_max(Pos2::new(left + lead, top), full.max);
            if split {
                match d.split[row] {
                    SplitRow::Header(h) => {
                        header(painter, rect, &d.diff.hunks[h].header);
                        if let Some(a) = hunk_button(ui, rect, h, lines) {
                            action = Some(a);
                        }
                    }
                    SplitRow::Pair(a, b) => {
                        let (l, r) =
                            pair_row(painter, rect, d, (a, b), (share, sideways), lines.picked);
                        for (at, half_rect, salt) in [(a, l, 0), (b, r, 1)] {
                            if let Some(at) = at
                                && let Some(picked) =
                                    line_input(ui, half_rect, d, (at, salt), lines, &mut menu)
                            {
                                action = Some(picked);
                            }
                        }
                    }
                }
            } else {
                match d.unified[row] {
                    Row::Header(h) => {
                        header(painter, rect, &d.diff.hunks[h].header);
                        if let Some(a) = hunk_button(ui, rect, h, lines) {
                            action = Some(a);
                        }
                    }
                    Row::Line(h, l) => {
                        let picked = lines.picked.contains(&(h, l));
                        unified_line(
                            painter,
                            rect,
                            &d.diff.hunks[h].lines[l],
                            &d.jobs[h][l],
                            picked,
                        );
                        if let Some(a) = line_input(ui, rect, d, ((h, l), 0), lines, &mut menu) {
                            action = Some(a);
                        }
                    }
                }
            }
        }
        // Dragging over lines picks them all, from where it started.
        let drag_id = egui::Id::new("diff-line-drag");
        if ui.data(|m| m.get_temp::<bool>(drag_id)).unwrap_or(false) {
            let (down, at) = ui.input(|i| (i.pointer.primary_down(), i.pointer.interact_pos()));
            if !down {
                ui.data_mut(|m| m.remove::<bool>(drag_id));
            } else if action.is_none()
                && let Some(pos) = at
            {
                #[expect(clippy::cast_possible_truncation, reason = "a row index")]
                #[expect(clippy::cast_sign_loss, reason = "clamped at zero first")]
                let row = (((pos.y - start_y) / LINE).floor().max(0.0) as usize + range.start)
                    .min(rows.saturating_sub(1));
                if let Some(at) = line_at(d, split, row) {
                    action = Some(Action::Select {
                        at,
                        how: Select::Range,
                    });
                }
            }
        }
        if menu.is_some() {
            action = menu;
        }
    });
    // Side by side, the two halves go on down to the bottom of the view,
    // past the last line.
    let view = output.inner_rect;
    if split {
        split_scrollbars(ui, sideways_id, (view, lead), share, code_width);
    }
    if split && let Some(end) = end.filter(|&end| end < view.bottom()) {
        ui.painter().with_clip_rect(view).vline(
            view.left() + lead + (width - lead) * share,
            end.max(view.top())..=view.bottom(),
            Stroke::new(1.0, theme::border()),
        );
    }
    if split {
        let from = view.left() + lead;
        split_handle(
            ui,
            from + (width - lead) * share,
            view.y_range(),
            egui::Rangef::new(from, view.left() + width),
        );
    }
    // Ctrl+C copies the picked lines (when nothing else has the focus).
    let copy = ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
    if copy && !lines.picked.is_empty() && ui.memory(|m| m.focused().is_none()) {
        action = Some(Action::CopyPicked);
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
fn hunk_button(ui: &mut egui::Ui, header: Rect, hunk: usize, lines: &Lines<'_>) -> Option<Action> {
    let label = format!("{} hunk", lines.verb()?);
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
            ui.place(rect.shrink2(egui::vec2(0.0, 1.0)), button)
        })
        .inner;
    (response.clicked() && !lines.busy()).then_some(Action::StageHunk(hunk))
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

pub(crate) fn header(painter: &egui::Painter, rect: Rect, text: &str) {
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

/// Marks a picked line: its numbers (`gutter` wide) in the accent, a bar
/// down its left, and a light wash over its code (which keeps its added
/// or deleted colour).
fn picked_mark(painter: &egui::Painter, rect: Rect, gutter: f32) {
    painter.rect_filled(
        Rect::from_min_size(rect.min, egui::vec2(gutter, rect.height())),
        0,
        theme::accent().gamma_multiply(0.45),
    );
    painter.rect_filled(rect, 0, theme::text().gamma_multiply(0.06));
    painter.rect_filled(
        Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())),
        0,
        theme::accent(),
    );
}

pub(crate) fn unified_line(
    painter: &egui::Painter,
    rect: Rect,
    line: &DiffLine,
    job: &egui::text::LayoutJob,
    picked: bool,
) {
    let (gutter_bg, code_bg) = backgrounds(line.kind);
    let gutter = Rect::from_min_size(rect.min, egui::vec2(2.0 * NUMBER, LINE));
    let body = Rect::from_min_max(Pos2::new(gutter.right(), rect.top()), rect.max);
    painter.rect_filled(gutter, 0, gutter_bg);
    painter.rect_filled(body, 0, code_bg);
    if picked {
        picked_mark(painter, rect, 2.0 * NUMBER);
    }
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

/// How much of a side-by-side diff the old side takes, from 0 to 1: set by
/// dragging the line between the sides, the same in every diff.
pub(crate) fn split_share(ctx: &egui::Context) -> f32 {
    ctx.data_mut(|d| d.get_persisted::<f32>(egui::Id::new("diff-split-share")))
        .unwrap_or(0.5)
}

/// "Open in Beyond Compare" and "Open in editor", for the tools chosen.
pub(crate) fn open_with_buttons(
    ui: &mut egui::Ui,
    (diff_tool, editor): (Option<&str>, Option<&str>),
) -> Option<Action> {
    let mut action = None;
    if let Some(name) = editor {
        let b = ui
            .button(RichText::new(format!("{}  Editor", icon::NOTE_PENCIL)).size(12.0))
            .on_hover_text(format!(
                "Open the file in {name}, at the line you're looking at"
            ));
        ui.ctx()
            .accesskit_node_builder(b.id, |n| n.set_label("Open in editor"));
        if b.clicked() {
            action = Some(Action::OpenInEditor);
        }
    }
    if let Some(name) = diff_tool {
        let b = ui
            .button(RichText::new(format!("{}  {name}", icon::ARROW_SQUARE_OUT)).size(12.0))
            .on_hover_text(format!("Open both versions in {name}"));
        let label = format!("Open in {name}");
        ui.ctx()
            .accesskit_node_builder(b.id, |n| n.set_label(label));
        if b.clicked() {
            action = Some(Action::OpenInDiffTool);
        }
    }
    action
}

/// The line between the two sides, at `x` from `top` to `bottom`, as a
/// handle: dragging it shares out `span` (where the two sides are) between
/// them; a double-click gives them half each.
pub(crate) fn split_handle(ui: &mut egui::Ui, x: f32, ys: egui::Rangef, span: egui::Rangef) {
    let rect = Rect::from_x_y_ranges(x - 3.0..=x + 3.0, ys);
    let handle = ui.interact(rect, ui.id().with("split-handle"), Sense::click_and_drag());
    handle.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Resize the two sides")
    });
    let handle = handle
        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
        .on_hover_text("Drag to resize the sides; double-click for half each");
    if handle.hovered() || handle.dragged() {
        ui.painter().vline(x, ys, Stroke::new(2.0, theme::accent()));
    }
    let id = egui::Id::new("diff-split-share");
    if handle.double_clicked() {
        ui.data_mut(|d| d.insert_persisted(id, 0.5_f32));
    } else if handle.dragged()
        && let Some(at) = handle.interact_pointer_pos()
        && span.span() > 0.0
    {
        let share = ((at.x - span.min) / span.span()).clamp(0.15, 0.85);
        ui.data_mut(|d| d.insert_persisted(id, share));
        ui.ctx().request_repaint();
    }
}

/// A line of a diff, if there is one: (hunk, line).
type At = Option<(usize, usize)>;

/// A side-by-side row: the old line and the new in `rect`'s two halves (the
/// old side `share` of it), their code `shift` scrolled sideways, and the
/// line between them. Gives the two halves.
pub(crate) fn pair_row(
    painter: &egui::Painter,
    rect: Rect,
    d: &RenderedDiff,
    (a, b): (At, At),
    (share, shift): (f32, f32),
    picked: &BTreeSet<(usize, usize)>,
) -> (Rect, Rect) {
    let old = rect.width() * share;
    let l = Rect::from_min_size(rect.min, egui::vec2(old, rect.height()));
    let r = Rect::from_min_max(Pos2::new(rect.left() + old, rect.top()), rect.max);
    side(painter, l, d, a, (false, shift), picked);
    side(painter, r, d, b, (true, shift), picked);
    painter.vline(l.right(), l.y_range(), Stroke::new(1.0, theme::border()));
    (l, r)
}

/// How far a side-by-side diff's code is scrolled sideways (kept under `id`).
pub(crate) fn sideways_offset(ui: &egui::Ui, id: egui::Id) -> f32 {
    ui.data(|d| d.get_temp::<f32>(id)).unwrap_or_default()
}

/// Side by side, each half's code has a scrollbar of its own, under its
/// code (its numbers stay put), and the two move together: either bar, or
/// the sideways wheel over either half, scrolls both. `view` is where the
/// lines are shown, with `lead` (blame) at its left.
pub(crate) fn split_scrollbars(
    ui: &mut egui::Ui,
    id: egui::Id,
    (view, lead): (Rect, f32),
    share: f32,
    code_width: f32,
) {
    let sideways = sideways_offset(ui, id);
    let line = view.left() + lead + (view.width() - lead) * share;
    let code = |old: bool| {
        let (from, to) = if old {
            (view.left() + lead, line)
        } else {
            (line, view.right())
        };
        Rect::from_x_y_ranges(from + NUMBER + MARKER..=to, view.y_range())
    };
    let mut x = sideways;
    for (which, old) in [(0u8, true), (1, false)] {
        x = widgets::sideways_scrollbar(ui, id.with(which), (code(old), view), x, code_width);
    }
    ui.data_mut(|d| d.insert_temp(id, x));
    if x != sideways {
        // The bar drawn first shows where they are next frame.
        ui.ctx().request_repaint();
    }
}

/// One half of a side-by-side row; its code is `shift` scrolled sideways.
pub(crate) fn side(
    painter: &egui::Painter,
    rect: Rect,
    d: &RenderedDiff,
    at: Option<(usize, usize)>,
    (new, shift): (bool, f32),
    picked: &BTreeSet<(usize, usize)>,
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
    if picked.contains(&(h, l)) {
        picked_mark(painter, rect, NUMBER);
    }
    number(
        painter,
        gutter,
        if new { line.new_line } else { line.old_line },
    );
    code(painter, body, line, &d.jobs[h][l], shift);
}

/// How a changed image is shown: the two versions side by side, one frame
/// with a line to slide between them, or one laid over the other.
#[derive(Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub(crate) enum ImageView {
    #[default]
    SideBySide,
    Swipe,
    Onion,
}

const IMAGE_VIEW: &str = "image-view";
/// Where the swipe line is, across the image (0 = all after, 1 = all before).
const SWIPE_AT: &str = "image-swipe-at";
/// How much of the after version shows in the onion skin.
const ONION_MIX: &str = "image-onion-mix";

pub(crate) fn image_preview(ui: &mut egui::Ui, pair: &ImagePair) {
    egui::Frame::new()
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            // Both versions are pictures: they can be compared in one frame.
            let both = match (&pair.old, &pair.new) {
                (Some(old), Some(new)) => match (&old.preview, &new.preview) {
                    (Preview::Image { .. }, Preview::Image { .. }) => Some((old, new)),
                    _ => None,
                },
                _ => None,
            };
            let id = egui::Id::new(IMAGE_VIEW);
            let mut view = if both.is_some() {
                ui.data_mut(|d| d.get_persisted::<ImageView>(id))
                    .unwrap_or_default()
            } else {
                ImageView::SideBySide
            };
            if both.is_some() {
                ui.horizontal(|ui| {
                    let picked = widgets::segmented(
                        ui,
                        view,
                        &[
                            (ImageView::SideBySide, "Side by side"),
                            (ImageView::Swipe, "Swipe"),
                            (ImageView::Onion, "Onion skin"),
                        ],
                    );
                    if picked != view {
                        view = picked;
                        ui.data_mut(|d| d.insert_persisted(id, view));
                    }
                    if view == ImageView::Onion {
                        ui.add_space(16.0);
                        onion_slider(ui);
                    }
                });
                ui.add_space(10.0);
            }
            match (view, both) {
                (ImageView::Swipe | ImageView::Onion, Some((old, new))) => {
                    overlaid(ui, view, old, new)
                }
                _ => {
                    let gap = 16.0;
                    let w = ((ui.available_width() - gap) / 2.0).max(120.0);
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        image_side(ui, "Before", pair.old.as_ref(), "New file", w);
                        image_side(ui, "After", pair.new.as_ref(), "Deleted", w);
                    });
                }
            }
        });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );
}

/// "Before ━━●━━ After": how much of each version the onion skin shows.
fn onion_slider(ui: &mut egui::Ui) {
    let id = egui::Id::new(ONION_MIX);
    let mut mix = ui.data_mut(|d| d.get_persisted::<f32>(id)).unwrap_or(0.5);
    let small = |t: &str| RichText::new(t).size(12.0).color(theme::muted());
    ui.label(small("Before"));
    let slider = ui.add(
        egui::Slider::new(&mut mix, 0.0..=1.0)
            .show_value(false)
            .trailing_fill(true),
    );
    slider.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Slider,
            true,
            "How much of the after version shows",
        )
    });
    ui.label(small("After"));
    if slider.changed() {
        ui.data_mut(|d| d.insert_persisted(id, mix));
    }
}

/// "64 × 69 · 653 B" for a side.
fn describe(side: &Side) -> String {
    match &side.preview {
        Preview::Image { width, height, .. } => {
            format!("{width} × {height} · {}", format::bytes(side.size))
        }
        _ => format::bytes(side.size),
    }
}

/// Both versions in one frame, centred on each other at the same scale: the
/// before on the left of a line you slide and the after on its right, or the
/// after laid over the before as much as the slider says.
fn overlaid(ui: &mut egui::Ui, view: ImageView, old: &Side, new: &Side) {
    let (
        Preview::Image {
            texture: before,
            width: bw,
            height: bh,
        },
        Preview::Image {
            texture: after,
            width: aw,
            height: ah,
        },
    ) = (&old.preview, &new.preview)
    else {
        return;
    };
    ui.horizontal(|ui| {
        let muted = |t: String| RichText::new(t).size(12.0).color(theme::muted());
        ui.label(RichText::new("Before").strong().size(12.0));
        ui.label(muted(describe(old)));
        ui.label(muted(icon::ARROW_RIGHT.into()));
        ui.label(RichText::new("After").strong().size(12.0));
        ui.label(muted(describe(new)));
    });
    let width = ui.available_width().max(120.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, IMAGE_BOX), Sense::hover());
    let painter = ui.painter_at(rect);
    checkerboard(&painter, rect);

    // One scale for both, so their pixels line up; never enlarged.
    let (bw, bh, aw, ah) = (*bw as f32, *bh as f32, *aw as f32, *ah as f32);
    let fit = rect.shrink(12.0);
    let scale = (fit.width() / bw.max(aw))
        .min(fit.height() / bh.max(ah))
        .min(1.0);
    let at = |w: f32, h: f32| Rect::from_center_size(rect.center(), egui::vec2(w, h) * scale);
    let (b_rect, a_rect) = (at(bw, bh), at(aw, ah));
    let both = b_rect.union(a_rect);
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    let draw = |painter: &egui::Painter, texture: &egui::TextureHandle, shown: Rect, alpha: f32| {
        painter.image(
            texture.id(),
            shown,
            uv,
            Color32::WHITE.gamma_multiply(alpha),
        );
    };

    match view {
        ImageView::Onion => {
            let mix = ui
                .data_mut(|d| d.get_persisted::<f32>(egui::Id::new(ONION_MIX)))
                .unwrap_or(0.5);
            draw(&painter, before, b_rect, 1.0 - mix);
            draw(&painter, after, a_rect, mix);
        }
        _ => {
            let id = egui::Id::new(SWIPE_AT);
            let mut share = ui.data_mut(|d| d.get_persisted::<f32>(id)).unwrap_or(0.5);
            // Dragging (or clicking) anywhere on the picture moves the line.
            let hit = ui.interact(rect, ui.id().with("image-swipe"), Sense::click_and_drag());
            hit.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Other,
                    true,
                    "Slide between before and after",
                )
            });
            let hit = hit.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
            if (hit.dragged() || hit.clicked())
                && let Some(p) = hit.interact_pointer_pos()
                && both.width() > 0.0
            {
                share = ((p.x - both.left()) / both.width()).clamp(0.0, 1.0);
                ui.data_mut(|d| d.insert_persisted(id, share));
                ui.ctx().request_repaint();
            }
            let x = both.left() + both.width() * share;
            let left = Rect::from_x_y_ranges(rect.left()..=x, rect.y_range());
            let right = Rect::from_x_y_ranges(x..=rect.right(), rect.y_range());
            draw(&painter.with_clip_rect(left), before, b_rect, 1.0);
            draw(&painter.with_clip_rect(right), after, a_rect, 1.0);

            // The line, with a grip in the middle and a word on each side.
            let ys = rect.shrink(1.0).y_range();
            let active = hit.hovered() || hit.dragged();
            let colour = if active {
                theme::accent()
            } else {
                theme::text()
            };
            painter.vline(x, ys, Stroke::new(2.0, colour));
            painter.circle(
                Pos2::new(x, rect.center().y),
                10.0,
                theme::bg_raised(),
                Stroke::new(2.0, colour),
            );
            painter.text(
                Pos2::new(x, rect.center().y),
                egui::Align2::CENTER_CENTER,
                icon::ARROWS_LEFT_RIGHT,
                egui::FontId::proportional(12.0),
                colour,
            );
            let tag = |pos: Pos2, align: egui::Align2, text: &str| {
                let galley = painter.layout_no_wrap(text.into(), theme::body(11.0), theme::text());
                let r = align.anchor_size(pos, galley.size() + egui::vec2(12.0, 6.0));
                painter.rect_filled(r, 4, theme::bg_raised().gamma_multiply(0.9));
                painter.galley(r.min + egui::vec2(6.0, 3.0), galley, theme::text());
            };
            if x - rect.left() > 70.0 {
                tag(
                    Pos2::new(x - 8.0, rect.top() + 8.0),
                    egui::Align2::RIGHT_TOP,
                    "Before",
                );
            }
            if rect.right() - x > 70.0 {
                tag(
                    Pos2::new(x + 8.0, rect.top() + 8.0),
                    egui::Align2::LEFT_TOP,
                    "After",
                );
            }
        }
    }
    painter.rect_stroke(
        rect,
        6,
        Stroke::new(1.0, theme::border()),
        egui::StrokeKind::Inside,
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
