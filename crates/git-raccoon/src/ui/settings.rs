//! The settings window: sections down the left (Appearance, Git,
//! Accounts, Tools, Updates and privacy, About), a search over all of
//! them, and each setting on a row of its own: its name and what it does
//! on the left, its control on the right. Changes apply straight away.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use crate::settings::{PullChanges, PullStrategy, Settings, ThemeChoice, ThemeMode, ZOOMS};
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Close,
    ClearRecent,
    Licenses,
    /// Sign in to a forge.
    AddAccount,
    /// Sign out of this account (by its place in the list).
    SignOut(usize),
    /// Show what's new in this version.
    WhatsNew,
    /// Open the feedback form.
    Feedback,
    /// Choose the folder clones go in.
    BrowseCloneDir,
}

/// A part of the settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Section {
    #[default]
    Appearance,
    Git,
    Accounts,
    Tools,
    Updates,
    About,
}

impl Section {
    const ALL: [Section; 6] = [
        Section::Appearance,
        Section::Git,
        Section::Accounts,
        Section::Tools,
        Section::Updates,
        Section::About,
    ];

    fn name(self) -> &'static str {
        match self {
            Section::Appearance => "Appearance",
            Section::Git => "Git",
            Section::Accounts => "Accounts",
            Section::Tools => "Tools",
            Section::Updates => "Updates and privacy",
            Section::About => "About",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Section::Appearance => icon::PALETTE,
            Section::Git => icon::GIT_BRANCH,
            Section::Accounts => icon::USER_CIRCLE,
            Section::Tools => icon::TERMINAL_WINDOW,
            Section::Updates => icon::ARROWS_CLOCKWISE,
            Section::About => icon::INFO,
        }
    }

    fn about(self) -> &'static str {
        match self {
            Section::Appearance => "How Git Raccoon looks, and how much fits on screen.",
            Section::Git => {
                "What Fetch, Pull and the diffs do, unless you choose otherwise at the time."
            }
            Section::Accounts => {
                "GitHub and GitLab accounts, for pull requests, issues and cloning."
            }
            Section::Tools => "Other programs Git Raccoon works with.",
            Section::Updates => "Updates, and what Git Raccoon remembers.",
            Section::About => "This version, its licenses, and telling us what you think.",
        }
    }

    /// Puts its settings back as they start out (not accounts or lists).
    fn reset(self, settings: &mut Settings) {
        let d = Settings::default();
        match self {
            Section::Appearance => {
                settings.theme = d.theme;
                settings.theme_mode = d.theme_mode;
                settings.set_zoom(1.0);
                settings.hide_sidebar = d.hide_sidebar;
                settings.columns = d.columns;
                settings.column_widths = d.column_widths;
            }
            Section::Git => {
                settings.pull = d.pull;
                settings.fetch_prune = d.fetch_prune;
                settings.fetch_force = d.fetch_force;
                settings.split_diff = d.split_diff;
                settings.file_tree = d.file_tree;
            }
            Section::Tools => {
                settings.terminal = d.terminal;
                settings.clone_dir = d.clone_dir;
            }
            Section::Updates => settings.no_update_checks = d.no_update_checks,
            Section::Accounts | Section::About => {}
        }
    }

    fn resettable(self) -> bool {
        !matches!(self, Section::Accounts | Section::About)
    }
}

/// One setting: where it is, its name, what it does, and other words it
/// can be found by.
struct Row {
    section: Section,
    name: &'static str,
    hint: &'static str,
    words: &'static str,
}

const ROWS: &[Row] = &[
    Row {
        section: Section::Appearance,
        name: "Light or dark",
        hint: "Every look has both.",
        words: "theme mode dark light system match",
    },
    Row {
        section: Section::Appearance,
        name: "Look",
        hint: "Colours of the app, the graph and diffs.",
        words: "theme colours colors raccoon classic pastel forest paper high contrast",
    },
    Row {
        section: Section::Appearance,
        name: "Interface size",
        hint: "",
        words: "zoom scale bigger smaller text font",
    },
    Row {
        section: Section::Appearance,
        name: "Sidebar",
        hint: "Ctrl+B shows or hides it.",
        words: "panel left branches show hide",
    },
    Row {
        section: Section::Appearance,
        name: "Graph columns",
        hint: "Drag a heading to move a column, or its edge to resize it.",
        words: "columns order width reset graph",
    },
    Row {
        section: Section::Git,
        name: "When both sides have new commits",
        hint: "Pull's menu can do it differently once.",
        words: "pull merge rebase fast-forward strategy",
    },
    Row {
        section: Section::Git,
        name: "Uncommitted changes",
        hint: "When pulling.",
        words: "pull stash leave changes",
    },
    Row {
        section: Section::Git,
        name: "Fetching",
        hint: "",
        words: "fetch prune deleted remote branches tags force",
    },
    Row {
        section: Section::Git,
        name: "Diffs",
        hint: "",
        words: "diff unified side by side split",
    },
    Row {
        section: Section::Git,
        name: "Changed files",
        hint: "In the working changes and in commits.",
        words: "files paths folders tree",
    },
    Row {
        section: Section::Accounts,
        name: "Signed in",
        hint: "Tokens are kept in your system's keychain.",
        words: "accounts github gitlab sign in out token login",
    },
    Row {
        section: Section::Tools,
        name: "Terminal",
        hint: "What \"Open a terminal here\" (Ctrl+`) runs.",
        words: "terminal command shell console",
    },
    Row {
        section: Section::Tools,
        name: "Clone into",
        hint: "Where new clones go, unless you choose another folder.",
        words: "clone folder directory default",
    },
    Row {
        section: Section::Updates,
        name: "Updates",
        hint: "",
        words: "update automatic check version",
    },
    Row {
        section: Section::Updates,
        name: "Recent repositories",
        hint: "The list on the start screen.",
        words: "recent history clear privacy",
    },
    Row {
        section: Section::About,
        name: "Version",
        hint: "",
        words: "version what's new changes",
    },
    Row {
        section: Section::About,
        name: "Feedback",
        hint: "A bug, a thought or an idea.",
        words: "feedback bug idea report",
    },
    Row {
        section: Section::About,
        name: "Licenses",
        hint: "",
        words: "licenses licences open source",
    },
];

impl Row {
    fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.split_whitespace().all(|w| {
            self.name.to_lowercase().contains(w)
                || self.hint.to_lowercase().contains(w)
                || self.words.contains(w)
                || self.section.name().to_lowercase().contains(w)
        })
    }
}

/// The window's own state: the section shown, and the search.
#[derive(Clone, Default)]
struct View {
    section: Section,
    search: String,
}

const LABEL: f32 = 190.0;

pub fn show(ctx: &egui::Context, settings: &mut Settings) -> Option<Action> {
    let mut action = None;
    let id = egui::Id::new("settings-window");
    let mut view: View = ctx.data(|d| d.get_temp(id)).unwrap_or_default();
    let modal = egui::Modal::new(egui::Id::new("settings")).show(ctx, |ui| {
        ui.set_width(900.0);
        // Title, search, close.
        ui.horizontal(|ui| {
            ui.set_height(36.0);
            ui.label(RichText::new("Settings").strong().size(17.0));
            ui.add_space(110.0);
            let search = ui.add(
                egui::TextEdit::singleline(&mut view.search)
                    .hint_text(format!("{}  Search settings", icon::MAGNIFYING_GLASS))
                    .desired_width(360.0),
            );
            ui.ctx()
                .accesskit_node_builder(search.id, |n| n.set_label("Search settings"));
            if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F)) {
                search.request_focus();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::icon_button(ui, icon::X, "Close settings").clicked() {
                    action = Some(Action::Close);
                }
            });
        });
        ui.separator();
        ui.horizontal_top(|ui| {
            // The sections.
            ui.vertical(|ui| {
                ui.set_width(180.0);
                ui.set_min_height(520.0);
                ui.spacing_mut().item_spacing.y = 2.0;
                for section in Section::ALL {
                    let on = view.search.trim().is_empty() && view.section == section;
                    let item = nav_item(ui, section, on);
                    if item.clicked() {
                        view.section = section;
                        view.search.clear();
                    }
                }
                ui.add_space(12.0);
                ui.label(
                    RichText::new("Changes apply as you make them and are remembered.")
                        .size(11.0)
                        .color(theme::muted()),
                );
            });
            ui.separator();
            // What's in it, or what matches the search.
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .id_salt("settings-rows")
                    .max_height(540.0)
                    .min_scrolled_height(540.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(4.0);
                        if view.search.trim().is_empty() {
                            page(ui, settings, view.section, &mut action);
                        } else {
                            results(ui, settings, &view.search, &mut action);
                        }
                    });
            });
        });
    });
    ctx.data_mut(|d| d.insert_temp(id, view));
    if modal.should_close() && action.is_none() {
        action = Some(Action::Close);
    }
    action
}

/// A section in the list on the left: its icon and name, from the left
/// edge, highlighted when it's the one shown.
fn nav_item(ui: &mut egui::Ui, section: Section, on: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(180.0, 34.0), egui::Sense::click());
    let name = section.name();
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, on, name));
    let painter = ui.painter();
    if on {
        painter.rect_filled(rect, 6, theme::bg_selected());
    } else if response.hovered() {
        painter.rect_filled(rect, 6, theme::bg_hover());
    }
    let colour = if on { theme::text() } else { theme::muted() };
    painter.text(
        egui::pos2(rect.left() + 20.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        section.glyph(),
        theme::body(15.0),
        if on { theme::accent() } else { colour },
    );
    painter.text(
        egui::pos2(rect.left() + 38.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        name,
        theme::body(14.0),
        colour,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A section's heading and its settings.
fn page(ui: &mut egui::Ui, settings: &mut Settings, section: Section, action: &mut Option<Action>) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(RichText::new(section.name()).strong().size(19.0));
            ui.label(
                RichText::new(section.about())
                    .size(13.0)
                    .color(theme::muted()),
            );
        });
        if section.resettable() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                let reset = ui
                    .add(
                        egui::Button::new(RichText::new("Reset to defaults").color(theme::muted()))
                            .frame(false),
                    )
                    .on_hover_text(format!(
                        "Put the {} settings back as they start out",
                        section.name().to_lowercase()
                    ));
                ui.ctx().accesskit_node_builder(reset.id, |n| {
                    n.set_label(format!("Reset {} to defaults", section.name()))
                });
                if reset.clicked() {
                    section.reset(settings);
                }
            });
        }
    });
    ui.add_space(8.0);
    for row in ROWS.iter().filter(|r| r.section == section) {
        draw(ui, settings, row, action);
    }
}

/// The settings that match a search, from every section.
fn results(ui: &mut egui::Ui, settings: &mut Settings, query: &str, action: &mut Option<Action>) {
    let found: Vec<&Row> = ROWS.iter().filter(|r| r.matches(query)).collect();
    ui.label(
        RichText::new(format!("Results for \u{201c}{}\u{201d}", query.trim()))
            .strong()
            .size(17.0),
    );
    ui.add_space(8.0);
    if found.is_empty() {
        ui.label(
            RichText::new("No settings match. Try fewer or other words.").color(theme::muted()),
        );
        return;
    }
    let mut last = None;
    for row in found {
        if last != Some(row.section) {
            ui.add_space(6.0);
            ui.label(widgets::section_label(row.section.name()));
            last = Some(row.section);
        }
        draw(ui, settings, row, action);
    }
}

/// One setting's row: its name and what it does, then its control.
fn draw(ui: &mut egui::Ui, settings: &mut Settings, row: &Row, action: &mut Option<Action>) {
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        egui::Stroke::new(1.0, theme::border()),
    );
    ui.add_space(10.0);
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(LABEL);
            ui.label(RichText::new(row.name).strong());
            if !row.hint.is_empty() {
                ui.add(
                    egui::Label::new(RichText::new(row.hint).size(12.0).color(theme::muted()))
                        .wrap(),
                );
            }
        });
        ui.add_space(12.0);
        ui.vertical(|ui| control(ui, settings, row.name, action));
    });
    ui.add_space(10.0);
}

fn hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(text.into()).size(12.0).color(theme::muted())).wrap());
}

/// A setting's control.
fn control(ui: &mut egui::Ui, settings: &mut Settings, name: &str, action: &mut Option<Action>) {
    match name {
        "Light or dark" => {
            ui.horizontal(|ui| {
                settings.theme_mode = widgets::segmented(
                    ui,
                    settings.theme_mode,
                    &[
                        (ThemeMode::Light, "Light"),
                        (ThemeMode::Dark, "Dark"),
                        (ThemeMode::System, "Match system"),
                    ],
                );
            });
            if settings.theme_mode == ThemeMode::System {
                let now = match ui.ctx().system_theme() {
                    Some(egui::Theme::Light) => "light",
                    _ => "dark",
                };
                hint(ui, format!("Follows your computer: {now} right now."));
            }
        }
        "Look" => {
            let mode = settings.theme_mode;
            let system = ui.ctx().system_theme();
            egui::Grid::new("themes")
                .spacing(egui::vec2(8.0, 8.0))
                .show(ui, |ui| {
                    for (i, choice) in ThemeChoice::ALL.into_iter().enumerate() {
                        if swatch(ui, choice, mode, system, settings.theme == choice).clicked() {
                            settings.theme = choice;
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
        }
        "Interface size" => {
            let current = settings.zoom.unwrap_or(1.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a small percentage"
            )]
            let percent = |z: f32| (z * 100.0).round() as u32;
            let options: Vec<(u32, String)> = ZOOMS
                .iter()
                .map(|z| (percent(*z), format!("{}%", percent(*z))))
                .collect();
            let now = percent(current);
            let labels: Vec<(u32, &str)> = options.iter().map(|(p, l)| (*p, l.as_str())).collect();
            ui.horizontal(|ui| {
                let chosen = widgets::segmented(ui, now, &labels);
                if chosen != now {
                    #[expect(clippy::cast_precision_loss, reason = "a small percentage")]
                    let z = chosen as f32 / 100.0;
                    settings.set_zoom(z);
                }
                // Zoomed to a size between these (Ctrl+scroll, Ctrl+plus/minus).
                if !labels.iter().any(|(p, _)| *p == now) {
                    ui.label(RichText::new(format!("{now}%")).color(theme::accent()));
                }
            });
            hint(
                ui,
                "Anywhere: Ctrl+scroll, or Ctrl+plus and Ctrl+minus; Ctrl+0 resets.",
            );
        }
        "Sidebar" => {
            let mut shown = !settings.hide_sidebar;
            ui.checkbox(&mut shown, "Show the sidebar");
            settings.hide_sidebar = !shown;
        }
        "Graph columns" => {
            let usual = settings.columns == crate::settings::Column::ALL
                && settings.column_widths.is_empty();
            if ui
                .add_enabled(!usual, egui::Button::new("Put them back as they were"))
                .clicked()
            {
                settings.columns = crate::settings::Column::ALL.to_vec();
                settings.column_widths.clear();
            }
            if usual {
                hint(ui, "In their usual order and widths.");
            }
        }
        "When both sides have new commits" => {
            for strategy in PullStrategy::ALL {
                let (title, about) = match strategy {
                    PullStrategy::FastForwardOnly => (
                        "Ask each time",
                        "Fast-forward when it can; otherwise ask whether to merge or rebase.",
                    ),
                    PullStrategy::FastForwardOrMerge => (
                        "Merge",
                        "Fast-forward when it can; otherwise a merge commit.",
                    ),
                    PullStrategy::Rebase => (
                        "Rebase my commits on top",
                        "Your commits replayed after the remote's.",
                    ),
                };
                let picked = settings.pull.strategy == strategy;
                if ui.radio(picked, title).on_hover_text(about).clicked() {
                    settings.pull.strategy = strategy;
                }
                if picked {
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        hint(ui, about);
                    });
                }
            }
        }
        "Uncommitted changes" => {
            // Throwing them away is offered when pulling, not as the usual
            // way (unless it already is).
            let mut options = vec![
                (PullChanges::Stash, "Put aside and back"),
                (PullChanges::Leave, "Leave them"),
            ];
            if settings.pull.changes == PullChanges::Discard {
                options.push((PullChanges::Discard, "Discard them (asks first)"));
            }
            ui.horizontal(|ui| {
                settings.pull.changes = widgets::segmented(ui, settings.pull.changes, &options);
            });
            hint(
                ui,
                match settings.pull.changes {
                    PullChanges::Stash => "Stashed before pulling and restored after.",
                    PullChanges::Leave => "Left as they are; Git stops if they're in the way.",
                    PullChanges::Discard => "Thrown away to the trash first, after asking.",
                },
            );
        }
        "Fetching" => {
            ui.checkbox(
                &mut settings.fetch_prune,
                "Remove remote branches deleted on the remote",
            );
            hint(ui, "Keeps the sidebar to branches that still exist.");
            ui.add_space(6.0);
            ui.checkbox(
                &mut settings.fetch_force,
                "Let fetched branches and tags replace local ones",
            );
            ui.label(
                RichText::new("Can drop tags you made here; asks first.")
                    .size(12.0)
                    .color(theme::modified()),
            );
        }
        "Diffs" => {
            ui.horizontal(|ui| {
                settings.split_diff = widgets::segmented(
                    ui,
                    settings.split_diff,
                    &[(false, "Unified"), (true, "Side by side")],
                );
            });
        }
        "Changed files" => {
            ui.horizontal(|ui| {
                settings.file_tree = widgets::segmented(
                    ui,
                    settings.file_tree,
                    &[(false, "By path"), (true, "In folders")],
                );
            });
        }
        "Signed in" => {
            if settings.accounts.is_empty() {
                hint(ui, "None yet.");
            }
            for (i, a) in settings.accounts.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(a.place()).strong());
                    ui.label(&a.login);
                    let how = if a.client.is_some() {
                        "browser"
                    } else {
                        "token"
                    };
                    ui.label(
                        RichText::new(format!("· signed in with a {how}"))
                            .size(12.0)
                            .color(theme::muted()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let out = ui.button("Sign out");
                        ui.ctx().accesskit_node_builder(out.id, |n| {
                            n.set_label(format!("Sign out of {} as {}", a.place(), a.login));
                        });
                        if out.clicked() {
                            *action = Some(Action::SignOut(i));
                        }
                    });
                });
            }
            ui.add_space(4.0);
            let add = ui.button(format!("{}  Add account…", icon::PLUS));
            ui.ctx()
                .accesskit_node_builder(add.id, |n| n.set_label("Add account…"));
            if add.clicked() {
                *action = Some(Action::AddAccount);
            }
        }
        "Terminal" => {
            let field = ui.add(
                egui::TextEdit::singleline(&mut settings.terminal)
                    .hint_text("The system's own")
                    .desired_width(360.0),
            );
            ui.ctx()
                .accesskit_node_builder(field.id, |n| n.set_label("Terminal command"));
            hint(
                ui,
                "For another terminal, its command, like alacritty --working-directory \"{path}\"; {path} is the repository's folder.",
            );
        }
        "Clone into" => {
            ui.horizontal(|ui| {
                let mut dir = settings.clone_dir.clone().unwrap_or_default();
                let field = ui.add(
                    egui::TextEdit::singleline(&mut dir)
                        .hint_text("Your home folder")
                        .desired_width(270.0),
                );
                ui.ctx()
                    .accesskit_node_builder(field.id, |n| n.set_label("Clone folder"));
                if field.changed() {
                    settings.clone_dir = (!dir.trim().is_empty()).then(|| dir.trim().to_string());
                }
                if ui.button("Browse…").clicked() {
                    *action = Some(Action::BrowseCloneDir);
                }
            });
        }
        "Updates" => {
            let mut check = !settings.no_update_checks;
            ui.checkbox(&mut check, "Check for updates automatically");
            settings.no_update_checks = !check;
            hint(
                ui,
                "When it starts, and every half hour while it's open; they download in the background.",
            );
        }
        "Recent repositories" => {
            let n = settings.recent.len();
            if ui
                .add_enabled(n > 0, egui::Button::new(format!("Clear the list ({n})")))
                .clicked()
            {
                *action = Some(Action::ClearRecent);
            }
        }
        "Version" => {
            ui.label(format!("Git Raccoon {}", env!("CARGO_PKG_VERSION")));
            if ui.link("What's new in this version").clicked() {
                *action = Some(Action::WhatsNew);
            }
        }
        "Feedback" => {
            if ui.button("Send feedback…").clicked() {
                *action = Some(Action::Feedback);
            }
        }
        "Licenses" => {
            if ui.button("Licenses…").clicked() {
                *action = Some(Action::Licenses);
            }
            hint(ui, "Git Raccoon's, and the libraries' it's built with.");
        }
        _ => {}
    }
}

/// A small preview of a look in the chosen mode (half dark, half light
/// when following the system), to pick it.
fn swatch(
    ui: &mut egui::Ui,
    choice: ThemeChoice,
    mode: ThemeMode,
    system: Option<egui::Theme>,
    chosen: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(136.0, 64.0), egui::Sense::click());
    let family = choice.family();
    let name = family.name().to_string();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, chosen, &name)
    });
    let look = crate::app::look_for(choice, mode, system);
    let (bg, raised, text, accent, lanes) = look.preview();
    let painter = ui.painter();
    painter.rect_filled(rect, 6, bg);
    if mode == ThemeMode::System {
        // The other half in the other version.
        let (other_bg, ..) = family.look(!look.light).preview();
        let (_, right) = rect.split_left_right_at_fraction(0.5);
        painter.rect_filled(
            right,
            egui::CornerRadius {
                nw: 0,
                sw: 0,
                ne: 6,
                se: 6,
            },
            other_bg,
        );
    }
    let strip = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 18.0));
    painter.rect_filled(
        strip,
        egui::CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        },
        raised,
    );
    for (i, c) in lanes.iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "four dots")]
        let x = rect.left() + 14.0 + i as f32 * 12.0;
        painter.circle_filled(egui::pos2(x, strip.center().y), 4.0, *c);
    }
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.right() - 34.0, strip.center().y - 4.0),
            egui::vec2(22.0, 8.0),
        ),
        3,
        accent,
    );
    let label = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 26.0), rect.max);
    if mode == ThemeMode::System {
        painter.rect_filled(label.shrink2(egui::vec2(24.0, 3.0)), 4, bg);
    }
    painter.text(
        label.center(),
        egui::Align2::CENTER_CENTER,
        &name,
        theme::body(13.0),
        text,
    );
    let stroke = if chosen {
        egui::Stroke::new(2.0, theme::accent())
    } else if response.hovered() {
        egui::Stroke::new(1.0, theme::muted())
    } else {
        egui::Stroke::new(1.0, theme::border())
    };
    painter.rect_stroke(rect, 6, stroke, egui::StrokeKind::Inside);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_has_a_control_and_search_finds_it() {
        let names: Vec<&str> = ROWS.iter().map(|r| r.name).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "names are unique");
        let find = |q: &str| -> Vec<&str> {
            ROWS.iter()
                .filter(|r| r.matches(q))
                .map(|r| r.name)
                .collect()
        };
        assert_eq!(find("zoom"), ["Interface size"]);
        assert_eq!(find("REBASE"), ["When both sides have new commits"]);
        assert!(find("dark").contains(&"Light or dark"));
        assert!(find("github").contains(&"Signed in"));
        assert_eq!(find("terminal command"), ["Terminal"]);
        assert!(find("no such thing").is_empty());
        // Searching a section's name finds its settings.
        assert_eq!(find("tools"), ["Terminal", "Clone into"]);
    }
}
