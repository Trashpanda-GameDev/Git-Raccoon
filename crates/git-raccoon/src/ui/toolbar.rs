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
}

pub struct OpenRepo<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub branch: Option<&'a str>,
    pub refreshing: bool,
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
