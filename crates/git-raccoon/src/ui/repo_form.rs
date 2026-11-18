//! What the new-repository and clone dialogs are made of: the account
//! they're for (none, for what needs no account), and the form's named rows.

use eframe::egui;

use crate::forge::{Account, Kind};
use crate::theme;

/// The width of the names' column, and of the fields beside it.
pub const LABELS: f32 = 150.0;
pub const FIELDS: f32 = 400.0;
/// How wide the dialogs are: the names, a gap and the fields.
pub const WIDTH: f32 = LABELS + 12.0 + FIELDS;
/// How tall a dropdown with a long list is. It's this tall from when it
/// opens: egui keeps a dropdown the size it opened at, which would be a line
/// or two if its list is still loading.
pub const LIST_HEIGHT: f32 = 280.0;

/// How an account is shown: its forge's icon, its name, and the forge.
pub fn account_name(account: &Account) -> String {
    use egui_phosphor::regular as icon;
    let glyph = match account.kind {
        Kind::GitHub => icon::GITHUB_LOGO,
        Kind::GitLab => icon::GITLAB_LOGO,
    };
    format!("{glyph}  {} ({})", account.login, account.place())
}

/// The account row's dropdown: none, or one of the accounts signed in to
/// (an index into them). `name` is its name for screen readers. Returns
/// whether the choice changed.
pub fn account_picker(
    ui: &mut egui::Ui,
    chosen: &mut Option<usize>,
    accounts: &[Account],
    name: &str,
) -> bool {
    let before = *chosen;
    let shown = chosen
        .and_then(|i| accounts.get(i))
        .map_or_else(|| "None".to_string(), account_name);
    let combo = egui::ComboBox::from_id_salt(name)
        .selected_text(shown)
        .width(FIELDS)
        .show_ui(ui, |ui| {
            ui.selectable_value(chosen, None, "None");
            for (i, a) in accounts.iter().enumerate() {
                ui.selectable_value(chosen, Some(i), account_name(a));
            }
            if accounts.is_empty() {
                ui.label(
                    egui::RichText::new(
                        "Sign in to GitHub or GitLab in Settings to see your accounts here.",
                    )
                    .size(12.0)
                    .color(theme::muted()),
                );
            }
        });
    ui.ctx()
        .accesskit_node_builder(combo.response.id, |n| n.set_label(name));
    *chosen != before
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
    let response = ui
        .scope(|ui| {
            theme::outline_toggles(ui);
            ui.checkbox(value, "")
        })
        .inner;
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
