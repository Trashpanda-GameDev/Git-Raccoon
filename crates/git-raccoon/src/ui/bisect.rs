//! Finding the commit that broke something, on screen: the start window,
//! the bar above the graph while testing by hand, the steps while a
//! command tests each commit, and the answer.
//!
//! In the app's words a commit is "broken" or "works" (or the person's
//! own words), never "bad", "good" or "bisect".

use std::time::Duration;

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;
use git_raccoon_core::bisect::{Outcome, Verdict};

use crate::bisect_view::{Bisect, Words, mark};
use crate::repo_view::RepoView;
use crate::theme;
use crate::ui::history::Click;
use crate::ui::widgets;

/// What the bisect views ask for that the app does.
pub enum Action {
    /// Open a terminal in the repository's folder.
    Terminal,
    /// What the history's menu would do (revert, create a branch).
    History(Click),
    /// Remember this command for the repository.
    Remember(String),
}

/// The start window: where it's broken, where it last worked, what's being
/// looked for, and how to test. `remembered` is the command used last time
/// in this repository.
pub fn start_window(
    ctx: &egui::Context,
    repo: &mut RepoView,
    remembered: Option<&str>,
) -> Option<Action> {
    let mut action = None;
    let path = repo.path().to_string();
    let Some(start) = &mut repo.bisect.starting else {
        return None;
    };
    if start.command.is_empty()
        && let Some(c) = remembered
    {
        start.command = c.to_string();
    }
    let mut open = true;
    let mut go: Option<bool> = None;
    let mut changed = false;
    let mut cancel = false;
    let words = start.words();
    egui::Window::new("Find the commit that broke something")
        .id(egui::Id::new("bisect-start"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .default_width(560.0)
        .show(ctx, |ui| {
            ui.set_width(560.0);
            ui.label(
                RichText::new(
                    "Git Raccoon checks out commits between one that works and one that's broken, halving the range each time; you say whether each one works.",
                )
                .size(12.5)
                .color(theme::muted()),
            );
            ui.add_space(8.0);
            egui::Grid::new("bisect-start-grid")
                .num_columns(2)
                .spacing(egui::vec2(14.0, 10.0))
                .show(ui, |ui| {
                    ui.label(RichText::new(format!("It's {} at", words.new)).color(theme::muted()));
                    ui.label(format!("{}  {}", mark(Verdict::New), start.broken_label));
                    ui.end_row();

                    ui.label(RichText::new(format!("It {} at", last_worked(&words))).color(theme::muted()));
                    ui.horizontal(|ui| {
                        let field = ui.add(
                            egui::TextEdit::singleline(&mut start.works)
                                .hint_text("A tag, branch or commit")
                                .desired_width(200.0),
                        );
                        ui.ctx()
                            .accesskit_node_builder(field.id, |n| n.set_label("Where it last worked"));
                        if field.changed() {
                            changed = true;
                        }
                        if !start.tags.is_empty() {
                            let tags = start.tags.clone();
                            egui::ComboBox::from_id_salt("bisect-tags")
                                .selected_text("Tags")
                                .width(90.0)
                                .show_ui(ui, |ui| {
                                    for (name, _) in tags.iter().take(30) {
                                        if ui.selectable_label(start.works == *name, name).clicked() {
                                            start.works = name.clone();
                                            changed = true;
                                        }
                                    }
                                });
                        }
                    });
                    ui.end_row();

                    ui.label("");
                    match &start.preview {
                        Some((_, _, Ok(p))) => {
                            let s = if p.commits == 1 { "" } else { "s" };
                            let st = if p.steps == 1 { "" } else { "s" };
                            ui.label(
                                RichText::new(format!(
                                    "{} commit{s} in between · about {} step{st}",
                                    p.commits, p.steps
                                ))
                                .color(theme::modified()),
                            );
                        }
                        Some((_, _, Err(e))) => {
                            ui.label(RichText::new(e).color(theme::danger()));
                        }
                        None => {
                            ui.spinner();
                        }
                    }
                    ui.end_row();

                    ui.label(RichText::new("Looking for").color(theme::muted()));
                    ui.vertical(|ui| {
                        for (i, text) in [
                            "When it broke",
                            "When something changed (old behaviour, new behaviour)",
                            "In my own words",
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            if ui.radio_value(&mut start.looking_for, i, text).changed() {
                                changed = true;
                            }
                        }
                        if start.looking_for == 2 {
                            ui.horizontal(|ui| {
                                let new = ui.add(
                                    egui::TextEdit::singleline(&mut start.own_new)
                                        .hint_text("fast")
                                        .desired_width(110.0),
                                );
                                ui.ctx()
                                    .accesskit_node_builder(new.id, |n| n.set_label("Word for the new behaviour"));
                                ui.label(RichText::new("now, and before it was").color(theme::muted()));
                                let old = ui.add(
                                    egui::TextEdit::singleline(&mut start.own_old)
                                        .hint_text("slow")
                                        .desired_width(110.0),
                                );
                                ui.ctx()
                                    .accesskit_node_builder(old.id, |n| n.set_label("Word for the old behaviour"));
                            });
                        }
                    });
                    ui.end_row();

                    ui.label("");
                    let fp = ui.checkbox(&mut start.first_parent, "Only follow the main line");
                    if fp.changed() {
                        changed = true;
                    }
                    fp.on_hover_text("Skips into merged branches' own commits: a merged branch is tested as one, at its merge. Quicker, when what you're after is which merge broke it.");
                    ui.end_row();
                });
            if start.dirty {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "{}  You have uncommitted changes: they're put aside first, and back at the end.",
                        icon::WARNING
                    ))
                    .size(12.5)
                    .color(theme::modified()),
                );
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Or let a command test each commit").color(theme::muted()));
            let cmd = ui.add(
                egui::TextEdit::singleline(&mut start.command)
                    .hint_text("cargo test session::")
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY),
            );
            ui.ctx()
                .accesskit_node_builder(cmd.id, |n| n.set_label("Test command"));
            ui.label(
                RichText::new(format!(
                    "Run in the repository's folder: exit 0 means it {}, 125 that it can't be tested, anything else that it's {}.",
                    words.old, words.new
                ))
                .size(11.5)
                .color(theme::muted()),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("Give up on a step after").size(12.0).color(theme::muted()));
                let limit = ui.add(
                    egui::TextEdit::singleline(&mut start.limit_minutes)
                        .hint_text("no limit")
                        .desired_width(70.0),
                );
                ui.ctx()
                    .accesskit_node_builder(limit.id, |n| n.set_label("Minutes a step may take"));
                ui.label(
                    RichText::new("minutes (it's then skipped)")
                        .size(12.0)
                        .color(theme::muted()),
                );
            });
            if let Some(e) = &start.error {
                ui.label(RichText::new(e).color(theme::danger()));
            }
            ui.add_space(10.0);
            let ready = matches!(start.preview, Some((_, _, Ok(_))));
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let by_hand = ui.add_enabled(
                        ready,
                        egui::Button::new(
                            RichText::new("Start, I'll test each one")
                                .color(theme::on_accent())
                                .strong(),
                        )
                        .fill(theme::accent_fill()),
                    );
                    if by_hand.clicked() {
                        go = Some(false);
                    }
                    let with = ui.add_enabled(
                        ready && !start.command.trim().is_empty(),
                        egui::Button::new("Test with this command"),
                    );
                    if with.clicked() {
                        go = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if !open || cancel {
        repo.bisect.starting = None;
        return None;
    }
    if changed {
        repo.bisect.check_start(ctx, &path);
    }
    if let Some(with_command) = go {
        let command = repo
            .bisect
            .starting
            .as_ref()
            .map(|s| s.command.trim().to_string())
            .unwrap_or_default();
        if with_command {
            action = Some(Action::Remember(command));
        }
        repo.bisect.start(ctx, &path, with_command);
    }
    action
}

fn last_worked(words: &Words) -> String {
    if words.is_default() {
        "last worked".into()
    } else {
        format!("was last {}", words.old)
    }
}

/// While searching: the bar above the graph (and "So far" below it), or,
/// in the graph's place, a command's steps or the answer. Returns whether
/// the graph's place was taken.
pub fn show(ui: &mut egui::Ui, repo: &mut RepoView) -> (bool, Option<Action>) {
    let ctx = ui.ctx().clone();
    let path = repo.path().to_string();
    if repo.bisect.found.is_some() {
        return (true, answer(ui, repo));
    }
    if repo.bisect.running.is_some() {
        steps(ui, &mut repo.bisect, &ctx, &path);
        return (true, None);
    }
    let Some(search) = repo.bisect.search.clone() else {
        if let Some(e) = repo.bisect.error.clone() {
            error_bar(ui, &mut repo.bisect, &e);
        }
        return (false, None);
    };
    let mut action = None;
    let words = Words::of(&search.terms);
    let busy = repo.bisect.is_busy() || repo.op_running();
    let testing = search
        .testing
        .as_ref()
        .and_then(|id| repo.bisect.summaries.get(id).cloned())
        .unwrap_or_default();
    egui::Panel::top("bisect-bar")
        .frame(
            egui::Frame::new()
                .fill(theme::modified().gamma_multiply(0.10))
                .stroke(egui::Stroke::new(1.0, theme::modified().gamma_multiply(0.5)))
                .inner_margin(egui::Margin::symmetric(12, 8)),
        )
        .show(ui, |ui| {
            // The buttons first, at the right; what's being tested in the
            // room left, cut short when it's tight (whole on hover).
            let s = if search.left() == 1 { "" } else { "s" };
            let st = if search.steps == 1 { "" } else { "s" };
            let info = format!(
                "· testing \u{201c}{testing}\u{201d} · {} commit{s} left, about {} step{st}",
                search.left(),
                search.steps
            );
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        let works = ui.add(egui::Button::new(
                            RichText::new(format!("{}  {}", icon::CHECK, words.button(Verdict::Old)))
                                .color(theme::add()),
                        ));
                        if works.clicked() {
                            repo.bisect.answer(&ctx, &path, Verdict::Old);
                        }
                        let broken = ui.add(egui::Button::new(
                            RichText::new(format!("{}  {}", icon::X, words.button(Verdict::New)))
                                .color(theme::danger()),
                        ));
                        if broken.clicked() {
                            repo.bisect.answer(&ctx, &path, Verdict::New);
                        }
                        if ui
                            .button("Stop")
                            .on_hover_text("Ends the search: back where it started, with anything put aside brought back")
                            .clicked()
                        {
                            repo.bisect.stop(&ctx, &path);
                        }
                        if ui
                            .button("Skip this one")
                            .on_hover_text("For a commit that can't be tested (it doesn't build, say): another one near it is tried")
                            .clicked()
                        {
                            repo.bisect.answer(&ctx, &path, Verdict::Skip);
                        }
                        command_button(ui, &mut repo.bisect, &ctx, &path, &mut action);
                    });
                    if busy {
                        ui.spinner();
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(theme::modified()));
                        ui.add(egui::Label::new(RichText::new(words.title()).strong()).truncate());
                        if let Some(agent) = &repo.bisect.agent {
                            ui.label(
                                RichText::new(format!("{}  {agent} is answering", icon::ROBOT))
                                    .color(theme::accent()),
                            );
                        }
                        ui.add(
                            egui::Label::new(RichText::new(&info).color(theme::muted())).truncate(),
                        );
                    });
                });
            });
            if let Some(e) = &repo.bisect.error {
                ui.label(RichText::new(e).size(12.0).color(theme::danger()));
            }
        });
    // So far: the answers in order, and what can be done meanwhile.
    egui::Panel::bottom("bisect-so-far")
        .frame(
            egui::Frame::new()
                .fill(theme::bg_raised())
                .stroke(egui::Stroke::new(1.0, theme::border()))
                .inner_margin(egui::Margin::symmetric(12, 8)),
        )
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("So far").strong());
                for a in &search.answers {
                    let name = repo
                        .bisect
                        .summaries
                        .get(&a.id)
                        .cloned()
                        .unwrap_or_else(|| a.id[..a.id.len().min(7)].to_string());
                    ui.label(
                        RichText::new(format!("· {name} {}", mark(a.verdict)))
                            .color(theme::muted()),
                    )
                    .on_hover_text(format!(
                        "{}: {}",
                        &a.id[..a.id.len().min(10)],
                        words.verdict(a.verdict)
                    ));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(format!("{}  Open a terminal here", icon::TERMINAL))
                        .clicked()
                    {
                        action = Some(Action::Terminal);
                    }
                    let can_undo = search.answers.len() > 2 && !busy;
                    if ui
                        .add_enabled(can_undo, egui::Button::new("Undo last answer"))
                        .clicked()
                    {
                        repo.bisect.undo(&ctx, &path);
                    }
                });
            });
        });
    (false, action)
}

/// "Test with a command…": a field, and run it on each commit from here.
fn command_button(
    ui: &mut egui::Ui,
    bisect: &mut Bisect,
    ctx: &egui::Context,
    path: &str,
    action: &mut Option<Action>,
) {
    let id = egui::Id::new("bisect-command");
    let response = ui.button("Test with a command…");
    egui::Popup::menu(&response).width(380.0).show(|ui| {
        let mut command: String = ui.data(|d| d.get_temp(id)).unwrap_or_default();
        ui.label(
            RichText::new("Runs on each commit from here, in the repository's folder")
                .size(12.0)
                .color(theme::muted()),
        );
        let field = ui.add(
            egui::TextEdit::singleline(&mut command)
                .hint_text("cargo test session::")
                .font(egui::TextStyle::Monospace)
                .desired_width(360.0),
        );
        ui.ctx()
            .accesskit_node_builder(field.id, |n| n.set_label("Command to test with"));
        if ui
            .add_enabled(
                !command.trim().is_empty(),
                egui::Button::new("Run it on each commit"),
            )
            .clicked()
        {
            *action = Some(Action::Remember(command.trim().to_string()));
            bisect.run_command(ctx, path, command.clone(), None);
            ui.close();
        }
        ui.data_mut(|d| d.insert_temp(id, command));
    });
}

fn error_bar(ui: &mut egui::Ui, bisect: &mut Bisect, e: &str) {
    egui::Panel::top("bisect-error")
        .frame(
            egui::Frame::new()
                .fill(theme::danger_bg())
                .inner_margin(egui::Margin::symmetric(12, 8)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(e).color(theme::danger()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, icon::X, "Dismiss").clicked() {
                        bisect.error = None;
                    }
                });
            });
        });
}

/// A command testing each commit: each step's result and time, the one
/// running with its output live, and Stop.
fn steps(ui: &mut egui::Ui, bisect: &mut Bisect, ctx: &egui::Context, path: &str) {
    let Some(running) = &bisect.running else {
        return;
    };
    let log = match running.log.lock() {
        Ok(l) => (
            l.command.clone(),
            l.steps.clone(),
            l.current.clone(),
            l.output.clone(),
        ),
        Err(_) => return,
    };
    let (command, done, current, output) = log;
    let stopping = running.stopping();
    let words = bisect
        .search
        .as_ref()
        .map_or_else(|| Words::of(&Default::default()), |s| Words::of(&s.terms));
    let estimate = bisect.search.as_ref().map_or(0, |s| s.steps) + done.len();
    let mut stop = false;
    egui::Panel::top("bisect-running")
        .frame(
            egui::Frame::new()
                .fill(theme::modified().gamma_multiply(0.10))
                .inner_margin(egui::Margin::symmetric(12, 8)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(theme::modified()));
                ui.label(RichText::new(words.title()).strong());
                ui.label(RichText::new("· running").color(theme::muted()));
                ui.label(RichText::new(&command).monospace());
                ui.label(RichText::new("on each commit").color(theme::muted()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let text = if stopping { "Stopping…" } else { "Stop" };
                    if ui
                        .add_enabled(
                            !stopping,
                            egui::Button::new(RichText::new(text).color(theme::danger())),
                        )
                        .on_hover_text(
                            "Stops the command; the answers so far are kept, to go on by hand",
                        )
                        .clicked()
                    {
                        stop = true;
                    }
                });
            });
        });
    egui::CentralPanel::no_frame()
        .frame(egui::Frame::new().inner_margin(egui::Margin::same(16)))
        .show(ui, |ui| {
        let n = done.len() + usize::from(current.is_some());
        ui.horizontal(|ui| {
            ui.label(RichText::new("Steps").strong());
            #[expect(clippy::cast_precision_loss, reason = "a few steps")]
            let fraction = if estimate == 0 { 0.0 } else { done.len() as f32 / estimate.max(n) as f32 };
            ui.add(egui::ProgressBar::new(fraction).desired_width(180.0));
            ui.label(RichText::new(format!("{n} of about {}", estimate.max(n))).color(theme::muted()));
        });
        ui.add_space(6.0);
        for (i, s) in done.iter().enumerate() {
            ui.horizontal(|ui| {
                let (glyph, colour) = match s.verdict {
                    Some(Verdict::New) => (icon::X, theme::danger()),
                    Some(Verdict::Old) => (icon::CHECK, theme::add()),
                    Some(Verdict::Skip) => (icon::MINUS, theme::muted()),
                    None => (icon::STOP, theme::muted()),
                };
                ui.label(RichText::new(glyph).color(colour));
                ui.label(&s.summary);
                let how = match (s.timed_out, s.code) {
                    (true, _) => "too long: skipped".to_string(),
                    (false, Some(c)) => format!("exit {c}"),
                    (false, None) => "stopped".to_string(),
                };
                ui.label(
                    RichText::new(format!("{how} · {}", seconds(s.took)))
                        .size(12.0)
                        .color(theme::muted()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let id = egui::Id::new(("bisect-output", i));
                    let response = ui.button("Output");
                    egui::Popup::menu(&response).width(560.0).show(|ui| {
                        egui::ScrollArea::vertical().max_height(320.0).id_salt(id).show(ui, |ui| {
                            ui.add(egui::Label::new(RichText::new(&s.output).monospace().size(11.5)).selectable(true));
                        });
                    });
                });
            });
        }
        if let Some((_, summary, since)) = &current {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(summary).strong());
                ui.label(
                    RichText::new(format!("running · {}", seconds(since.elapsed())))
                        .size(12.0)
                        .color(theme::muted()),
                );
            });
        }
        ui.add_space(6.0);
        egui::Frame::new()
            .fill(theme::bg_raised())
            .corner_radius(6)
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("bisect-live-output")
                    .max_height(280.0)
                    .stick_to_bottom(true)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.label(RichText::new(format!("$ {command}")).monospace().color(theme::muted()));
                        ui.add(egui::Label::new(RichText::new(&output).monospace().size(12.0)).selectable(true));
                    });
            });
        ui.label(
            RichText::new(format!(
                "Exit 0 counts as {}, 125 as skip, anything else as {} (as git bisect run). Remembered for this repository.",
                words.old, words.new
            ))
            .size(11.5)
            .color(theme::muted()),
        );
    });
    if stop {
        bisect.stop(ctx, path);
    }
}

fn seconds(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s} s")
    } else {
        format!("{} min {} s", s / 60, s % 60)
    }
}

/// The answer: the first broken commit, how it was found, and what to do
/// with it.
fn answer(ui: &mut egui::Ui, repo: &mut RepoView) -> Option<Action> {
    let mut action = None;
    let found = repo.bisect.found.as_ref()?;
    let words = found.words();
    let f = &found.finished;
    let steps = found.steps();
    let mut close = false;
    let mut select = None;
    let st = if steps == 1 { "" } else { "s" };
    let head = match &f.outcome {
        Outcome::Found(_) => format!("Found it in {steps} step{st}"),
        Outcome::OneOf(ids) => format!("Narrowed it to {} commits", ids.len()),
    };
    let back = format!(
        "· back on {}{}",
        short_ref(&f.back_on),
        if f.restored {
            ", your changes restored"
        } else {
            ""
        }
    );
    egui::Panel::top("bisect-found")
        .frame(
            egui::Frame::new()
                .fill(theme::add().gamma_multiply(0.10))
                .inner_margin(egui::Margin::symmetric(12, 8)),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon::CHECK).color(theme::add()));
                ui.label(RichText::new(&head).strong());
                ui.label(RichText::new(&back).color(theme::muted()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Done").clicked() {
                        close = true;
                    }
                });
            });
        });
    let agents_connected = ui
        .ctx()
        .data(|d| d.get_temp::<crate::app::AgentsStatus>(crate::app::agents_status_id()))
        .is_some_and(|a| !a.clients.is_empty());
    egui::CentralPanel::no_frame()
        .frame(egui::Frame::new().inner_margin(egui::Margin::same(16)))
        .show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_max_width(900.0);
            let title = match &f.outcome {
                Outcome::Found(_) => format!("The first {} commit", words.new),
                Outcome::OneOf(_) => format!(
                    "One of these is the first {} commit (some couldn't be tested)",
                    words.new
                ),
            };
            ui.label(RichText::new(title).color(theme::muted()));
            for d in &found.details {
                widgets_card(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&d.id[..7.min(d.id.len())]).monospace().color(theme::muted()));
                        ui.label(RichText::new(&d.summary).strong().size(16.0));
                    });
                    let mut by = format!("{} · {}", d.author.name, crate::format::date_time(d.author.time));
                    if let Some(n) = request_mentioned(&d.summary, &d.body) {
                        by.push_str(&format!(" · came with {n}"));
                    }
                    ui.label(RichText::new(by).size(12.5).color(theme::muted()));
                    if !d.body.trim().is_empty() {
                        ui.add(egui::Label::new(RichText::new(d.body.trim()).size(13.0)).wrap());
                    }
                    ui.horizontal_wrapped(|ui| {
                        for file in d.files.iter().take(12) {
                            ui.label(
                                RichText::new(format!(
                                    "{} +{} -{}",
                                    file.path, file.additions, file.deletions
                                ))
                                .monospace()
                                .size(12.0),
                            );
                        }
                        if d.files.len() > 12 {
                            ui.label(RichText::new(format!("and {} more", d.files.len() - 12)).color(theme::muted()));
                        }
                    });
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        let show = ui.add(
                            egui::Button::new(
                                RichText::new("Show it in the history").color(theme::on_accent()),
                            )
                            .fill(theme::accent_fill()),
                        );
                        if show.clicked() {
                            select = Some(d.id.clone());
                        }
                        let label = format!("{} \u{201c}{}\u{201d}", &d.id[..7.min(d.id.len())], d.summary);
                        if ui.button("Revert it…").clicked() {
                            action = Some(Action::History(Click::Revert { id: d.id.clone() }));
                        }
                        if ui.button("Create a branch here…").clicked() {
                            action = Some(Action::History(Click::NewBranch {
                                id: d.id.clone(),
                                label,
                            }));
                        }
                        if ui.button("Copy a summary").on_hover_text("For an issue: the commit, and the answers that found it").clicked() {
                            ui.ctx().copy_text(found.summary());
                        }
                        if agents_connected {
                            let ask = ui.button(RichText::new(format!("{}  Ask an agent why", icon::ROBOT)));
                            egui::Popup::menu(&ask).width(380.0).show(|ui| {
                                let prompt = format!(
                                    "In Git Raccoon's repository, commit {} (\u{201c}{}\u{201d}) is the first {} one: everything before it {}. Read it with get_commit and its diff, and tell me what in it likely caused this.",
                                    &d.id[..10.min(d.id.len())],
                                    d.summary,
                                    words.new,
                                    words.old
                                );
                                ui.label("Paste this to your agent (it's connected):");
                                ui.add(egui::Label::new(RichText::new(&prompt).size(12.0)).selectable(true));
                                if ui.button(format!("{}  Copy", icon::COPY)).clicked() {
                                    ui.ctx().copy_text(prompt);
                                    ui.close();
                                }
                            });
                        }
                    });
                });
            }
            ui.add_space(8.0);
            widgets_card(ui, |ui| {
                ui.label(RichText::new("How it was found").strong());
                ui.horizontal_wrapped(|ui| {
                    for (i, a) in f.answers.iter().enumerate() {
                        if i > 0 {
                            ui.label(RichText::new("·").color(theme::muted()));
                        }
                        let name = found
                            .summaries
                            .get(&a.id)
                            .cloned()
                            .unwrap_or_else(|| a.id[..a.id.len().min(7)].to_string());
                        ui.label(RichText::new(format!("{} {name}", mark(a.verdict))).size(13.0))
                            .on_hover_text(words.verdict(a.verdict).to_string());
                    }
                });
            });
        });
    });
    if let Some(id) = select {
        repo.bisect.close_found();
        repo.select(&id);
    } else if close {
        repo.bisect.close_found();
    }
    action
}

fn widgets_card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::bg_raised())
        .stroke(egui::Stroke::new(1.0, theme::border()))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            add(ui);
        });
}

/// "main" from "main" or "refs/heads/main"; a commit's short id.
fn short_ref(r: &str) -> String {
    let r = r.trim_start_matches("refs/heads/");
    if r.len() == 40 && r.chars().all(|c| c.is_ascii_hexdigit()) {
        r[..7].to_string()
    } else {
        r.to_string()
    }
}

/// A pull/merge request the message names ("Merge pull request #38",
/// "See merge request group/app!38", "(#38)").
fn request_mentioned(summary: &str, body: &str) -> Option<String> {
    for text in [summary, body] {
        for (i, c) in text.char_indices() {
            if c == '#' || c == '!' {
                let digits: String = text[i + 1..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                let before_ok =
                    i == 0 || !text[..i].ends_with(|p: char| p.is_alphanumeric() && c == '#');
                if !digits.is_empty() && before_ok {
                    return Some(format!("{c}{digits}"));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_request_and_the_branch() {
        assert_eq!(
            request_mentioned("Merge pull request #38 from x", ""),
            Some("#38".into())
        );
        assert_eq!(request_mentioned("Fix (#12)", ""), Some("#12".into()));
        assert_eq!(
            request_mentioned("x", "See merge request team/app!7"),
            Some("!7".into())
        );
        assert_eq!(request_mentioned("Use C# 12", ""), None);
        assert_eq!(short_ref("refs/heads/main"), "main");
        assert_eq!(short_ref(&"a".repeat(40)), "aaaaaaa");
    }
}
