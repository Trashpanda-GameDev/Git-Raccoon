//! Colours, fonts and widget styling.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

/// The colours of one theme.
struct Palette {
    bg: Color32,
    bg_raised: Color32,
    bg_hover: Color32,
    bg_selected: Color32,
    border: Color32,
    text: Color32,
    muted: Color32,
    accent: Color32,
    on_accent: Color32,
    danger: Color32,
    danger_bg: Color32,
    add: Color32,
    del: Color32,
    modified: Color32,
    renamed: Color32,
    typechange: Color32,
    hunk: Color32,
    add_bg: Color32,
    add_bg_strong: Color32,
    del_bg: Color32,
    del_bg_strong: Color32,
    word_add: Color32,
    word_del: Color32,
    hunk_bg: Color32,
}

const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x1b, 0x1d, 0x23),
    bg_raised: Color32::from_rgb(0x23, 0x26, 0x2e),
    bg_hover: Color32::from_rgb(0x2a, 0x2e, 0x38),
    bg_selected: Color32::from_rgb(0x33, 0x39, 0x4a),
    border: Color32::from_rgb(0x31, 0x35, 0x3f),
    text: Color32::from_rgb(0xd7, 0xda, 0xe0),
    muted: Color32::from_rgb(0x8a, 0x91, 0xa0),
    accent: Color32::from_rgb(0x2e, 0xce, 0x9d),
    on_accent: Color32::from_rgb(0x10, 0x23, 0x1d),
    danger: Color32::from_rgb(0xff, 0x6b, 0x6b),
    danger_bg: Color32::from_rgb(0x3b, 0x24, 0x28),
    add: Color32::from_rgb(0x4c, 0xc3, 0x8a),
    del: Color32::from_rgb(0xf2, 0x6d, 0x6d),
    modified: Color32::from_rgb(0xe5, 0xb9, 0x4b),
    renamed: Color32::from_rgb(0x6e, 0xa8, 0xfe),
    typechange: Color32::from_rgb(0xb8, 0x8c, 0xff),
    hunk: Color32::from_rgb(0x8f, 0xb6, 0xf5),
    add_bg: Color32::from_rgba_premultiplied(9, 23, 17, 31),
    add_bg_strong: Color32::from_rgba_premultiplied(17, 43, 30, 56),
    del_bg: Color32::from_rgba_premultiplied(29, 13, 13, 31),
    del_bg_strong: Color32::from_rgba_premultiplied(53, 24, 24, 56),
    word_add: Color32::from_rgba_premultiplied(29, 74, 52, 97),
    word_del: Color32::from_rgba_premultiplied(97, 44, 44, 102),
    hunk_bg: Color32::from_rgba_premultiplied(9, 13, 20, 20),
};

const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xf6, 0xf7, 0xf9),
    bg_raised: Color32::from_rgb(0xff, 0xff, 0xff),
    bg_hover: Color32::from_rgb(0xec, 0xee, 0xf2),
    bg_selected: Color32::from_rgb(0xdc, 0xe6, 0xf6),
    border: Color32::from_rgb(0xd5, 0xd9, 0xe0),
    text: Color32::from_rgb(0x1f, 0x23, 0x28),
    muted: Color32::from_rgb(0x5c, 0x64, 0x70),
    accent: Color32::from_rgb(0x0b, 0x8a, 0x64),
    on_accent: Color32::from_rgb(0xff, 0xff, 0xff),
    danger: Color32::from_rgb(0xc9, 0x25, 0x2c),
    danger_bg: Color32::from_rgb(0xfd, 0xe8, 0xe8),
    add: Color32::from_rgb(0x1a, 0x7f, 0x37),
    del: Color32::from_rgb(0xcf, 0x22, 0x2e),
    modified: Color32::from_rgb(0x9a, 0x67, 0x00),
    renamed: Color32::from_rgb(0x09, 0x69, 0xda),
    typechange: Color32::from_rgb(0x82, 0x50, 0xdf),
    hunk: Color32::from_rgb(0x05, 0x50, 0xae),
    add_bg: Color32::from_rgba_premultiplied(7, 24, 10, 38),
    add_bg_strong: Color32::from_rgba_premultiplied(14, 48, 20, 77),
    del_bg: Color32::from_rgba_premultiplied(25, 4, 6, 31),
    del_bg_strong: Color32::from_rgba_premultiplied(52, 9, 12, 64),
    word_add: Color32::from_rgba_premultiplied(16, 56, 23, 89),
    word_del: Color32::from_rgba_premultiplied(62, 10, 14, 77),
    hunk_bg: Color32::from_rgba_premultiplied(1, 8, 17, 20),
};

static LIGHT_ON: AtomicBool = AtomicBool::new(false);

fn palette() -> &'static Palette {
    if LIGHT_ON.load(Ordering::Relaxed) {
        &LIGHT
    } else {
        &DARK
    }
}

/// Whether the light theme is in use.
pub fn is_light() -> bool {
    LIGHT_ON.load(Ordering::Relaxed)
}

/// The window background.
pub fn bg() -> Color32 {
    palette().bg
}

/// Panels and cards.
pub fn bg_raised() -> Color32 {
    palette().bg_raised
}

/// Under the pointer.
pub fn bg_hover() -> Color32 {
    palette().bg_hover
}

/// The selection.
pub fn bg_selected() -> Color32 {
    palette().bg_selected
}

pub fn border() -> Color32 {
    palette().border
}

pub fn text() -> Color32 {
    palette().text
}

pub fn muted() -> Color32 {
    palette().muted
}

/// The brand colour: primary buttons, focus.
pub fn accent() -> Color32 {
    palette().accent
}

/// Text on the accent colour.
pub fn on_accent() -> Color32 {
    palette().on_accent
}

pub fn danger() -> Color32 {
    palette().danger
}

pub fn danger_bg() -> Color32 {
    palette().danger_bg
}

pub fn add() -> Color32 {
    palette().add
}

pub fn del() -> Color32 {
    palette().del
}

pub fn modified() -> Color32 {
    palette().modified
}

pub fn renamed() -> Color32 {
    palette().renamed
}

pub fn typechange() -> Color32 {
    palette().typechange
}

pub fn hunk() -> Color32 {
    palette().hunk
}

pub fn add_bg() -> Color32 {
    palette().add_bg
}

pub fn add_bg_strong() -> Color32 {
    palette().add_bg_strong
}

pub fn del_bg() -> Color32 {
    palette().del_bg
}

pub fn del_bg_strong() -> Color32 {
    palette().del_bg_strong
}

pub fn word_add() -> Color32 {
    palette().word_add
}

pub fn word_del() -> Color32 {
    palette().word_del
}

pub fn hunk_bg() -> Color32 {
    palette().hunk_bg
}

const LANE_COLORS: [Color32; 10] = [
    Color32::from_rgb(0x15, 0xa0, 0xbf),
    Color32::from_rgb(0x06, 0x69, 0xf7),
    Color32::from_rgb(0x8e, 0x00, 0xc2),
    Color32::from_rgb(0xc5, 0x17, 0xb6),
    Color32::from_rgb(0xd9, 0x01, 0x71),
    Color32::from_rgb(0xcd, 0x01, 0x01),
    Color32::from_rgb(0xf2, 0x5d, 0x2e),
    Color32::from_rgb(0xf2, 0xca, 0x33),
    Color32::from_rgb(0x7b, 0xd9, 0x38),
    Color32::from_rgb(0x2e, 0xce, 0x9d),
];

pub fn lane_color(lane: usize) -> Color32 {
    LANE_COLORS[lane % LANE_COLORS.len()]
}

pub const FONT_SIZE: f32 = 13.0;
pub const MONO_SIZE: f32 = 12.5;

pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Installs fonts (with Phosphor icons) and the dark theme.
pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
    set_light(ctx, false);
}

/// Switches between the light and dark themes.
pub fn set_light(ctx: &egui::Context, light: bool) {
    LIGHT_ON.store(light, Ordering::Relaxed);
    ctx.set_theme(if light {
        egui::Theme::Light
    } else {
        egui::Theme::Dark
    });
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, body(11.0)),
            (TextStyle::Body, body(FONT_SIZE)),
            (TextStyle::Button, body(FONT_SIZE)),
            (TextStyle::Heading, body(17.0)),
            (TextStyle::Monospace, mono(MONO_SIZE)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.interact_size.y = 24.0;
        style.interaction.selectable_labels = false;

        let v = &mut style.visuals;
        *v = if light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        v.panel_fill = bg();
        v.window_fill = bg_raised();
        v.window_stroke = Stroke::new(1.0, border());
        v.extreme_bg_color = bg();
        v.faint_bg_color = bg_raised();
        v.override_text_color = Some(text());
        v.hyperlink_color = accent();
        v.selection.bg_fill = bg_selected();
        v.selection.stroke = Stroke::new(1.0, accent());
        v.text_cursor.stroke = Stroke::new(2.0, accent());
        let radius = CornerRadius::same(4);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = radius;
        }
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border());
        v.widgets.noninteractive.bg_fill = bg_raised();
        v.widgets.inactive.weak_bg_fill = bg_raised();
        v.widgets.inactive.bg_fill = bg_raised();
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, text());
        // A visible edge, so checkboxes and buttons stand out from raised panels.
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, border());
        v.widgets.hovered.weak_bg_fill = bg_hover();
        v.widgets.hovered.bg_fill = bg_hover();
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, border());
        v.widgets.active.weak_bg_fill = bg_selected();
        v.widgets.active.bg_fill = bg_selected();
        v.widgets.open.weak_bg_fill = bg_hover();
    });
}
