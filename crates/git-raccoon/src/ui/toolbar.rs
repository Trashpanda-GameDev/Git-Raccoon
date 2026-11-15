//! The bar across the top: sidebar toggle, app name, repo menu, branch, refresh.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use crate::settings::RecentRepo;
use crate::theme;
use crate::ui::{welcome, widgets};

pub enum Action {
    ToggleSidebar,
    Browse,
    Open(String),
    Forget(String),
    Close,
    Refresh,
    Undo,
}

pub struct OpenRepo<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub branch: Option<&'a str>,
    pub refreshing: bool,
    /// What undo would do, if anything.
    pub undo: Option<&'a str>,
}

pub fn show(
    ui: &mut egui::Ui,
    logo: &egui::TextureHandle,
    repo: Option<OpenRepo<'_>>,
    recent: &[RecentRepo],
    opening: bool,
    now: i64,
) -> Option<Action> {
    let mut action = None;
    ui.horizontal_centered(|ui| {
        if widgets::icon_button(ui, icon::SIDEBAR_SIMPLE, "Show or hide the sidebar").clicked() {
            action = Some(Action::ToggleSidebar);
        }
        ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(20.0, 20.0)));
        ui.label(RichText::new("Git Raccoon").strong().size(14.0));

        if let Some(repo) = repo {
            ui.add_space(8.0);
            let button = RichText::new(format!("{}  {}", repo.name, icon::CARET_DOWN)).strong();
            let menu = ui.menu_button(button, |ui| {
                ui.set_width(420.0);
                if ui
                    .button(format!("{}  Open repository…", icon::FOLDER_OPEN))
                    .clicked()
                {
                    action = Some(Action::Browse);
                    ui.close();
                }
                if ui
                    .button(format!("{}  Close repository", icon::X_CIRCLE))
                    .clicked()
                {
                    action = Some(Action::Close);
                    ui.close();
                }
                if !recent.is_empty() {
                    ui.separator();
                    ui.label(widgets::section_label("Recent"));
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for r in recent {
                        match welcome::recent_row(ui, r, now) {
                            Some(welcome::Action::Open(p)) => {
                                action = Some(Action::Open(p));
                                ui.close();
                            }
                            Some(welcome::Action::Forget(p)) => action = Some(Action::Forget(p)),
                            _ => {}
                        }
                    }
                }
            });
            menu.response.on_hover_text(repo.path);

            let branch = repo.branch.unwrap_or("detached HEAD");
            egui::Frame::new()
                .fill(theme::BG)
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(8, 1))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("{}  {branch}", icon::GIT_BRANCH))
                            .size(12.0)
                            .color(theme::ACCENT),
                    );
                });

            ui.add_enabled_ui(!repo.refreshing, |ui| {
                let glyph = if repo.refreshing {
                    icon::HOURGLASS
                } else {
                    icon::ARROWS_CLOCKWISE
                };
                if widgets::icon_button(
                    ui,
                    glyph,
                    "Refresh (also happens automatically when the repo changes)",
                )
                .clicked()
                {
                    action = Some(Action::Refresh);
                }
            });
            // Says what it would undo, so there are no surprises.
            match repo.undo {
                Some(what) => {
                    let text: String = if what.chars().count() > 48 {
                        what.chars().take(47).chain(['…']).collect()
                    } else {
                        what.to_string()
                    };
                    let button = ui.add(egui::Button::new(
                        RichText::new(format!("{}  {text}", icon::ARROW_U_UP_LEFT)).size(12.0),
                    ));
                    ui.ctx()
                        .accesskit_node_builder(button.id, |n| n.set_label("Undo"));
                    if button.on_hover_text(format!("{what} (Ctrl+Z)")).clicked() {
                        action = Some(Action::Undo);
                    }
                }
                None => {
                    widgets::named_icon_button(ui, icon::ARROW_U_UP_LEFT, "Undo", false)
                        .on_disabled_hover_text("Nothing to undo");
                }
            }
        }
        if opening {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("Opening…").color(theme::MUTED));
                ui.spinner();
            });
        }
    });
    action
}
