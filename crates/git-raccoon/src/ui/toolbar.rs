//! The bar across the top: sidebar toggle, repo menu, branch, refresh and
//! undo; branch, fetch, pull and push in the middle; the palette, feedback
//! and settings at the right. (The app's name and icon are in the window's
//! title bar.)

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::sync::FetchOptions;

use crate::settings::{PullChanges, PullOptions, PullStrategy, RecentRepo};
use crate::theme;
use crate::ui::{welcome, widgets};

pub enum Action {
    ToggleSidebar,
    Open(String),
    Forget(String),
    Refresh,
    NewBranch,
    Undo,
    /// Fetch from this remote, or every remote (`None`).
    Fetch(Option<String>),
    /// How Fetch goes from now on.
    SetFetchOptions(FetchOptions),
    /// Pull from the upstream, or (`Some`) from this remote.
    Pull(Option<String>),
    /// How Pull goes from now on.
    SetPullOptions(PullOptions),
    Push,
    CancelSync,
    Palette,
    /// Search the commits (as Ctrl+F does).
    Search,
    Settings,
    Feedback,
    /// Open a terminal in the repository's folder.
    Terminal,
    RestartToUpdate,
    /// From the profile switcher.
    Profile(crate::ui::profiles::Pick),
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
    /// Its remotes' names, to fetch from one.
    pub remotes: Vec<&'a str>,
    /// How Fetch goes.
    pub fetch: FetchOptions,
    /// Background fetching: when it last ran, or why it didn't work.
    pub auto_fetch: Option<String>,
    /// How Pull goes.
    pub pull: PullOptions,
}

pub fn show(
    ui: &mut egui::Ui,
    repo: Option<OpenRepo<'_>>,
    recent: &[RecentRepo],
    // For the profile switcher.
    settings: &crate::settings::Settings,
    opening: bool,
    now: i64,
    // A downloaded update's version, waiting for a restart.
    update: Option<&str>,
) -> Option<Action> {
    let mut action = None;
    // Commits can only be searched in a repository.
    let searchable = repo.is_some();
    // How much room each part took last time decides how they fit now:
    // first the right-hand buttons as icons (compact), then the names on
    // the left cut short (tight).
    let width = ui.available_width();
    let fit = Fit::recall(ui.ctx());
    let compact = fit.left_full + fit.middle + fit.right_full + 36.0 > width;
    let tight = compact && fit.left_full + fit.middle + fit.right_compact + 36.0 > width;
    ui.horizontal_centered(|ui| {
        let bar_left = ui.max_rect().left();
        let (mut left_used, mut middle_used) = (0.0, 0.0);
        if widgets::icon_button(ui, icon::SIDEBAR_SIMPLE, "Show or hide the sidebar").clicked() {
            action = Some(Action::ToggleSidebar);
        }

        if let Some(repo) = repo {
            ui.add_space(8.0);
            let button =
                RichText::new(format!("{}  {}", cut(repo.name, tight, 18), icon::CARET_DOWN))
                    .strong();
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

            let full_branch = repo.branch.unwrap_or("detached HEAD");
            let branch = cut(full_branch, tight, 22);
            let pill = egui::Frame::new()
                .fill(theme::bg())
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(8, 1))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("{}  {branch}", icon::GIT_BRANCH))
                            .size(12.0)
                            .color(theme::accent()),
                    );
                })
                .response;
            if tight {
                pill.on_hover_text(full_branch);
            }

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
                    let label = if tight {
                        icon::ARROW_U_UP_LEFT.to_string()
                    } else {
                        format!("{}  {text}", icon::ARROW_U_UP_LEFT)
                    };
                    let button = ui.add(egui::Button::new(RichText::new(label).size(12.0)));
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
            // Never under the right-hand buttons: slid left to end before them.
            let right = if compact { fit.right_compact } else { fit.right_full };
            let room = bar.right() - right - 12.0;
            let left = (bar.center().x - width / 2.0).min(room - width).max(after);
            left_used = after - 12.0 - bar_left;
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
            middle_used = used;
        }
        // Settings and the command palette, at the far right.
        let right_start = ui.max_rect().right();
        let right_group = ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icon::GEAR_SIX, "Settings").clicked() {
                action = Some(Action::Settings);
            }
            if widgets::icon_button(ui, icon::CHAT_CIRCLE_TEXT, "Send feedback: a bug, a thought or an idea")
                .clicked()
            {
                action = Some(Action::Feedback);
            }
            // The profile in use, to switch.
            if let Some(pick) = crate::ui::profiles::switcher(ui, settings, compact) {
                action = Some(Action::Profile(pick));
            }
            // Agents connected now (see Settings › Agents).
            let agents: crate::app::AgentsStatus = ui
                .ctx()
                .data(|d| d.get_temp(crate::app::agents_status_id()))
                .unwrap_or_default();
            if !agents.clients.is_empty() {
                let tip = format!(
                    "Connected: {}. They read freely, and ask before changing anything.",
                    agents.clients.join(", ")
                );
                let button = ui.add(
                    egui::Button::new(RichText::new(icon::ROBOT).size(16.0).color(theme::accent()))
                        .frame(false),
                );
                ui.ctx()
                    .accesskit_node_builder(button.id, |n| n.set_label("Agents connected"));
                if button.on_hover_text(tip).clicked() {
                    crate::ui::settings::open_on_agents(ui.ctx());
                    action = Some(Action::Settings);
                }
            }
            if let Some(version) = update {
                let name = format!("Restart to update to {version}");
                let text = if compact {
                    icon::ARROW_CIRCLE_UP.to_string()
                } else {
                    format!("{}  {name}", icon::ARROW_CIRCLE_UP)
                };
                let button = egui::Button::new(
                    RichText::new(text).size(12.0).color(theme::on_accent()),
                )
                .fill(theme::accent_fill());
                let button = ui.add(button);
                ui.ctx()
                    .accesskit_node_builder(button.id, |n| n.set_label(name.as_str()));
                if button
                    .on_hover_text(format!(
                        "{name}: a new version is downloaded. Your tabs and settings are kept."
                    ))
                    .clicked()
                {
                    action = Some(Action::RestartToUpdate);
                }
            }
            let shortcut = crate::commands::shortcut(&crate::commands::Command::Palette)
                .map(|s| ui.ctx().format_shortcut(&s))
                .unwrap_or_default();
            let button = ui
                .button(
                    RichText::new(if compact {
                        icon::COMMAND.to_string()
                    } else {
                        format!("{}  Commands  {shortcut}", icon::COMMAND)
                    })
                    .size(12.0),
                )
                .on_hover_text(format!("Commands ({shortcut})"));
            ui.ctx()
                .accesskit_node_builder(button.id, |n| n.set_label("Commands"));
            if button
                .on_hover_text("Every action, searchable, with its keyboard shortcut")
                .clicked()
            {
                action = Some(Action::Palette);
            }
            if searchable {
                let shortcut = crate::commands::shortcut(&crate::commands::Command::Find)
                    .map(|s| ui.ctx().format_shortcut(&s))
                    .unwrap_or_default();
                let button = ui
                    .button(
                        RichText::new(if compact {
                            icon::MAGNIFYING_GLASS.to_string()
                        } else {
                            format!("{}  Search  {shortcut}", icon::MAGNIFYING_GLASS)
                        })
                        .size(12.0),
                    )
                    .on_hover_text(format!("Search ({shortcut})"));
                ui.ctx()
                    .accesskit_node_builder(button.id, |n| n.set_label("Search"));
                if button
                    .on_hover_text("Search the commits: messages, authors, SHAs or changed files")
                    .clicked()
                {
                    action = Some(Action::Search);
                }
            }
        });
        // How wide the right-hand buttons came out, for placing the middle
        // ones next time; and how wide they are in full.
        let right_used = right_start - right_group.response.rect.left();
        let mut next = fit;
        next.middle = middle_used;
        if !tight {
            next.left_full = left_used;
        }
        if compact {
            next.right_compact = right_used;
        } else {
            next.right_full = right_used;
        }
        next.remember(ui.ctx());
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

/// How wide the toolbar's parts came out last time: the left one at full
/// size, the middle one, and the right one in full and as icons.
#[derive(Clone, Copy, Default)]
struct Fit {
    left_full: f32,
    middle: f32,
    right_full: f32,
    right_compact: f32,
}

impl Fit {
    fn id() -> egui::Id {
        egui::Id::new("toolbar fit")
    }

    fn recall(ctx: &egui::Context) -> Fit {
        ctx.data(|d| d.get_temp(Fit::id())).unwrap_or_default()
    }

    fn remember(self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_temp(Fit::id(), self));
    }
}

/// `text`, cut to `most` characters (with an ellipsis) when `tight`.
fn cut(text: &str, tight: bool, most: usize) -> String {
    if tight && text.chars().count() > most {
        text.chars().take(most - 1).chain(['…']).collect()
    } else {
        text.to_string()
    }
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
            match &repo.auto_fetch {
                Some(line) => format!(
                    "Fetch from all remotes: see new commits without changing your branches\n{line}"
                ),
                None => "Fetch from all remotes: see new commits without changing your branches"
                    .to_string(),
            },
            Action::Fetch(None),
        ),
        (
            "Pull",
            icon::ARROW_FAT_LINE_DOWN,
            count(icon::ARROW_DOWN, behind),
            match repo.upstream {
                Some((up, _, b)) => format!("Pull {b} new commits from {up} into this branch"),
                None => "This branch has no upstream to pull from yet. Push it first.".into(),
            },
            Action::Pull(None),
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
        // Fetch and Pull are split buttons: their options' arrow joined on
        // at the right.
        let split = name == "Fetch" || name == "Pull";
        ui.scope(|ui| {
            let round = ui.visuals().widgets.inactive.corner_radius;
            let mut button =
                egui::Button::new(RichText::new(format!("{glyph}  {name}{extra}")).size(12.0));
            if split {
                ui.spacing_mut().item_spacing.x = 0.0;
                button = button.corner_radius(egui::CornerRadius {
                    ne: 0,
                    se: 0,
                    ..round
                });
            }
            let button = ui.add_enabled(enabled, button);
            ui.ctx()
                .accesskit_node_builder(button.id, |n| n.set_label(name));
            let height = button.rect.height();
            if button
                .on_hover_text(&tip)
                .on_disabled_hover_text(&tip)
                .clicked()
            {
                action = Some(act);
            }
            let menu = match name {
                "Fetch" => fetch_menu(ui, repo, enabled, height),
                // Pulling from a remote needs no upstream.
                "Pull" => pull_menu(ui, repo, !repo.busy, height),
                _ => None,
            };
            if menu.is_some() {
                action = menu;
            }
        });
    }
    action
}

/// The menu beside Fetch: where from (every remote, or one), and how
/// (pruning, forcing), remembered for Fetch itself.
fn fetch_menu(
    ui: &mut egui::Ui,
    repo: &OpenRepo<'_>,
    enabled: bool,
    height: f32,
) -> Option<Action> {
    let mut action = None;
    // With one remote, Fetch itself is the only choice of where from.
    let choose = repo.remotes.len() > 1;
    let arrow = options_arrow(ui, enabled, height, "Fetch options", |ui| {
        if choose {
            heading(ui, "Fetch from");
            if ui.button("All remotes").clicked() {
                action = Some(Action::Fetch(None));
                ui.close();
            }
            for remote in &repo.remotes {
                if ui.button(*remote).clicked() {
                    action = Some(Action::Fetch(Some((*remote).to_string())));
                    ui.close();
                }
            }
            ui.separator();
        }
        let mut options = repo.fetch;
        toggles(ui, |ui| {
            ui.checkbox(
                &mut options.prune,
                "Prune: remove remote branches deleted there",
            );
            ui.checkbox(
                &mut options.force,
                "Force: let fetched refs overwrite local ones",
            );
        });
        if options != repo.fetch {
            action = Some(Action::SetFetchOptions(options));
        }
    });
    arrow.on_hover_text(if choose {
        "Fetch from one remote, or change how Fetch goes"
    } else {
        "Change how Fetch goes"
    });
    action
}

/// The menu beside Pull: where from (the upstream, or a remote's branch of
/// the same name), and how (joining both sides' commits, and what to do
/// with uncommitted changes), remembered for Pull itself.
fn pull_menu(ui: &mut egui::Ui, repo: &OpenRepo<'_>, enabled: bool, height: f32) -> Option<Action> {
    let mut action = None;
    // With one remote and an upstream there, Pull itself is the only choice
    // of where from; without an upstream, the remote is how to pull at all.
    let choose = repo.remotes.len() > 1 || repo.upstream.is_none();
    let arrow = options_arrow(ui, enabled, height, "Pull options", |ui| {
        if choose {
            heading(ui, "Pull from");
            if let Some((up, ..)) = repo.upstream
                && ui.button(format!("Upstream ({up})")).clicked()
            {
                action = Some(Action::Pull(None));
                ui.close();
            }
            for remote in &repo.remotes {
                if ui.button(*remote).clicked() {
                    action = Some(Action::Pull(Some((*remote).to_string())));
                    ui.close();
                }
            }
            ui.separator();
        }
        let mut options = repo.pull;
        toggles(ui, |ui| {
            heading(ui, "When both sides have new commits");
            for strategy in PullStrategy::ALL {
                ui.radio_value(&mut options.strategy, strategy, strategy.label());
            }
            ui.separator();
            heading(ui, "Uncommitted changes");
            for changes in PullChanges::ALL {
                ui.radio_value(&mut options.changes, changes, changes.label());
            }
        });
        if options != repo.pull {
            action = Some(Action::SetPullOptions(options));
        }
    });
    arrow.on_hover_text(if choose {
        "Pull from another remote, or change how Pull goes"
    } else {
        "Change how Pull goes"
    });
    action
}

/// Checkboxes and radio buttons in a menu, outlined so unticked ones show.
fn toggles(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.scope(|ui| {
        theme::outline_toggles(ui);
        add(ui);
    });
}

/// A small heading in an options menu.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(theme::muted()));
}

/// The right half of a split button: an arrow, as tall as the left half
/// and rounded at the right, opening a menu of options. Ticking options
/// leaves the menu open; `contents` closes it on a choice.
fn options_arrow(
    ui: &mut egui::Ui,
    enabled: bool,
    height: f32,
    label: &str,
    contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let menu = ui.add_enabled_ui(enabled, |ui| {
        let config = egui::containers::menu::MenuConfig::new()
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
        let round = ui.visuals().widgets.inactive.corner_radius;
        let arrow = egui::Button::new(RichText::new(icon::CARET_DOWN).size(12.0))
            .corner_radius(egui::CornerRadius {
                nw: 0,
                sw: 0,
                ..round
            })
            .min_size(egui::vec2(0.0, height));
        egui::containers::menu::MenuButton::from_button(arrow)
            .config(config)
            .ui(ui, contents)
            .0
    });
    let button = menu.inner;
    // A line between the halves.
    ui.painter().vline(
        button.rect.left(),
        button.rect.y_range().shrink(5.0),
        egui::Stroke::new(1.0, theme::border()),
    );
    let label = label.to_string();
    ui.ctx()
        .accesskit_node_builder(button.id, |n| n.set_label(label));
    button
}
