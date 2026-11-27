//! Drawing a terminal's screen with egui, and turning keys and the mouse
//! into what a terminal program expects.

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, CursorShape, NamedColor};
use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{self, Color32, Event, Key, Modifiers, Pos2, Rect, Sense, Stroke};

use super::{Size, Terminal};
use crate::theme;

/// The font size of the terminal's text.
pub const SIZE: f32 = 13.0;

/// Whether the look is a dark one.
fn dark() -> bool {
    let bg = theme::bg();
    (u32::from(bg.r()) + u32::from(bg.g()) + u32::from(bg.b())) < 3 * 128
}

/// The 16 named colours, for a dark or a light look.
fn named(index: usize) -> Color32 {
    const DARK: [u32; 16] = [
        0x1d1f21, 0xe06c62, 0x8fbf6a, 0xe5c07b, 0x6aa5e8, 0xc58ad9, 0x5fbfbf, 0xd6ccc0, 0x6b6560,
        0xff8a7a, 0xa8d98a, 0xf2d58e, 0x8cbcff, 0xdaa6eb, 0x80d6d6, 0xf7f0e6,
    ];
    const LIGHT: [u32; 16] = [
        0x2b2622, 0xb7372c, 0x3f7f2a, 0x8a6100, 0x2a5fb0, 0x8a3fa6, 0x16777a, 0xd8d0c6, 0x6b6560,
        0xd04a3c, 0x4f9a36, 0xa77a10, 0x3c74cc, 0xa456c2, 0x22908f, 0xffffff,
    ];
    let c = if dark() { DARK } else { LIGHT }[index % 16];
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// A colour by its number: the 16 named, the 6×6×6 cube, then the greys.
pub fn colour_of_index(index: usize) -> Color32 {
    match index {
        0..=15 => named(index),
        16..=231 => {
            let i = index - 16;
            let level = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
            Color32::from_rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
        }
        232..=255 => {
            let v = (8 + (index - 232) * 10) as u8;
            Color32::from_rgb(v, v, v)
        }
        256 => theme::text(),
        257 => theme::bg(),
        258 => theme::accent(),
        _ => theme::muted(),
    }
}

/// A cell's colour: the program's own choice for a numbered colour, else
/// the look's.
fn colour(c: AnsiColor, colours: &alacritty_terminal::term::color::Colors, bold: bool) -> Color32 {
    match c {
        AnsiColor::Spec(rgb) => Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        AnsiColor::Indexed(i) => {
            let i = usize::from(i);
            // Bold makes the first eight their bright selves, as terminals do.
            let i = if bold && i < 8 { i + 8 } else { i };
            colours[i].map_or_else(
                || colour_of_index(i),
                |rgb| Color32::from_rgb(rgb.r, rgb.g, rgb.b),
            )
        }
        AnsiColor::Named(n) => {
            let mut i = n as usize;
            if bold && i < 8 {
                i += 8;
            }
            if let Some(rgb) = colours[i] {
                return Color32::from_rgb(rgb.r, rgb.g, rgb.b);
            }
            match n {
                NamedColor::Foreground | NamedColor::BrightForeground => theme::text(),
                NamedColor::Background => Color32::TRANSPARENT,
                NamedColor::Cursor => theme::accent(),
                NamedColor::DimForeground => theme::muted(),
                _ if i >= 259 => named(i - 259).gamma_multiply(0.7),
                _ => colour_of_index(i),
            }
        }
    }
}

/// The size of a cell in the terminal's font.
fn cell_size(ui: &egui::Ui) -> egui::Vec2 {
    let font = theme::mono(SIZE);
    ui.fonts_mut(|f| {
        let width = f.glyph_width(&font, 'M');
        egui::vec2(width.max(1.0), f.row_height(&font).ceil().max(1.0))
    })
}

/// What happened in the terminal this frame, for its holder.
#[derive(Default)]
pub struct Shown {
    pub response: Option<egui::Response>,
    /// The word under a ctrl/cmd-click (or plain click) on the screen.
    pub clicked_word: Option<String>,
}

/// Draws the terminal in all of `ui`'s room, and, while it has focus,
/// sends it what's typed.
pub fn show(ui: &mut egui::Ui, t: &mut Terminal, id: egui::Id) -> Shown {
    t.poll();
    if let Some(text) = t.copied.take() {
        ui.ctx().copy_text(text);
    }
    let cell = cell_size(ui);
    let room = ui.available_rect_before_wrap();
    let pad = 6.0;
    let inner = room.shrink(pad);
    let size = Size {
        columns: ((inner.width() / cell.x).floor() as usize).max(2),
        lines: ((inner.height() / cell.y).floor() as usize).max(1),
        cell,
    };
    t.resize(size);
    let response = ui.interact(room, id, Sense::click_and_drag());
    ui.allocate_rect(room, Sense::hover());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, "Terminal"));
    let focused = response.has_focus();
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    if focused {
        // Tab, arrows and Esc are the program's, not egui's.
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });
        input(ui, t);
    }
    let mut shown = Shown::default();
    mouse(ui, t, &response, inner.min, cell, &mut shown);
    draw(ui, t, inner, cell, focused);
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    shown.response = Some(response);
    shown
}

/// Keys, text and pastes, as the program expects them.
fn input(ui: &egui::Ui, t: &mut Terminal) {
    let mode = t.mode();
    let events = ui.input(|i| i.events.clone());
    let now = ui.input(|i| i.time);
    for event in events {
        match event {
            Event::Text(text) => {
                t.last_input = now;
                t.type_text(&text);
            }
            Event::Paste(text) => {
                t.last_input = now;
                t.paste(&text);
            }
            // Ctrl+C copies what's selected; with nothing selected, it's
            // the program's (an interrupt).
            Event::Copy => match t.selected() {
                Some(text) => ui.ctx().copy_text(text),
                None => t.type_text("\x03"),
            },
            Event::Cut => t.type_text("\x18"),
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                if let Some(bytes) = key_bytes(key, modifiers, mode) {
                    t.last_input = now;
                    t.type_text(&bytes);
                }
            }
            _ => {}
        }
    }
}

/// What a key sends: `None` for keys that type text (sent as `Text`).
pub fn key_bytes(key: Key, m: Modifiers, mode: TermMode) -> Option<String> {
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    // xterm's modifier number: 1 + shift + 2·alt + 4·ctrl.
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.ctrl);
    let cursor = |c: char| {
        if modifier > 1 {
            format!("\x1b[1;{modifier}{c}")
        } else if app_cursor {
            format!("\x1bO{c}")
        } else {
            format!("\x1b[{c}")
        }
    };
    let tilde = |n: u8| {
        if modifier > 1 {
            format!("\x1b[{n};{modifier}~")
        } else {
            format!("\x1b[{n}~")
        }
    };
    let s = match key {
        Key::Enter => "\r".into(),
        Key::Tab if m.shift => "\x1b[Z".into(),
        Key::Tab => "\t".into(),
        Key::Backspace if m.alt => "\x1b\x7f".into(),
        Key::Backspace if m.ctrl => "\x08".into(),
        Key::Backspace => "\x7f".into(),
        Key::Escape => "\x1b".into(),
        Key::ArrowUp => cursor('A'),
        Key::ArrowDown => cursor('B'),
        Key::ArrowRight => cursor('C'),
        Key::ArrowLeft => cursor('D'),
        Key::Home => cursor('H'),
        Key::End => cursor('F'),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::F1 => "\x1bOP".into(),
        Key::F2 => "\x1bOQ".into(),
        Key::F3 => "\x1bOR".into(),
        Key::F4 => "\x1bOS".into(),
        Key::F5 => tilde(15),
        Key::F6 => tilde(17),
        Key::F7 => tilde(18),
        Key::F8 => tilde(19),
        Key::F9 => tilde(20),
        Key::F10 => tilde(21),
        Key::F11 => tilde(23),
        Key::F12 => tilde(24),
        _ => {
            // Ctrl+letter (and a few marks): the control character; Alt+key:
            // Esc first. egui doesn't type text for these.
            let name = key.symbol_or_name();
            let mut chars = name.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            let c = c.to_ascii_lowercase();
            if m.ctrl && !m.alt {
                let code = match c {
                    'a'..='z' => c as u8 - b'a' + 1,
                    '[' => 0x1b,
                    '\\' => 0x1c,
                    ']' => 0x1d,
                    ' ' | '2' | '@' => 0,
                    '6' | '^' => 0x1e,
                    '-' | '_' => 0x1f,
                    _ => return None,
                };
                // Ctrl+C and Ctrl+V come as Copy and Paste.
                if c == 'c' || c == 'v' || c == 'x' {
                    return None;
                }
                return Some((code as char).to_string());
            }
            if m.alt && !m.ctrl && c.is_ascii_graphic() {
                let c = if m.shift { c.to_ascii_uppercase() } else { c };
                return Some(format!("\x1b{c}"));
            }
            return None;
        }
    };
    Some(s)
}

/// Where a pointer position falls on the screen (row and column), and on
/// which half of the cell.
fn cell_at(pos: Pos2, origin: Pos2, cell: egui::Vec2, size: Size) -> (usize, usize, Side) {
    let x = ((pos.x - origin.x) / cell.x).max(0.0);
    let y = ((pos.y - origin.y) / cell.y).max(0.0);
    let column = (x.floor() as usize).min(size.columns.saturating_sub(1));
    let row = (y.floor() as usize).min(size.lines.saturating_sub(1));
    let side = if x.fract() < 0.5 {
        Side::Left
    } else {
        Side::Right
    };
    (row, column, side)
}

/// Selecting with the mouse, and the wheel.
fn mouse(
    ui: &egui::Ui,
    t: &mut Terminal,
    response: &egui::Response,
    origin: Pos2,
    cell: egui::Vec2,
    shown: &mut Shown,
) {
    let size = t.size();
    let point_of = |t: &Terminal, pos: Pos2| {
        let (row, column, side) = cell_at(pos, origin, cell, size);
        let offset = t.lock().grid().display_offset();
        let point =
            alacritty_terminal::term::viewport_to_point(offset, Point::new(row, Column(column)));
        (point, side)
    };
    let now = ui.input(|i| i.time);
    if let Some(pos) = response.interact_pointer_pos() {
        if response.drag_started() {
            let (point, side) = point_of(t, pos);
            t.lock().selection = Some(Selection::new(SelectionType::Simple, point, side));
            t.last_input = now;
        } else if response.dragged() {
            let (point, side) = point_of(t, pos);
            if let Some(s) = t.lock().selection.as_mut() {
                s.update(point, side);
            }
        }
        if response.double_clicked() || response.triple_clicked() {
            let (point, side) = point_of(t, pos);
            let kind = if response.triple_clicked() {
                SelectionType::Lines
            } else {
                SelectionType::Semantic
            };
            let mut s = Selection::new(kind, point, side);
            s.update(point, side);
            t.lock().selection = Some(s);
        } else if response.clicked() {
            t.lock().selection = None;
            // Ctrl+click (⌘ on a Mac) follows what's clicked, as terminals do.
            if ui.input(|i| i.modifiers.command) {
                let (point, _) = point_of(t, pos);
                shown.clicked_word = word_at(t, point);
            }
        }
    }
    if response.hovered() {
        let wheel = ui.input(|i| i.smooth_scroll_delta.y);
        if wheel.abs() > 0.0 {
            let lines = (wheel / cell.y).round() as i32;
            if lines != 0 {
                let mode = t.mode();
                if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL)
                {
                    // Full-screen programs (less, vim) get arrow keys.
                    let key = if lines > 0 { "A" } else { "B" };
                    let prefix = if mode.contains(TermMode::APP_CURSOR) {
                        "\x1bO"
                    } else {
                        "\x1b["
                    };
                    t.type_text(&format!("{prefix}{key}").repeat(lines.unsigned_abs() as usize));
                } else {
                    t.scroll(Scroll::Delta(lines));
                }
            }
        }
    }
}

/// The word (no spaces) at a point on the screen.
fn word_at(t: &Terminal, point: Point) -> Option<String> {
    let term = t.lock();
    let grid = term.grid();
    let columns = grid.columns();
    let row: String = (0..columns)
        .map(|c| grid[Point::new(point.line, Column(c))].c)
        .collect();
    let chars: Vec<char> = row.chars().collect();
    let at = point.column.0.min(chars.len().saturating_sub(1));
    if chars.get(at).is_none_or(|c| c.is_whitespace()) {
        return None;
    }
    let start = (0..=at)
        .rev()
        .take_while(|&i| !chars[i].is_whitespace())
        .last()
        .unwrap_or(at);
    let end = (at..chars.len())
        .take_while(|&i| !chars[i].is_whitespace())
        .last()
        .unwrap_or(at);
    Some(chars[start..=end].iter().collect())
}

/// Paints the screen: each row as one run of text, with its cells'
/// colours, the selection, and the cursor.
fn draw(ui: &egui::Ui, t: &Terminal, rect: Rect, cell: egui::Vec2, focused: bool) {
    let painter = ui.painter_at(rect.expand(6.0));
    let term = t.lock();
    let content = term.renderable_content();
    let colours = content.colors;
    let offset = content.display_offset as i32;
    let selection = content.selection;
    let cursor = content.cursor;
    let mut rows: Vec<Vec<(char, Color32, Color32, Flags)>> =
        vec![Vec::new(); term.grid().screen_lines()];
    for indexed in content.display_iter {
        let row = indexed.point.line.0 + offset;
        let Ok(row) = usize::try_from(row) else {
            continue;
        };
        let Some(line) = rows.get_mut(row) else {
            continue;
        };
        let c = indexed.cell;
        let bold = c.flags.contains(Flags::BOLD);
        let (mut fg, mut bg) = (colour(c.fg, colours, bold), colour(c.bg, colours, false));
        if c.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
            if fg == Color32::TRANSPARENT {
                fg = theme::bg();
            }
            if bg == theme::text() || bg == Color32::TRANSPARENT {
                bg = theme::text();
            }
        }
        if c.flags.contains(Flags::DIM) {
            fg = fg.gamma_multiply(0.7);
        }
        if c.flags.contains(Flags::HIDDEN) {
            fg = Color32::TRANSPARENT;
        }
        if selection.is_some_and(|s| s.contains(indexed.point)) {
            bg = theme::bg_selected();
        }
        let ch = if c.flags.contains(Flags::WIDE_CHAR_SPACER) || c.c == '\0' {
            ' '
        } else {
            c.c
        };
        line.push((ch, fg, bg, c.flags));
    }
    for (row, cells) in rows.iter().enumerate() {
        let y = rect.top() + row as f32 * cell.y;
        // Backgrounds, in runs.
        let mut x = rect.left();
        for (_, _, bg, flags) in cells {
            let w = if flags.contains(Flags::WIDE_CHAR) {
                2.0
            } else {
                1.0
            } * cell.x;
            if *bg != Color32::TRANSPARENT && !flags.contains(Flags::WIDE_CHAR_SPACER) {
                painter.rect_filled(
                    Rect::from_min_size(Pos2::new(x, y), egui::vec2(w, cell.y)),
                    0,
                    *bg,
                );
            }
            if !flags.contains(Flags::WIDE_CHAR_SPACER) {
                x += w;
            }
        }
        // Text: one job per row, grouped by look.
        let mut job = LayoutJob::default();
        for (ch, fg, _, flags) in cells {
            if flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let format = TextFormat {
                font_id: theme::mono(SIZE),
                color: *fg,
                italics: flags.contains(Flags::ITALIC),
                underline: if flags.intersects(Flags::ALL_UNDERLINES) {
                    Stroke::new(1.0, *fg)
                } else {
                    Stroke::NONE
                },
                strikethrough: if flags.contains(Flags::STRIKEOUT) {
                    Stroke::new(1.0, *fg)
                } else {
                    Stroke::NONE
                },
                ..Default::default()
            };
            let mut buf = [0u8; 4];
            job.append(ch.encode_utf8(&mut buf), 0.0, format);
        }
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        painter.galley(Pos2::new(rect.left(), y), galley, theme::text());
    }
    // The cursor: solid while typing and focused, blinking after; an
    // outline without focus.
    if cursor.shape != CursorShape::Hidden && content.mode.contains(TermMode::SHOW_CURSOR) {
        let row = cursor.point.line.0 + offset;
        if row >= 0 && (row as usize) < rows.len() {
            let at = Rect::from_min_size(
                Pos2::new(
                    rect.left() + cursor.point.column.0 as f32 * cell.x,
                    rect.top() + row as f32 * cell.y,
                ),
                cell,
            );
            let now = ui.input(|i| i.time);
            let on = !focused || now - t.last_input < 0.6 || (now * 1.6) as i64 % 2 == 0;
            if focused {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(320));
            }
            let colour = theme::accent();
            match (focused, cursor.shape) {
                (false, _) | (_, CursorShape::HollowBlock) => {
                    painter.rect_stroke(at, 0, Stroke::new(1.0, colour), egui::StrokeKind::Inside);
                }
                (true, _) if !on => {}
                (true, CursorShape::Beam) => {
                    painter.rect_filled(
                        Rect::from_min_size(at.min, egui::vec2(2.0, cell.y)),
                        0,
                        colour,
                    );
                }
                (true, CursorShape::Underline) => {
                    painter.rect_filled(
                        Rect::from_min_size(
                            Pos2::new(at.left(), at.bottom() - 2.0),
                            egui::vec2(cell.x, 2.0),
                        ),
                        0,
                        colour,
                    );
                }
                (true, _) => {
                    painter.rect_filled(at, 0, colour.gamma_multiply(0.85));
                    // The character under it, readable.
                    if let Some(&(ch, _, _, _)) = rows[row as usize].get(cursor.point.column.0) {
                        painter.text(
                            at.min,
                            egui::Align2::LEFT_TOP,
                            ch,
                            theme::mono(SIZE),
                            theme::bg(),
                        );
                    }
                }
            }
        }
    }
    // How far back it's scrolled.
    if offset > 0 {
        painter.text(
            rect.right_top() + egui::vec2(-4.0, 2.0),
            egui::Align2::RIGHT_TOP,
            format!("↑ {offset} lines back"),
            theme::body(11.0),
            theme::muted(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_what_terminals_send_for_keys() {
        let none = Modifiers::NONE;
        let normal = TermMode::empty();
        assert_eq!(key_bytes(Key::Enter, none, normal).as_deref(), Some("\r"));
        assert_eq!(
            key_bytes(Key::ArrowUp, none, normal).as_deref(),
            Some("\x1b[A")
        );
        assert_eq!(
            key_bytes(Key::ArrowUp, none, TermMode::APP_CURSOR).as_deref(),
            Some("\x1bOA"),
            "full-screen programs ask for these"
        );
        assert_eq!(
            key_bytes(Key::ArrowRight, Modifiers::CTRL, normal).as_deref(),
            Some("\x1b[1;5C")
        );
        assert_eq!(
            key_bytes(Key::Delete, none, normal).as_deref(),
            Some("\x1b[3~")
        );
        assert_eq!(
            key_bytes(Key::D, Modifiers::CTRL, normal).as_deref(),
            Some("\x04")
        );
        assert_eq!(
            key_bytes(Key::B, Modifiers::ALT, normal).as_deref(),
            Some("\x1bb")
        );
        assert_eq!(key_bytes(Key::A, none, normal), None, "typed as text");
        assert_eq!(
            key_bytes(Key::C, Modifiers::CTRL, normal),
            None,
            "comes as Copy"
        );
    }

    #[test]
    fn has_256_colours() {
        assert_eq!(colour_of_index(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(colour_of_index(231), Color32::from_rgb(255, 255, 255));
        assert_eq!(colour_of_index(232), Color32::from_rgb(8, 8, 8));
    }
}
