//! A pull/merge request's view, in place of the graph: its header (state,
//! branches, checks, approvals, threads, and Check out, Finish review and
//! Merge), and its Conversation, Commits and Changes.

use std::sync::Arc;

use eframe::egui::{self, Pos2, Rect, RichText, Sense, Stroke};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use egui_phosphor::regular as icon;
use git_raccoon_core::details::{DiffLine, FileDiff, FileStatus, Hunk, LineKind};

use crate::forge::Kind;
use crate::forge::review::{
    self, Anchor, Change, Draft, Event, Finish, Line, Merge, Method, Outcome, Overview, Part,
    State, Suggestion, Thread, Verdict,
};
use crate::forge::{Request, Review};
use crate::forge_view::{Composer, Finishing, ForgeView, Merging, RequestState, Tab};
use crate::highlight::{RenderedDiff, Row, SplitRow};
use crate::theme;
use crate::ui::diff::{LINE, NUMBER};
use crate::ui::file_view::{self, GUTTER};
use crate::ui::widgets;
use git_raccoon_core::file_history::Blame;

pub enum Action {
    Close,
    Open(String),
    Reload,
    /// Check out its branch; or into a new worktree.
    CheckOut,
    CheckOutWorktree,
    /// With uncommitted changes: put them aside, then check it out.
    CheckOutAside,
    /// On its branch: pull, or go back to the branch it goes into.
    PullLatest,
    BackTo(String),
    /// After it's merged: pull the branch it went into.
    PullMerged,
    GoToCommit(String),
    /// Show its commits marked in the graph (and select this one, if given).
    ShowInGraph(Option<String>),
    /// Keep a draft: new, or in place of the one at this index.
    SaveDraft(Draft, Option<usize>),
    DeleteDraft(usize),
    Submit {
        summary: String,
        finish: Finish,
    },
    Resolve(String, bool),
    Merge(Merge),
    /// Bring it up to date with its target, on the forge.
    UpdateBranch,
    /// Change a posted comment's text, or delete it.
    EditComment(review::Comment, String),
    DeleteComment(review::Comment),
    /// Apply suggestions on the forge (GitLab), in one commit with this
    /// message.
    ApplySuggestions(Vec<String>, String),
    /// Apply a suggestion to the file here, on the request's branch.
    ApplyHere(Suggestion),
}

/// What the view needs from the repository.
pub struct Here<'a> {
    /// The branch checked out.
    pub branch: Option<&'a str>,
    pub now: i64,
    /// This request's drafts.
    pub drafts: &'a [Draft],
    /// Uncommitted changes were put aside to check it out, on this branch:
    /// going back there brings them back.
    pub aside: Option<String>,
}

/// What a stash made to check out a request says, so going back can find it.
pub fn aside_message(kind: Kind, number: u64, from: &str) -> String {
    format!(
        "Git Raccoon: put aside to check out {} (from {from})",
        kind.number(number)
    )
}

/// The branch changes were put aside on, to check out this request, if a
/// stash `message` says so.
pub fn aside_from(message: &str, kind: Kind, number: u64) -> Option<String> {
    let marker = format!("to check out {} (from ", kind.number(number));
    let rest = &message[message.find(&marker)? + marker.len()..];
    rest.strip_suffix(')').map(str::to_string)
}

/// Whether a request's branch is checked out here.
pub fn on_its_branch(r: &Request, kind: Kind, branch: Option<&str>) -> bool {
    let Some(b) = branch else { return false };
    if r.from_fork {
        b == fork_branch(kind, r.number)
    } else {
        b == r.source
    }
}

/// The local branch a request from a fork is fetched into.
pub fn fork_branch(kind: Kind, number: u64) -> String {
    match kind {
        Kind::GitHub => format!("pr/{number}"),
        Kind::GitLab => format!("mr/{number}"),
    }
}

pub fn show(ui: &mut egui::Ui, forge: &mut ForgeView, here: &Here<'_>) -> Option<Action> {
    let mut action = None;
    let kind = forge.project.kind;
    let Some(crate::forge_view::Opened::Request(number)) = forge.open else {
        return None;
    };
    let loading = forge.view_loading();
    let viewed = match &forge.details {
        Some((n, Ok(v))) if *n == number => Some(v.clone()),
        Some((n, Err(e))) if *n == number => {
            title_bar(ui, kind, number, None, &mut action);
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.add_space(16.0);
                ui.label(
                    RichText::new(format!("It couldn't be loaded: {e}")).color(theme::danger()),
                );
                if ui.button("Try again").clicked() {
                    action = Some(Action::Reload);
                }
            });
            return action;
        }
        _ => None,
    };
    let Some((r, o)) = viewed else {
        title_bar(ui, kind, number, forge.request(number), &mut action);
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.label(
                RichText::new("Loading its conversation, commits and changes…")
                    .color(theme::muted()),
            );
        });
        return action;
    };
    let web =
        (kind == Kind::GitLab).then(|| format!("{}/{}", forge.project.base, forge.project.path));
    ui.data_mut(|d| d.insert_temp(egui::Id::new("request-web"), web));
    let me = forge.account.as_ref().map(|a| a.login.clone());
    let state = &mut forge.state;
    state.me = me;
    state.gitlab = kind == Kind::GitLab;
    state.on_branch = on_its_branch(&r, kind, here.branch);
    header(ui, kind, &r, &o, state, here, loading, &mut action);
    if let Some(a) = batch_bar(ui, state) {
        action = Some(a);
    }
    tabs(ui, &o, state);
    egui::Frame::new()
        .fill(theme::bg())
        .show(ui, |ui| match state.tab {
            Tab::Conversation => conversation(ui, kind, &r, &o, state, here, &mut action),
            Tab::Commits => commits(ui, &o, here.now, &mut action),
            Tab::Changes => changes(ui, &o, state, here, &mut action),
        });
    if let Some(a) = finish_window(ui.ctx(), kind, state, here.drafts) {
        action = Some(a);
    }
    if let Some(a) = merge_window(ui.ctx(), kind, (&r, &o), state) {
        action = Some(a);
    }
    if let Some(a) = changes_in_the_way(ui.ctx(), state) {
        action = Some(a);
    }
    action
}

/// The bar along the top: its number, and closing.
fn title_bar(
    ui: &mut egui::Ui,
    kind: Kind,
    number: u64,
    listed: Option<&Request>,
    action: &mut Option<Action>,
) {
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin::symmetric(16, 0))
        .show(ui, |ui| {
            ui.set_height(40.0);
            ui.horizontal_centered(|ui| {
                ui.label(
                    RichText::new(format!("{} {}", kind.request(), kind.number(number))).strong(),
                );
                if let Some(r) = listed {
                    ui.label(RichText::new(&r.title).color(theme::muted()));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, icon::X, "Back to the graph (Esc)").clicked() {
                        *action = Some(Action::Close);
                    }
                });
            });
        });
}

fn state_pill(ui: &mut egui::Ui, r: &Request, o: &Overview) {
    let (text, colour) = match o.state {
        State::Merged => ("Merged", theme::accent()),
        State::Closed => ("Closed", theme::danger()),
        State::Open if r.draft => ("Draft", theme::muted()),
        State::Open => ("Open", theme::add()),
    };
    egui::Frame::new()
        .fill(colour.gamma_multiply(0.18))
        .stroke(Stroke::new(1.0, colour))
        .corner_radius(10)
        .inner_margin(egui::Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(12.0).color(colour).strong())
        });
}

fn branch_name(ui: &mut egui::Ui, name: &str) {
    ui.label(
        RichText::new(name)
            .font(theme::mono(12.0))
            .background_color(theme::bg_hover())
            .color(theme::text()),
    );
}

#[expect(
    clippy::too_many_arguments,
    reason = "the header shows a bit of everything"
)]
fn header(
    ui: &mut egui::Ui,
    kind: Kind,
    r: &Request,
    o: &Overview,
    state: &mut RequestState,
    here: &Here<'_>,
    loading: bool,
    action: &mut Option<Action>,
) {
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin {
            left: 16,
            right: 12,
            top: 10,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                state_pill(ui, r, o);
                ui.label(RichText::new(&r.title).strong().size(16.0));
                ui.label(RichText::new(kind.number(r.number)).size(16.0).color(theme::muted()));
                if loading {
                    ui.spinner();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_button(ui, icon::X, "Back to the graph (Esc)").clicked() {
                        *action = Some(Action::Close);
                    }
                    if widgets::icon_button(ui, icon::ARROW_SQUARE_OUT, &format!("Open on {}", kind.name()))
                        .clicked()
                    {
                        *action = Some(Action::Open(r.url.clone()));
                    }
                    if widgets::icon_button(ui, icon::ARROW_CLOCKWISE, "Reload").clicked() {
                        *action = Some(Action::Reload);
                    }
                    ui.add_space(6.0);
                    if o.state == State::Open {
                        let merge = ui.add(
                            egui::Button::new(
                                RichText::new(format!("{}  Merge  {}", icon::GIT_MERGE, icon::CARET_DOWN))
                                    .color(theme::on_accent())
                                    .strong(),
                            )
                            .fill(theme::accent_fill()),
                        );
                        ui.ctx().accesskit_node_builder(merge.id, |n| n.set_label("Merge…"));
                        if merge.on_hover_text("Merge it, or see why it can't be yet").clicked() {
                            state.merging = Some(Merging {
                                method: o.merge.methods.first().copied().unwrap_or(Method::Merge),
                                message: default_message(kind, r, o, o.merge.methods.first().copied().unwrap_or(Method::Merge)),
                                delete_branch: o.merge.can_delete_branch,
                            });
                        }
                    }
                    let count = here.drafts.len();
                    let review = if count > 0 {
                        format!("{}  Finish review ({count})", icon::CHAT_CIRCLE_TEXT)
                    } else {
                        format!("{}  Review", icon::CHAT_CIRCLE_TEXT)
                    };
                    let review = ui.button(review);
                    let name = if count > 0 { format!("Finish review ({count})") } else { "Review".into() };
                    ui.ctx().accesskit_node_builder(review.id, |n| n.set_label(name));
                    if review
                        .on_hover_text("Comment, approve or ask for changes: your draft comments go with it")
                        .clicked()
                    {
                        state.finishing = Some(Finishing {
                            summary: String::new(),
                            finish: Finish::Comment,
                        });
                    }
                    ui.menu_button(format!("{}  Check out  {}", icon::SIGN_OUT, icon::CARET_DOWN), |ui| {
                        let branch = if r.from_fork {
                            fork_branch(kind, r.number)
                        } else {
                            r.source.clone()
                        };
                        if ui
                            .button(format!("Check out {branch}"))
                            .on_hover_text("Check out its branch here, following the request (Pull brings its new commits)")
                            .clicked()
                        {
                            *action = Some(Action::CheckOut);
                            ui.close();
                        }
                        if ui
                            .button("In a new worktree…")
                            .on_hover_text("Check it out in a folder of its own, beside this one, and open that in a tab")
                            .clicked()
                        {
                            *action = Some(Action::CheckOutWorktree);
                            ui.close();
                        }
                    });
                });
            });
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new(&r.author).strong());
                ui.label(RichText::new("wants to merge").color(theme::muted()));
                branch_name(ui, &r.source);
                ui.label(RichText::new("into").color(theme::muted()));
                branch_name(ui, &r.target);
                let fork = if r.from_fork { " (from a fork)" } else { "" };
                ui.label(
                    RichText::new(format!("{fork} · {}", crate::format::age(r.created, here.now)))
                        .color(theme::muted()),
                );
            });
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                summary_line(ui, r, o);
            });
            // Parts the forge didn't give: say which, and why.
            if !o.missing.is_empty() {
                ui.add_space(4.0);
                banner(ui, theme::modified(), |ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "{}  Some of it couldn't be loaded, so it's not shown:",
                                icon::WARNING
                            ))
                            .color(theme::modified()),
                        );
                        for what in &o.missing {
                            ui.label(RichText::new(format!("• {what}")).size(12.0).color(theme::muted()));
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        if ui.button("Try again").clicked() {
                            *action = Some(Action::Reload);
                        }
                    });
                });
            }
            if o.cut_short {
                ui.label(
                    RichText::new("It's very long: some of its lists show only their first 5,000.")
                        .size(12.0)
                        .color(theme::muted()),
                );
            }
            // On its branch: keep up with it, or go back.
            if o.state == State::Open && on_its_branch(r, kind, here.branch) {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}  You're on its branch.", icon::CHECK)).color(theme::add()));
                    if ui.small_button("Pull the latest").clicked() {
                        *action = Some(Action::PullLatest);
                    }
                    let back = here.aside.clone().unwrap_or_else(|| r.target.clone());
                    let tip = if here.aside.is_some() {
                        "Check it out again, and bring back the changes put aside"
                    } else {
                        "Check out the branch it goes into"
                    };
                    if ui.small_button(format!("Back to {back}")).on_hover_text(tip).clicked() {
                        *action = Some(Action::BackTo(back));
                    }
                });
            }
            match o.state {
                State::Merged => {
                    ui.add_space(4.0);
                    banner(ui, theme::accent(), |ui| {
                        ui.label(
                            RichText::new(format!("{}  Merged into {}.", icon::GIT_MERGE, r.target))
                                .color(theme::accent())
                                .strong(),
                        );
                        if ui.small_button(format!("Pull {} to get it", r.target)).clicked() {
                            *action = Some(Action::PullMerged);
                        }
                    });
                }
                State::Closed => {
                    ui.add_space(4.0);
                    banner(ui, theme::danger(), |ui| {
                        ui.label(RichText::new("Closed without being merged.").color(theme::danger()));
                    });
                }
                State::Open => {}
            }
            match &state.sent {
                Some(Ok(done)) => {
                    ui.label(RichText::new(done).color(theme::add()));
                }
                Some(Err(e)) => {
                    ui.label(RichText::new(format!("It wasn't sent: {e}")).color(theme::danger()));
                }
                None => {}
            }
        });
}

fn banner(ui: &mut egui::Ui, colour: egui::Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(colour.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, colour.gamma_multiply(0.6)))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(add);
        });
}

/// Checks, approvals and open threads, in a line.
fn summary_line(ui: &mut egui::Ui, r: &Request, o: &Overview) {
    let c = r.checks;
    let checks = |n: u32| if n == 1 { "check" } else { "checks" };
    if c.is_empty() {
        ui.label(RichText::new(format!("{}  No checks", icon::CIRCLE)).color(theme::muted()));
    } else if c.failed > 0 {
        let all = c.passed + c.failed + c.pending;
        ui.label(
            RichText::new(format!(
                "{}  {} of {all} {} failed",
                icon::X_CIRCLE,
                c.failed,
                checks(all)
            ))
            .color(theme::danger()),
        );
    } else if c.pending > 0 {
        ui.label(
            RichText::new(format!(
                "{}  {} {} running",
                icon::CLOCK,
                c.pending,
                checks(c.pending)
            ))
            .color(theme::modified()),
        );
    } else {
        ui.label(
            RichText::new(format!(
                "{}  {} {} passed",
                icon::CHECK_CIRCLE,
                c.passed,
                checks(c.passed)
            ))
            .color(theme::add()),
        );
    }
    let who: Vec<&str> = r.reviews.iter().map(|(w, _)| w.as_str()).collect();
    let (glyph, colour, text) = match r.review {
        Review::Approved => (
            icon::CHECK_CIRCLE,
            theme::add(),
            format!("Approved by {}", who.join(", ")),
        ),
        Review::ChangesRequested => (
            icon::WARNING_CIRCLE,
            theme::modified(),
            "Changes requested".to_string(),
        ),
        Review::None => (icon::CIRCLE, theme::muted(), "Not approved yet".to_string()),
    };
    ui.label(RichText::new(format!("{glyph}  {text}")).color(colour));
    let open = o.unresolved();
    if open > 0 {
        let s = if open == 1 { "" } else { "s" };
        ui.label(
            RichText::new(format!(
                "{}  {open} unresolved thread{s}",
                icon::CHAT_CIRCLE_DOTS
            ))
            .color(theme::modified()),
        );
    } else if !o.threads.is_empty() {
        ui.label(
            RichText::new(format!("{}  All threads resolved", icon::CHAT_CIRCLE_DOTS))
                .color(theme::muted()),
        );
    }
}

fn tabs(ui: &mut egui::Ui, o: &Overview, state: &mut RequestState) {
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 0,
            bottom: 0,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(32.0);
                ui.set_width(ui.available_width());
                let comments = o
                    .events
                    .iter()
                    .filter(|e| !matches!(e, Event::Review { .. }))
                    .count();
                for (tab, name, count) in [
                    (Tab::Conversation, "Conversation", comments),
                    (Tab::Commits, "Commits", o.commits.len()),
                    (Tab::Changes, "Changes", o.files.len()),
                ] {
                    let on = state.tab == tab;
                    let label = RichText::new(format!("{name}  {count}"))
                        .color(if on { theme::text() } else { theme::muted() })
                        .strong();
                    let b = ui.add(egui::Button::new(label).frame(false).selected(false));
                    ui.ctx()
                        .accesskit_node_builder(b.id, |n| n.set_label(format!("{name} tab")));
                    if on {
                        ui.painter().hline(
                            b.rect.x_range(),
                            ui.max_rect().bottom() - 1.0,
                            Stroke::new(2.0, theme::accent()),
                        );
                    }
                    if b.clicked() {
                        state.tab = tab;
                    }
                }
            });
        });
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme::border()),
    );
}

/// GitLab's uploads ("/uploads/…") at the project's address (`web`).
pub fn absolute_uploads(text: &str, web: &str) -> String {
    text.replace("](/uploads/", &format!("]({web}/uploads/"))
        .replace("src=\"/uploads/", &format!("src=\"{web}/uploads/"))
}

/// A comment's body, as Markdown.
fn markdown(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    if text.trim().is_empty() {
        ui.label(
            RichText::new("No description.")
                .italics()
                .color(theme::muted()),
        );
    } else {
        // GitLab's uploads are relative to the project's page.
        let web = ui
            .data(|d| d.get_temp::<Option<String>>(egui::Id::new("request-web")))
            .flatten();
        let text = match web {
            Some(web) => absolute_uploads(text, &web),
            None => text.to_string(),
        };
        CommonMarkViewer::new()
            .max_image_width(Some(640))
            .show(ui, cache, &text);
    }
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(theme::bg_raised())
        .stroke(Stroke::new(1.0, theme::border()))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn who_and_when(ui: &mut egui::Ui, who: &str, what: &str, when: i64, now: i64) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(who).strong());
        let age = crate::format::age(when, now);
        let text = if what.is_empty() {
            age
        } else {
            format!("{what} · {age}")
        };
        ui.label(RichText::new(text).color(theme::muted()));
    });
}

fn draft_badge(ui: &mut egui::Ui) {
    ui.label(
        RichText::new(" Draft ")
            .size(11.0)
            .color(theme::modified())
            .background_color(theme::modified().gamma_multiply(0.15)),
    );
}

/// A draft: marked, with Edit and Delete.
fn draft_card(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    (index, draft): (usize, &Draft),
    composer: &mut Option<Composer>,
    action: &mut Option<Action>,
) {
    if composer.as_ref().is_some_and(|c| c.editing == Some(index)) {
        if let Some(a) = composer_box(ui, composer) {
            *action = Some(a);
        }
        return;
    }
    egui::Frame::new()
        .fill(theme::modified().gamma_multiply(0.06))
        .stroke(Stroke::new(1.0, theme::modified().gamma_multiply(0.5)))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                draft_badge(ui);
                ui.label(
                    RichText::new("Only you can see it until you finish the review.")
                        .size(11.0)
                        .color(theme::muted()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Delete").clicked() {
                        *action = Some(Action::DeleteDraft(index));
                    }
                    if ui.small_button("Edit").clicked() {
                        *composer = Some(Composer {
                            at: draft.at.clone(),
                            reply_to: draft.reply_to.clone(),
                            body: draft.body.clone(),
                            editing: Some(index),
                        });
                    }
                });
            });
            body_with_suggestions(ui, cache, (&draft.body, &[]), None, action);
        });
}

/// Writing a comment: Add to review, or Cancel.
fn composer_box(ui: &mut egui::Ui, composer: &mut Option<Composer>) -> Option<Action> {
    let c = composer.as_mut()?;
    let mut action = None;
    let mut cancel = false;
    egui::Frame::new()
        .fill(theme::bg_raised())
        .stroke(Stroke::new(1.0, theme::accent()))
        .corner_radius(6)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(at) = &c.at {
                ui.label(RichText::new(place(at)).size(11.0).color(theme::muted()));
            }
            let edit = ui.add(
                egui::TextEdit::multiline(&mut c.body)
                    .hint_text("Leave a comment (Markdown)")
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            ui.ctx()
                .accesskit_node_builder(edit.id, |n| n.set_label("Comment"));
            if c.body.is_empty() && !edit.has_focus() && ui.memory(|m| m.focused().is_none()) {
                edit.request_focus();
            }
            ui.horizontal(|ui| {
                let label = if c.editing.is_some() {
                    "Save draft"
                } else {
                    "Add to review"
                };
                if ui
                    .add_enabled(!c.body.trim().is_empty(), egui::Button::new(label))
                    .clicked()
                {
                    action = Some(Action::SaveDraft(
                        Draft {
                            at: c.at.clone(),
                            reply_to: c.reply_to.clone(),
                            body: c.body.trim().to_string(),
                            ..Draft::default()
                        },
                        c.editing,
                    ));
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    if cancel {
        *composer = None;
    }
    if action.is_some() {
        *composer = None;
    }
    action
}

/// "src/map.rs line 12" or "lines 10–12".
fn place(at: &Anchor) -> String {
    let n = |l: &Line| l.new.or(l.old).unwrap_or(0);
    match at.start {
        Some(s) if n(&s) != n(&at.line) => format!("{}, lines {}–{}", at.path, n(&s), n(&at.line)),
        _ => format!("{}, line {}", at.path, n(&at.line)),
    }
}

/// A posted comment: who and when, and its text (with any suggested
/// changes); yours can be edited and deleted.
fn posted(
    ui: &mut egui::Ui,
    c: &review::Comment,
    what: &str,
    now: i64,
    state: &mut RequestState,
    action: &mut Option<Action>,
) {
    let mine = state.me.as_deref() == Some(c.author.as_str());
    let sending = state.sending.is_some();
    ui.horizontal(|ui| {
        ui.label(RichText::new(&c.author).strong());
        let age = crate::format::age(c.created, now);
        let text = if what.is_empty() {
            age
        } else {
            format!("{what} · {age}")
        };
        ui.label(RichText::new(text).color(theme::muted()));
        if mine && state.editing_comment.is_none() && state.deleting_comment.is_none() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let delete = ui
                    .small_button(icon::TRASH)
                    .on_hover_text("Delete your comment");
                ui.ctx()
                    .accesskit_node_builder(delete.id, |n| n.set_label("Delete comment"));
                if delete.clicked() {
                    state.deleting_comment = Some(c.id.clone());
                }
                let edit = ui
                    .small_button(icon::PENCIL_SIMPLE)
                    .on_hover_text("Edit your comment");
                ui.ctx()
                    .accesskit_node_builder(edit.id, |n| n.set_label("Edit comment"));
                if edit.clicked() {
                    state.editing_comment = Some((c.id.clone(), c.body.clone()));
                }
            });
        }
    });
    // Being edited: the text, then Save or Cancel (it's changed straight
    // away, not kept for the review).
    let mut close = false;
    if let Some((id, text)) = state.editing_comment.as_mut().filter(|(id, _)| *id == c.id) {
        let edit = ui.add(
            egui::TextEdit::multiline(text)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.ctx()
            .accesskit_node_builder(edit.id, |n| n.set_label("Edit the comment"));
        ui.horizontal(|ui| {
            let ready = !text.trim().is_empty() && !sending;
            if ui.add_enabled(ready, egui::Button::new("Save")).clicked() {
                *action = Some(Action::EditComment(c.clone(), text.trim().to_string()));
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
            if sending {
                ui.spinner();
            }
        });
        let _ = id;
        if close {
            state.editing_comment = None;
        }
        return;
    }
    if state.deleting_comment.as_deref() == Some(c.id.as_str()) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Delete this comment, for everyone?").color(theme::danger()));
            if ui
                .add_enabled(!sending, egui::Button::new("Delete"))
                .clicked()
            {
                *action = Some(Action::DeleteComment(c.clone()));
            }
            if ui.button("Cancel").clicked() {
                state.deleting_comment = None;
            }
        });
    }
    let frame = Some((state.gitlab, state.on_branch, &mut state.batch));
    body_with_suggestions(
        ui,
        &mut state.markdown,
        (&c.body, &c.suggestions),
        frame,
        action,
    );
}

/// A thread: its comments, drafts answering it, and Reply and Resolve.
#[expect(
    clippy::too_many_arguments,
    reason = "a thread shows in the conversation and the diff"
)]
fn thread_card(
    ui: &mut egui::Ui,
    t: &Thread,
    code: Option<&[review::PatchLine]>,
    in_changes: bool,
    state: &mut RequestState,
    drafts: &[Draft],
    now: i64,
    action: &mut Option<Action>,
) {
    let id = egui::Id::new(("thread-open", &t.id));
    let mut open = ui.data(|m| m.get_temp::<bool>(id)).unwrap_or(!t.resolved);
    card(ui, |ui| {
        ui.horizontal(|ui| {
            if let Some(at) = &t.at {
                ui.label(
                    RichText::new(format!("{}  {}", icon::FILE_CODE, place(at)))
                        .size(12.0)
                        .color(theme::muted()),
                );
            }
            if t.outdated {
                ui.label(RichText::new("Outdated").size(11.0).color(theme::muted()));
            }
            if t.resolved {
                ui.label(
                    RichText::new(format!("{}  Resolved", icon::CHECK))
                        .size(11.0)
                        .color(theme::add()),
                );
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if t.resolved {
                    let label = if open { "Hide" } else { "Show" };
                    if ui.small_button(label).clicked() {
                        open = !open;
                    }
                }
            });
        });
        if !open {
            let first = t
                .comments
                .first()
                .map(|c| c.author.as_str())
                .unwrap_or_default();
            ui.label(
                RichText::new(format!(
                    "{first} and {} more",
                    t.comments.len().saturating_sub(1)
                ))
                .size(12.0)
                .color(theme::muted()),
            );
            return;
        }
        if let Some(code) = code.filter(|c| !c.is_empty()) {
            excerpt(ui, code);
        }
        for c in &t.comments {
            ui.add_space(4.0);
            posted(ui, c, "", now, state, action);
        }
        for (i, d) in drafts.iter().enumerate() {
            if d.reply_to.as_deref() == Some(t.reply.as_str()) && !t.reply.is_empty() {
                ui.add_space(4.0);
                draft_card(ui, &mut state.markdown, (i, d), &mut state.composer, action);
            }
        }
        ui.add_space(4.0);
        let replying = state.composer.as_ref().is_some_and(|c| {
            c.editing.is_none() && c.reply_to.as_deref() == Some(t.reply.as_str())
        });
        if replying {
            if let Some(a) = composer_box(ui, &mut state.composer) {
                *action = Some(a);
            }
        } else {
            ui.horizontal(|ui| {
                if ui
                    .small_button(format!("{}  Reply", icon::ARROW_BEND_UP_LEFT))
                    .clicked()
                {
                    state.composer = Some(Composer {
                        reply_to: Some(t.reply.clone()),
                        ..Composer::default()
                    });
                }
                if t.resolvable {
                    let (label, to) = if t.resolved {
                        ("Unresolve", false)
                    } else {
                        ("Resolve", true)
                    };
                    if ui.small_button(label).clicked() {
                        *action = Some(Action::Resolve(t.id.clone(), to));
                    }
                }
                if !in_changes
                    && t.at.is_some()
                    && !t.outdated
                    && ui.small_button("Show in Changes").clicked()
                {
                    go_to_thread(ui, t, state);
                }
            });
        }
    });
    ui.data_mut(|m| m.insert_temp(id, open));
}

/// What a suggestion card can do: whether it's GitLab (applied there) or
/// GitHub (applied here, on the request's branch), whether that branch is
/// checked out, and the suggestions picked to apply together.
type SuggestionFrame<'a> = Option<(bool, bool, &'a mut std::collections::BTreeSet<String>)>;

/// A comment's body: its text as Markdown, and each suggested change as a
/// card (with Apply, when `frame` says how).
fn body_with_suggestions(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    (body, suggestions): (&str, &[Suggestion]),
    mut frame: SuggestionFrame<'_>,
    action: &mut Option<Action>,
) {
    let parts = review::parts(body);
    if !parts.iter().any(|p| matches!(p, Part::Suggestion { .. })) {
        markdown(ui, cache, body);
        return;
    }
    let mut n = 0;
    for part in parts {
        match part {
            Part::Text(text) => markdown(ui, cache, &text),
            Part::Suggestion { to, .. } => {
                let g = suggestions.get(n);
                n += 1;
                let frame = frame
                    .as_mut()
                    .map(|(gitlab, on, batch)| (*gitlab, *on, &mut **batch));
                if let Some(a) = suggestion_card(ui, g, &to, frame) {
                    *action = Some(a);
                }
            }
        }
    }
}

/// A suggested change: the lines now (red) and what they'd become (green),
/// and applying it.
fn suggestion_card(
    ui: &mut egui::Ui,
    g: Option<&Suggestion>,
    to: &str,
    frame: SuggestionFrame<'_>,
) -> Option<Action> {
    let mut action = None;
    egui::Frame::new()
        .fill(theme::bg())
        .stroke(Stroke::new(1.0, theme::border()))
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{}  Suggested change", icon::PENCIL_SIMPLE))
                        .size(12.0)
                        .strong(),
                );
                if let Some(g) = g {
                    let lines = if g.start == g.end {
                        format!("line {}", g.start)
                    } else {
                        format!("lines {}–{}", g.start, g.end)
                    };
                    ui.label(RichText::new(lines).size(11.0).color(theme::muted()));
                    if g.applied {
                        ui.label(
                            RichText::new(format!("{}  Applied", icon::CHECK))
                                .size(11.0)
                                .color(theme::add()),
                        );
                    } else if !g.appliable {
                        ui.label(RichText::new("Outdated").size(11.0).color(theme::muted()))
                            .on_hover_text("The lines changed since: it can't be applied as it is");
                    }
                }
            });
            ui.spacing_mut().item_spacing.y = 0.0;
            let row = |ui: &mut egui::Ui, sign: &str, text: &str, colour, bg| {
                egui::Frame::new().fill(bg).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(format!("{sign} {text}"))
                            .font(theme::mono(12.0))
                            .color(colour),
                    );
                });
            };
            if let Some(g) = g {
                for line in g.from.lines() {
                    row(ui, "−", line, theme::del(), theme::del_bg());
                }
            }
            for line in to.lines() {
                row(ui, "+", line, theme::add(), theme::add_bg());
            }
            if to.is_empty() {
                ui.label(
                    RichText::new("(removes the lines)")
                        .size(11.0)
                        .color(theme::muted()),
                );
            }
            ui.spacing_mut().item_spacing.y = 4.0;
            let (Some(g), Some((gitlab, on_branch, batch))) = (g, frame) else {
                return;
            };
            if g.applied || !g.appliable {
                return;
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if gitlab {
                    let apply = ui.small_button("Apply").on_hover_text(
                        "GitLab commits it to the request's branch (pull to get it here)",
                    );
                    ui.ctx()
                        .accesskit_node_builder(apply.id, |n| n.set_label("Apply suggestion"));
                    if apply.clicked() {
                        action = Some(Action::ApplySuggestions(
                            vec![g.id.clone()],
                            String::new(),
                        ));
                    }
                    let mut picked = batch.contains(&g.id);
                    if ui
                        .checkbox(&mut picked, "Apply with others")
                        .on_hover_text("Pick several, then apply them together in one commit")
                        .changed()
                    {
                        if picked {
                            batch.insert(g.id.clone());
                        } else {
                            batch.remove(&g.id);
                        }
                    }
                } else {
                    let apply = ui
                        .add_enabled(on_branch, egui::Button::new("Apply here").small())
                        .on_hover_text(
                            "Changes the file on its branch here; commit and push it when you're ready",
                        )
                        .on_disabled_hover_text(
                            "Check out its branch first (GitHub can't apply it for you)",
                        );
                    ui.ctx().accesskit_node_builder(apply.id, |n| {
                        n.set_label("Apply suggestion here");
                    });
                    if apply.clicked() {
                        action = Some(Action::ApplyHere(g.clone()));
                    }
                }
            });
        });
    action
}

/// Suggestions picked to apply together: how many, and applying them.
fn batch_bar(ui: &mut egui::Ui, state: &mut RequestState) -> Option<Action> {
    if state.batch.is_empty() {
        return None;
    }
    let mut action = None;
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin {
            left: 16,
            right: 12,
            top: 4,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let n = state.batch.len();
                let s = if n == 1 { "" } else { "s" };
                ui.label(RichText::new(format!("{n} suggestion{s} picked")).strong());
                let message = ui.add(
                    egui::TextEdit::singleline(&mut state.batch_message)
                        .hint_text("Commit message (optional)")
                        .desired_width(320.0),
                );
                ui.ctx().accesskit_node_builder(message.id, |n| {
                    n.set_label("Suggestions commit message");
                });
                let go = egui::Button::new(format!("Apply {n} in one commit"));
                if ui.add_enabled(state.sending.is_none(), go).clicked() {
                    action = Some(Action::ApplySuggestions(
                        state.batch.iter().cloned().collect(),
                        state.batch_message.clone(),
                    ));
                }
                if ui.small_button("Clear").clicked() {
                    state.batch.clear();
                }
            });
        });
    action
}

/// A few lines of code, as in the diff.
fn excerpt(ui: &mut egui::Ui, lines: &[review::PatchLine]) {
    egui::Frame::new()
        .fill(theme::bg())
        .corner_radius(4)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            for l in lines {
                let colour = match l.kind {
                    '+' => theme::add(),
                    '-' => theme::del(),
                    _ => theme::text(),
                };
                let n = l
                    .line
                    .new
                    .or(l.line.old)
                    .map(|n| n.to_string())
                    .unwrap_or_default();
                ui.label(
                    RichText::new(format!("{n:>4} {} {}", l.kind, l.text))
                        .font(theme::mono(12.0))
                        .color(colour),
                );
            }
        });
}

/// The code a thread is about: from its file's patch, or as the forge kept it.
fn code_for(o: &Overview, t: &Thread) -> Vec<review::PatchLine> {
    let Some(at) = &t.at else { return Vec::new() };
    if !t.outdated
        && let Some(f) = o.files.iter().find(|f| f.path == at.path)
    {
        let lines = review::excerpt(&f.patch, at);
        if !lines.is_empty() {
            return lines;
        }
    }
    // GitHub's hunk ends at the line: its last few.
    let lines: Vec<_> = review::number(&t.hunk)
        .into_iter()
        .filter(|l| l.kind != '@')
        .collect();
    lines[lines.len().saturating_sub(4)..].to_vec()
}

fn conversation(
    ui: &mut egui::Ui,
    kind: Kind,
    r: &Request,
    o: &Overview,
    state: &mut RequestState,
    here: &Here<'_>,
    action: &mut Option<Action>,
) {
    egui::ScrollArea::vertical()
        .id_salt("request-conversation")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    ui.set_max_width(900.0_f32.min(ui.available_width()));
                    ui.spacing_mut().item_spacing.y = 8.0;
                    card(ui, |ui| {
                        who_and_when(ui, &r.author, "opened it", r.created, here.now);
                        ui.add_space(2.0);
                        markdown(ui, &mut state.markdown, &r.body);
                    });
                    if !o.checks.is_empty() {
                        card(ui, |ui| {
                            ui.label(RichText::new("Checks").strong());
                            for c in &o.checks {
                                let (glyph, colour) = match c.outcome {
                                    Outcome::Passed => (icon::CHECK_CIRCLE, theme::add()),
                                    Outcome::Failed => (icon::X_CIRCLE, theme::danger()),
                                    Outcome::Running => (icon::CLOCK, theme::modified()),
                                };
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(glyph).color(colour));
                                    if c.url.is_empty() {
                                        ui.label(&c.name);
                                    } else if ui
                                        .link(&c.name)
                                        .on_hover_text("See its log")
                                        .clicked()
                                    {
                                        *action = Some(Action::Open(c.url.clone()));
                                    }
                                });
                            }
                        });
                    }
                    for e in &o.events {
                        match e {
                            Event::Comment(c) => card(ui, |ui| {
                                posted(ui, c, "commented", here.now, state, action);
                            }),
                            Event::Review {
                                author,
                                verdict,
                                body,
                                created,
                            } => {
                                let (glyph, colour, said) = match verdict {
                                    Verdict::Approved => {
                                        (icon::CHECK_CIRCLE, theme::add(), "approved")
                                    }
                                    Verdict::ChangesRequested => (
                                        icon::WARNING_CIRCLE,
                                        theme::modified(),
                                        "asked for changes",
                                    ),
                                    Verdict::Commented => {
                                        (icon::CHAT_CIRCLE, theme::muted(), "reviewed")
                                    }
                                };
                                ui.horizontal(|ui| {
                                    ui.add_space(12.0);
                                    ui.label(RichText::new(glyph).color(colour));
                                    ui.label(RichText::new(author).strong());
                                    ui.label(
                                        RichText::new(format!(
                                            "{said} · {}",
                                            crate::format::age(*created, here.now)
                                        ))
                                        .color(theme::muted()),
                                    );
                                });
                                if !body.trim().is_empty() {
                                    card(ui, |ui| markdown(ui, &mut state.markdown, body));
                                }
                            }
                            Event::Thread(i) => {
                                if let Some(t) = o.threads.get(*i) {
                                    let code = code_for(o, t);
                                    thread_card(
                                        ui,
                                        t,
                                        Some(&code),
                                        false,
                                        state,
                                        here.drafts,
                                        here.now,
                                        action,
                                    );
                                }
                            }
                        }
                    }
                    // Draft comments on the whole request.
                    for (i, d) in here.drafts.iter().enumerate() {
                        if d.at.is_none() && d.reply_to.is_none() {
                            draft_card(
                                ui,
                                &mut state.markdown,
                                (i, d),
                                &mut state.composer,
                                action,
                            );
                        }
                    }
                    ui.add_space(6.0);
                    ui.label(RichText::new("Add a comment").strong());
                    let edit = ui.add(
                        egui::TextEdit::multiline(&mut state.comment)
                            .hint_text(format!(
                                "A comment on the whole {} (Markdown)",
                                kind.request().to_lowercase()
                            ))
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    );
                    ui.ctx()
                        .accesskit_node_builder(edit.id, |n| n.set_label("Comment on the request"));
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !state.comment.trim().is_empty(),
                                egui::Button::new("Add to review"),
                            )
                            .on_hover_text("It's sent with the rest when you finish the review")
                            .clicked()
                        {
                            *action = Some(Action::SaveDraft(
                                Draft {
                                    at: None,
                                    reply_to: None,
                                    body: state.comment.trim().to_string(),
                                    ..Draft::default()
                                },
                                None,
                            ));
                            state.comment.clear();
                        }
                    });
                });
        });
}

fn commits(ui: &mut egui::Ui, o: &Overview, now: i64, action: &mut Option<Action>) {
    egui::ScrollArea::vertical()
        .id_salt("request-commits")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::symmetric(16, 12))
                .show(ui, |ui| {
                    if o.commits.is_empty() {
                        ui.label(RichText::new("No commits.").color(theme::muted()));
                    } else if ui
                        .button(format!("{}  Show them in the graph", icon::GRAPH))
                        .on_hover_text("Back to the graph, with its commits marked")
                        .clicked()
                    {
                        *action = Some(Action::ShowInGraph(None));
                    }
                    for c in &o.commits {
                        let row = ui
                            .horizontal(|ui| {
                                ui.label(
                                    RichText::new(crate::format::short_sha(&c.sha))
                                        .font(theme::mono(12.0))
                                        .color(theme::accent()),
                                );
                                ui.label(&c.summary);
                                ui.label(
                                    RichText::new(format!(
                                        "{} · {}",
                                        c.author,
                                        crate::format::age(c.created, now)
                                    ))
                                    .color(theme::muted()),
                                );
                            })
                            .response;
                        let row = ui
                            .interact(row.rect, ui.id().with(("commit", &c.sha)), Sense::click())
                            .on_hover_text("Show it in the graph, with the others marked");
                        if row.hovered() {
                            ui.painter().rect_filled(
                                row.rect.expand(2.0),
                                4,
                                theme::bg_hover().gamma_multiply(0.5),
                            );
                        }
                        if row.clicked() {
                            *action = Some(Action::ShowInGraph(Some(c.sha.clone())));
                        }
                    }
                });
        });
}

/// A patch, as a diff the diff view can lay out.
pub fn file_diff(f: &review::File) -> FileDiff {
    let mut hunks: Vec<Hunk> = Vec::new();
    for l in review::number(&f.patch) {
        if l.kind == '@' {
            hunks.push(Hunk {
                header: l.text,
                lines: Vec::new(),
            });
            continue;
        }
        if hunks.is_empty() {
            hunks.push(Hunk {
                header: String::new(),
                lines: Vec::new(),
            });
        }
        let kind = match l.kind {
            '+' => LineKind::Add,
            '-' => LineKind::Delete,
            _ => LineKind::Context,
        };
        if let Some(h) = hunks.last_mut() {
            h.lines.push(DiffLine {
                kind,
                old_line: l.line.old,
                new_line: l.line.new,
                content: l.text,
            });
        }
    }
    FileDiff {
        path: f.path.clone(),
        old_path: f.old_path.clone(),
        status: match f.change {
            Change::Added => FileStatus::Added,
            Change::Deleted => FileStatus::Deleted,
            Change::Renamed => FileStatus::Renamed,
            Change::Modified => FileStatus::Modified,
        },
        binary: f.patch.is_empty(),
        hunks,
        truncated: false,
    }
}

fn line_of(d: &DiffLine) -> Line {
    Line {
        old: d.old_line,
        new: d.new_line,
    }
}

/// Where a comment on the picked lines goes (the range ends at the last).
fn picked_anchor(d: &RenderedDiff, state: &RequestState) -> Option<Anchor> {
    let last = *state.picked.last()?;
    // Lines shown only by the whole file can't take comments.
    if state.picked.iter().any(|(h, _)| *h >= d.changes) {
        return None;
    }
    let first = state
        .picked
        .iter()
        .find(|(h, _)| *h == last.0)
        .copied()
        .unwrap_or(last);
    let line = line_of(&d.diff.hunks[last.0].lines[last.1]);
    let start = line_of(&d.diff.hunks[first.0].lines[first.1]);
    Some(Anchor {
        path: d.diff.path.clone(),
        old_path: d.diff.old_path.clone(),
        line,
        start: (first != last).then_some(start),
    })
}

fn changes(
    ui: &mut egui::Ui,
    o: &Overview,
    state: &mut RequestState,
    here: &Here<'_>,
    action: &mut Option<Action>,
) {
    if o.files.is_empty() {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| ui.label(RichText::new("No changes.").color(theme::muted())));
        return;
    }
    if state
        .file
        .as_ref()
        .is_none_or(|f| !o.files.iter().any(|x| &x.path == f))
    {
        state.file = o.files.first().map(|f| f.path.clone());
        state.picked.clear();
    }
    egui::Panel::left("request-files")
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
                .id_salt("request-file-list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for f in &o.files {
                        let open = o.threads_on(&f.path).filter(|(_, t)| !t.resolved).count();
                        let drafts = here
                            .drafts
                            .iter()
                            .filter(|d| d.at.as_ref().is_some_and(|a| a.path == f.path))
                            .count();
                        let on = state.file.as_deref() == Some(f.path.as_str());
                        let (dir, name) = crate::format::split_path(&f.path);
                        let (letter, colour) = match f.change {
                            Change::Added => ("A", theme::add()),
                            Change::Deleted => ("D", theme::del()),
                            Change::Renamed => ("R", theme::accent()),
                            Change::Modified => ("M", theme::modified()),
                        };
                        let row = ui
                            .horizontal(|ui| {
                                ui.set_width(ui.available_width());
                                ui.label(
                                    RichText::new(letter)
                                        .font(theme::mono(12.0))
                                        .color(colour)
                                        .strong(),
                                );
                                ui.label(RichText::new(name).color(if on {
                                    theme::text()
                                } else {
                                    theme::text().gamma_multiply(0.9)
                                }));
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
                                        if drafts > 0 {
                                            ui.label(
                                                RichText::new(format!("{drafts}"))
                                                    .size(11.0)
                                                    .color(theme::modified()),
                                            )
                                            .on_hover_text("Your draft comments");
                                        }
                                        if open > 0 {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{}{open}",
                                                    icon::CHAT_CIRCLE
                                                ))
                                                .size(11.0)
                                                .color(theme::accent()),
                                            )
                                            .on_hover_text("Open threads");
                                        }
                                    },
                                );
                            })
                            .response;
                        let row =
                            ui.interact(row.rect, ui.id().with(("file", &f.path)), Sense::click());
                        row.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::SelectableLabel,
                                true,
                                on,
                                &f.path,
                            )
                        });
                        if on {
                            ui.painter().rect_filled(
                                row.rect.expand(2.0),
                                4,
                                theme::accent().gamma_multiply(0.15),
                            );
                        } else if row.hovered() {
                            ui.painter().rect_filled(
                                row.rect.expand(2.0),
                                4,
                                theme::bg_hover().gamma_multiply(0.5),
                            );
                        }
                        if row.clicked() && !on {
                            state.file = Some(f.path.clone());
                            state.picked.clear();
                            state.pick_from = None;
                        }
                    }
                });
        });
    let Some(f) = state
        .file
        .as_ref()
        .and_then(|p| o.files.iter().find(|f| &f.path == p))
    else {
        return;
    };
    // The whole file and blame need the request's commits here.
    let local = state.local.clone();
    let why_not = match &local {
        None => Some("Fetching its commits…".to_string()),
        Some(Err(e)) => Some(format!("Needs its commits here, but {e}")),
        Some(Ok(())) => None,
    };
    let whole = state
        .whole
        .then(|| state.wholes.get(&f.path).cloned())
        .flatten();
    let d = match &whole {
        Some(Ok(d)) => Arc::clone(d),
        _ => Arc::clone(
            state
                .rendered
                .entry(f.path.clone())
                .or_insert_with(|| Arc::new(RenderedDiff::new(file_diff(f)))),
        ),
    };
    // The file's name, and how it's shown.
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin::symmetric(12, 0))
        .show(ui, |ui| {
            ui.set_height(36.0);
            ui.set_width(ui.available_width());
            ui.horizontal_centered(|ui| {
                let title = match &f.old_path {
                    Some(old) => format!("{old}  {}  {}", icon::ARROW_RIGHT, f.path),
                    None => f.path.clone(),
                };
                ui.label(RichText::new(title).strong());
                let loading = (state.whole && whole.is_none() && why_not.is_none())
                    || (state.blame && !state.blames.contains_key(&f.path) && why_not.is_none())
                    || (why_not.is_some() && local.is_none() && (state.whole || state.blame));
                if loading {
                    ui.spinner();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let blame = ui.add_enabled(
                        why_not.is_none(),
                        egui::Button::new(
                            RichText::new(format!("{}  Blame", icon::USER_LIST)).size(12.0),
                        )
                        .selected(state.blame),
                    );
                    ui.ctx()
                        .accesskit_node_builder(blame.id, |n| n.set_label("Blame"));
                    let blame = blame
                        .on_hover_text("Who last changed each line")
                        .on_disabled_hover_text(why_not.clone().unwrap_or_default());
                    if blame.clicked() {
                        state.blame = !state.blame;
                    }
                    let chosen =
                        widgets::segmented(ui, state.split, &[(true, "Split"), (false, "Unified")]);
                    if chosen != state.split {
                        state.split = chosen;
                        state.picked.clear();
                    }
                    let chosen = widgets::segmented(
                        ui,
                        state.whole,
                        &[(true, "Whole file"), (false, "Changes")],
                    );
                    if chosen != state.whole {
                        state.whole = chosen;
                        state.picked.clear();
                    }
                    ui.add_space(8.0);
                    thread_nav(ui, o, state);
                });
            });
        });
    if state.whole
        && let Some(why) = why_not.as_ref().filter(|_| local.is_some())
    {
        ui.label(
            RichText::new(format!("  Whole file: {why}."))
                .size(12.0)
                .color(theme::muted()),
        );
    }
    if let Some(Err(e)) = &whole {
        ui.label(
            RichText::new(format!("  The whole file couldn't be shown: {e}."))
                .size(12.0)
                .color(theme::muted()),
        );
    }
    if d.diff.binary {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("The forge doesn't show this file's changes (binary, or too big).")
                    .color(theme::muted()),
            )
        });
        return;
    }
    // Threads on code that's changed since: not on a line any more.
    let outdated: Vec<&Thread> = o
        .threads
        .iter()
        .filter(|t| t.outdated && t.at.as_ref().is_some_and(|a| a.path == f.path))
        .collect();
    if !outdated.is_empty() {
        let n = outdated.len();
        let s = if n == 1 { "" } else { "s" };
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(12, 4))
            .show(ui, |ui| {
                egui::CollapsingHeader::new(
                    RichText::new(format!(
                        "{n} outdated thread{s}: on code that has changed since"
                    ))
                    .color(theme::muted()),
                )
                .id_salt(("outdated", &f.path))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt(("outdated-list", &f.path))
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for t in outdated {
                                let code = code_for(o, t);
                                thread_card(
                                    ui,
                                    t,
                                    Some(&code),
                                    true,
                                    state,
                                    here.drafts,
                                    here.now,
                                    action,
                                );
                            }
                        });
                });
            });
    }
    let blame = state.blame.then(|| {
        state
            .blames
            .get(&f.path)
            .and_then(|b| b.as_ref().ok())
            .cloned()
    });
    diff_with_threads(
        ui,
        &d,
        o,
        state,
        (here, blame.as_ref().map(|b| b.as_deref())),
        action,
    );
}

/// Previous / Next through the threads on the code, across files, and
/// which one of them you're on.
fn thread_nav(ui: &mut egui::Ui, o: &Overview, state: &mut RequestState) {
    let threads = o.code_threads();
    if threads.is_empty() {
        return;
    }
    let n = threads.len();
    let current = state
        .at_thread
        .as_ref()
        .and_then(|id| threads.iter().position(|t| &t.id == id));
    // Right to left: Next, the count, Previous.
    let next = ui
        .add(egui::Button::new(icon::CARET_DOWN).small())
        .on_hover_text("Next comment");
    ui.ctx()
        .accesskit_node_builder(next.id, |b| b.set_label("Next comment"));
    let s = if n == 1 { "" } else { "s" };
    let count = match current {
        Some(i) => format!("{} of {n}", i + 1),
        None => format!("{n} thread{s}"),
    };
    ui.label(RichText::new(count).size(12.0).color(theme::muted()));
    let previous = ui
        .add(egui::Button::new(icon::CARET_UP).small())
        .on_hover_text("Previous comment");
    ui.ctx()
        .accesskit_node_builder(previous.id, |b| b.set_label("Previous comment"));
    let to = if next.clicked() {
        Some(current.map_or(0, |i| (i + 1) % n))
    } else if previous.clicked() {
        Some(current.map_or(n - 1, |i| (i + n - 1) % n))
    } else {
        None
    };
    if let Some(t) = to.map(|i| threads[i]) {
        go_to_thread(ui, t, state);
    }
}

/// Opens a thread's file in Changes, and the thread, and scrolls to it.
fn go_to_thread(ui: &egui::Ui, t: &Thread, state: &mut RequestState) {
    let Some(at) = &t.at else {
        return;
    };
    state.tab = Tab::Changes;
    if state.file.as_ref() != Some(&at.path) {
        state.file = Some(at.path.clone());
        state.picked.clear();
        state.pick_from = None;
    }
    state.at_thread = Some(t.id.clone());
    state.scroll_to_thread = true;
    ui.data_mut(|m| m.insert_temp(egui::Id::new(("thread-open", &t.id)), true));
}

/// A row of a request's diff.
#[derive(Clone, Copy)]
enum Shown {
    Header(usize),
    One((usize, usize)),
    /// Side by side: the old line and the new.
    Pair(Option<(usize, usize)>, Option<(usize, usize)>),
}

/// The diff, unified or side by side, with blame down its left if shown,
/// and each line's threads, drafts and the comment being written under it.
fn diff_with_threads(
    ui: &mut egui::Ui,
    d: &RenderedDiff,
    o: &Overview,
    state: &mut RequestState,
    (here, blame): (&Here<'_>, Option<Option<&Blame>>),
    action: &mut Option<Action>,
) {
    let char_width = ui
        .painter()
        .layout_no_wrap("M".repeat(20), theme::mono(theme::MONO_SIZE), theme::text())
        .size()
        .x
        / 20.0;
    #[expect(clippy::cast_precision_loss, reason = "line lengths are small")]
    let code_width = d.widest as f32 * char_width + 24.0;
    let path = d.diff.path.clone();
    let threads: Vec<(usize, &Thread)> = o.threads_on(&path).collect();
    let split = state.split;
    let rows: Vec<Shown> = if split {
        d.split
            .iter()
            .map(|r| match *r {
                SplitRow::Header(h) => Shown::Header(h),
                SplitRow::Pair(a, b) => Shown::Pair(a, b),
            })
            .collect()
    } else {
        d.unified
            .iter()
            .map(|r| match *r {
                Row::Header(h) => Shown::Header(h),
                Row::Line(h, l) => Shown::One((h, l)),
            })
            .collect()
    };
    let lead = if blame.is_some() { GUTTER } else { 0.0 };
    let blamed = blame.flatten().map(|b| (b, file_view::ages(b)));
    egui::ScrollArea::both()
        .id_salt(("request-diff", &path, split))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let view_width = ui.available_width();
            let half = ((view_width - lead) / 2.0).max(NUMBER + 18.0 + code_width);
            let width = if split {
                lead + 2.0 * half
            } else {
                (lead + 2.0 * NUMBER + 18.0 + code_width).max(view_width)
            };
            ui.spacing_mut().item_spacing.y = 0.0;
            // The blame's commit on the row above, for where a run starts.
            let mut above: Option<usize> = None;
            for row in rows {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(width, LINE), Sense::hover());
                let body = Rect::from_min_max(Pos2::new(rect.left() + lead, rect.top()), rect.max);
                if let Shown::Header(h) = row {
                    crate::ui::diff::header(ui.painter(), rect, &d.diff.hunks[h].header);
                    continue;
                }
                // Blame, for the new side's line.
                if blame.is_some() {
                    let cell = Rect::from_min_size(rect.min, egui::vec2(lead, LINE));
                    let at = match row {
                        Shown::One(at) => Some(at),
                        Shown::Pair(_, b) => b,
                        Shown::Header(_) => None,
                    };
                    let new_line = at
                        .and_then(|(h, l)| d.diff.hunks[h].lines[l].new_line)
                        .and_then(|n| usize::try_from(n).ok())
                        .and_then(|n| n.checked_sub(1));
                    match blamed {
                        Some((b, ages)) => {
                            let commit = new_line.and_then(|n| b.lines.get(n)).map(|l| l.commit);
                            let starts = commit.is_some() && commit != above;
                            if commit.is_some() {
                                above = commit;
                            }
                            if let Some(id) =
                                file_view::blame_cell(ui, cell, b, new_line, starts, ages, here.now)
                            {
                                *action = Some(Action::GoToCommit(id));
                            }
                        }
                        None => {
                            ui.painter().rect_filled(cell, 0, theme::bg_raised());
                        }
                    }
                }
                let lines: Vec<(usize, usize)> = match row {
                    Shown::One(at) => {
                        let line = &d.diff.hunks[at.0].lines[at.1];
                        let picked = state.picked.contains(&at);
                        crate::ui::diff::unified_line(
                            ui.painter(),
                            body,
                            line,
                            &d.jobs[at.0][at.1],
                            picked,
                        );
                        line_input(ui, (body, 2.0 * NUMBER), d, (at, 0), state);
                        vec![at]
                    }
                    Shown::Pair(a, b) => {
                        let l = Rect::from_min_size(body.min, egui::vec2(half, LINE));
                        let r = Rect::from_min_size(
                            Pos2::new(body.left() + half, body.top()),
                            egui::vec2(half, LINE),
                        );
                        crate::ui::diff::side(ui.painter(), l, d, a, (false, 0.0), &state.picked);
                        crate::ui::diff::side(ui.painter(), r, d, b, (true, 0.0), &state.picked);
                        ui.painter().vline(
                            l.right(),
                            l.y_range(),
                            Stroke::new(1.0, theme::border()),
                        );
                        for (at, half_rect, salt) in [(a, l, 0u8), (b, r, 1)] {
                            if let Some(at) = at {
                                line_input(ui, (half_rect, NUMBER), d, (at, salt), state);
                            }
                        }
                        let mut lines: Vec<(usize, usize)> = b.into_iter().chain(a).collect();
                        lines.dedup();
                        lines
                    }
                    Shown::Header(_) => Vec::new(),
                };
                under_row(ui, (d, &lines), &threads, state, (here, view_width), action);
            }
        });
    // Dragging over lines picks them all, from where it started.
    let drag = egui::Id::new("request-line-drag");
    if !ui.input(|i| i.pointer.primary_down()) {
        ui.data_mut(|m| m.remove::<bool>(drag));
    }
}

/// A line's own space (`rect`, with its code `code` from the left): a click
/// picks it, the + by it comments on it, and right-click has its menu.
/// Lines the whole file adds around the changes can only be copied.
fn line_input(
    ui: &mut egui::Ui,
    (rect, code): (Rect, f32),
    d: &RenderedDiff,
    (at, salt): ((usize, usize), u8),
    state: &mut RequestState,
) {
    let line = &d.diff.hunks[at.0].lines[at.1];
    let response = ui.interact(
        rect,
        ui.id().with(("request-line", at, salt)),
        Sense::click_and_drag(),
    );
    let picked = state.picked.contains(&at);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            true,
            picked,
            format!("Select line: {}", line.content.trim()),
        )
    });
    pick(ui, &response, (d, at), state);
    // Only the changes' own lines can be commented on.
    let commentable = at.0 < d.changes;
    if commentable && ui.rect_contains_pointer(rect) {
        let plus = Rect::from_center_size(
            Pos2::new(rect.left() + code + 9.0, rect.center().y),
            egui::vec2(16.0, 16.0),
        );
        let b = ui.put(
            plus,
            egui::Button::new(RichText::new("+").strong().color(theme::on_accent()))
                .fill(theme::accent_fill())
                .corner_radius(3),
        );
        ui.ctx()
            .accesskit_node_builder(b.id, |n| n.set_label("Comment on this line"));
        if b.on_hover_text("Comment on this line").clicked() {
            if !picked {
                state.picked.clear();
                state.picked.insert(at);
            }
            state.composer = picked_anchor(d, state).map(|at| Composer {
                at: Some(at),
                ..Composer::default()
            });
        }
    }
    response.context_menu(|ui| {
        let anchor = picked_anchor(d, state);
        let label = if state.picked.len() > 1 {
            "Comment on selection"
        } else {
            "Comment on this line"
        };
        if ui
            .add_enabled(anchor.is_some(), egui::Button::new(label))
            .on_disabled_hover_text(
                "Only the lines the request changes, and those around them, take comments",
            )
            .clicked()
        {
            state.composer = anchor.clone().map(|at| Composer {
                at: Some(at),
                ..Composer::default()
            });
            ui.close();
        }
        // The picked lines, as the request has them, to change.
        let suggest = anchor.and_then(|at| {
            let end = at.line.new?;
            let start = at.start.map_or(Some(end), |s| s.new)?;
            let text: Vec<&str> = state
                .picked
                .iter()
                .map(|&(h, l)| &d.diff.hunks[h].lines[l])
                .filter(|l| l.new_line.is_some_and(|n| (start..=end).contains(&n)))
                .map(|l| l.content.as_str())
                .collect();
            let whole = text.len() == (end - start + 1) as usize;
            whole.then(|| {
                (
                    at,
                    review::suggestion_block(state.gitlab, start, end, &text.join("\n")),
                )
            })
        });
        if ui
            .add_enabled(suggest.is_some(), egui::Button::new("Suggest a change"))
            .on_disabled_hover_text("Pick lines of the new version, one after another")
            .clicked()
            && let Some((at, body)) = suggest
        {
            state.composer = Some(Composer {
                at: Some(at),
                body,
                ..Composer::default()
            });
            ui.close();
        }
        if ui.button("Copy").clicked() {
            let text: Vec<&str> = state
                .picked
                .iter()
                .map(|&(h, l)| d.diff.hunks[h].lines[l].content.as_str())
                .collect();
            ui.ctx().copy_text(text.join("\n"));
            ui.close();
        }
        if ui.button("Select all lines").clicked() {
            state.picked = d
                .unified
                .iter()
                .filter_map(|r| match r {
                    Row::Line(h, l) => Some((*h, *l)),
                    Row::Header(_) => None,
                })
                .collect();
            ui.close();
        }
    });
}

/// What's under a row's lines: their threads, drafts, and the comment
/// being written.
fn under_row(
    ui: &mut egui::Ui,
    (d, lines): (&RenderedDiff, &[(usize, usize)]),
    threads: &[(usize, &Thread)],
    state: &mut RequestState,
    (here, view_width): (&Here<'_>, f32),
    action: &mut Option<Action>,
) {
    let path = &d.diff.path;
    let on = |a: &Anchor| {
        lines.iter().any(|&(h, l)| {
            let line = &d.diff.hunks[h].lines[l];
            h < d.changes && a.line.same(&line_of(line)) && line_side_matches(&a.line, line)
        })
    };
    let under: Vec<&Thread> = threads
        .iter()
        .filter(|(_, t)| t.at.as_ref().is_some_and(on))
        .map(|(_, t)| *t)
        .collect();
    let drafts: Vec<(usize, &Draft)> = here
        .drafts
        .iter()
        .enumerate()
        .filter(|(_, dr)| {
            dr.reply_to.is_none() && dr.at.as_ref().is_some_and(|a| &a.path == path && on(a))
        })
        .collect();
    let writing = state.composer.as_ref().is_some_and(|c| {
        c.editing.is_none()
            && c.reply_to.is_none()
            && c.at.as_ref().is_some_and(|a| &a.path == path && on(a))
    });
    if under.is_empty() && drafts.is_empty() && !writing {
        return;
    }
    egui::Frame::new()
        .fill(theme::bg())
        .inner_margin(egui::Margin {
            // Past the line numbers (2 × NUMBER).
            left: 104,
            right: 12,
            top: 6,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_max_width(view_width - 2.0 * NUMBER - 24.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            for t in under {
                let rect = ui
                    .scope(|ui| {
                        thread_card(ui, t, None, true, state, here.drafts, here.now, action);
                    })
                    .response
                    .rect;
                // The one gone to with Previous / Next: marked, and in view.
                if state.at_thread.as_ref() == Some(&t.id) {
                    ui.painter().rect_stroke(
                        rect.expand(2.0),
                        8,
                        Stroke::new(1.5, theme::accent()),
                        egui::StrokeKind::Outside,
                    );
                    if state.scroll_to_thread {
                        state.scroll_to_thread = false;
                        ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    }
                }
            }
            for (i, dr) in drafts {
                draft_card(
                    ui,
                    &mut state.markdown,
                    (i, dr),
                    &mut state.composer,
                    action,
                );
            }
            if writing && let Some(a) = composer_box(ui, &mut state.composer) {
                *action = Some(a);
            }
        });
}

/// A thread on one side of a line shows under it only once: an unchanged
/// line matches either side, a changed line only its own.
fn line_side_matches(at: &Line, line: &DiffLine) -> bool {
    match line.kind {
        LineKind::Add => at.new.is_some(),
        LineKind::Delete => at.old.is_some() && at.new.is_none(),
        _ => true,
    }
}

/// Clicking a line picks it (Ctrl adds, Shift picks a range); dragging
/// over lines picks them all.
fn pick(
    ui: &egui::Ui,
    response: &egui::Response,
    (d, at): (&RenderedDiff, (usize, usize)),
    state: &mut RequestState,
) {
    let mods = ui.input(|i| i.modifiers);
    let drag = egui::Id::new("request-line-drag");
    let range = |state: &mut RequestState, to: (usize, usize)| {
        let from = state.pick_from.unwrap_or(to);
        let (a, b) = if from <= to { (from, to) } else { (to, from) };
        state.picked = d
            .unified
            .iter()
            .filter_map(|r| match r {
                Row::Line(h, l) => Some((*h, *l)),
                Row::Header(_) => None,
            })
            .filter(|at| (a..=b).contains(at))
            .collect();
    };
    if response.drag_started() {
        ui.data_mut(|m| m.insert_temp(drag, true));
        if !mods.shift {
            state.pick_from = Some(at);
        }
        range(state, at);
        return;
    }
    if ui.data(|m| m.get_temp::<bool>(drag)).unwrap_or(false)
        && ui.input(|i| i.pointer.primary_down())
        && ui.rect_contains_pointer(response.rect)
    {
        range(state, at);
        return;
    }
    if response.clicked() {
        if mods.shift && state.pick_from.is_some() {
            range(state, at);
        } else if mods.command {
            if !state.picked.remove(&at) {
                state.picked.insert(at);
            }
            state.pick_from = Some(at);
        } else {
            state.picked.clear();
            state.picked.insert(at);
            state.pick_from = Some(at);
        }
    } else if response.secondary_clicked() && !state.picked.contains(&at) {
        state.picked.clear();
        state.picked.insert(at);
        state.pick_from = Some(at);
    }
}

/// Checking out with uncommitted changes: put them aside, use a worktree
/// instead, or cancel.
fn changes_in_the_way(ctx: &egui::Context, state: &mut RequestState) -> Option<Action> {
    if !state.changes_in_the_way {
        return None;
    }
    let mut action = None;
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::new("request-changes-in-the-way")).show(ctx, |ui| {
        ui.set_width(460.0);
        ui.label(
            RichText::new("You have uncommitted changes")
                .strong()
                .size(16.0),
        );
        ui.add_space(6.0);
        ui.label("Checking out its branch here needs them out of the way.");
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if widgets::primary_button(ui, "Put them aside and check out")
                .on_hover_text("They're stashed, and come back when you go back")
                .clicked()
            {
                action = Some(Action::CheckOutAside);
                close = true;
            }
            if ui
                .button("Use a worktree instead")
                .on_hover_text("Check it out in a folder of its own; your changes stay here")
                .clicked()
            {
                action = Some(Action::CheckOutWorktree);
                close = true;
            }
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });
    });
    if close || modal.should_close() {
        state.changes_in_the_way = false;
    }
    action
}

/// The message a merge starts with.
pub fn default_message(kind: Kind, r: &Request, o: &Overview, method: Method) -> String {
    match method {
        Method::Squash => {
            let mut m = format!("{} ({})", r.title, kind.number(r.number));
            if o.commits.len() > 1 {
                m.push_str("\n\n");
                for c in &o.commits {
                    m.push_str(&format!("* {}\n", c.summary));
                }
            }
            m.trim_end().to_string()
        }
        Method::Merge => format!(
            "Merge {} {} from {}\n\n{}",
            kind.request().to_lowercase(),
            kind.number(r.number),
            r.source,
            r.title
        ),
        Method::Rebase | Method::FastForward => String::new(),
    }
}

fn finish_window(
    ctx: &egui::Context,
    kind: Kind,
    state: &mut RequestState,
    drafts: &[Draft],
) -> Option<Action> {
    let mut action = None;
    let sending = state.sending.is_some();
    let mut go_to = None;
    let mut cancel = false;
    let f = state.finishing.as_mut()?;
    let modal = egui::Modal::new(egui::Id::new("finish-review")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.label(RichText::new("Finish your review").strong().size(16.0));
        ui.add_space(6.0);
        if drafts.is_empty() {
            ui.label(RichText::new("No draft comments.").color(theme::muted()));
        } else {
            let s = if drafts.len() == 1 { "" } else { "s" };
            ui.label(
                RichText::new(format!("{} draft comment{s}, sent with it:", drafts.len()))
                    .color(theme::muted()),
            );
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for d in drafts {
                        let whereabouts = match (&d.at, &d.reply_to) {
                            (_, Some(_)) => "A reply".to_string(),
                            (Some(at), None) => place(at),
                            (None, None) => "On the whole request".to_string(),
                        };
                        let first = d.body.lines().next().unwrap_or_default();
                        let row = ui.add(
                            egui::Button::new(
                                RichText::new(format!("{whereabouts}: {first}")).size(12.0),
                            )
                            .frame(false)
                            .truncate(),
                        );
                        if row.on_hover_text("Go to it").clicked() {
                            go_to = Some(d.clone());
                        }
                    }
                });
        }
        ui.add_space(6.0);
        let summary = ui.add(
            egui::TextEdit::multiline(&mut f.summary)
                .hint_text("A summary (optional)")
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        ui.ctx()
            .accesskit_node_builder(summary.id, |n| n.set_label("Review summary"));
        ui.add_space(4.0);
        let mut options = vec![
            (Finish::Comment, "Comment", "Feedback, without approving"),
            (Finish::Approve, "Approve", "Good to merge"),
        ];
        if kind.can_request_changes() {
            options.push((
                Finish::RequestChanges,
                "Request changes",
                "Needs changes before merging",
            ));
        }
        for (value, name, tip) in options {
            ui.radio_value(&mut f.finish, value, name)
                .on_hover_text(tip);
        }
        if let Some(Err(e)) = &state.sent {
            ui.label(RichText::new(format!("It wasn't sent: {e}")).color(theme::danger()));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let n = drafts.len();
            let label = if sending {
                "Sending…".to_string()
            } else if n > 0 {
                format!("Submit review and send {n}")
            } else {
                "Submit review".to_string()
            };
            let empty = n == 0 && f.summary.trim().is_empty() && f.finish == Finish::Comment;
            if ui
                .add_enabled(!sending && !empty, egui::Button::new(label))
                .clicked()
            {
                action = Some(Action::Submit {
                    summary: f.summary.clone(),
                    finish: f.finish,
                });
            }
            if sending {
                ui.spinner();
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    if modal.should_close() || cancel {
        state.finishing = None;
    }
    if let Some(d) = go_to {
        state.finishing = None;
        if let Some(at) = &d.at {
            state.tab = Tab::Changes;
            state.file = Some(at.path.clone());
            state.picked.clear();
        } else {
            state.tab = Tab::Conversation;
        }
    }
    action
}

fn merge_window(
    ctx: &egui::Context,
    kind: Kind,
    (r, o): (&Request, &Overview),
    state: &mut RequestState,
) -> Option<Action> {
    let mut action = None;
    let sending = state.sending.is_some();
    let mut cancel = false;
    let m = state.merging.as_mut()?;
    let modal = egui::Modal::new(egui::Id::new("merge-request")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.label(
            RichText::new(format!("Merge into {}", r.target))
                .strong()
                .size(16.0),
        );
        ui.add_space(6.0);
        if let Some(why) = &o.merge.blocked {
            ui.label(
                RichText::new(format!("{}  It can't be merged yet: {why}.", icon::WARNING))
                    .color(theme::modified()),
            );
            if o.merge.can_update {
                let (label, tip) = match kind {
                    Kind::GitHub => (
                        "Update branch".to_string(),
                        format!("{} merges {} into it", kind.name(), r.target),
                    ),
                    Kind::GitLab => (
                        format!("Rebase onto {}", r.target),
                        format!("{} rebases its commits onto {}", kind.name(), r.target),
                    ),
                };
                if ui
                    .add_enabled(!sending, egui::Button::new(label))
                    .on_hover_text(tip)
                    .clicked()
                {
                    action = Some(Action::UpdateBranch);
                }
            }
            ui.add_space(4.0);
        }
        let before = m.method;
        for method in &o.merge.methods {
            ui.radio_value(&mut m.method, *method, method.name());
            ui.label(
                RichText::new(method.explain())
                    .size(11.0)
                    .color(theme::muted()),
            );
        }
        // A message not changed by hand follows the method.
        if m.method != before && m.message == default_message(kind, r, o, before) {
            m.message = default_message(kind, r, o, m.method);
        }
        if !matches!(m.method, Method::Rebase | Method::FastForward) {
            ui.add_space(4.0);
            let message = ui.add(
                egui::TextEdit::multiline(&mut m.message)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY)
                    .font(theme::mono(12.0)),
            );
            ui.ctx()
                .accesskit_node_builder(message.id, |n| n.set_label("Merge commit message"));
        }
        if o.merge.can_delete_branch {
            ui.checkbox(
                &mut m.delete_branch,
                format!("Delete {} after merging", r.source),
            );
        }
        if let Some(Err(e)) = &state.sent {
            ui.label(RichText::new(format!("It wasn't merged: {e}")).color(theme::danger()));
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let ready = o.merge.blocked.is_none() && !sending;
            let label = if sending {
                "Merging…"
            } else {
                m.method.name()
            };
            if ui.add_enabled(ready, egui::Button::new(label)).clicked() {
                action = Some(Action::Merge(Merge {
                    method: m.method,
                    message: m.message.clone(),
                    delete_branch: m.delete_branch && o.merge.can_delete_branch,
                }));
            }
            if sending {
                ui.spinner();
            }
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
        });
    });
    if modal.should_close() || cancel {
        state.merging = None;
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_changes_put_aside_for_a_request() {
        let message = aside_message(Kind::GitLab, 12, "feature/x");
        assert_eq!(
            aside_from(&message, Kind::GitLab, 12).as_deref(),
            Some("feature/x")
        );
        assert_eq!(
            aside_from(&format!("On main: {message}"), Kind::GitLab, 12).as_deref(),
            Some("feature/x")
        );
        assert_eq!(aside_from(&message, Kind::GitLab, 13), None);
    }

    #[test]
    fn puts_gitlabs_uploads_at_the_project() {
        assert_eq!(
            absolute_uploads(
                "See ![shot](/uploads/ab/shot.png) and <img src=\"/uploads/cd/x.png\">",
                "https://gitlab.com/team/app"
            ),
            "See ![shot](https://gitlab.com/team/app/uploads/ab/shot.png) and <img src=\"https://gitlab.com/team/app/uploads/cd/x.png\">"
        );
    }
}
