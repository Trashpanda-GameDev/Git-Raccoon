//! A pull/merge request's or issue's details (the right-hand pane), and
//! the window for opening a new pull/merge request.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use crate::forge::Kind;
use crate::forge_view::{ForgeView, Opened};
use crate::theme;
use crate::ui::widgets;

pub enum Action {
    Open(String),
    Close,
}

fn heading(ui: &mut egui::Ui, text: &str, action: &mut Option<Action>) {
    ui.horizontal(|ui| {
        ui.set_height(36.0);
        ui.label(widgets::section_label(text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, icon::X, "Close (Esc)").clicked() {
                *action = Some(Action::Close);
            }
        });
    });
}

fn body(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    if text.trim().is_empty() {
        ui.label(RichText::new("No description.").color(theme::muted()));
    } else {
        ui.label(RichText::new(text.trim()).size(13.0));
    }
}

/// Shows what's open from the forge.
pub fn show(ui: &mut egui::Ui, forge: &ForgeView, now: i64) -> Option<Action> {
    let mut action = None;
    let kind = forge.project.kind;
    let site = kind.name();
    match forge.open {
        Some(Opened::Issue(n)) => {
            heading(ui, &format!("Issue #{n}"), &mut action);
            let Some(i) = forge.issue(n) else {
                ui.label(RichText::new("It isn't open any more.").color(theme::muted()));
                return action;
            };
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(RichText::new(&i.title).strong().size(15.0));
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!(
                            "Opened by {} · {} · {} comments",
                            i.author,
                            crate::format::age(i.created, now),
                            i.comments
                        ))
                        .color(theme::muted()),
                    );
                    if !i.labels.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for l in &i.labels {
                                ui.label(RichText::new(l).size(11.0).color(theme::accent()));
                            }
                        });
                    }
                    ui.add_space(8.0);
                    if ui
                        .button(format!("{}  Open on {site}", icon::ARROW_SQUARE_OUT))
                        .clicked()
                    {
                        action = Some(Action::Open(i.url.clone()));
                    }
                    ui.separator();
                    body(ui, &i.body);
                });
        }
        // Requests have a view of their own, in place of the graph.
        Some(Opened::Request(_)) | None => {}
    }
    action
}

/// The window for opening a new pull/merge request.
pub struct NewForm {
    pub kind: Kind,
    pub source: String,
    pub target: String,
    /// The remote's branches it can go into (the default one first).
    pub targets: Vec<String>,
    pub title: String,
    pub body: String,
    pub draft: bool,
    /// The branch isn't on the remote as it is here (push it first).
    pub unpushed: bool,
    /// The branch is the one checked out (so it can be pushed from here),
    /// and how many commits pushing it sends.
    pub checked_out: bool,
    pub to_push: usize,
    /// Its commits the target doesn't have (id, message), how far behind
    /// the target it is, and whether merging it would conflict; for the
    /// target in `compared`.
    pub commits: Vec<(String, String)>,
    pub behind: usize,
    pub conflicts: bool,
    pub compared: String,
    /// The files it changes, laid out, and the one open.
    pub files: Vec<std::sync::Arc<crate::highlight::RenderedDiff>>,
    pub open_file: Option<usize>,
    /// The repository's templates (name, text), and the one picked.
    pub templates: Vec<(String, String)>,
    pub template: Option<usize>,
    /// Who it asks to review it, who it's assigned to, and its labels:
    /// picked from `choices`, once they're listed (`None` while they load).
    pub reviewers: Vec<String>,
    pub assignees: Vec<String>,
    pub labels: Vec<String>,
    pub choices: Option<Result<crate::forge::Choices, String>>,
    /// What the open picker's list is narrowed to.
    pub filter: String,
    /// GitLab: delete the branch, and squash, when it's merged.
    pub remove_source: bool,
    pub squash: bool,
    /// Pushing, to open it once that's done.
    pub pushing: bool,
    /// The tab shown.
    pub tab: NewTab,
    pub sending: bool,
    pub error: Option<String>,
}

impl NewForm {
    /// Fills in the title and description from its commits (one: its
    /// message; several: the branch's name, and the commits listed), or
    /// the template.
    pub fn fill(&mut self) {
        match self.commits.as_slice() {
            [(_, message)] => {
                let (title, rest) = message.split_once('\n').unwrap_or((message, ""));
                self.title = title.trim().to_string();
                self.body = rest.trim().to_string();
            }
            commits => {
                self.title = title_from_branch(&self.source);
                self.body = commits
                    .iter()
                    .map(|(_, m)| format!("* {}", m.lines().next().unwrap_or_default()))
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
        if let Some((_, text)) = self.template.and_then(|i| self.templates.get(i)) {
            self.body = text.clone();
        }
    }
}

/// "feature/pipeline-cache" → "Pipeline cache".
pub fn title_from_branch(branch: &str) -> String {
    let last = branch.rsplit('/').next().unwrap_or(branch);
    let words = last.replace(['-', '_'], " ");
    let mut chars = words.trim().chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The repository's pull/merge request templates: (name, text).
pub fn templates(root: &std::path::Path, kind: Kind) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut read = |path: std::path::PathBuf| {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let name = path.file_stem().map_or_else(String::new, |n| {
                n.to_string_lossy().replace(['_', '-'], " ")
            });
            found.push((name, text.trim().to_string()));
        }
    };
    let folder = |dir: std::path::PathBuf| -> Vec<std::path::PathBuf> {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md")))
            .collect();
        files.sort();
        files
    };
    match kind {
        Kind::GitHub => {
            for dir in [root.join(".github"), root.to_path_buf(), root.join("docs")] {
                for name in ["pull_request_template.md", "PULL_REQUEST_TEMPLATE.md"] {
                    let path = dir.join(name);
                    if path.is_file() {
                        read(path);
                    }
                }
            }
            for path in folder(root.join(".github").join("PULL_REQUEST_TEMPLATE")) {
                read(path);
            }
        }
        Kind::GitLab => {
            for path in folder(root.join(".gitlab").join("merge_request_templates")) {
                read(path);
            }
        }
    }
    found
}

/// "#d73a4a" as a colour.
fn hex(colour: &str) -> Option<egui::Color32> {
    let c = colour.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(c.get(i..i + 2)?, 16).ok();
    Some(egui::Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?))
}

/// Picks some of `options` (name, colour): a dropdown of checkboxes, with
/// a box to narrow it down.
fn picker(
    ui: &mut egui::Ui,
    name: &str,
    chosen: &mut Vec<String>,
    options: &[(String, String)],
    filter: &mut String,
) {
    if options.is_empty() {
        ui.label(RichText::new("None to pick from").color(theme::muted()));
        return;
    }
    let shown = if chosen.is_empty() {
        "None".to_string()
    } else {
        chosen.join(", ")
    };
    let combo = egui::ComboBox::from_id_salt(("request-pick", name))
        .selected_text(shown)
        .width(400.0)
        .height(260.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            if options.len() > 8 {
                let edit = ui.add(
                    egui::TextEdit::singleline(filter)
                        .hint_text("Filter")
                        .desired_width(f32::INFINITY),
                );
                ui.ctx()
                    .accesskit_node_builder(edit.id, |n| n.set_label(format!("Filter {name}")));
            }
            let needle = filter.to_lowercase();
            for (option, colour) in options {
                if !needle.is_empty() && !option.to_lowercase().contains(&needle) {
                    continue;
                }
                let mut on = chosen.contains(option);
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut on, option.as_str()).changed() {
                        if on {
                            chosen.push(option.clone());
                        } else {
                            chosen.retain(|c| c != option);
                        }
                    }
                    if let Some(c) = hex(colour) {
                        let (dot, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().circle_filled(dot.center(), 5.0, c);
                    }
                });
            }
        });
    ui.ctx().accesskit_node_builder(combo.response.id, |n| {
        n.set_label(format!("Request {}", name.to_lowercase()))
    });
    if combo.inner.is_none() && !filter.is_empty() && !egui::Popup::is_any_open(ui.ctx()) {
        filter.clear();
    }
}

/// The tabs of a new request's view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewTab {
    #[default]
    Description,
    Commits,
    Changes,
}

pub enum NewAction {
    /// Push the branch, then open it.
    PushAndCreate,
    Create,
    Close,
}

/// A new pull/merge request, in place of the graph: where it goes and
/// opening it, then its description, commits and changes, in tabs.
pub fn new_request(ui: &mut egui::Ui, f: &mut NewForm) -> Option<NewAction> {
    let mut action = None;
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin {
            left: 16,
            right: 12,
            top: 10,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("New {}", f.kind.request().to_lowercase()))
                        .strong()
                        .size(16.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, icon::X, "Cancel (Esc)").clicked() {
                        action = Some(NewAction::Close);
                    }
                });
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&f.source).font(theme::mono(13.0)));
                ui.label(RichText::new("into").color(theme::muted()));
                if f.targets.is_empty() {
                    // No branches known for the remote (not fetched yet): type one.
                    let target =
                        ui.add(egui::TextEdit::singleline(&mut f.target).desired_width(200.0));
                    ui.ctx()
                        .accesskit_node_builder(target.id, |n| n.set_label("Target branch"));
                } else {
                    let combo = egui::ComboBox::from_id_salt("request-target")
                        .selected_text(f.target.as_str())
                        .width(220.0)
                        .height(320.0)
                        .show_ui(ui, |ui| {
                            for t in &f.targets {
                                ui.selectable_value(&mut f.target, t.clone(), t);
                            }
                        });
                    ui.ctx().accesskit_node_builder(combo.response.id, |n| {
                        n.set_label("Target branch");
                    });
                }
            });
            // What it will contain, and how it stands with the target.
            ui.add_space(2.0);
            let n = f.commits.len();
            if n == 0 && !f.compared.is_empty() {
                ui.label(
                    RichText::new(format!(
                        "{} has nothing {} doesn't have.",
                        f.source, f.target
                    ))
                    .color(theme::modified()),
                );
            } else {
                let (c, s) = (n, if n == 1 { "" } else { "s" });
                let files = f.files.len();
                let fs = if files == 1 { "" } else { "s" };
                ui.label(
                    RichText::new(format!("{c} commit{s}, {files} file{fs} changed"))
                        .color(theme::muted()),
                );
            }
            if f.conflicts {
                ui.label(
                    RichText::new(format!(
                        "{}  It won't merge cleanly into {}: they change the same lines.",
                        icon::WARNING,
                        f.target
                    ))
                    .color(theme::danger()),
                );
            } else if f.behind > 0 {
                let s = if f.behind == 1 { "" } else { "s" };
                ui.label(
                    RichText::new(format!(
                        "{}  It's {} commit{s} behind {}.",
                        icon::INFO,
                        f.behind,
                        f.target
                    ))
                    .color(theme::muted()),
                );
            }
            if let Some(e) = &f.error {
                ui.label(RichText::new(format!("It wasn't opened: {e}.")).color(theme::danger()));
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let busy = f.sending || f.pushing;
                let ready = !busy && !f.title.trim().is_empty() && !f.target.trim().is_empty();
                let label = if f.pushing {
                    "Pushing…".to_string()
                } else if f.sending {
                    "Opening…".to_string()
                } else if f.unpushed {
                    let n = f.to_push;
                    let s = if n == 1 { "" } else { "s" };
                    if n > 0 {
                        format!("Push {n} commit{s} and open")
                    } else {
                        "Push and open".to_string()
                    }
                } else {
                    "Open it".to_string()
                };
                let go = egui::Button::new(
                    RichText::new(label)
                        .color(if ready {
                            theme::on_accent()
                        } else {
                            theme::muted()
                        })
                        .strong(),
                )
                .fill(if ready {
                    theme::accent_fill()
                } else {
                    theme::bg_hover()
                });
                if ui.add_enabled(ready, go).clicked() {
                    action = Some(if f.unpushed {
                        NewAction::PushAndCreate
                    } else {
                        NewAction::Create
                    });
                }
                if busy {
                    ui.spinner();
                }
                if ui.button("Cancel").clicked() {
                    action = Some(NewAction::Close);
                }
                ui.add_space(8.0);
                ui.checkbox(&mut f.draft, "Draft (not ready for review yet)");
            });
        });
    // The tabs.
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin::symmetric(12, 0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(32.0);
                ui.set_width(ui.available_width());
                for (tab, name, count) in [
                    (NewTab::Description, "Description", None),
                    (NewTab::Commits, "Commits", Some(f.commits.len())),
                    (NewTab::Changes, "Changes", Some(f.files.len())),
                ] {
                    let on = f.tab == tab;
                    let text = count.map_or_else(|| name.to_string(), |c| format!("{name}  {c}"));
                    let b = ui.add(
                        egui::Button::new(
                            RichText::new(text)
                                .color(if on { theme::text() } else { theme::muted() })
                                .strong(),
                        )
                        .frame(false),
                    );
                    ui.ctx()
                        .accesskit_node_builder(b.id, |n| n.set_label(format!("{name} tab")));
                    if on {
                        ui.painter().hline(
                            b.rect.x_range(),
                            ui.max_rect().bottom() - 1.0,
                            egui::Stroke::new(2.0, theme::accent()),
                        );
                    }
                    if b.clicked() {
                        f.tab = tab;
                    }
                }
            });
        });
    match f.tab {
        NewTab::Description => description(ui, f),
        NewTab::Commits => commits(ui, f),
        NewTab::Changes => changed_files(ui, &f.files, &mut f.open_file),
    }
    action
}

/// Its title, description, template, people, labels and options.
fn description(ui: &mut egui::Ui, f: &mut NewForm) {
    egui::ScrollArea::vertical()
        .id_salt("new-request-description")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.set_max_width(900.0_f32.min(ui.available_width()));
                    if f.templates.len() > 1 {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Template").color(theme::muted()));
                            let before = f.template;
                            let shown = f
                                .template
                                .and_then(|i| f.templates.get(i))
                                .map_or("No template", |(n, _)| n.as_str());
                            let combo = egui::ComboBox::from_id_salt("request-template")
                                .selected_text(shown)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut f.template, None, "No template");
                                    for (i, (name, _)) in f.templates.iter().enumerate() {
                                        ui.selectable_value(&mut f.template, Some(i), name);
                                    }
                                });
                            ui.ctx().accesskit_node_builder(combo.response.id, |n| {
                                n.set_label("Template");
                            });
                            if f.template != before {
                                f.fill();
                            }
                        });
                        ui.add_space(4.0);
                    }
                    let title = ui.add(
                        egui::TextEdit::singleline(&mut f.title)
                            .hint_text("Title")
                            .font(egui::TextStyle::Heading)
                            .desired_width(f32::INFINITY),
                    );
                    ui.ctx()
                        .accesskit_node_builder(title.id, |n| n.set_label("Request title"));
                    ui.add_space(6.0);
                    let body = ui.add(
                        egui::TextEdit::multiline(&mut f.body)
                            .hint_text("What it changes, and why (Markdown)")
                            .desired_rows(12)
                            .desired_width(f32::INFINITY),
                    );
                    ui.ctx()
                        .accesskit_node_builder(body.id, |n| n.set_label("Request description"));
                    ui.add_space(8.0);
                    match &f.choices {
                        None => {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(
                                    RichText::new("Listing who can review it, and the labels…")
                                        .color(theme::muted()),
                                );
                            });
                        }
                        Some(Err(e)) => {
                            ui.label(
                                RichText::new(format!(
                                    "Reviewers, assignees and labels couldn't be listed: {e}."
                                ))
                                .color(theme::muted()),
                            );
                        }
                        Some(Ok(choices)) => {
                            let people: Vec<(String, String)> = choices
                                .people
                                .iter()
                                .map(|p| (p.clone(), String::new()))
                                .collect();
                            egui::Grid::new("request-people")
                                .num_columns(2)
                                .show(ui, |ui| {
                                    for (name, chosen, options) in [
                                        ("Reviewers", &mut f.reviewers, &people),
                                        ("Assignees", &mut f.assignees, &people),
                                        ("Labels", &mut f.labels, &choices.labels),
                                    ] {
                                        ui.label(RichText::new(name).color(theme::muted()));
                                        picker(ui, name, chosen, options, &mut f.filter);
                                        ui.end_row();
                                    }
                                });
                        }
                    }
                    if f.kind == Kind::GitLab {
                        ui.add_space(4.0);
                        ui.checkbox(
                            &mut f.remove_source,
                            format!("Delete {} when it's merged", f.source),
                        );
                        ui.checkbox(&mut f.squash, "Squash its commits when it's merged");
                    }
                });
        });
}

/// Its commits, oldest first.
fn commits(ui: &mut egui::Ui, f: &NewForm) {
    egui::ScrollArea::vertical()
        .id_salt("new-request-commits")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    if f.commits.is_empty() {
                        ui.label(RichText::new("No commits.").color(theme::muted()));
                    }
                    for (id, message) in &f.commits {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(crate::format::short_sha(id))
                                    .font(theme::mono(12.0))
                                    .color(theme::accent()),
                            );
                            ui.label(message.lines().next().unwrap_or_default());
                        });
                    }
                });
        });
}

/// The files it changes, with how many lines each adds and removes, beside
/// the diff of the one picked.
fn changed_files(
    ui: &mut egui::Ui,
    files: &[std::sync::Arc<crate::highlight::RenderedDiff>],
    open: &mut Option<usize>,
) {
    use git_raccoon_core::details::LineKind;
    if files.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("No changes.").color(theme::muted())));
        return;
    }
    if open.is_none_or(|i| i >= files.len()) {
        *open = Some(0);
    }
    egui::Panel::left("new-request-files")
        .resizable(true)
        .default_size(260.0)
        .size_range(160.0..=500.0)
        .frame(
            egui::Frame::new()
                .fill(theme::bg_raised())
                .inner_margin(egui::Margin::symmetric(6, 6)),
        )
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("new-request-file-list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (i, d) in files.iter().enumerate() {
                        let lines = d.diff.hunks[..d.changes].iter().flat_map(|h| &h.lines);
                        let (added, removed) = lines.fold((0, 0), |(a, r), l| match l.kind {
                            LineKind::Add => (a + 1, r),
                            LineKind::Delete => (a, r + 1),
                            _ => (a, r),
                        });
                        let on = *open == Some(i);
                        let row = ui.horizontal(|ui| {
                            ui.set_width(ui.available_width());
                            let (dir, name) = crate::format::split_path(&d.diff.path);
                            ui.label(name);
                            if !dir.is_empty() {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(dir).size(11.0).color(theme::muted()),
                                    )
                                    .truncate(),
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        RichText::new(format!("−{removed}"))
                                            .size(11.0)
                                            .color(theme::del()),
                                    );
                                    ui.label(
                                        RichText::new(format!("+{added}"))
                                            .size(11.0)
                                            .color(theme::add()),
                                    );
                                },
                            );
                        });
                        let row = ui.interact(
                            row.response.rect,
                            ui.id().with(("new-file", i)),
                            egui::Sense::click(),
                        );
                        row.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                true,
                                on,
                                &d.diff.path,
                            )
                        });
                        if on {
                            ui.painter().rect_filled(
                                row.rect.expand(2.0),
                                4,
                                theme::accent().gamma_multiply(0.15),
                            );
                        }
                        if row.clicked() {
                            *open = Some(i);
                        }
                    }
                });
        });
    let Some(d) = open.and_then(|i| files.get(i)) else {
        return;
    };
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin::symmetric(12, 0))
        .show(ui, |ui| {
            ui.set_height(32.0);
            ui.set_width(ui.available_width());
            ui.horizontal_centered(|ui| ui.label(RichText::new(&d.diff.path).strong()));
        });
    if d.diff.binary {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("Binary file.").color(theme::muted())));
        return;
    }
    egui::ScrollArea::both()
        .id_salt(("new-request-diff", &d.diff.path))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let width = ui.available_width().max(800.0);
            for row in &d.unified {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(width, crate::ui::diff::LINE),
                    egui::Sense::hover(),
                );
                match *row {
                    crate::highlight::Row::Header(h) => {
                        crate::ui::diff::header(ui.painter(), rect, &d.diff.hunks[h].header);
                    }
                    crate::highlight::Row::Line(h, l) => crate::ui::diff::unified_line(
                        ui.painter(),
                        rect,
                        &d.diff.hunks[h].lines[l],
                        &d.jobs[h][l],
                        false,
                    ),
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_from_branch_names() {
        assert_eq!(
            title_from_branch("feature/pipeline-cache"),
            "Pipeline cache"
        );
        assert_eq!(title_from_branch("fix_it"), "Fix it");
    }

    #[test]
    fn fills_in_from_its_commits_or_the_template() {
        let mut f = NewForm {
            kind: Kind::GitHub,
            source: "feature/faster-start".into(),
            target: "main".into(),
            targets: Vec::new(),
            title: String::new(),
            body: String::new(),
            draft: false,
            unpushed: false,
            checked_out: true,
            to_push: 0,
            commits: vec![("a".into(), "Load lazily\n\nStarts in half the time.".into())],
            behind: 0,
            conflicts: false,
            compared: "main".into(),
            files: Vec::new(),
            open_file: None,
            templates: Vec::new(),
            template: None,
            reviewers: Vec::new(),
            assignees: Vec::new(),
            labels: Vec::new(),
            choices: None,
            filter: String::new(),
            remove_source: false,
            squash: false,
            pushing: false,
            tab: NewTab::default(),
            sending: false,
            error: None,
        };
        f.fill();
        assert_eq!(
            (f.title.as_str(), f.body.as_str()),
            ("Load lazily", "Starts in half the time.")
        );
        f.commits.push(("b".into(), "Cache icons".into()));
        f.fill();
        assert_eq!(f.title, "Faster start");
        assert_eq!(f.body, "* Load lazily\n* Cache icons");
        f.templates = vec![("default".into(), "## Why".into())];
        f.template = Some(0);
        f.fill();
        assert_eq!(f.body, "## Why");
    }
}
