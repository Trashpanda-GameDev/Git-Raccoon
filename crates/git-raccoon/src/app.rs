//! The application: which repo is open, the panels, and keyboard shortcuts.

use eframe::egui::{self, Frame, Margin, Panel, RichText};

use crate::repo_view::{self, Dialog, Loaded, Op, RepoView, Source};
use crate::settings::{self, Settings};
use crate::task::Task;
use crate::theme;
use crate::ui::{
    details, dialog, diff, history, search_bar, sidebar, toolbar, welcome, widgets, working,
};
use git_raccoon_core::working::Side;

pub struct App {
    settings: Settings,
    logo: egui::TextureHandle,
    repo: Option<RepoView>,
    /// A repository being opened: its path and the load in progress.
    opening: Option<(String, Task<Result<Loaded, String>>)>,
    error: Option<String>,
    scroll: history::Scroll,
    /// Answers Git's questions (passwords, passphrases) while running.
    askpass: Option<git_raccoon_core::askpass::Server>,
    /// The question being shown.
    prompt: Option<crate::ui::askpass::Prompt>,
}

fn logo(ctx: &egui::Context) -> egui::TextureHandle {
    let image = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .map(|i| i.to_rgba8())
        .map(|i| {
            egui::ColorImage::from_rgba_unmultiplied(
                [i.width() as usize, i.height() as usize],
                i.as_raw(),
            )
        })
        .unwrap_or_else(|_| egui::ColorImage::filled([1, 1], theme::ACCENT));
    ctx.load_texture("logo", image, egui::TextureOptions::LINEAR)
}

impl App {
    /// Creates the app, restoring saved settings and reopening the last repo.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = cc
            .storage
            .and_then(|s| eframe::get_value::<Settings>(s, Settings::KEY))
            .unwrap_or_default();
        let mut app = Self::with_settings(&cc.egui_ctx, settings);
        if let Ok(exe) = std::env::current_exe() {
            app.answer_git_questions(&cc.egui_ctx, &exe);
        }
        if let Some(last) = app.settings.last_repo.clone() {
            app.open(&cc.egui_ctx, last);
        }
        app
    }

    /// Creates the app with the given settings and no repo open.
    pub fn with_settings(ctx: &egui::Context, settings: Settings) -> Self {
        theme::apply(ctx);
        App {
            settings,
            logo: logo(ctx),
            repo: None,
            opening: None,
            error: None,
            scroll: history::Scroll::default(),
            askpass: None,
            prompt: None,
        }
    }

    /// Makes Git ask for passwords and passphrases in the app, through
    /// `program` (this app's executable, which answers when started by Git).
    /// Without this, Git can't ask, and fails instead.
    pub fn answer_git_questions(&mut self, ctx: &egui::Context, program: &std::path::Path) {
        let ctx = ctx.clone();
        // Without it, signing in only works through credential helpers and agents.
        self.askpass =
            git_raccoon_core::askpass::Server::start(program, move || ctx.request_repaint()).ok();
    }

    /// The question from Git being shown, if any.
    pub fn git_question(&self) -> Option<&str> {
        self.prompt.as_ref().map(|p| p.request.prompt.as_str())
    }

    /// Shows Git's questions one at a time.
    fn questions(&mut self, ctx: &egui::Context) {
        if self.prompt.is_none()
            && let Some(request) = self.askpass.as_ref().and_then(|s| s.poll())
        {
            self.prompt = Some(crate::ui::askpass::Prompt {
                request,
                input: String::new(),
            });
        }
        // Only a running Git asks; once it has finished (or was cancelled),
        // its question is moot.
        let git_running = self
            .repo
            .as_ref()
            .is_some_and(|r| r.is_staging() || r.is_committing());
        if !git_running && let Some(prompt) = self.prompt.take() {
            prompt.request.answer(None);
        }
        let Some(prompt) = &mut self.prompt else {
            return;
        };
        if let Some(answer) = crate::ui::askpass::show(ctx, prompt)
            && let Some(prompt) = self.prompt.take()
        {
            prompt.request.answer(answer);
        }
    }

    /// Starts opening a repository in the background.
    pub fn open(&mut self, ctx: &egui::Context, path: impl Into<String>) {
        let path = path.into();
        let p = path.clone();
        self.error = None;
        self.opening = Some((path, Task::spawn(ctx, move || repo_view::load(&p))));
    }

    pub fn repo(&self) -> Option<&RepoView> {
        self.repo.as_ref()
    }

    pub fn repo_mut(&mut self) -> Option<&mut RepoView> {
        self.repo.as_mut()
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Whether anything is still loading (for tests waiting on results).
    pub fn is_busy(&self) -> bool {
        self.opening.is_some()
            || self.repo.as_ref().is_some_and(|r| {
                r.is_refreshing()
                    || r.is_loading_selection()
                    || r.is_loading_status()
                    || r.is_staging()
                    || r.is_committing()
                    || r.details.is_loading()
                    || r.diff.is_loading()
                    || r.images.is_loading()
                    || r.search.running
            })
    }

    fn browse(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Open a Git repository")
            .pick_folder()
        {
            self.open(ctx, dir.to_string_lossy());
        }
    }

    fn close(&mut self, ctx: &egui::Context) {
        self.repo = None;
        self.opening = None;
        self.error = None;
        self.settings.last_repo = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("Git Raccoon".into()));
    }

    fn poll_opening(&mut self, ctx: &egui::Context) {
        let Some((path, task)) = &self.opening else {
            return;
        };
        let Some(result) = task.poll() else { return };
        let path = path.clone();
        self.opening = None;
        match result {
            Ok(loaded) => {
                let repo = RepoView::new(ctx, loaded);
                let root = repo.path().trim_end_matches(['/', '\\']).to_string();
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                    "{} — Git Raccoon",
                    settings::repo_name(&root)
                )));
                self.settings.remember(&root);
                self.settings.last_repo = Some(root);
                self.scroll = history::Scroll::default();
                self.repo = Some(repo);
            }
            // Keep whatever repo was open; just report the failure.
            Err(e) => self.error = Some(format!("Could not open {path}: {e}")),
        }
    }

    fn run(&mut self, ctx: &egui::Context, op: Op) {
        if let Some(r) = &mut self.repo {
            r.run(ctx, op);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.prompt.is_some() {
            return;
        }
        let Some(repo) = &mut self.repo else { return };
        // An open dialog has the keyboard (Esc closes just the dialog).
        if repo.dialog.is_some() {
            return;
        }
        let (find, escape) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::F),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if find {
            repo.active_file = None;
            ctx.memory_mut(|m| m.request_focus(search_bar::input_id()));
            return;
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
            repo.undo(ctx);
            return;
        }
        if escape {
            if repo.active_file.is_some() {
                repo.active_file = None;
            } else {
                repo.deselect();
            }
            return;
        }
        if repo.active_file.is_some() {
            return;
        }
        let step = ctx.input(|i| {
            [
                (egui::Key::ArrowDown, 1),
                (egui::Key::ArrowUp, -1),
                (egui::Key::PageDown, 20),
                (egui::Key::PageUp, -20),
            ]
            .into_iter()
            .find(|(k, _)| i.key_pressed(*k))
            .map(|(_, s)| s)
        });
        if let Some(step) = step {
            repo.step(step);
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_opening(ctx);
        if let Some(repo) = &mut self.repo {
            repo.poll(ctx);
            if let Some(e) = repo.refresh_error.take() {
                self.error = Some(e);
            }
        }
        self.shortcuts(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = settings::now();

        // Toolbar.
        let bar = Panel::top("toolbar")
            .frame(
                Frame::new()
                    .fill(theme::BG_RAISED)
                    .inner_margin(Margin::symmetric(10, 6)),
            )
            .show(ui, |ui| {
                let open = self.repo.as_ref().map(|r| toolbar::OpenRepo {
                    name: settings::repo_name(r.path()),
                    path: r.path(),
                    branch: r.history.branch(),
                    refreshing: r.is_refreshing(),
                    undo: r.undo.as_ref().map(|u| u.label.as_str()),
                    upstream: r
                        .head_upstream()
                        .map(|u| (u.name.as_str(), u.ahead, u.behind)),
                    syncing: r.sync_progress().map(|(verb, p)| toolbar::Syncing {
                        verb,
                        phase: p.as_ref().map(|p| p.phase.clone()),
                        percent: p.and_then(|p| p.percent),
                    }),
                    busy: r.is_staging() || r.is_committing(),
                });
                toolbar::show(
                    ui,
                    &self.logo,
                    open,
                    &self.settings.recent,
                    self.opening.is_some(),
                    now,
                )
            })
            .inner;
        match bar {
            Some(toolbar::Action::ToggleSidebar) => {
                self.settings.hide_sidebar = !self.settings.hide_sidebar
            }
            Some(toolbar::Action::Browse) => self.browse(&ctx),
            Some(toolbar::Action::Open(p)) => self.open(&ctx, p),
            Some(toolbar::Action::Forget(p)) => self.settings.forget(&p),
            Some(toolbar::Action::Close) => self.close(&ctx),
            Some(toolbar::Action::Fetch) => self.run(&ctx, Op::Fetch),
            Some(toolbar::Action::Pull) => {
                self.run(
                    &ctx,
                    Op::Pull(git_raccoon_core::sync::PullMode::FastForward),
                );
            }
            Some(toolbar::Action::Push) => self.run(&ctx, Op::Push { force: false }),
            Some(toolbar::Action::CancelSync) => {
                if let Some(r) = &self.repo {
                    r.cancel_sync();
                }
            }
            Some(toolbar::Action::Undo) => {
                if let Some(r) = &mut self.repo {
                    r.undo(&ctx);
                }
            }
            Some(toolbar::Action::Refresh) => {
                if let Some(r) = &mut self.repo {
                    r.refresh(&ctx);
                }
            }
            None => {}
        }

        // Error banner.
        if let Some(message) = self.error.clone() {
            Panel::top("error")
                .frame(
                    Frame::new()
                        .fill(theme::DANGER_BG)
                        .inner_margin(Margin::symmetric(12, 8)),
                )
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(message).color(theme::DANGER));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(ui, widgets::ICON_CLOSE, "Dismiss").clicked() {
                                self.error = None;
                            }
                        });
                    });
                });
        }

        let Some(repo) = &mut self.repo else {
            egui::CentralPanel::default_margins()
                .frame(Frame::new().fill(theme::BG).inner_margin(Margin::same(16)))
                .show(ui, |ui| {
                    if self.opening.is_some() {
                        return;
                    }
                    match welcome::show(ui, &self.logo, &self.settings.recent, now) {
                        Some(welcome::Action::Browse) => self.browse(&ctx),
                        Some(welcome::Action::Open(p)) => self.open(&ctx, p),
                        Some(welcome::Action::Forget(p)) => self.settings.forget(&p),
                        None => {}
                    }
                });
            return;
        };

        // Sidebar.
        if !self.settings.hide_sidebar {
            let jump = Panel::left("sidebar")
                .resizable(true)
                .default_size(240.0)
                .size_range(180.0..=420.0)
                .frame(Frame::new().fill(theme::BG_RAISED))
                .show(ui, |ui| {
                    let selected = repo.selected.as_ref().map(|s| s.id.clone());
                    sidebar::show(
                        ui,
                        &repo.refs,
                        &mut repo.sidebar_filter,
                        &mut repo.collapsed,
                        selected.as_deref(),
                    )
                })
                .inner;
            match jump {
                Some(sidebar::Action::Jump(id)) => repo.select(&id),
                Some(sidebar::Action::StashApply(id)) => repo.run(&ctx, Op::StashApply(id)),
                Some(sidebar::Action::StashPop(id)) => repo.run(&ctx, Op::StashPop(id)),
                Some(sidebar::Action::StashDrop(id)) => repo.confirm_drop(id),
                Some(sidebar::Action::Checkout(name)) => repo.checkout(&ctx, name),
                Some(sidebar::Action::CheckoutRemote(name)) => {
                    repo.run(&ctx, Op::CheckoutRemote(name));
                }
                Some(sidebar::Action::NewBranch { start, label }) => repo.new_branch(start, label),
                Some(sidebar::Action::Rename(name)) => repo.rename_branch(name),
                Some(sidebar::Action::DeleteBranch(name)) => repo.confirm_delete_branch(name),
                Some(sidebar::Action::DeleteTag(name)) => repo.confirm_delete_tag(name),
                None => {}
            }
        }

        // Details pane: a commit, or the working changes.
        let pane = Frame::new().fill(theme::BG_RAISED).inner_margin(Margin {
            left: 12,
            right: 8,
            top: 0,
            bottom: 8,
        });
        if let Some(sel) = &repo.selected {
            let id = sel.id.clone();
            let action = Panel::right("details")
                .resizable(true)
                .default_size(380.0)
                .size_range(280.0..=640.0)
                .frame(pane)
                .show(ui, |ui| {
                    let stash = repo.stash(&id).map(|_| repo.is_staging());
                    details::show(
                        ui,
                        &id,
                        repo.details.get(&id),
                        repo.details.is_loading(),
                        repo.active_file.as_deref(),
                        now,
                        stash,
                    )
                })
                .inner;
            match action {
                Some(details::Action::OpenFile(f)) => repo.active_file = Some(f),
                Some(details::Action::Select(p)) => repo.select(&p),
                Some(details::Action::Close) => repo.deselect(),
                Some(details::Action::StashApply) => repo.run(&ctx, Op::StashApply(id.clone())),
                Some(details::Action::StashPop) => repo.run(&ctx, Op::StashPop(id.clone())),
                Some(details::Action::StashDrop) => repo.confirm_drop(id.clone()),
                None => {}
            }
        } else if repo.working_selected {
            let active = repo.active_file.clone().map(|f| (repo.active_side, f));
            let action = Panel::right("details")
                .resizable(true)
                .default_size(380.0)
                .size_range(280.0..=640.0)
                .frame(pane)
                .show(ui, |ui| {
                    let (loading, staging, committing) = (
                        repo.is_loading_status(),
                        repo.is_staging(),
                        repo.is_committing(),
                    );
                    working::show(
                        ui,
                        working::State {
                            status: repo.status.as_ref(),
                            loading,
                            staging,
                            committing,
                            active: active.as_ref().map(|(s, f)| (*s, f.as_str())),
                            draft: &mut repo.draft,
                        },
                    )
                })
                .inner;
            match action {
                Some(working::Action::Open(side, f)) => repo.open_working_file(side, f),
                Some(working::Action::Stage(f)) => repo.run(&ctx, Op::StageFile(f)),
                Some(working::Action::Unstage(f)) => repo.run(&ctx, Op::UnstageFile(f)),
                Some(working::Action::StageAll) => repo.run(&ctx, Op::StageAll),
                Some(working::Action::UnstageAll) => repo.run(&ctx, Op::UnstageAll),
                Some(working::Action::SetAmend(on)) => repo.set_amend(on),
                Some(working::Action::Commit) => repo.commit(&ctx),
                Some(working::Action::Discard(files)) => repo.confirm_discard(files),
                Some(working::Action::Stash) => {
                    repo.dialog = Some(Dialog::Stash {
                        message: String::new(),
                        untracked: true,
                    });
                }
                Some(working::Action::Close) => repo.deselect(),
                None => {}
            }
        }

        // History (or a diff over it).
        egui::CentralPanel::no_frame()
            .frame(Frame::new().fill(theme::BG))
            .show(ui, |ui| {
                if let Some(key) = repo.diff_key() {
                    let file = key.1.clone();
                    let images = if crate::images::is_image(&file) {
                        repo.images.get(&key)
                    } else {
                        None
                    };
                    let side_label = match key.0 {
                        Source::Working(Side::Staged) => Some("Staged"),
                        Source::Working(Side::Unstaged) => Some("Unstaged"),
                        Source::Commit(_) => None,
                    };
                    let staging = match key.0 {
                        Source::Working(side) => Some(diff::Staging {
                            side,
                            picked: &repo.picked,
                            busy: repo.is_staging(),
                        }),
                        Source::Commit(_) => None,
                    };
                    let action = diff::show(
                        ui,
                        &file,
                        side_label,
                        repo.diff.get(&key),
                        repo.diff.is_loading(),
                        images,
                        self.settings.split_diff,
                        staging.as_ref(),
                    );
                    match action {
                        Some(diff::Action::Close) => repo.active_file = None,
                        Some(diff::Action::SetSplit(s)) => self.settings.split_diff = s,
                        Some(diff::Action::StageHunk(h)) => repo.stage_hunk(&ctx, h),
                        Some(diff::Action::ToggleLine { hunk, line, extend }) => {
                            repo.toggle_pick(hunk, line, extend)
                        }
                        Some(diff::Action::StagePicked) => repo.stage_picked(&ctx),
                        Some(diff::Action::ClearPicked) => repo.picked.clear(),
                        None => {}
                    }
                    return;
                }
                Frame::new()
                    .inner_margin(Margin::symmetric(12, 0))
                    .show(ui, |ui| {
                        ui.set_height(36.0);
                        search_bar::show(ui, repo);
                    });
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    ui.cursor().top(),
                    egui::Stroke::new(1.0, theme::BORDER),
                );
                match history::show(ui, repo, &mut self.scroll, now) {
                    Some(history::Click::Row(row)) => {
                        repo.select_index(row);
                        repo.scroll_to = None;
                    }
                    Some(history::Click::Working) => repo.select_working(),
                    Some(history::Click::NewBranch { id, label }) => repo.new_branch(id, label),
                    Some(history::Click::NewTag { id, label }) => repo.new_tag(id, label),
                    Some(history::Click::CheckoutDetached { id, label }) => {
                        repo.confirm_checkout_detached(id, label);
                    }
                    None => {}
                }
            });
        if let Some(op) = dialog::show(&ctx, &mut repo.dialog, &repo.refs) {
            repo.run(&ctx, op);
        }
        self.questions(&ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::BG.to_normalized_gamma_f32()
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, Settings::KEY, &self.settings);
    }
}
