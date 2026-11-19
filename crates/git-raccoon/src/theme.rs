//! Colours, fonts and widget styling.

use std::sync::{PoisonError, RwLock, RwLockReadGuard};

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};
use two_face::theme::EmbeddedThemeName;

/// The colours of one look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
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

/// Raccoon in the light: cream and rust.
const RACCOON_LIGHT: Palette = Palette {
    light: true,
    bg: Color32::from_rgb(0xf6, 0xf0, 0xe7),
    bg_raised: Color32::from_rgb(0xff, 0xfa, 0xf2),
    bg_hover: Color32::from_rgb(0xee, 0xe5, 0xd8),
    bg_selected: Color32::from_rgb(0xe7, 0xd5, 0xc4),
    border: Color32::from_rgb(0xe0, 0xd3, 0xc2),
    text: Color32::from_rgb(0x2c, 0x27, 0x23),
    muted: Color32::from_rgb(0x7a, 0x6d, 0x61),
    accent: Color32::from_rgb(0xc0, 0x53, 0x2f),
    on_accent: Color32::from_rgb(0xff, 0xfa, 0xf2),
    danger: Color32::from_rgb(0xb8, 0x38, 0x2a),
    danger_bg: Color32::from_rgb(0xf8, 0xe0, 0xd8),
    add: Color32::from_rgb(0x3f, 0x7f, 0x2a),
    del: Color32::from_rgb(0xb8, 0x38, 0x2a),
    modified: Color32::from_rgb(0x8a, 0x62, 0x00),
    renamed: Color32::from_rgb(0x2e, 0x5f, 0x9a),
    typechange: Color32::from_rgb(0x7a, 0x4a, 0x9a),
    hunk: Color32::from_rgb(0x6a, 0x5a, 0x4a),
    add_bg: Color32::from_rgba_premultiplied(7, 24, 10, 38),
    add_bg_strong: Color32::from_rgba_premultiplied(14, 48, 20, 77),
    del_bg: Color32::from_rgba_premultiplied(25, 4, 6, 31),
    del_bg_strong: Color32::from_rgba_premultiplied(52, 9, 12, 64),
    word_add: Color32::from_rgba_premultiplied(16, 56, 23, 89),
    word_del: Color32::from_rgba_premultiplied(62, 10, 14, 77),
    hunk_bg: Color32::from_rgba_premultiplied(1, 8, 17, 20),
    lanes: [
        Color32::from_rgb(0xc0, 0x53, 0x2f),
        Color32::from_rgb(0x4a, 0x44, 0x40),
        Color32::from_rgb(0x8a, 0x85, 0x80),
        Color32::from_rgb(0xc0, 0x8a, 0x20),
        Color32::from_rgb(0x3a, 0x8a, 0x7a),
        Color32::from_rgb(0x5a, 0x70, 0x90),
        Color32::from_rgb(0xb0, 0x56, 0x6a),
        Color32::from_rgb(0x7a, 0x8a, 0x3a),
        Color32::from_rgb(0x7a, 0x5a, 0x9a),
        Color32::from_rgb(0xa0, 0x82, 0x5a),
    ],
    syntax: EmbeddedThemeName::GruvboxLight,
};

/// Pastel at night.
const PASTEL_DARK: Palette = Palette {
    light: false,
    bg: Color32::from_rgb(0x22, 0x1f, 0x2b),
    bg_raised: Color32::from_rgb(0x2a, 0x26, 0x35),
    bg_hover: Color32::from_rgb(0x33, 0x2e, 0x40),
    bg_selected: Color32::from_rgb(0x3e, 0x36, 0x50),
    border: Color32::from_rgb(0x3a, 0x34, 0x47),
    text: Color32::from_rgb(0xec, 0xe6, 0xf5),
    muted: Color32::from_rgb(0xa4, 0x9c, 0xb4),
    accent: Color32::from_rgb(0xb9, 0xa0, 0xf0),
    on_accent: Color32::from_rgb(0x1d, 0x18, 0x28),
    danger: Color32::from_rgb(0xf0, 0x8a, 0xa0),
    danger_bg: Color32::from_rgb(0x3d, 0x25, 0x30),
    add: Color32::from_rgb(0x7f, 0xd4, 0xa8),
    del: Color32::from_rgb(0xf0, 0x8a, 0xa0),
    modified: Color32::from_rgb(0xe8, 0xc0, 0x70),
    renamed: Color32::from_rgb(0x8a, 0xb8, 0xf0),
    typechange: Color32::from_rgb(0xc8, 0xa0, 0xf0),
    hunk: Color32::from_rgb(0xb0, 0xa0, 0xd8),
    add_bg: Color32::from_rgba_premultiplied(9, 23, 17, 31),
    add_bg_strong: Color32::from_rgba_premultiplied(17, 43, 30, 56),
    del_bg: Color32::from_rgba_premultiplied(29, 13, 13, 31),
    del_bg_strong: Color32::from_rgba_premultiplied(53, 24, 24, 56),
    word_add: Color32::from_rgba_premultiplied(29, 74, 52, 97),
    word_del: Color32::from_rgba_premultiplied(97, 44, 44, 102),
    hunk_bg: Color32::from_rgba_premultiplied(9, 13, 20, 20),
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
    syntax: EmbeddedThemeName::CatppuccinMocha,
};

/// Forest in daylight: sage and amber.
const FOREST_LIGHT: Palette = Palette {
    light: true,
    bg: Color32::from_rgb(0xf1, 0xf4, 0xec),
    bg_raised: Color32::from_rgb(0xfb, 0xfc, 0xf8),
    bg_hover: Color32::from_rgb(0xe6, 0xeb, 0xdf),
    bg_selected: Color32::from_rgb(0xd6, 0xe2, 0xcc),
    border: Color32::from_rgb(0xd2, 0xdb, 0xc8),
    text: Color32::from_rgb(0x1f, 0x2a, 0x1f),
    muted: Color32::from_rgb(0x5f, 0x6e, 0x5a),
    accent: Color32::from_rgb(0xa8, 0x6a, 0x00),
    on_accent: Color32::from_rgb(0xff, 0xfb, 0xf0),
    danger: Color32::from_rgb(0xb8, 0x40, 0x2e),
    danger_bg: Color32::from_rgb(0xf6, 0xe2, 0xdc),
    add: Color32::from_rgb(0x3a, 0x7a, 0x2a),
    del: Color32::from_rgb(0xb8, 0x40, 0x2e),
    modified: Color32::from_rgb(0x8a, 0x62, 0x00),
    renamed: Color32::from_rgb(0x2a, 0x6a, 0x8a),
    typechange: Color32::from_rgb(0x7a, 0x4a, 0x8a),
    hunk: Color32::from_rgb(0x5a, 0x70, 0x50),
    add_bg: Color32::from_rgba_premultiplied(7, 24, 10, 38),
    add_bg_strong: Color32::from_rgba_premultiplied(14, 48, 20, 77),
    del_bg: Color32::from_rgba_premultiplied(25, 4, 6, 31),
    del_bg_strong: Color32::from_rgba_premultiplied(52, 9, 12, 64),
    word_add: Color32::from_rgba_premultiplied(16, 56, 23, 89),
    word_del: Color32::from_rgba_premultiplied(62, 10, 14, 77),
    hunk_bg: Color32::from_rgba_premultiplied(1, 8, 17, 20),
    lanes: [
        Color32::from_rgb(0x4a, 0x8a, 0x3a),
        Color32::from_rgb(0xb0, 0x7a, 0x10),
        Color32::from_rgb(0x2f, 0x80, 0x70),
        Color32::from_rgb(0x7a, 0x8a, 0x20),
        Color32::from_rgb(0xb0, 0x5a, 0x40),
        Color32::from_rgb(0x4a, 0x62, 0xa8),
        Color32::from_rgb(0x8a, 0x5a, 0xa0),
        Color32::from_rgb(0xa0, 0x8a, 0x30),
        Color32::from_rgb(0x2a, 0x80, 0xa0),
        Color32::from_rgb(0x5a, 0x8a, 0x50),
    ],
    syntax: EmbeddedThemeName::SolarizedLight,
};

/// Paper by lamplight.
const PAPER_DARK: Palette = Palette {
    light: false,
    bg: Color32::from_rgb(0x26, 0x22, 0x1c),
    bg_raised: Color32::from_rgb(0x2e, 0x29, 0x22),
    bg_hover: Color32::from_rgb(0x38, 0x32, 0x2a),
    bg_selected: Color32::from_rgb(0x46, 0x3d, 0x30),
    border: Color32::from_rgb(0x3e, 0x37, 0x2d),
    text: Color32::from_rgb(0xee, 0xe5, 0xd3),
    muted: Color32::from_rgb(0xa6, 0x9a, 0x85),
    accent: Color32::from_rgb(0xe0, 0x8a, 0x5a),
    on_accent: Color32::from_rgb(0x2a, 0x1a, 0x0e),
    danger: Color32::from_rgb(0xf0, 0x7a, 0x68),
    danger_bg: Color32::from_rgb(0x45, 0x26, 0x1f),
    add: Color32::from_rgb(0x9f, 0xc0, 0x7a),
    del: Color32::from_rgb(0xf0, 0x7a, 0x68),
    modified: Color32::from_rgb(0xe0, 0xb8, 0x60),
    renamed: Color32::from_rgb(0x7f, 0xa8, 0xd8),
    typechange: Color32::from_rgb(0xc0, 0x9a, 0xd8),
    hunk: Color32::from_rgb(0xb0, 0xa0, 0x80),
    add_bg: Color32::from_rgba_premultiplied(9, 23, 17, 31),
    add_bg_strong: Color32::from_rgba_premultiplied(17, 43, 30, 56),
    del_bg: Color32::from_rgba_premultiplied(29, 13, 13, 31),
    del_bg_strong: Color32::from_rgba_premultiplied(53, 24, 24, 56),
    word_add: Color32::from_rgba_premultiplied(29, 74, 52, 97),
    word_del: Color32::from_rgba_premultiplied(97, 44, 44, 102),
    hunk_bg: Color32::from_rgba_premultiplied(9, 13, 20, 20),
    lanes: [
        Color32::from_rgb(0xe0, 0x8a, 0x5a),
        Color32::from_rgb(0x6a, 0xa8, 0xc8),
        Color32::from_rgb(0xa8, 0xc0, 0x6a),
        Color32::from_rgb(0xc8, 0x8a, 0xb0),
        Color32::from_rgb(0xe0, 0xb8, 0x50),
        Color32::from_rgb(0x6a, 0xc0, 0xa8),
        Color32::from_rgb(0xc0, 0x9a, 0x7a),
        Color32::from_rgb(0x9a, 0x9a, 0xe0),
        Color32::from_rgb(0xe0, 0x7a, 0x7a),
        Color32::from_rgb(0x8a, 0xc0, 0x8a),
    ],
    syntax: EmbeddedThemeName::GruvboxDark,
};

/// High contrast, black on white.
const HIGHCONTRAST_LIGHT: Palette = Palette {
    light: true,
    bg: Color32::from_rgb(0xff, 0xff, 0xff),
    bg_raised: Color32::from_rgb(0xf4, 0xf4, 0xf4),
    bg_hover: Color32::from_rgb(0xe0, 0xe0, 0xe0),
    bg_selected: Color32::from_rgb(0xb8, 0xd8, 0xff),
    border: Color32::from_rgb(0x5a, 0x5a, 0x5a),
    text: Color32::from_rgb(0x00, 0x00, 0x00),
    muted: Color32::from_rgb(0x2a, 0x2a, 0x2a),
    accent: Color32::from_rgb(0x00, 0x40, 0xc0),
    on_accent: Color32::from_rgb(0xff, 0xff, 0xff),
    danger: Color32::from_rgb(0xb0, 0x00, 0x00),
    danger_bg: Color32::from_rgb(0xff, 0xd8, 0xd8),
    add: Color32::from_rgb(0x00, 0x6a, 0x1a),
    del: Color32::from_rgb(0xb0, 0x00, 0x00),
    modified: Color32::from_rgb(0x7a, 0x50, 0x00),
    renamed: Color32::from_rgb(0x00, 0x50, 0xb0),
    typechange: Color32::from_rgb(0x6a, 0x00, 0xa0),
    hunk: Color32::from_rgb(0x00, 0x3a, 0x80),
    add_bg: Color32::from_rgba_premultiplied(11, 36, 15, 57),
    add_bg_strong: Color32::from_rgba_premultiplied(21, 72, 30, 115),
    del_bg: Color32::from_rgba_premultiplied(38, 6, 9, 47),
    del_bg_strong: Color32::from_rgba_premultiplied(78, 14, 18, 96),
    word_add: Color32::from_rgba_premultiplied(24, 84, 35, 133),
    word_del: Color32::from_rgba_premultiplied(93, 15, 21, 115),
    hunk_bg: Color32::from_rgba_premultiplied(2, 12, 26, 30),
    lanes: [
        Color32::from_rgb(0x00, 0x60, 0xd0),
        Color32::from_rgb(0xc0, 0x50, 0x00),
        Color32::from_rgb(0xa0, 0x00, 0xa0),
        Color32::from_rgb(0x00, 0x7a, 0x20),
        Color32::from_rgb(0xb0, 0x70, 0x00),
        Color32::from_rgb(0x00, 0x90, 0xa0),
        Color32::from_rgb(0xc0, 0x00, 0x30),
        Color32::from_rgb(0x50, 0x7a, 0x00),
        Color32::from_rgb(0x6a, 0x30, 0xd0),
        Color32::from_rgb(0x00, 0x00, 0x00),
    ],
    syntax: EmbeddedThemeName::InspiredGithub,
};

/// A family of looks, each with a dark and a light version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Classic,
    Raccoon,
    Pastel,
    Forest,
    Paper,
    HighContrast,
}

impl Family {
    /// In the order they're offered: Git Raccoon's own first.
    pub const ALL: [Family; 6] = [
        Family::Raccoon,
        Family::Classic,
        Family::Pastel,
        Family::Forest,
        Family::Paper,
        Family::HighContrast,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Family::Classic => "Classic",
            Family::Raccoon => "Raccoon",
            Family::Pastel => "Pastel",
            Family::Forest => "Forest",
            Family::Paper => "Paper",
            Family::HighContrast => "High contrast",
        }
    }

    /// Its dark or light version.
    pub fn look(self, light: bool) -> Look {
        Look {
            family: self,
            light,
        }
    }
}

/// One look: a family's dark or light version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub family: Family,
    pub light: bool,
}

impl Look {
    /// Its colours.
    pub fn palette(self) -> &'static Palette {
        match (self.family, self.light) {
            (Family::Classic, false) => &DARK,
            (Family::Classic, true) => &LIGHT,
            (Family::Raccoon, false) => &RACCOON,
            (Family::Raccoon, true) => &RACCOON_LIGHT,
            (Family::Pastel, false) => &PASTEL_DARK,
            (Family::Pastel, true) => &PASTEL,
            (Family::Forest, false) => &FOREST,
            (Family::Forest, true) => &FOREST_LIGHT,
            (Family::Paper, false) => &PAPER_DARK,
            (Family::Paper, true) => &PAPER,
            (Family::HighContrast, false) => &HIGHCONTRAST,
            (Family::HighContrast, true) => &HIGHCONTRAST_LIGHT,
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

/// The colours in use: a built-in look's, or a custom theme's. Raccoon's
/// dark ones to start with.
static CURRENT: RwLock<Palette> = RwLock::new(RACCOON);

fn palette() -> RwLockReadGuard<'static, Palette> {
    CURRENT.read().unwrap_or_else(PoisonError::into_inner)
}

impl Palette {
    /// Whether it's built on the light visuals.
    pub fn is_light(&self) -> bool {
        self.light
    }

    /// The colours a custom theme edits, as they are here.
    pub fn colours(&self) -> crate::themes::Colours {
        use crate::themes::Hex;
        crate::themes::Colours {
            background: Hex(self.bg),
            panels: Hex(self.bg_raised),
            hover: Hex(self.bg_hover),
            selected: Hex(self.bg_selected),
            borders: Hex(self.border),
            text: Hex(self.text),
            secondary_text: Hex(self.muted),
            accent: Hex(self.accent),
            on_accent: Hex(self.on_accent),
            added: Hex(self.add),
            deleted: Hex(self.del),
            modified: Hex(self.modified),
            renamed: Hex(self.renamed),
            danger: Hex(self.danger),
            lanes: self.lanes.iter().map(|c| Hex(*c)).collect(),
        }
    }

    /// This look with a custom theme's colours, and what follows from them
    /// (the diffs' backgrounds from the added and deleted colours).
    pub fn with(&self, c: &crate::themes::Colours) -> Palette {
        let tint = |colour: Color32, alpha: u8| {
            Color32::from_rgba_unmultiplied(colour.r(), colour.g(), colour.b(), alpha)
        };
        let mut lanes = self.lanes;
        for (lane, hex) in lanes.iter_mut().zip(&c.lanes) {
            *lane = hex.0;
        }
        Palette {
            light: self.light,
            bg: c.background.0,
            bg_raised: c.panels.0,
            bg_hover: c.hover.0,
            bg_selected: c.selected.0,
            border: c.borders.0,
            text: c.text.0,
            muted: c.secondary_text.0,
            accent: c.accent.0,
            on_accent: c.on_accent.0,
            danger: c.danger.0,
            danger_bg: c.background.0.lerp_to_gamma(c.danger.0, 0.18),
            add: c.added.0,
            del: c.deleted.0,
            modified: c.modified.0,
            renamed: c.renamed.0,
            typechange: self.typechange,
            hunk: c.secondary_text.0,
            add_bg: tint(c.added.0, 31),
            add_bg_strong: tint(c.added.0, 56),
            del_bg: tint(c.deleted.0, 31),
            del_bg_strong: tint(c.deleted.0, 56),
            word_add: tint(c.added.0, 89),
            word_del: tint(c.deleted.0, 89),
            hunk_bg: tint(c.secondary_text.0, 15),
            lanes,
            syntax: self.syntax,
        }
    }
}

/// How two colours contrast (WCAG): from 1 (the same) to 21.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn luminance(c: Color32) -> f32 {
    let linear = |c: u8| {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
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

/// Gives unticked checkboxes and radio buttons in `ui` an edge that shows:
/// otherwise they're filled with the dialog's or menu's own colour, with at
/// most a faint border (and in menus none: egui's menu style takes it off).
pub fn outline_toggles(ui: &mut egui::Ui) {
    ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1.0, muted());
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

/// Text that reads on `fill`: dark on light colours, white on dark ones.
pub fn text_on(fill: Color32) -> Color32 {
    // Relative luminance (WCAG). White is kept up to medium colours (the
    // looks' oranges and reds), which still read at 3:1 or better.
    let linear = |c: u8| {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let l = 0.2126 * linear(fill.r()) + 0.7152 * linear(fill.g()) + 0.0722 * linear(fill.b());
    if l > 0.3 {
        Color32::from_rgb(0x10, 0x13, 0x1a)
    } else {
        Color32::WHITE
    }
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

/// Installs fonts (with Phosphor icons) and the default look.
pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
    set(ctx, Family::Raccoon.look(false));
}

/// Switches to a look.
pub fn set(ctx: &egui::Context, look: Look) {
    set_palette(ctx, *look.palette());
}

/// Switches to these colours (a look's, or a custom theme's).
pub fn set_palette(ctx: &egui::Context, colours: Palette) {
    *CURRENT.write().unwrap_or_else(PoisonError::into_inner) = colours;
    let light = colours.light;
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

#[cfg(test)]
mod contrast_tests {
    use super::*;

    #[test]
    fn label_text_reads_on_every_lane_colour() {
        assert_eq!(text_on(Color32::WHITE), Color32::from_rgb(0x10, 0x13, 0x1a));
        assert_eq!(text_on(Color32::BLACK), Color32::WHITE);
        // Raccoon's orange keeps white; its cream (the screenshot) turns dark.
        assert_eq!(text_on(Color32::from_rgb(0xc4, 0x55, 0x3a)), Color32::WHITE);
        assert_ne!(text_on(Color32::from_rgb(0xf5, 0xea, 0xdb)), Color32::WHITE);
        for family in Family::ALL {
            for light in [false, true] {
                let p = family.look(light).palette();
                for (i, &lane) in p.lanes.iter().chain([&p.accent]).enumerate() {
                    let c = contrast(lane, text_on(lane));
                    assert!(c >= 3.0, "{} {light} colour {i}: {c:.1}", family.name());
                }
            }
        }
    }
}
