//! Small drawing helpers shared by the views.

use std::sync::Arc;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Galley, Painter, Pos2, Rect, Response, RichText,
    Sense, Stroke, StrokeKind, Ui,
    text::{LayoutJob, TextFormat, TextWrapping},
};
use egui_phosphor::regular as icon;
use git_raccoon_core::details::FileStatus;

use crate::theme;

/// One line of text, cut with "…" to fit `width`.
pub fn elided(
    painter: &Painter,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(text.to_owned(), TextFormat::simple(font, color));
    job.wrap = TextWrapping {
        max_width: width.max(0.0),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    painter.layout_job(job)
}

/// Whether a label's text is cut short in a pill at most `max_width` wide.
pub fn pill_elided(painter: &Painter, text: &str, max_width: f32) -> bool {
    elided(
        painter,
        text,
        FontId::proportional(12.0),
        theme::text(),
        max_width - 14.0,
    )
    .elided
}

/// Draws elided text vertically centred in `rect`, from its left edge.
pub fn text_in(painter: &Painter, rect: Rect, text: &str, font: FontId, color: Color32) -> Rect {
    let galley = elided(painter, text, font, color, rect.width());
    let pos = Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0);
    let r = Rect::from_min_size(pos, galley.size());
    painter.galley(pos, galley, color);
    r
}

/// A borderless button showing a Phosphor icon, with a tooltip.
/// A frameless icon button. Its tooltip is also its name for screen
/// readers (the icon alone says nothing).
pub fn icon_button(ui: &mut Ui, glyph: &str, tooltip: &str) -> Response {
    let response = ui
        .add(egui::Button::new(RichText::new(glyph).size(16.0).color(theme::muted())).frame(false));
    let name = tooltip.to_string();
    ui.ctx()
        .accesskit_node_builder(response.id, |n| n.set_label(name));
    response.on_hover_text(tooltip)
}

/// The shadow under a panel floating over the rest (the search box, a
/// commit's spread-out labels).
pub const POPUP_SHADOW: egui::Shadow = egui::Shadow {
    offset: [0, 4],
    blur: 12,
    spread: 0,
    color: Color32::from_black_alpha(90),
};

/// How long things take to slide out of the way of one being dragged
/// (a tab, a column heading), in seconds.
pub const DRAG_SLIDE: f32 = 0.12;
/// How see-through a dragged tab or heading is.
pub const DRAG_OPACITY: f32 = 0.65;

/// `area.show_rows` for rows exactly `row` tall with no gaps between
/// them. egui works out which rows are on screen, and how tall the list
/// is, from the row height plus the spacing it finds *before* drawing
/// them: left in, it draws too few rows and leaves room to scroll past.
pub fn gapless_rows<R>(
    ui: &mut Ui,
    area: egui::ScrollArea,
    row: f32,
    total: usize,
    add: impl FnOnce(&mut Ui, std::ops::Range<usize>) -> R,
) -> egui::scroll_area::ScrollAreaOutput<R> {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        area.show_rows(ui, row, total, add)
    })
    .inner
}

/// A horizontal scrollbar along the bottom of `view`, for something
/// `content` wide shown there `offset` scrolled; the horizontal wheel (or
/// Shift and the wheel) over `view` moves it too. Drawn as egui draws its
/// own (thin until pointed at, in the theme's colours, showing while the
/// pointer is over `around`), for when one view has more than one thing
/// scrolling sideways. Returns the new offset.
pub fn sideways_scrollbar(
    ui: &mut Ui,
    id: egui::Id,
    (view, around): (Rect, Rect),
    offset: f32,
    content: f32,
) -> f32 {
    let spare = (content - view.width()).max(0.0);
    if spare <= 0.0 {
        return 0.0;
    }
    let mut offset = offset;
    let over_view = ui.rect_contains_pointer(view);
    if over_view {
        let dx = ui.input(|i| i.smooth_scroll_delta.x);
        if dx != 0.0 {
            offset -= dx;
            ui.ctx().request_repaint();
        }
    }

    let style = ui.spacing().scroll;
    let bottom = view.bottom() - style.bar_outer_margin;
    let full = Rect::from_x_y_ranges(view.x_range(), bottom - style.bar_width..=bottom);
    let response = ui.interact(full, id, Sense::click_and_drag());
    response.widget_info(|| egui::WidgetInfo::new(egui::WidgetType::ScrollBar));
    let active = response.hovered() || response.dragged();
    let grown = ui.ctx().animate_bool_responsive(id.with("grown"), active);
    let thickness = if style.floating {
        egui::lerp(style.floating_width..=style.bar_width, grown)
    } else {
        style.bar_width
    };
    let track = Rect::from_x_y_ranges(view.x_range(), bottom - thickness..=bottom);
    let handle_width = (track.width() * view.width() / content).max(style.handle_min_length);
    let travel = (track.width() - handle_width).max(1.0);

    if response.dragged() {
        offset += response.drag_delta().x * spare / travel;
    } else if response.clicked()
        && let Some(p) = response.interact_pointer_pos()
    {
        offset = (p.x - track.left() - handle_width / 2.0) / travel * spare;
    }
    let offset = offset.clamp(0.0, spare);

    let handle = Rect::from_min_size(
        Pos2::new(track.left() + offset / spare * travel, track.top()),
        egui::vec2(handle_width, thickness),
    );
    let on_handle = response.hovered()
        && ui.input(|i| i.pointer.latest_pos().is_some_and(|p| handle.contains(p)));
    let visuals = if response.is_pointer_button_down_on() {
        &ui.visuals().widgets.active
    } else if on_handle {
        &ui.visuals().widgets.hovered
    } else {
        &ui.visuals().widgets.inactive
    };
    let (handle_opacity, background_opacity) = if !style.floating {
        (1.0, 1.0)
    } else if active {
        (
            style.interact_handle_opacity,
            style.interact_background_opacity,
        )
    } else {
        let over = ui.rect_contains_pointer(around);
        let shown = ui.ctx().animate_bool_responsive(id.with("shown"), over);
        (
            egui::lerp(
                style.dormant_handle_opacity..=style.active_handle_opacity,
                shown,
            ),
            if over {
                style.active_background_opacity
            } else {
                style.dormant_background_opacity
            },
        )
    };
    let handle_colour = if style.foreground_color {
        visuals.fg_stroke.color
    } else {
        visuals.bg_fill
    };
    let painter = ui.painter();
    painter.rect_filled(
        track,
        visuals.corner_radius,
        ui.visuals()
            .extreme_bg_color
            .gamma_multiply(background_opacity),
    );
    painter.rect_filled(
        handle,
        visuals.corner_radius,
        handle_colour.gamma_multiply(handle_opacity),
    );
    offset
}

/// A filled accent button.
pub fn primary_button(ui: &mut Ui, label: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(label).color(theme::on_accent()).strong())
            .fill(theme::accent())
            .min_size(egui::vec2(0.0, 32.0)),
    )
}

/// A two-option toggle like "Unified | Split". Returns the new choice.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: T, options: &[(T, &str)]) -> T {
    let mut chosen = value;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (option, label) in options {
            let on = *option == value;
            let text = RichText::new(*label).size(12.0).color(if on {
                theme::text()
            } else {
                theme::muted()
            });
            let button = egui::Button::new(text)
                .fill(if on {
                    theme::bg_selected()
                } else {
                    theme::bg()
                })
                .stroke(Stroke::new(1.0, theme::border()))
                .corner_radius(CornerRadius::ZERO);
            if ui.add(button).clicked() {
                chosen = *option;
            }
        }
    });
    chosen
}

pub fn status_colour(status: FileStatus) -> (char, Color32) {
    match status {
        FileStatus::Added => ('A', theme::add()),
        FileStatus::Modified => ('M', theme::modified()),
        FileStatus::Deleted => ('D', theme::del()),
        FileStatus::Renamed => ('R', theme::renamed()),
        FileStatus::Copied => ('C', theme::renamed()),
        FileStatus::Typechange => ('T', theme::typechange()),
        FileStatus::Untracked => ('?', theme::add()),
        FileStatus::Conflicted => ('!', theme::danger()),
    }
}

/// The coloured A/M/D/R letter for a file status, drawn at `rect`.
pub fn status_badge(painter: &Painter, rect: Rect, status: FileStatus) {
    let (letter, colour) = status_colour(status);
    painter.rect_filled(rect, CornerRadius::same(3), colour);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        letter,
        FontId::proportional(11.0),
        Color32::from_rgb(0x10, 0x13, 0x1a),
    );
}

/// A circle with someone's initials, coloured by their email.
pub fn avatar(ui: &mut Ui, name: &str, email: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), Sense::hover());
    let colour = egui::ecolor::Hsva::new(crate::format::hue(email) / 360.0, 0.55, 0.42, 1.0);
    ui.painter()
        .circle_filled(rect.center(), 16.0, Color32::from(colour));
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        crate::format::initials(name),
        FontId::proportional(12.0),
        Color32::WHITE,
    );
}

/// A small framed button with an icon and a count, like "🗑 7", in the
/// danger colour when `danger`. `name` is its tooltip and accessible name.
pub fn count_button(
    ui: &mut Ui,
    glyph: &str,
    count: usize,
    name: &str,
    enabled: bool,
    danger: bool,
) -> Response {
    let colour = if !enabled {
        theme::muted()
    } else if danger {
        theme::danger()
    } else {
        theme::text()
    };
    let button = egui::Button::new(
        RichText::new(format!("{glyph} {count}"))
            .size(12.0)
            .color(colour),
    )
    .fill(if danger && enabled {
        theme::danger_bg()
    } else {
        theme::bg_raised()
    })
    .stroke(Stroke::new(1.0, colour.gamma_multiply(0.6)))
    .corner_radius(CornerRadius::same(4));
    let response = ui.add_enabled(enabled, button);
    let label = name.to_string();
    ui.ctx()
        .accesskit_node_builder(response.id, |n| n.set_label(label));
    response.on_hover_text(name).on_disabled_hover_text(name)
}

/// A small rounded label with an outline, e.g. a branch name.
pub fn pill(
    painter: &Painter,
    left: Pos2,
    max_width: f32,
    text: &str,
    outline: Color32,
    filled: bool,
) -> Rect {
    let font = FontId::proportional(12.0);
    let galley = elided(
        painter,
        text,
        font,
        if filled {
            theme::text_on(outline)
        } else {
            theme::text()
        },
        max_width - 14.0,
    );
    let size = egui::vec2(galley.size().x + 14.0, 20.0);
    let rect = Rect::from_min_size(Pos2::new(left.x, left.y - size.y / 2.0), size);
    painter.rect(
        rect,
        CornerRadius::same(4),
        if filled { outline } else { theme::bg_raised() },
        Stroke::new(1.0, outline),
        StrokeKind::Inside,
    );
    painter.galley(
        Pos2::new(rect.left() + 7.0, rect.center().y - galley.size().y / 2.0),
        galley,
        theme::text(),
    );
    rect
}

/// A muted section heading ("LOCAL", "COMMIT").
pub fn section_label(text: &str) -> RichText {
    RichText::new(text.to_uppercase())
        .size(11.0)
        .strong()
        .color(theme::muted())
}

pub const ICON_CLOSE: &str = icon::X;

/// An icon button with a readable name for screen readers (and tests) and
/// the same text as its tooltip.
pub fn named_icon_button(ui: &mut Ui, glyph: &str, name: &str, enabled: bool) -> Response {
    let response = ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(glyph).size(15.0).color(theme::muted())).frame(false),
    );
    ui.ctx()
        .accesskit_node_builder(response.id, |node| node.set_label(name));
    response.on_hover_text(name)
}
