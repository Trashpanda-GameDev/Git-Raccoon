//! Colours, fonts and widget styling.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

pub const BG: Color32 = Color32::from_rgb(0x1b, 0x1d, 0x23);
pub const BG_RAISED: Color32 = Color32::from_rgb(0x23, 0x26, 0x2e);
pub const BG_HOVER: Color32 = Color32::from_rgb(0x2a, 0x2e, 0x38);
pub const BG_SELECTED: Color32 = Color32::from_rgb(0x33, 0x39, 0x4a);
pub const BORDER: Color32 = Color32::from_rgb(0x31, 0x35, 0x3f);
pub const TEXT: Color32 = Color32::from_rgb(0xd7, 0xda, 0xe0);
pub const MUTED: Color32 = Color32::from_rgb(0x8a, 0x91, 0xa0);
pub const ACCENT: Color32 = Color32::from_rgb(0x2e, 0xce, 0x9d);
pub const ON_ACCENT: Color32 = Color32::from_rgb(0x10, 0x23, 0x1d);
pub const DANGER: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
pub const DANGER_BG: Color32 = Color32::from_rgb(0x3b, 0x24, 0x28);
pub const ADD: Color32 = Color32::from_rgb(0x4c, 0xc3, 0x8a);
pub const DEL: Color32 = Color32::from_rgb(0xf2, 0x6d, 0x6d);
pub const MODIFIED: Color32 = Color32::from_rgb(0xe5, 0xb9, 0x4b);
pub const RENAMED: Color32 = Color32::from_rgb(0x6e, 0xa8, 0xfe);
pub const TYPECHANGE: Color32 = Color32::from_rgb(0xb8, 0x8c, 0xff);
pub const HUNK: Color32 = Color32::from_rgb(0x8f, 0xb6, 0xf5);

pub const ADD_BG: Color32 = Color32::from_rgba_premultiplied(9, 23, 17, 31);
pub const ADD_BG_STRONG: Color32 = Color32::from_rgba_premultiplied(17, 43, 30, 56);
pub const DEL_BG: Color32 = Color32::from_rgba_premultiplied(29, 13, 13, 31);
pub const DEL_BG_STRONG: Color32 = Color32::from_rgba_premultiplied(53, 24, 24, 56);
pub const WORD_ADD: Color32 = Color32::from_rgba_premultiplied(29, 74, 52, 97);
pub const WORD_DEL: Color32 = Color32::from_rgba_premultiplied(97, 44, 44, 102);
pub const HUNK_BG: Color32 = Color32::from_rgba_premultiplied(9, 13, 20, 20);

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

    // One dark look, whatever the system theme.
    ctx.set_theme(egui::Theme::Dark);
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
        *v = egui::Visuals::dark();
        v.panel_fill = BG;
        v.window_fill = BG_RAISED;
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.extreme_bg_color = BG;
        v.faint_bg_color = BG_RAISED;
        v.override_text_color = Some(TEXT);
        v.hyperlink_color = ACCENT;
        v.selection.bg_fill = BG_SELECTED;
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.text_cursor.stroke = Stroke::new(2.0, ACCENT);
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
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.noninteractive.bg_fill = BG_RAISED;
        v.widgets.inactive.weak_bg_fill = BG_RAISED;
        v.widgets.inactive.bg_fill = BG_RAISED;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        // A visible edge, so checkboxes and buttons stand out from raised panels.
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.hovered.weak_bg_fill = BG_HOVER;
        v.widgets.hovered.bg_fill = BG_HOVER;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.active.weak_bg_fill = BG_SELECTED;
        v.widgets.active.bg_fill = BG_SELECTED;
        v.widgets.open.weak_bg_fill = BG_HOVER;
    });
}
