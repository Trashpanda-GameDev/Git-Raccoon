//! The settings window: sections down the left (Appearance, Git,
//! Accounts, Tools, Updates and privacy, About), a search over all of
//! them, and each setting on a row of its own: its name and what it does
//! on the left, its control on the right. Changes apply straight away.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use crate::settings::{
    LineStyle, PullChanges, PullStrategy, Settings, ThemeChoice, ThemeMode, ZOOMS,
};
use crate::theme;
use crate::themes::{CustomTheme, Field, Hex};
use crate::ui::widgets;

pub enum Action {
    Close,
    /// Add a theme from a file.
    ImportTheme,
    /// Save this theme (by name) as a file.
    ExportTheme(String),
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
                settings.branch_lines = d.branch_lines;
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
        name: "Where a branch starts",
        hint: "The line from a branch's first commit into the commit it started from.",
        words: "graph branch lines curves curved rounded start fork out",
    },
    Row {
        section: Section::Appearance,
        name: "Where a branch is merged",
        hint: "The line from a merge commit out to the branch it merges.",
        words: "graph branch lines curves curved rounded merge in",
    },
    Row {
        section: Section::Appearance,

        name: "Corner roundness",

        hint: "For Rounded lines: 0 makes square corners.",

        words: "graph branch lines corners round radius square curves",
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
    /// The custom theme being edited (by name), and its name as typed.
    editing: Option<String>,
    name: String,
    /// Editing its light version (otherwise its dark one).
    light: bool,
    /// Deleting it was asked for once: the button now says to confirm.
    deleting: bool,
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
                        if !view.search.trim().is_empty() {
                            let query = view.search.clone();
                            results(ui, settings, &query, &mut view, &mut action);
                        } else if view.editing.is_some() {
                            editor(ui, settings, &mut view, &mut action);
                        } else {
                            let section = view.section;
                            page(ui, settings, section, &mut view, &mut action);
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

/// The themes made here, under the built-in looks: each to pick (or
/// edit), and a new one, or one from a file.
fn your_themes(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    view: &mut View,
    action: &mut Option<Action>,
) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Your themes").strong().size(13.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Import…").clicked() {
                *action = Some(Action::ImportTheme);
            }
        });
    });
    ui.add_space(4.0);
    let light = crate::app::look_for(settings.theme, settings.theme_mode, ui.ctx().system_theme())
        .is_light();
    let mut pick = None;
    egui::Grid::new("your-themes")
        .spacing(egui::vec2(8.0, 8.0))
        .show(ui, |ui| {
            for (i, t) in settings.themes.iter().enumerate() {
                ui.vertical(|ui| {
                    let chosen = settings.custom_theme.as_deref() == Some(t.name.as_str());
                    let colours = t.colours(light);
                    if custom_swatch(ui, &t.name, colours, chosen).clicked() {
                        pick = Some(t.name.clone());
                    }
                    let edit = ui.small_button(format!("{}  Edit", icon::PENCIL_SIMPLE));
                    let name = format!("Edit {}", t.name);
                    ui.ctx()
                        .accesskit_node_builder(edit.id, |n| n.set_label(name));
                    if edit.clicked() {
                        open_editor(view, &t.name, light);
                    }
                });
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
            // A new one, from the look in use.
            let (rect, new) = ui.allocate_exact_size(egui::vec2(136.0, 64.0), egui::Sense::click());
            new.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "New theme")
            });
            let stroke = if new.hovered() {
                theme::text()
            } else {
                theme::muted()
            };
            ui.painter().rect_stroke(
                rect,
                6,
                egui::Stroke::new(1.0, stroke),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("{}  New theme", icon::PLUS),
                theme::body(13.0),
                stroke,
            );
            if new.on_hover_text("Starts from the look in use").clicked() {
                let base = settings.current_theme().map_or(settings.theme, |t| t.base);
                let mut made = settings
                    .current_theme()
                    .cloned()
                    .unwrap_or_else(|| CustomTheme::starting_from("", base));
                made.name = crate::themes::unique_name("My theme", &settings.themes);
                let name = made.name.clone();
                settings.themes.push(made);
                pick = Some(name.clone());
                open_editor(view, &name, light);
            }
        });
    if let Some(name) = pick {
        settings.custom_theme = Some(name);
    }
}

fn open_editor(view: &mut View, name: &str, light: bool) {
    view.editing = Some(name.to_string());
    view.name = name.to_string();
    view.light = light;
    view.deleting = false;
}

/// A custom theme's swatch: its colours, with its name.
fn custom_swatch(
    ui: &mut egui::Ui,
    name: &str,
    c: &crate::themes::Colours,
    chosen: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(136.0, 64.0), egui::Sense::click());
    let label = name.to_string();
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, chosen, &label)
    });
    let painter = ui.painter();
    painter.rect_filled(rect, 6, c.background.0);
    let strip = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 18.0));
    painter.rect_filled(
        strip,
        egui::CornerRadius {
            nw: 6,
            ne: 6,
            sw: 0,
            se: 0,
        },
        c.panels.0,
    );
    for (i, lane) in c.lanes.iter().take(4).enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "four dots")]
        let x = rect.left() + 14.0 + i as f32 * 12.0;
        painter.circle_filled(egui::pos2(x, strip.center().y), 4.0, lane.0);
    }
    painter.rect_filled(
        egui::Rect::from_min_size(
            egui::pos2(rect.right() - 34.0, strip.center().y - 4.0),
            egui::vec2(22.0, 8.0),
        ),
        3,
        c.accent.0,
    );
    let text = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 26.0), rect.max);
    let shown = widgets::elided(
        painter,
        name,
        theme::body(13.0),
        c.text.0,
        rect.width() - 16.0,
    );
    painter.galley(
        egui::pos2(
            text.center().x - shown.size().x / 2.0,
            text.center().y - shown.size().y / 2.0,
        ),
        shown,
        c.text.0,
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

/// Editing a custom theme: its name, which version, its colours (with
/// warnings where text would be hard to read), and a preview.
fn editor(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    view: &mut View,
    action: &mut Option<Action>,
) {
    let Some(index) = view
        .editing
        .as_deref()
        .and_then(|name| settings.themes.iter().position(|t| t.name == name))
    else {
        view.editing = None;
        return;
    };
    // Back, and its name.
    ui.horizontal(|ui| {
        let back = ui.button(format!("{}  Appearance", icon::CARET_LEFT));
        ui.ctx()
            .accesskit_node_builder(back.id, |n| n.set_label("Back to Appearance"));
        if back.clicked() {
            view.editing = None;
        }
        ui.add_space(8.0);
        let field = ui.add(
            egui::TextEdit::singleline(&mut view.name)
                .desired_width(240.0)
                .font(theme::body(16.0)),
        );
        ui.ctx()
            .accesskit_node_builder(field.id, |n| n.set_label("Theme name"));
        let wanted = view.name.trim().to_string();
        let old = settings.themes[index].name.clone();
        let taken = settings
            .themes
            .iter()
            .any(|t| t.name == wanted && t.name != old);
        if field.changed() && !wanted.is_empty() && !taken {
            settings.themes[index].name = wanted.clone();
            if settings.custom_theme.as_deref() == Some(old.as_str()) {
                settings.custom_theme = Some(wanted.clone());
            }
            view.editing = Some(wanted);
        }
        if taken {
            ui.label(
                RichText::new("Another theme has that name")
                    .size(12.0)
                    .color(theme::danger()),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Clear of the scrollbar.
            ui.add_space(14.0);
            let name = settings.themes[index].name.clone();
            if settings.custom_theme.as_deref() == Some(name.as_str()) {
                ui.label(
                    RichText::new("In use: changes show as you make them")
                        .size(12.0)
                        .color(theme::muted()),
                );
            } else if ui.button("Use this theme").clicked() {
                settings.custom_theme = Some(name);
            }
        });
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Editing the").color(theme::muted()));
        view.light = widgets::segmented(ui, view.light, &[(false, "Dark"), (true, "Light")]);
        ui.label(RichText::new("version").color(theme::muted()));
    });
    let light = view.light;
    // Text that would be hard to read, and the fix, where it's seen.
    {
        // Text that would be hard to read, and the fix.
        let colours = settings.themes[index].colours_mut(light);
        let problems = colours.problems();
        for problem in &problems {
            ui.add_space(6.0);
            egui::Frame::new()
                .fill(theme::bg_raised())
                .stroke(egui::Stroke::new(1.0, theme::modified()))
                .corner_radius(6)
                .inner_margin(8)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{}  {} is {:.1}:1, hard to read.",
                                icon::WARNING,
                                problem.what,
                                problem.ratio
                            ))
                            .color(theme::modified()),
                        );
                        let fix = ui.link("Make it readable");
                        let name = format!("Make {} readable", problem.what.to_lowercase());
                        ui.ctx()
                            .accesskit_node_builder(fix.id, |n| n.set_label(name));
                        if fix.clicked() {
                            colours.fix(problem);
                        }
                    });
                });
        }
    }
    ui.add_space(6.0);
    ui.horizontal_top(|ui| {
        // The colours.
        ui.vertical(|ui| {
            ui.set_width(330.0);
            let colours = settings.themes[index].colours_mut(light);
            for (group, fields) in Field::GROUPS {
                ui.add_space(6.0);
                ui.label(widgets::section_label(group));
                for &field in fields {
                    colour_row(
                        ui,
                        field.name(),
                        colours.get_mut(field),
                        (index, light, field.name()),
                    );
                }
            }
            ui.add_space(6.0);
            ui.label(widgets::section_label("Graph lines"));
            ui.horizontal_wrapped(|ui| {
                for (i, lane) in colours.lanes.iter_mut().enumerate() {
                    let button = egui::widgets::color_picker::color_edit_button_srgba(
                        ui,
                        &mut lane.0,
                        egui::widgets::color_picker::Alpha::Opaque,
                    );
                    let name = format!("Graph line {}", i + 1);
                    ui.ctx()
                        .accesskit_node_builder(button.id, |n| n.set_label(name));
                }
            });
        });
        ui.add_space(16.0);
        // How it looks.
        ui.vertical(|ui| {
            ui.label(widgets::section_label("Preview"));
            preview(ui, settings.themes[index].colours(light));
        });
    });
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        let name = settings.themes[index].name.clone();
        if view.deleting {
            let sure = ui.add(
                egui::Button::new(
                    RichText::new(format!("Delete {name}")).color(theme::on_accent()),
                )
                .fill(theme::danger().gamma_multiply(0.8)),
            );
            if sure.clicked() {
                settings.themes.remove(index);
                if settings.custom_theme.as_deref() == Some(name.as_str()) {
                    settings.custom_theme = None;
                }
                view.editing = None;
                return;
            }
            if ui.button("Keep it").clicked() {
                view.deleting = false;
            }
        } else if ui
            .add(
                egui::Button::new(RichText::new("Delete theme…").color(theme::danger()))
                    .frame(false),
            )
            .clicked()
        {
            view.deleting = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let done = egui::Button::new(RichText::new("Done").color(theme::on_accent()).strong())
                .fill(theme::accent());
            if ui.add(done).clicked() {
                view.editing = None;
            }
            if ui.button("Export…").clicked() {
                *action = Some(Action::ExportTheme(name.clone()));
            }
            if ui.button("Duplicate").clicked() {
                let mut copy = settings.themes[index].clone();
                copy.name = crate::themes::unique_name(&format!("{name} copy"), &settings.themes);
                let copy_name = copy.name.clone();
                settings.themes.push(copy);
                open_editor(view, &copy_name, light);
            }
        });
    });
}

/// One colour: its name, a picker, and its "#rrggbb" to type.
fn colour_row(ui: &mut egui::Ui, name: &str, colour: &mut Hex, key: (usize, bool, &str)) {
    ui.horizontal(|ui| {
        // A column of names, so the colours line up.
        let (cell, _) = ui.allocate_exact_size(egui::vec2(130.0, 22.0), egui::Sense::hover());
        ui.painter().text(
            cell.left_center(),
            egui::Align2::LEFT_CENTER,
            name,
            theme::body(theme::FONT_SIZE),
            theme::text(),
        );
        let button = egui::widgets::color_picker::color_edit_button_srgba(
            ui,
            &mut colour.0,
            egui::widgets::color_picker::Alpha::Opaque,
        );
        ui.ctx()
            .accesskit_node_builder(button.id, |n| n.set_label(format!("{name} colour picker")));
        // The text being typed is kept while it isn't a colour yet.
        let id = egui::Id::new(("theme-hex", key));
        let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| colour.text());
        let field = ui.add(
            egui::TextEdit::singleline(&mut text)
                .desired_width(80.0)
                .font(theme::mono(12.0)),
        );
        ui.ctx()
            .accesskit_node_builder(field.id, |n| n.set_label(format!("{name} colour")));
        if field.changed()
            && let Some(hex) = Hex::parse(&text)
        {
            *colour = hex;
        }
        if field.has_focus() {
            ui.data_mut(|d| d.insert_temp(id, text));
        } else {
            ui.data_mut(|d| d.remove::<String>(id));
        }
    });
}

/// A small picture of the app in these colours: a toolbar, some of the
/// graph with a selected row, and a diff.
fn preview(ui: &mut egui::Ui, c: &crate::themes::Colours) {
    let width = ui.available_width().max(200.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 300.0), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 8, c.background.0);
    p.rect_stroke(
        rect,
        8,
        egui::Stroke::new(1.0, c.borders.0),
        egui::StrokeKind::Inside,
    );
    // Toolbar.
    let bar = egui::Rect::from_min_size(rect.min, egui::vec2(width, 34.0));
    p.rect_filled(
        bar,
        egui::CornerRadius {
            nw: 8,
            ne: 8,
            sw: 0,
            se: 0,
        },
        c.panels.0,
    );
    let pill = egui::Rect::from_min_size(bar.min + egui::vec2(10.0, 8.0), egui::vec2(64.0, 18.0));
    p.rect_filled(pill, 4, c.accent.0);
    p.text(
        pill.center(),
        egui::Align2::CENTER_CENTER,
        format!("{} main", icon::CHECK),
        theme::body(12.0),
        c.on_accent.0,
    );
    p.text(
        pill.right_center() + egui::vec2(12.0, 0.0),
        egui::Align2::LEFT_CENTER,
        "Fetch  Pull  Push",
        theme::body(12.0),
        c.secondary_text.0,
    );
    // Graph rows.
    let rows = [
        ("Pipeline cache", "2 days ago", 1),
        ("Faster start", "3 days ago", 0),
        ("Add icon", "6 days ago", 0),
        ("Lay out commit lanes", "2 weeks ago", 2),
    ];
    for (i, (message, when, lane)) in rows.iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "four rows")]
        let top = bar.bottom() + 4.0 + i as f32 * 28.0;
        let row = egui::Rect::from_min_size(egui::pos2(rect.left(), top), egui::vec2(width, 28.0));
        if i == 1 {
            p.rect_filled(row, 0, c.selected.0);
        }
        let colour = c.lanes.get(*lane).map_or(c.accent.0, |h| h.0);
        #[expect(clippy::cast_precision_loss, reason = "a few lanes")]
        let x = rect.left() + 18.0 + *lane as f32 * 14.0;
        p.circle_stroke(
            egui::pos2(x, row.center().y),
            5.0,
            egui::Stroke::new(2.5, colour),
        );
        p.text(
            egui::pos2(rect.left() + 70.0, row.center().y),
            egui::Align2::LEFT_CENTER,
            message,
            theme::body(13.0),
            c.text.0,
        );
        p.text(
            egui::pos2(rect.right() - 10.0, row.center().y),
            egui::Align2::RIGHT_CENTER,
            when,
            theme::body(12.0),
            c.secondary_text.0,
        );
    }
    // A diff.
    let top = bar.bottom() + 4.0 + 4.0 * 28.0 + 10.0;
    let lines = [
        (" let repo = Repo::discover(path)?;", None, c.text.0),
        ("-Ok(repo)", Some(c.deleted.0), c.text.0),
        (
            "+if repo.is_bare() { return Err(Bare); }",
            Some(c.added.0),
            c.text.0,
        ),
        ("+Ok(repo)", Some(c.added.0), c.text.0),
    ];
    for (i, (text, tint, colour)) in lines.iter().enumerate() {
        #[expect(clippy::cast_precision_loss, reason = "four lines")]
        let y = top + i as f32 * 22.0;
        let line = egui::Rect::from_min_size(
            egui::pos2(rect.left() + 8.0, y),
            egui::vec2(width - 16.0, 22.0),
        );
        if let Some(tint) = tint {
            p.rect_filled(
                line,
                0,
                egui::Color32::from_rgba_unmultiplied(tint.r(), tint.g(), tint.b(), 40),
            );
        }
        p.text(
            line.left_center() + egui::vec2(6.0, 0.0),
            egui::Align2::LEFT_CENTER,
            *text,
            theme::mono(12.0),
            *colour,
        );
    }
}

/// A choice of how a line looks: a branch starting (or, for `merge`, a
/// merge) drawn that way, with its name.
fn line_tile(
    ui: &mut egui::Ui,
    style: LineStyle,
    merge: bool,
    chosen: bool,
    corner: f32,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(96.0, 76.0), egui::Sense::click());
    let kind = if merge { "merges" } else { "branch starts" };
    let name = format!("{} {kind}", style.name());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::RadioButton, true, chosen, &name)
    });
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6, theme::bg());
    let (left, right) = (rect.left() + 34.0, rect.left() + 62.0);
    let (top, upper, lower, bottom) = (
        rect.top() + 6.0,
        rect.top() + 16.0,
        rect.top() + 42.0,
        rect.top() + 54.0,
    );
    let main = egui::Stroke::new(2.0, theme::lane_color(0));
    let side = egui::Stroke::new(2.0, theme::lane_color(1));
    painter.line_segment([egui::pos2(left, top), egui::pos2(left, bottom)], main);
    let dots = if merge {
        // A merge on the main line, and the branch it brings in.
        painter.line_segment([egui::pos2(right, lower), egui::pos2(right, bottom)], side);
        crate::ui::history::branch_line(
            &painter,
            egui::pos2(left, upper),
            egui::pos2(right, lower),
            (style, corner),
            side,
            false,
            true,
        );
        [(left, upper, main.color), (right, lower, side.color)]
    } else {
        // A branch's first commit, and the commit it started from.
        painter.line_segment([egui::pos2(right, top), egui::pos2(right, upper)], side);
        crate::ui::history::branch_line(
            &painter,
            egui::pos2(right, upper),
            egui::pos2(left, lower),
            (style, corner),
            side,
            false,
            false,
        );
        [(right, upper, side.color), (left, lower, main.color)]
    };
    for (x, y, colour) in dots {
        painter.circle_filled(egui::pos2(x, y), 3.5, theme::bg());
        painter.circle_stroke(egui::pos2(x, y), 3.5, egui::Stroke::new(2.0, colour));
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 11.0),
        egui::Align2::CENTER_CENTER,
        style.name(),
        theme::body(12.0),
        theme::text(),
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
fn page(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    section: Section,
    view: &mut View,
    action: &mut Option<Action>,
) {
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
        draw(ui, settings, row, view, action);
    }
}

/// The settings that match a search, from every section.
fn results(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    query: &str,
    view: &mut View,
    action: &mut Option<Action>,
) {
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
        draw(ui, settings, row, view, action);
    }
}

/// One setting's row: its name and what it does, then its control.
fn draw(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    row: &Row,
    view: &mut View,
    action: &mut Option<Action>,
) {
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
        ui.vertical(|ui| control(ui, settings, row.name, view, action));
    });
    ui.add_space(10.0);
}

fn hint(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(text.into()).size(12.0).color(theme::muted())).wrap());
}

/// A setting's control.
fn control(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    name: &str,
    view: &mut View,
    action: &mut Option<Action>,
) {
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
                        let chosen = settings.theme == choice && settings.current_theme().is_none();
                        if swatch(ui, choice, mode, system, chosen).clicked() {
                            settings.theme = choice;
                            settings.custom_theme = None;
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
            your_themes(ui, settings, view, action);
        }
        "Where a branch starts" => {
            ui.horizontal(|ui| {
                for style in LineStyle::ALL {
                    let chosen = settings.branch_lines.starts == style;
                    if line_tile(ui, style, false, chosen, settings.branch_lines.corner).clicked() {
                        settings.branch_lines.starts = style;
                    }
                }
            });
        }
        "Where a branch is merged" => {
            ui.horizontal(|ui| {
                for style in LineStyle::ALL {
                    let chosen = settings.branch_lines.merges == style;
                    if line_tile(ui, style, true, chosen, settings.branch_lines.corner).clicked() {
                        settings.branch_lines.merges = style;
                    }
                }
            });
        }
        "Corner roundness" => {
            let rounded = settings.branch_lines.starts == LineStyle::Rounded
                || settings.branch_lines.merges == LineStyle::Rounded;
            let slider = ui.add_enabled(
                rounded,
                egui::Slider::new(
                    &mut settings.branch_lines.corner,
                    crate::settings::BranchLines::CORNERS,
                )
                .step_by(1.0)
                .fixed_decimals(0)
                .suffix(" px"),
            );
            ui.ctx()
                .accesskit_node_builder(slider.id, |n| n.set_label("Corner roundness"));
            if !rounded {
                hint(ui, "Choose Rounded above to use it.");
            }
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
