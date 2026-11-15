//! Colours, fonts and widget styling.

use std::sync::atomic::{AtomicU8, Ordering};

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};
use two_face::theme::EmbeddedThemeName;

/// The colours of one look.
struct Palette {
    /// Built on egui's light visuals (otherwise dark).
    light: bool,
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
    /// The graph's lane colours.
    lanes: [Color32; 10],
    /// The syntax colours for diffs.
    syntax: EmbeddedThemeName,
}

const DARK: Palette = Palette {
    light: false,
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
    lanes: [
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
    ],
    syntax: EmbeddedThemeName::TwoDark,
};

const LIGHT: Palette = Palette {
    light: true,
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
    lanes: [
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
    ],
    syntax: EmbeddedThemeName::OneHalfLight,
};

const RACCOON: Palette = Palette {
    light: false,
    bg: Color32::from_rgb(0x22, 0x21, 0x20),
    bg_raised: Color32::from_rgb(0x2c, 0x2a, 0x28),
    bg_hover: Color32::from_rgb(0x36, 0x33, 0x30),
    bg_selected: Color32::from_rgb(0x45, 0x3d, 0x37),
    border: Color32::from_rgb(0x3d, 0x39, 0x35),
    text: Color32::from_rgb(0xf3, 0xe9, 0xdc),
    muted: Color32::from_rgb(0xa3, 0x9a, 0x90),
    accent: Color32::from_rgb(0xd0, 0x5f, 0x3f),
    on_accent: Color32::from_rgb(0xfb, 0xf3, 0xe8),
    danger: Color32::from_rgb(0xff, 0x7a, 0x6b),
    danger_bg: Color32::from_rgb(0x44, 0x26, 0x22),
    add: Color32::from_rgb(0x8f, 0xc9, 0x7a),
    del: Color32::from_rgb(0xf0, 0x7a, 0x6a),
    modified: Color32::from_rgb(0xe8, 0xb9, 0x5a),
    renamed: Color32::from_rgb(0x8a, 0xb4, 0xe8),
    typechange: Color32::from_rgb(0xc0, 0x9a, 0xe8),
    hunk: Color32::from_rgb(0xb5, 0xa9, 0x9c),
    add_bg: Color32::from_rgba_premultiplied(17, 24, 15, 31),
    add_bg_strong: Color32::from_rgba_premultiplied(31, 44, 27, 56),
    del_bg: Color32::from_rgba_premultiplied(29, 15, 13, 31),
    del_bg_strong: Color32::from_rgba_premultiplied(53, 27, 23, 56),
    word_add: Color32::from_rgba_premultiplied(50, 70, 43, 89),
    word_del: Color32::from_rgba_premultiplied(84, 43, 37, 89),
    hunk_bg: Color32::from_rgba_premultiplied(11, 10, 9, 15),
    lanes: [
        Color32::from_rgb(0xc4, 0x55, 0x3a),
        Color32::from_rgb(0xf5, 0xea, 0xdb),
        Color32::from_rgb(0x9a, 0x9a, 0x9a),
        Color32::from_rgb(0xe0, 0xa0, 0x40),
        Color32::from_rgb(0x5f, 0xb3, 0xa1),
        Color32::from_rgb(0x8a, 0xa0, 0xc0),
        Color32::from_rgb(0xd7, 0x7a, 0x8a),
        Color32::from_rgb(0xa8, 0xb0, 0x6a),
        Color32::from_rgb(0xa5, 0x7f, 0xc0),
        Color32::from_rgb(0xd9, 0xc3, 0xa0),
    ],
    syntax: EmbeddedThemeName::Base16EightiesDark,
};

const PASTEL: Palette = Palette {
    light: true,
    bg: Color32::from_rgb(0xfb, 0xf8, 0xfc),
    bg_raised: Color32::from_rgb(0xff, 0xff, 0xff),
    bg_hover: Color32::from_rgb(0xf3, 0xee, 0xf7),
    bg_selected: Color32::from_rgb(0xe8, 0xde, 0xf5),
    border: Color32::from_rgb(0xe4, 0xdc, 0xea),
    text: Color32::from_rgb(0x3b, 0x35, 0x46),
    muted: Color32::from_rgb(0x85, 0x7c, 0x92),
    accent: Color32::from_rgb(0x9a, 0x7a, 0xd6),
    on_accent: Color32::from_rgb(0xff, 0xff, 0xff),
    danger: Color32::from_rgb(0xd0, 0x60, 0x7a),
    danger_bg: Color32::from_rgb(0xfb, 0xe6, 0xec),
    add: Color32::from_rgb(0x3f, 0x9e, 0x74),
    del: Color32::from_rgb(0xcf, 0x5c, 0x7a),
    modified: Color32::from_rgb(0xb8, 0x86, 0x2d),
    renamed: Color32::from_rgb(0x5b, 0x8f, 0xd6),
    typechange: Color32::from_rgb(0x9a, 0x6b, 0xd6),
    hunk: Color32::from_rgb(0x7f, 0x6f, 0xb0),
    add_bg: Color32::from_rgba_premultiplied(9, 24, 17, 38),
    add_bg_strong: Color32::from_rgba_premultiplied(19, 47, 35, 76),
    del_bg: Color32::from_rgba_premultiplied(31, 14, 18, 38),
    del_bg_strong: Color32::from_rgba_premultiplied(62, 28, 37, 76),
    word_add: Color32::from_rgba_premultiplied(22, 55, 41, 89),
    word_del: Color32::from_rgba_premultiplied(72, 32, 43, 89),
    hunk_bg: Color32::from_rgba_premultiplied(10, 9, 14, 20),
    lanes: [
        Color32::from_rgb(0xe8, 0x8a, 0xa3),
        Color32::from_rgb(0x7f, 0xb2, 0xec),
        Color32::from_rgb(0x6f, 0xc7, 0x9a),
        Color32::from_rgb(0xf0, 0xa8, 0x68),
        Color32::from_rgb(0xa9, 0x8b, 0xe0),
        Color32::from_rgb(0xd9, 0xb8, 0x4f),
        Color32::from_rgb(0x5c, 0xc0, 0xc0),
        Color32::from_rgb(0xd9, 0x89, 0xcf),
        Color32::from_rgb(0x9c, 0xc4, 0x6a),
        Color32::from_rgb(0xe8, 0x90, 0x7e),
    ],
    syntax: EmbeddedThemeName::CatppuccinLatte,
};

const ABYSS: Palette = Palette {
    light: false,
    bg: Color32::from_rgb(0x15, 0x1a, 0x26),
    bg_raised: Color32::from_rgb(0x1c, 0x23, 0x33),
    bg_hover: Color32::from_rgb(0x24, 0x2c, 0x3f),
    bg_selected: Color32::from_rgb(0x2c, 0x38, 0x52),
    border: Color32::from_rgb(0x2a, 0x33, 0x47),
    text: Color32::from_rgb(0xdc, 0xe4, 0xf2),
    muted: Color32::from_rgb(0x86, 0x94, 0xad),
    accent: Color32::from_rgb(0x2e, 0xc4, 0xc4),
    on_accent: Color32::from_rgb(0x08, 0x1f, 0x24),
    danger: Color32::from_rgb(0xff, 0x6b, 0x81),
    danger_bg: Color32::from_rgb(0x3a, 0x20, 0x30),
    add: Color32::from_rgb(0x4f, 0xd1, 0xa1),
    del: Color32::from_rgb(0xff, 0x7a, 0x8a),
    modified: Color32::from_rgb(0xf0, 0xc0, 0x60),
    renamed: Color32::from_rgb(0x79, 0xa8, 0xff),
    typechange: Color32::from_rgb(0xb4, 0x8c, 0xff),
    hunk: Color32::from_rgb(0x8f, 0xb0, 0xf0),
    add_bg: Color32::from_rgba_premultiplied(9, 25, 19, 31),
    add_bg_strong: Color32::from_rgba_premultiplied(17, 46, 35, 56),
    del_bg: Color32::from_rgba_premultiplied(31, 15, 17, 31),
    del_bg_strong: Color32::from_rgba_premultiplied(56, 27, 30, 56),
    word_add: Color32::from_rgba_premultiplied(28, 73, 56, 89),
    word_del: Color32::from_rgba_premultiplied(89, 43, 48, 89),
    hunk_bg: Color32::from_rgba_premultiplied(9, 11, 14, 15),
    lanes: [
        Color32::from_rgb(0x2e, 0xc4, 0xc4),
        Color32::from_rgb(0x4f, 0x8c, 0xff),
        Color32::from_rgb(0xa9, 0x70, 0xff),
        Color32::from_rgb(0xff, 0x5f, 0xa2),
        Color32::from_rgb(0xff, 0x8a, 0x3d),
        Color32::from_rgb(0xff, 0xd2, 0x3f),
        Color32::from_rgb(0x7e, 0xe0, 0x81),
        Color32::from_rgb(0x3f, 0xb8, 0xff),
        Color32::from_rgb(0xe0, 0x5f, 0xf0),
        Color32::from_rgb(0x5f, 0xe0, 0xc0),
    ],
    syntax: EmbeddedThemeName::Base16OceanDark,
};

const FOREST: Palette = Palette {
    light: false,
    bg: Color32::from_rgb(0x1a, 0x20, 0x1b),
    bg_raised: Color32::from_rgb(0x22, 0x2a, 0x23),
    bg_hover: Color32::from_rgb(0x2a, 0x33, 0x2b),
    bg_selected: Color32::from_rgb(0x33, 0x42, 0x35),
    border: Color32::from_rgb(0x2f, 0x3a, 0x30),
    text: Color32::from_rgb(0xe2, 0xe8, 0xd8),
    muted: Color32::from_rgb(0x95, 0xa3, 0x8f),
    accent: Color32::from_rgb(0xe0, 0xa3, 0x3a),
    on_accent: Color32::from_rgb(0x23, 0x19, 0x0a),
    danger: Color32::from_rgb(0xf0, 0x7a, 0x6a),
    danger_bg: Color32::from_rgb(0x3c, 0x26, 0x22),
    add: Color32::from_rgb(0x8f, 0xd0, 0x7a),
    del: Color32::from_rgb(0xf0, 0x80, 0x70),
    modified: Color32::from_rgb(0xe8, 0xc0, 0x60),
    renamed: Color32::from_rgb(0x7f, 0xb8, 0xd8),
    typechange: Color32::from_rgb(0xc0, 0x9a, 0xd8),
    hunk: Color32::from_rgb(0xa8, 0xc0, 0x9a),
    add_bg: Color32::from_rgba_premultiplied(17, 25, 15, 31),
    add_bg_strong: Color32::from_rgba_premultiplied(31, 46, 27, 56),
    del_bg: Color32::from_rgba_premultiplied(29, 15, 13, 31),
    del_bg_strong: Color32::from_rgba_premultiplied(53, 28, 25, 56),
    word_add: Color32::from_rgba_premultiplied(50, 73, 43, 89),
    word_del: Color32::from_rgba_premultiplied(84, 45, 39, 89),
    hunk_bg: Color32::from_rgba_premultiplied(10, 12, 9, 15),
    lanes: [
        Color32::from_rgb(0x7f, 0xbf, 0x6a),
        Color32::from_rgb(0xe0, 0xa3, 0x3a),
        Color32::from_rgb(0x5f, 0xb0, 0xa0),
        Color32::from_rgb(0xc0, 0xd0, 0x6a),
        Color32::from_rgb(0xd0, 0x80, 0x6a),
        Color32::from_rgb(0x8a, 0xa0, 0xd8),
        Color32::from_rgb(0xb8, 0x8a, 0xd0),
        Color32::from_rgb(0xe8, 0xd0, 0x8a),
        Color32::from_rgb(0x6a, 0xc0, 0xd8),
        Color32::from_rgb(0xa0, 0xc0, 0x90),
    ],
    syntax: EmbeddedThemeName::GruvboxDark,
};

const PAPER: Palette = Palette {
    light: true,
    bg: Color32::from_rgb(0xf6, 0xf1, 0xe7),
    bg_raised: Color32::from_rgb(0xfd, 0xfa, 0xf3),
    bg_hover: Color32::from_rgb(0xef, 0xe8, 0xda),
    bg_selected: Color32::from_rgb(0xe6, 0xdc, 0xc6),
    border: Color32::from_rgb(0xe0, 0xd6, 0xc2),
    text: Color32::from_rgb(0x3a, 0x33, 0x28),
    muted: Color32::from_rgb(0x7a, 0x6f, 0x5e),
    accent: Color32::from_rgb(0xb5, 0x56, 0x2c),
    on_accent: Color32::from_rgb(0xff, 0xfa, 0xf0),
    danger: Color32::from_rgb(0xb3, 0x26, 0x1e),
    danger_bg: Color32::from_rgb(0xf8, 0xe0, 0xd8),
    add: Color32::from_rgb(0x4a, 0x7a, 0x2a),
    del: Color32::from_rgb(0xb3, 0x3a, 0x2a),
    modified: Color32::from_rgb(0x8a, 0x62, 0x00),
    renamed: Color32::from_rgb(0x2e, 0x5f, 0x9a),
    typechange: Color32::from_rgb(0x7a, 0x4a, 0x9a),
    hunk: Color32::from_rgb(0x5a, 0x4a, 0x7a),
    add_bg: Color32::from_rgba_premultiplied(11, 18, 6, 38),
    add_bg_strong: Color32::from_rgba_premultiplied(22, 37, 13, 76),
    del_bg: Color32::from_rgba_premultiplied(27, 9, 6, 38),
    del_bg_strong: Color32::from_rgba_premultiplied(54, 17, 13, 76),
    word_add: Color32::from_rgba_premultiplied(26, 43, 15, 89),
    word_del: Color32::from_rgba_premultiplied(63, 20, 15, 89),
    hunk_bg: Color32::from_rgba_premultiplied(7, 6, 10, 20),
    lanes: [
        Color32::from_rgb(0xb5, 0x56, 0x2c),
        Color32::from_rgb(0x2e, 0x6f, 0x8a),
        Color32::from_rgb(0x6a, 0x8a, 0x2a),
        Color32::from_rgb(0x9a, 0x4a, 0x7a),
        Color32::from_rgb(0xc0, 0x8a, 0x20),
        Color32::from_rgb(0x3a, 0x7a, 0x6a),
        Color32::from_rgb(0x8a, 0x5a, 0x3a),
        Color32::from_rgb(0x5a, 0x5a, 0xa0),
        Color32::from_rgb(0xa0, 0x3a, 0x3a),
        Color32::from_rgb(0x4a, 0x8a, 0x4a),
    ],
    syntax: EmbeddedThemeName::GruvboxLight,
};

const HIGHCONTRAST: Palette = Palette {
    light: false,
    bg: Color32::from_rgb(0x00, 0x00, 0x00),
    bg_raised: Color32::from_rgb(0x0d, 0x0d, 0x0d),
    bg_hover: Color32::from_rgb(0x26, 0x26, 0x26),
    bg_selected: Color32::from_rgb(0x0a, 0x3a, 0x6b),
    border: Color32::from_rgb(0x7a, 0x7a, 0x7a),
    text: Color32::from_rgb(0xff, 0xff, 0xff),
    muted: Color32::from_rgb(0xd0, 0xd0, 0xd0),
    accent: Color32::from_rgb(0xff, 0xd4, 0x00),
    on_accent: Color32::from_rgb(0x00, 0x00, 0x00),
    danger: Color32::from_rgb(0xff, 0x5c, 0x5c),
    danger_bg: Color32::from_rgb(0x4a, 0x00, 0x00),
    add: Color32::from_rgb(0x3d, 0xff, 0x7a),
    del: Color32::from_rgb(0xff, 0x5c, 0x5c),
    modified: Color32::from_rgb(0xff, 0xd4, 0x00),
    renamed: Color32::from_rgb(0x5c, 0xc8, 0xff),
    typechange: Color32::from_rgb(0xe0, 0x8c, 0xff),
    hunk: Color32::from_rgb(0x9f, 0xd0, 0xff),
    add_bg: Color32::from_rgba_premultiplied(15, 64, 30, 64),
    add_bg_strong: Color32::from_rgba_premultiplied(24, 102, 49, 102),
    del_bg: Color32::from_rgba_premultiplied(64, 23, 23, 64),
    del_bg_strong: Color32::from_rgba_premultiplied(102, 37, 37, 102),
    word_add: Color32::from_rgba_premultiplied(34, 140, 67, 140),
    word_del: Color32::from_rgba_premultiplied(140, 51, 51, 140),
    hunk_bg: Color32::from_rgba_premultiplied(19, 25, 31, 31),
    lanes: [
        Color32::from_rgb(0x00, 0xe5, 0xff),
        Color32::from_rgb(0xff, 0xd4, 0x00),
        Color32::from_rgb(0xff, 0x5c, 0xf0),
        Color32::from_rgb(0x3d, 0xff, 0x7a),
        Color32::from_rgb(0xff, 0x8c, 0x1a),
        Color32::from_rgb(0x5c, 0x9c, 0xff),
        Color32::from_rgb(0xff, 0x5c, 0x5c),
        Color32::from_rgb(0xb8, 0xff, 0x3d),
        Color32::from_rgb(0xc8, 0x8c, 0xff),
        Color32::from_rgb(0xff, 0xff, 0xff),
    ],
    syntax: EmbeddedThemeName::MonokaiExtendedBright,
};

/// The looks to choose from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Dark,
    Light,
    Raccoon,
    Pastel,
    Abyss,
    Forest,
    Paper,
    HighContrast,
}

impl Look {
    pub const ALL: [Look; 8] = [
        Look::Dark,
        Look::Light,
        Look::Raccoon,
        Look::Pastel,
        Look::Abyss,
        Look::Forest,
        Look::Paper,
        Look::HighContrast,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Look::Dark => "Dark",
            Look::Light => "Light",
            Look::Raccoon => "Raccoon",
            Look::Pastel => "Pastel",
            Look::Abyss => "Abyss",
            Look::Forest => "Forest",
            Look::Paper => "Paper",
            Look::HighContrast => "High contrast",
        }
    }

    fn palette(self) -> &'static Palette {
        match self {
            Look::Dark => &DARK,
            Look::Light => &LIGHT,
            Look::Raccoon => &RACCOON,
            Look::Pastel => &PASTEL,
            Look::Abyss => &ABYSS,
            Look::Forest => &FOREST,
            Look::Paper => &PAPER,
            Look::HighContrast => &HIGHCONTRAST,
        }
    }

    fn index(self) -> u8 {
        match self {
            Look::Dark => 0,
            Look::Light => 1,
            Look::Raccoon => 2,
            Look::Pastel => 3,
            Look::Abyss => 4,
            Look::Forest => 5,
            Look::Paper => 6,
            Look::HighContrast => 7,
        }
    }

    /// Whether it's a light look.
    pub fn is_light(self) -> bool {
        self.palette().light
    }

    /// Colours to preview it with: background, raised, text, accent and
    /// four lane colours.
    pub fn preview(self) -> (Color32, Color32, Color32, Color32, [Color32; 4]) {
        let p = self.palette();
        (
            p.bg,
            p.bg_raised,
            p.text,
            p.accent,
            [p.lanes[0], p.lanes[1], p.lanes[2], p.lanes[3]],
        )
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

fn palette() -> &'static Palette {
    let i = CURRENT.load(Ordering::Relaxed);
    Look::ALL
        .into_iter()
        .find(|l| l.index() == i)
        .unwrap_or(Look::Dark)
        .palette()
}

/// The syntax colours for diffs in the current look.
pub fn syntax() -> EmbeddedThemeName {
    palette().syntax
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

pub fn lane_color(lane: usize) -> Color32 {
    let lanes = &palette().lanes;
    lanes[lane % lanes.len()]
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
    set(ctx, Look::Dark);
}

/// Switches to a look.
pub fn set(ctx: &egui::Context, look: Look) {
    CURRENT.store(look.index(), Ordering::Relaxed);
    let light = look.is_light();
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
