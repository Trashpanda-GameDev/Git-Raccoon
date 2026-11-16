//! The bar across the top: sidebar toggle, repo menu, branch, refresh and
//! undo; branch, fetch, pull and push in the middle; the palette, feedback
//! and settings at the right. (The app's name and icon are in the window's
//! title bar.)

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use crate::settings::RecentRepo;
use crate::theme;
use crate::ui::{welcome, widgets};

pub enum Action {
    ToggleSidebar,
    Open(String),
    Forget(String),
    Refresh,
    NewBranch,
    Undo,
    Fetch,
    Pull,
    Push,
    CancelSync,
    Palette,
    Settings,
    Feedback,
    /// Open a terminal in the repository's folder.
    Terminal,
    RestartToUpdate,
}

/// A fetch, pull or push in progress.
pub struct Syncing {
    /// "fetch", "pull" or "push".
    pub verb: &'static str,
    pub phase: Option<String>,
    pub percent: Option<u8>,
}

pub struct OpenRepo<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub branch: Option<&'a str>,
    pub refreshing: bool,
    /// What undo would do, if anything.
    pub undo: Option<&'a str>,
    /// The checked-out branch's upstream ("origin/main") and how far ahead
    /// and behind it the branch is.
    pub upstream: Option<(&'a str, usize, usize)>,
    pub syncing: Option<Syncing>,
    /// Another operation is running.
    pub busy: bool,
    /// `Some(installed)` if the repository uses Git LFS.
    pub lfs: Option<bool>,
}

pub fn show(
    ui: &mut egui::Ui,
    repo: Option<OpenRepo<'_>>,
    recent: &[RecentRepo],
    opening: bool,
    now: i64,
    // A downloaded update's version, waiting for a restart.
    update: Option<&str>,
) -> Option<Action> {
    let mut action = None;
    ui.horizontal_centered(|ui| {
        if widgets::icon_button(ui, icon::SIDEBAR_SIMPLE, "Show or hide the sidebar").clicked() {
            action = Some(Action::ToggleSidebar);
        }

        if let Some(repo) = repo {
            ui.add_space(8.0);
            let button = RichText::new(format!("{}  {}", repo.name, icon::CARET_DOWN)).strong();
            let menu = ui.menu_button(button, |ui| {
                ui.set_width(420.0);
                // Open, clone and new are on a new tab; closing is the tab's ✕.
                if recent.is_empty() {
                    ui.label(RichText::new("No recent repositories").color(theme::muted()));
                } else {
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
                .fill(theme::bg())
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(8, 1))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("{}  {branch}", icon::GIT_BRANCH))
                            .size(12.0)
                            .color(theme::accent()),
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
                if widgets::icon_button(ui, icon::TERMINAL_WINDOW, "Open a terminal here (Ctrl+`)")
                    .clicked()
                {
                    action = Some(Action::Terminal);
                }
            });
            if let Some(installed) = repo.lfs {
                let (colour, tip) = if installed {
                    (theme::muted(), "This repository stores large files with Git LFS.")
                } else {
                    (
                        theme::modified(),
                        "This repository stores large files with Git LFS, but git-lfs isn't installed: those files are only small pointer files here. Install it from git-lfs.com, then run `git lfs pull`.",
                    )
                };
                let glyph = if installed { "" } else { "⚠ " };
                let pill = ui.label(RichText::new(format!("{glyph}LFS")).size(11.0).color(colour));
                let name = if installed { "Uses Git LFS" } else { "Uses Git LFS, which isn't installed" };
                ui.ctx().accesskit_node_builder(pill.id, |n| n.set_label(name));
                pill.on_hover_text(tip);
            }
            ui.add_space(6.0);
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
                    // Branch, fetch, pull and push in the middle of the bar (#5), or
            // just after the rest when the window is too narrow for that.
            let bar = ui.max_rect();
            let after = ui.cursor().left() + 12.0;
            let width_id = ui.id().with("centre width");
            let width: f32 = ui.data(|d| d.get_temp(width_id)).unwrap_or(320.0);
            let left = (bar.center().x - width / 2.0).max(after);
            let mut centre = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(egui::Rect::from_min_max(
                        egui::pos2(left, bar.top()),
                        egui::pos2(bar.right(), bar.bottom()),
                    ))
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
            );
            {
                let ui = &mut centre;
                let branch = ui.add_enabled(
                    !repo.busy,
                    egui::Button::new(RichText::new(format!("{}  Branch", icon::GIT_BRANCH)).size(12.0)),
                );
                ui.ctx()
                    .accesskit_node_builder(branch.id, |n| n.set_label("Branch"));
                let shortcut = crate::commands::shortcut(&crate::commands::Command::NewBranch)
                    .map(|s| ui.ctx().format_shortcut(&s))
                    .unwrap_or_default();
                if branch
                    .on_hover_text(format!("Create a branch from the checked-out commit ({shortcut})"))
                    .clicked()
                {
                    action = Some(Action::NewBranch);
                }
                if let Some(a) = sync_buttons(ui, &repo) {
                    action = Some(a);
                }
            }
            let used = centre.min_rect().width();
            ui.data_mut(|d| d.insert_temp(width_id, used));
        }
        // Settings and the command palette, at the far right.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icon::GEAR_SIX, "Settings").clicked() {
                action = Some(Action::Settings);
            }
            if widgets::icon_button(ui, icon::CHAT_CIRCLE_TEXT, "Send feedback: a bug, a thought or an idea")
                .clicked()
            {
                action = Some(Action::Feedback);
            }
            if let Some(version) = update {
                let button = egui::Button::new(
                    RichText::new(format!("{}  Restart to update to {version}", icon::ARROW_CIRCLE_UP))
                        .size(12.0)
                        .color(theme::on_accent()),
                )
                .fill(theme::accent());
                if ui
                    .add(button)
                    .on_hover_text("A new version is downloaded. Your tabs and settings are kept.")
                    .clicked()
                {
                    action = Some(Action::RestartToUpdate);
                }
            }
            let shortcut = crate::commands::shortcut(&crate::commands::Command::Palette)
                .map(|s| ui.ctx().format_shortcut(&s))
                .unwrap_or_default();
            let button = ui.button(
                RichText::new(format!("{}  Commands  {shortcut}", icon::COMMAND)).size(12.0),
            );
            ui.ctx()
                .accesskit_node_builder(button.id, |n| n.set_label("Commands"));
            if button
                .on_hover_text("Every action, searchable, with its keyboard shortcut")
                .clicked()
            {
                action = Some(Action::Palette);
            }
        });
        if opening {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("Opening…").color(theme::muted()));
                ui.spinner();
            });
        }
    });
    action
}

/// Fetch, pull and push; or, while one runs, its progress and Cancel.
fn sync_buttons(ui: &mut egui::Ui, repo: &OpenRepo<'_>) -> Option<Action> {
    let mut action = None;
    if let Some(sync) = &repo.syncing {
        ui.spinner();
        let doing = match sync.verb {
            "fetch" => "Fetching",
            "pull" => "Pulling",
            "push" => "Pushing",
            _ => "Updating",
        };
        let detail = match (&sync.phase, sync.percent) {
            (Some(phase), Some(p)) => format!("{doing}… {phase} {p}%"),
            (Some(phase), None) => format!("{doing}… {phase}"),
            _ => format!("{doing}…"),
        };
        ui.label(RichText::new(detail).size(12.0).color(theme::muted()));
        if let Some(p) = sync.percent {
            ui.add(
                egui::ProgressBar::new(f32::from(p) / 100.0)
                    .desired_width(80.0)
                    .desired_height(4.0),
            );
        }
        let cancel = ui.button(RichText::new("Cancel").size(12.0));
        let name = format!("Cancel {}", sync.verb);
        ui.ctx()
            .accesskit_node_builder(cancel.id, |n| n.set_label(name));
        if cancel.on_hover_text("Stop it").clicked() {
            action = Some(Action::CancelSync);
        }
        return action;
    }
    let (ahead, behind) = repo.upstream.map_or((0, 0), |(_, a, b)| (a, b));
    let count = |glyph: &str, n: usize| {
        if n > 0 {
            format!("  {glyph}{n}")
        } else {
            String::new()
        }
    };
    let buttons = [
        (
            "Fetch",
            icon::CLOUD_ARROW_DOWN,
            String::new(),
            "Fetch from all remotes: see new commits without changing your branches".to_string(),
            Action::Fetch,
        ),
        (
            "Pull",
            icon::ARROW_FAT_LINE_DOWN,
            count(icon::ARROW_DOWN, behind),
            match repo.upstream {
                Some((up, _, b)) => format!("Pull {b} new commits from {up} into this branch"),
                None => "This branch has no upstream to pull from yet. Push it first.".into(),
            },
            Action::Pull,
        ),
        (
            "Push",
            icon::ARROW_FAT_LINE_UP,
            count(icon::ARROW_UP, ahead),
            match repo.upstream {
                Some((up, a, _)) => format!("Push {a} commits to {up}"),
                None => "Push this branch, setting its upstream on the remote".into(),
            },
            Action::Push,
        ),
    ];
    for (name, glyph, extra, tip, act) in buttons {
        let enabled = !repo.busy && (name != "Pull" || repo.upstream.is_some());
        let button = ui.add_enabled(
            enabled,
            egui::Button::new(RichText::new(format!("{glyph}  {name}{extra}")).size(12.0)),
        );
        ui.ctx()
            .accesskit_node_builder(button.id, |n| n.set_label(name));
        if button
            .on_hover_text(&tip)
            .on_disabled_hover_text(&tip)
            .clicked()
        {
            action = Some(act);
        }
    }
    action
}
