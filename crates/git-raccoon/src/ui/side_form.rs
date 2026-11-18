//! What the dialogs with places down the side and a form beside them (new
//! repository, clone) are made of: the places, their rows, and the form's
//! named rows.

use eframe::egui;

use crate::forge::{Account, Kind};
use crate::theme;
use crate::ui::sign_in::Place as Forge;

/// The width of the names' column, and of the fields beside it.
pub const LABELS: f32 = 150.0;
pub const FIELDS: f32 = 400.0;
/// The width of the places down the side.
pub const PLACES: f32 = 190.0;

/// The forges, in the order they're listed.
pub const FORGES: [Forge; 3] = [Forge::GitHub, Forge::GitLab, Forge::SelfManaged];

pub fn forge_name(forge: Forge) -> &'static str {
    match forge {
        Forge::GitHub => "GitHub",
        Forge::GitLab => "GitLab.com",
        Forge::SelfManaged => "Self-managed GitLab",
    }
}

pub fn forge_glyph(forge: Forge) -> &'static str {
    use egui_phosphor::regular as icon;
    match forge {
        Forge::GitHub => icon::GITHUB_LOGO,
        Forge::GitLab => icon::GITLAB_LOGO,
        Forge::SelfManaged => icon::GITLAB_LOGO_SIMPLE,
    }
}

/// Whether `account` is on `forge`.
pub fn forge_has(forge: Forge, account: &Account) -> bool {
    match forge {
        Forge::GitHub => account.kind == Kind::GitHub && account.base == "https://github.com",
        Forge::GitLab => account.base == "https://gitlab.com",
        Forge::SelfManaged => account.kind == Kind::GitLab && account.base != "https://gitlab.com",
    }
}

/// One of the places down the side: its icon and name, at the left.
pub fn place_row(ui: &mut egui::Ui, glyph: &str, name: &str, chosen: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(PLACES, 32.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            chosen,
            name,
        )
    });
    let painter = ui.painter();
    if chosen {
        painter.rect_filled(rect, 4, theme::bg_selected());
    } else if response.hovered() {
        painter.rect_filled(rect, 4, theme::bg_hover());
    }
    let colour = if chosen {
        theme::text()
    } else {
        theme::muted()
    };
    painter.text(
        rect.left_center() + egui::vec2(18.0, 0.0),
        egui::Align2::CENTER_CENTER,
        glyph,
        theme::body(15.0),
        colour,
    );
    painter.text(
        rect.left_center() + egui::vec2(36.0, 0.0),
        egui::Align2::LEFT_CENTER,
        name,
        theme::body(theme::FONT_SIZE),
        colour,
    );
    response
}

/// A row's name, at the right of the names' column.
pub fn label(ui: &mut egui::Ui, text: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(LABELS, 22.0), egui::Sense::hover());
    ui.painter().text(
        rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        text,
        theme::body(theme::FONT_SIZE),
        theme::muted(),
    );
}

/// A one-line field, with its name for screen readers.
pub fn field(ui: &mut egui::Ui, text: &mut String, name: &str, hint: &str) -> egui::Response {
    field_sized(ui, text, name, hint, FIELDS)
}

/// A one-line field `width` wide.
pub fn field_sized(
    ui: &mut egui::Ui,
    text: &mut String,
    name: &str,
    hint: &str,
    width: f32,
) -> egui::Response {
    let response = ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(width),
    );
    ui.ctx()
        .accesskit_node_builder(response.id, |n| n.set_label(name));
    response
}

/// A checkbox beside its row's name, and named for screen readers.
pub fn check(ui: &mut egui::Ui, value: &mut bool, name: &str) -> egui::Response {
    let response = ui.checkbox(value, "");
    ui.ctx()
        .accesskit_node_builder(response.id, |n| n.set_label(name));
    response
}

/// A path shown in full where it fits, cut short (and whole on hover) where
/// it doesn't.
pub fn path(ui: &mut egui::Ui, full: &str) {
    ui.scope(|ui| {
        ui.set_max_width(FIELDS);
        ui.add(egui::Label::new(egui::RichText::new(full).color(theme::muted())).truncate())
            .on_hover_text(full);
    });
}
