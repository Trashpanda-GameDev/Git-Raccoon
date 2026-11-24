//! Profiles in the window: the switcher at the top right, and the editor
//! in Settings › Profiles.

use eframe::egui::{self, Color32, RichText, Sense};
use egui_phosphor::regular as icon;

use crate::forge::Account;
use crate::profiles::Profile;
use crate::settings::Settings;
use crate::theme;
use crate::ui::widgets;

/// A profile's colours: its name, the badge's fill and the letter on it.
pub const COLOURS: [(&str, Color32, Color32); 5] = [
    (
        "Blue",
        Color32::from_rgb(0x6f, 0xa8, 0xff),
        Color32::from_rgb(0x0b, 0x12, 0x20),
    ),
    (
        "Green",
        Color32::from_rgb(0x8f, 0xc9, 0x7a),
        Color32::from_rgb(0x10, 0x20, 0x0b),
    ),
    (
        "Orange",
        Color32::from_rgb(0xe8, 0x87, 0x6a),
        Color32::from_rgb(0x2a, 0x0f, 0x06),
    ),
    (
        "Purple",
        Color32::from_rgb(0xc7, 0x9b, 0xff),
        Color32::from_rgb(0x1a, 0x0b, 0x2e),
    ),
    (
        "Yellow",
        Color32::from_rgb(0xf0, 0xc0, 0x60),
        Color32::from_rgb(0x26, 0x1a, 0x00),
    ),
];

fn colours(p: &Profile) -> (Color32, Color32) {
    let (_, fill, ink) = COLOURS[p.colour % COLOURS.len()];
    (fill, ink)
}

/// Paints the profile's round badge, its colour with its initial, `size`
/// across, centred on `at`.
pub fn paint_badge(painter: &egui::Painter, at: egui::Pos2, p: &Profile, size: f32) {
    let (fill, ink) = colours(p);
    painter.circle_filled(at, size / 2.0, fill);
    painter.text(
        at,
        egui::Align2::CENTER_CENTER,
        p.initial(),
        egui::FontId::new(size * 0.46, theme::bold()),
        ink,
    );
}

/// "GitHub · rocky-acme".
pub fn account_name(a: &Account) -> String {
    format!("{} · {}", a.place(), a.login)
}

/// What the switcher asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Use(usize),
    New,
    Manage,
}

/// The chip at the top right: the profile in use (its badge and name, or
/// just the badge when the toolbar is short of room), opening the list to
/// switch between them.
pub fn switcher(ui: &mut egui::Ui, settings: &Settings, compact: bool) -> Option<Pick> {
    let active = settings.profile()?;
    let mut pick = None;
    let text = if compact {
        String::new()
    } else {
        format!("{}  {}", active.name, icon::CARET_DOWN)
    };
    let galley = ui
        .painter()
        .layout_no_wrap(text, egui::FontId::proportional(12.5), theme::text());
    let size = egui::vec2(
        28.0 + galley.size().x + if compact { 0.0 } else { 8.0 },
        26.0,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let response = response.on_hover_text(format!(
        "Committing as {} ({}). Click to switch profile",
        active.name,
        active.who()
    ));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("Profile: {}", active.name),
        )
    });
    let fill = if response.hovered() || egui::Popup::is_id_open(ui.ctx(), popup_id()) {
        theme::bg_hover()
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 13, fill);
    paint_badge(
        ui.painter(),
        egui::pos2(rect.left() + 13.0, rect.center().y),
        active,
        20.0,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 28.0, rect.center().y - galley.size().y / 2.0),
        galley,
        theme::text(),
    );
    egui::Popup::menu(&response)
        .id(popup_id())
        .width(330.0)
        .show(|ui| {
            ui.label(widgets::section_label("COMMITTING AS"));
            ui.add_space(2.0);
            for (i, p) in settings.profiles.iter().enumerate() {
                if profile_row(ui, settings, p, i == settings.active_profile).clicked() {
                    pick = Some(Pick::Use(i));
                    ui.close();
                }
            }
            ui.add(
                egui::Label::new(
                    RichText::new(
                        "New commits use this name and email. Push, pull and merge requests use its accounts.",
                    )
                    .size(11.5)
                    .color(theme::muted()),
                )
                .wrap(),
            );
            ui.separator();
            if ui.button(format!("{}  New profile…", icon::PLUS)).clicked() {
                pick = Some(Pick::New);
                ui.close();
            }
            if ui
                .button(format!("{}  Manage profiles…", icon::GEAR_SIX))
                .clicked()
            {
                pick = Some(Pick::Manage);
                ui.close();
            }
        });
    pick
}

fn popup_id() -> egui::Id {
    egui::Id::new("profile-switcher")
}

/// One profile in the switcher: its badge, name, who it commits as and its
/// accounts; the one in use is ticked.
fn profile_row(
    ui: &mut egui::Ui,
    settings: &Settings,
    p: &Profile,
    active: bool,
) -> egui::Response {
    let accounts: Vec<String> = settings
        .accounts
        .iter()
        .filter(|a| p.uses(a))
        .map(account_name)
        .collect();
    // Laid out first, so the row is as tall as its text (an account per
    // line, a long one wrapped).
    let width = (ui.available_width() - 38.0 - 26.0).max(80.0);
    let layout = |text: String, size: f32, colour: Color32| {
        ui.painter()
            .layout(text, egui::FontId::proportional(size), colour, width)
    };
    let mut galleys = vec![
        layout(p.name.clone(), 13.5, theme::text()),
        layout(p.who(), 11.5, theme::muted()),
    ];
    galleys.extend(
        accounts
            .into_iter()
            .map(|a| layout(a, 11.5, theme::muted())),
    );
    const GAP: f32 = 2.0;
    let text_height: f32 = galleys.iter().map(|g| g.size().y + GAP).sum();
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), (text_height + 12.0).max(36.0)),
        Sense::click(),
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Button, true, active, &p.name)
    });
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, 6, theme::bg_selected());
    } else if response.hovered() {
        painter.rect_filled(rect, 6, theme::bg_hover());
    }
    paint_badge(
        painter,
        egui::pos2(rect.left() + 18.0, rect.top() + 18.0),
        p,
        24.0,
    );
    let x = rect.left() + 38.0;
    let mut y = rect.top() + 6.0;
    for g in galleys {
        let h = g.size().y;
        painter.galley(egui::pos2(x, y), g, theme::text());
        y += h + GAP;
    }
    if active {
        painter.text(
            egui::pos2(rect.right() - 12.0, rect.top() + 18.0),
            egui::Align2::CENTER_CENTER,
            icon::CHECK,
            egui::FontId::proportional(14.0),
            theme::accent(),
        );
    }
    response
}

/// What the editor asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    /// The profile in use, or its accounts, changed.
    Changed,
    /// Sign in to another account, for this profile.
    SignInFor(usize),
}

/// Which profile the editor shows, and whether deleting it was asked once.
#[derive(Debug, Clone, Copy, Default)]
pub struct Editing {
    pub profile: usize,
    pub deleting: bool,
}

/// Settings › Profiles: the list of profiles, and the chosen one's name,
/// colour, who it commits as and its accounts.
pub fn editor(ui: &mut egui::Ui, settings: &mut Settings, editing: &mut Editing) -> Option<Edit> {
    let mut edit = None;
    if settings.profiles.is_empty() {
        return None;
    }
    editing.profile = editing.profile.min(settings.profiles.len() - 1);
    ui.horizontal_top(|ui| {
        // The list.
        ui.vertical(|ui| {
            ui.set_width(190.0);
            ui.spacing_mut().item_spacing.y = 4.0;
            for i in 0..settings.profiles.len() {
                let p = &settings.profiles[i];
                let chosen = i == editing.profile;
                let status = if i == settings.active_profile {
                    "In use".to_string()
                } else if p.email.trim().is_empty() {
                    "No email yet".to_string()
                } else {
                    p.email.clone()
                };
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(190.0, 44.0), Sense::click());
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::WidgetType::Button, true, chosen, &p.name)
                });
                let painter = ui.painter();
                if chosen {
                    painter.rect_filled(rect, 8, theme::bg_selected());
                } else if response.hovered() {
                    painter.rect_filled(rect, 8, theme::bg_hover());
                }
                paint_badge(
                    painter,
                    egui::pos2(rect.left() + 22.0, rect.center().y),
                    p,
                    28.0,
                );
                let x = rect.left() + 44.0;
                let width = rect.right() - x - 6.0;
                widgets::text_in(
                    painter,
                    egui::Rect::from_min_max(
                        egui::pos2(x, rect.top() + 4.0),
                        egui::pos2(x + width, rect.center().y + 1.0),
                    ),
                    &p.name,
                    theme::body(13.5),
                    theme::text(),
                );
                widgets::text_in(
                    painter,
                    egui::Rect::from_min_max(
                        egui::pos2(x, rect.center().y + 1.0),
                        egui::pos2(x + width, rect.bottom() - 4.0),
                    ),
                    &status,
                    theme::body(11.0),
                    theme::muted(),
                );
                if response.clicked() {
                    editing.profile = i;
                    editing.deleting = false;
                }
            }
            let new = ui.add(
                egui::Button::new(format!("{}  New profile", icon::PLUS))
                    .min_size(egui::vec2(190.0, 32.0)),
            );
            if new.clicked() {
                editing.profile = new_profile(settings, None);
                editing.deleting = false;
            }
        });
        ui.add_space(16.0);
        // The chosen one.
        ui.vertical(|ui| {
            if let Some(e) = details(ui, settings, editing) {
                edit = Some(e);
            }
        });
    });
    edit
}

/// Adds a profile (a copy of `from`, or an empty one) and returns where.
pub fn new_profile(settings: &mut Settings, from: Option<usize>) -> usize {
    let mut p = match from.and_then(|i| settings.profiles.get(i)) {
        Some(p) => {
            let mut copy = p.clone();
            copy.name = crate::profiles::free_name(&settings.profiles, &format!("{} copy", p.name));
            copy
        }
        None => Profile {
            name: crate::profiles::free_name(&settings.profiles, "New profile"),
            ..Profile::default()
        },
    };
    p.colour = settings.profiles.len() % COLOURS.len();
    settings.profiles.push(p);
    settings.profiles.len() - 1
}

fn field(ui: &mut egui::Ui, label: &str, text: &mut String, hint: &str, width: f32) -> bool {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(12.0).color(theme::muted()));
        let response = ui.add(
            egui::TextEdit::singleline(text)
                .hint_text(hint)
                .desired_width(width),
        );
        ui.ctx()
            .accesskit_node_builder(response.id, |n| n.set_label(label));
        response.changed()
    })
    .inner
}

fn details(ui: &mut egui::Ui, settings: &mut Settings, editing: &mut Editing) -> Option<Edit> {
    let mut edit = None;
    let i = editing.profile;
    let width = ui.available_width().clamp(260.0, 520.0);
    ui.set_width(width);
    ui.spacing_mut().item_spacing.y = 8.0;
    let in_use = i == settings.active_profile;
    {
        let p = &mut settings.profiles[i];
        ui.horizontal(|ui| {
            field(ui, "Profile name", &mut p.name, "Work", width - 170.0);
            ui.vertical(|ui| {
                ui.label(RichText::new("Colour").size(12.0).color(theme::muted()));
                ui.horizontal(|ui| {
                    for (c, (name, fill, _)) in COLOURS.iter().enumerate() {
                        let (rect, dot) =
                            ui.allocate_exact_size(egui::vec2(24.0, 24.0), Sense::click());
                        dot.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::RadioButton,
                                true,
                                p.colour == c,
                                *name,
                            )
                        });
                        ui.painter().circle_filled(rect.center(), 10.0, *fill);
                        if p.colour == c {
                            ui.painter().circle_stroke(
                                rect.center(),
                                11.5,
                                egui::Stroke::new(2.0, theme::text()),
                            );
                        }
                        if dot.on_hover_text(*name).clicked() {
                            p.colour = c;
                        }
                    }
                });
            });
        });
        ui.add_space(4.0);
        ui.label(widgets::section_label("COMMIT AS"));
        let (git_name, git_email) = crate::profiles::git_identity();
        ui.horizontal(|ui| {
            let half = (width - 12.0) / 2.0;
            let name_hint = if git_name.is_empty() {
                "Your name"
            } else {
                &git_name
            };
            let email_hint = if git_email.is_empty() {
                "you@example.com"
            } else {
                &git_email
            };
            field(ui, "Name", &mut p.commit_name, name_hint, half - 8.0);
            field(ui, "Email", &mut p.email, email_hint, half);
        });
        if p.identity().is_none() {
            ui.label(
                RichText::new("Without both, commits use Git's own name and email.")
                    .size(11.5)
                    .color(theme::muted()),
            );
        }
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(widgets::section_label("ACCOUNTS"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("One per forge; ticking another swaps it")
                    .size(11.5)
                    .color(theme::muted()),
            );
        });
    });
    if settings.accounts.is_empty() {
        ui.label(
            RichText::new("No accounts signed in yet.")
                .size(12.0)
                .color(theme::muted()),
        );
    }
    for a in settings.accounts.clone() {
        let mut on = settings.profiles[i].uses(&a);
        let others: Vec<String> = settings
            .profiles
            .iter()
            .enumerate()
            .filter(|(j, p)| *j != i && p.uses(&a))
            .map(|(_, p)| p.name.clone())
            .collect();
        ui.horizontal(|ui| {
            let tick = ui.checkbox(&mut on, RichText::new(a.place()).strong());
            ui.ctx().accesskit_node_builder(tick.id, |n| {
                n.set_label(format!("Use {} in this profile", account_name(&a)));
            });
            let mut detail = a.login.clone();
            if !others.is_empty() {
                detail.push_str(&format!(" · in {}", others.join(", ")));
            }
            ui.label(RichText::new(detail).size(12.0).color(theme::muted()));
            if tick.changed() {
                let p = &mut settings.profiles[i];
                if on {
                    p.take_account(&a);
                } else {
                    p.drop_account(&a);
                }
                if in_use {
                    edit = Some(Edit::Changed);
                }
            }
        });
    }
    ui.horizontal(|ui| {
        if ui.button("Sign in to another account…").clicked() {
            edit = Some(Edit::SignInFor(i));
        }
        ui.label(
            RichText::new("It's added to this profile.")
                .size(11.5)
                .color(theme::muted()),
        );
    });
    ui.add_space(6.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Saved as you type")
                .size(11.5)
                .color(theme::muted()),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !in_use
                && ui
                    .add(egui::Button::new("Use this profile").fill(theme::accent_fill()))
                    .clicked()
            {
                settings.active_profile = i;
                edit = Some(Edit::Changed);
            }
            if ui.button("Duplicate").clicked() {
                editing.profile = new_profile(settings, Some(i));
                editing.deleting = false;
            }
            if settings.profiles.len() > 1 {
                let text = if editing.deleting {
                    format!("Delete {}? Click again", settings.profiles[i].name)
                } else {
                    "Delete profile…".to_string()
                };
                let delete = ui.add(
                    egui::Button::new(RichText::new(text).color(theme::danger())).frame(false),
                );
                ui.ctx()
                    .accesskit_node_builder(delete.id, |n| n.set_label("Delete profile"));
                if delete.clicked() {
                    if editing.deleting {
                        settings.delete_profile(i);
                        editing.profile = editing.profile.min(settings.profiles.len() - 1);
                        editing.deleting = false;
                        if in_use {
                            edit = Some(Edit::Changed);
                        }
                    } else {
                        editing.deleting = true;
                    }
                }
            }
        });
    });
    edit
}
