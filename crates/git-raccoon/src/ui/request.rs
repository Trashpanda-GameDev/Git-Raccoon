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
use crate::highlight::{RenderedDiff, Row};
use crate::theme;
use crate::ui::diff::{LINE, NUMBER};
use crate::ui::widgets;

pub enum Action {
    Close,
    Open(String),
    Reload,
    /// Check out its branch; or into a new worktree.
    CheckOut,
    CheckOutWorktree,
    /// On its branch: pull, or go back to the branch it goes into.
    PullLatest,
    BackTo(String),
    /// After it's merged: pull the branch it went into.
    PullMerged,
    GoToCommit(String),
    /// Keep a draft: new, or in place of the one at this index.
    SaveDraft(Draft, Option<usize>),
    DeleteDraft(usize),
    Submit {
        summary: String,
        finish: Finish,
    },
    Resolve(String, bool),
    Merge(Merge),
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
    let state = &mut forge.state;
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
            // On its branch: keep up with it, or go back.
            if o.state == State::Open && on_its_branch(r, kind, here.branch) {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("{}  You're on its branch.", icon::CHECK)).color(theme::add()));
                    if ui.small_button("Pull the latest").clicked() {
                        *action = Some(Action::PullLatest);
                    }
                    if ui.small_button(format!("Back to {}", r.target)).clicked() {
                        *action = Some(Action::BackTo(r.target.clone()));
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
                    ui.ctx().accesskit_node_builder(b.id, |n| n.set_label(name));
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

/// A comment's body, as Markdown.
fn markdown(ui: &mut egui::Ui, cache: &mut CommonMarkCache, text: &str) {
    if text.trim().is_empty() {
        ui.label(
            RichText::new("No description.")
                .italics()
                .color(theme::muted()),
        );
    } else {
        CommonMarkViewer::new().show(ui, cache, text);
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
            who_and_when(ui, &c.author, "", c.created, now);
            let frame = Some((state.gitlab, state.on_branch, &mut state.batch));
            body_with_suggestions(
                ui,
                &mut state.markdown,
                (&c.body, &c.suggestions),
                frame,
                action,
            );
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
                    && let Some(at) = t.at.as_ref().filter(|_| !t.outdated)
                    && ui.small_button("Show in Changes").clicked()
                {
                    state.tab = Tab::Changes;
                    state.file = Some(at.path.clone());
                    state.picked.clear();
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
                                who_and_when(ui, &c.author, "commented", c.created, here.now);
                                markdown(ui, &mut state.markdown, &c.body);
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
                            .on_hover_text("Show it in the graph");
                        if row.hovered() {
                            ui.painter().rect_filled(
                                row.rect.expand(2.0),
                                4,
                                theme::bg_hover().gamma_multiply(0.5),
                            );
                        }
                        if row.clicked() {
                            *action = Some(Action::GoToCommit(c.sha.clone()));
                        }
                    }
                });
        });
}

/// A patch, as a diff the diff view can lay out.
fn file_diff(f: &review::File) -> FileDiff {
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
    let d = Arc::clone(
        state
            .rendered
            .entry(f.path.clone())
            .or_insert_with(|| Arc::new(RenderedDiff::new(file_diff(f)))),
    );
    // The file's name.
    egui::Frame::new()
        .fill(theme::bg_raised())
        .inner_margin(egui::Margin::symmetric(12, 0))
        .show(ui, |ui| {
            ui.set_height(32.0);
            ui.set_width(ui.available_width());
            ui.horizontal_centered(|ui| {
                let title = match &f.old_path {
                    Some(old) => format!("{old}  {}  {}", icon::ARROW_RIGHT, f.path),
                    None => f.path.clone(),
                };
                ui.label(RichText::new(title).strong());
                ui.label(
                    RichText::new("Click lines to pick them (Shift for a range), then right-click to comment, or use the + by a line.")
                        .size(11.0)
                        .color(theme::muted()),
                );
            });
        });
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
    diff_with_threads(ui, &d, o, state, here, action);
}

/// The diff, with each line's threads, drafts and the comment being
/// written under it.
fn diff_with_threads(
    ui: &mut egui::Ui,
    d: &RenderedDiff,
    o: &Overview,
    state: &mut RequestState,
    here: &Here<'_>,
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
    egui::ScrollArea::both()
        .id_salt(("request-diff", &path))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let view_width = ui.available_width();
            let width = (2.0 * NUMBER + 18.0 + code_width).max(view_width);
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in &d.unified {
                match *row {
                    Row::Header(h) => {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(width, LINE), Sense::hover());
                        crate::ui::diff::header(ui.painter(), rect, &d.diff.hunks[h].header);
                    }
                    Row::Line(h, l) => {
                        let line = &d.diff.hunks[h].lines[l];
                        let (rect, response) = ui
                            .allocate_exact_size(egui::vec2(width, LINE), Sense::click_and_drag());
                        let picked = state.picked.contains(&(h, l));
                        crate::ui::diff::unified_line(
                            ui.painter(),
                            rect,
                            line,
                            &d.jobs[h][l],
                            picked,
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::WidgetType::Checkbox,
                                true,
                                picked,
                                format!("Select line: {}", line.content.trim()),
                            )
                        });
                        pick(ui, &response, (d, (h, l)), state);
                        // The + to comment on it (or on the lines picked, if it's one).
                        if response.hovered() || ui.rect_contains_pointer(rect) {
                            let plus = Rect::from_center_size(
                                Pos2::new(rect.left() + 2.0 * NUMBER + 9.0, rect.center().y),
                                egui::vec2(16.0, 16.0),
                            );
                            let b = ui.put(
                                plus,
                                egui::Button::new(
                                    RichText::new("+").strong().color(theme::on_accent()),
                                )
                                .fill(theme::accent_fill())
                                .corner_radius(3),
                            );
                            ui.ctx().accesskit_node_builder(b.id, |n| {
                                n.set_label("Comment on this line")
                            });
                            if b.on_hover_text("Comment on this line").clicked() {
                                if !picked {
                                    state.picked.clear();
                                    state.picked.insert((h, l));
                                }
                                state.composer = picked_anchor(d, state).map(|at| Composer {
                                    at: Some(at),
                                    ..Composer::default()
                                });
                            }
                        }
                        response.context_menu(|ui| {
                            let several = state.picked.len() > 1;
                            let label = if several {
                                "Comment on selection"
                            } else {
                                "Comment on this line"
                            };
                            if ui.button(label).clicked() {
                                state.composer = picked_anchor(d, state).map(|at| Composer {
                                    at: Some(at),
                                    ..Composer::default()
                                });
                                ui.close();
                            }
                            // The picked lines, as the request has them, to change.
                            let suggest = picked_anchor(d, state).and_then(|at| {
                                let end = at.line.new?;
                                let start = at.start.map_or(Some(end), |s| s.new)?;
                                let text: Vec<&str> = state
                                    .picked
                                    .iter()
                                    .map(|&(h, l)| &d.diff.hunks[h].lines[l])
                                    .filter(|l| {
                                        l.new_line.is_some_and(|n| (start..=end).contains(&n))
                                    })
                                    .map(|l| l.content.as_str())
                                    .collect();
                                let whole = text.len() == (end - start + 1) as usize;
                                whole.then(|| {
                                    (
                                        at,
                                        review::suggestion_block(
                                            state.gitlab,
                                            start,
                                            end,
                                            &text.join("\n"),
                                        ),
                                    )
                                })
                            });
                            if ui
                                .add_enabled(
                                    suggest.is_some(),
                                    egui::Button::new("Suggest a change"),
                                )
                                .on_disabled_hover_text(
                                    "Pick lines of the new version, one after another",
                                )
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
                        // What's under it: threads, drafts, the comment being written.
                        let here_line = line_of(line);
                        let under: Vec<&(usize, &Thread)> = threads
                            .iter()
                            .filter(|(_, t)| {
                                t.at.as_ref().is_some_and(|a| {
                                    a.line.same(&here_line) && line_side_matches(&a.line, line)
                                })
                            })
                            .collect();
                        let drafts: Vec<(usize, &Draft)> = here
                            .drafts
                            .iter()
                            .enumerate()
                            .filter(|(_, dr)| {
                                dr.reply_to.is_none()
                                    && dr.at.as_ref().is_some_and(|a| {
                                        a.path == path
                                            && a.line.same(&here_line)
                                            && line_side_matches(&a.line, line)
                                    })
                            })
                            .collect();
                        let writing = state.composer.as_ref().is_some_and(|c| {
                            c.editing.is_none()
                                && c.reply_to.is_none()
                                && c.at.as_ref().is_some_and(|a| {
                                    a.path == path
                                        && a.line.same(&here_line)
                                        && line_side_matches(&a.line, line)
                                })
                        });
                        if under.is_empty() && drafts.is_empty() && !writing {
                            continue;
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
                                for (_, t) in under {
                                    thread_card(
                                        ui,
                                        t,
                                        None,
                                        true,
                                        state,
                                        here.drafts,
                                        here.now,
                                        action,
                                    );
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
                }
            }
        });
    // Dragging over lines picks them all, from where it started.
    let drag = egui::Id::new("request-line-drag");
    if !ui.input(|i| i.pointer.primary_down()) {
        ui.data_mut(|m| m.remove::<bool>(drag));
    }
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
        Method::Rebase => String::new(),
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
        if m.method != Method::Rebase {
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
