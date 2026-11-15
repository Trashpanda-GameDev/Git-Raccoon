//! The application: which repo is open, the panels, and keyboard shortcuts.

use eframe::egui::{self, Frame, Margin, Panel, RichText};

use crate::repo_view::{self, Dialog, Loaded, Op, RepoView, Source};
use crate::settings::{self, Settings, ThemeChoice};
use crate::task::Task;
use crate::theme;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use git_raccoon_core::create;
use git_raccoon_core::sync::Progress;

use crate::commands::{self, Command, Entry};
use crate::repo_view::FileMode;
use crate::ui::{clone, conflict, file_view, palette, rebase, tabs};
use crate::ui::{
    details, dialog, diff, history, search_bar, sidebar, toolbar, welcome, widgets, working,
};
use git_raccoon_core::working::Side;

pub struct App {
    settings: Settings,
    logo: egui::TextureHandle,
    /// The active tab's repository (the others are parked in `tabs`).
    repo: Option<RepoView>,
    /// Repositories being opened: path, the load, and the tab they're for.
    opening: Vec<Opening>,
    error: Option<String>,
    /// The active tab's history scroll position.
    scroll: history::Scroll,
    tabs: Vec<Tab>,
    /// Which tab is showing.
    active: usize,
    next_tab: u64,
    /// Answers Git's questions (passwords, passphrases) while running.
    askpass: Option<git_raccoon_core::askpass::Server>,
    /// The question being shown.
    prompt: Option<crate::ui::askpass::Prompt>,
    /// The clone dialog, and a clone in progress.
    clone_form: Option<clone::Form>,
    cloning: Option<Cloning>,
    /// The command palette, while open.
    palette: Option<palette::State>,
    /// The settings window is open.
    settings_open: bool,
    /// The look and zoom last applied.
    applied: Option<(theme::Look, f32)>,
    updater: crate::updates::Updater,
    /// Restart into the downloaded update once settings are saved.
    restart_to_update: bool,
    /// Reports of earlier crashes, offered one at a time.
    crash_reports: Vec<crate::crash::Report>,
}

/// A repository being opened: its path, the load, and its tab's id.
type Opening = (String, Task<Result<Loaded, String>>, u64);

/// A tab. While it isn't the active one, its repository is parked here.
struct Tab {
    id: u64,
    /// Its repository's path, once one is open (or opening).
    path: Option<String>,
    parked: Option<(Option<RepoView>, history::Scroll)>,
}

/// A clone running in the background.
struct Cloning {
    progress: Arc<Mutex<Option<Progress>>>,
    cancel: Arc<AtomicBool>,
    task: Task<Result<PathBuf, git_raccoon_core::Error>>,
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
        .unwrap_or_else(|_| egui::ColorImage::filled([1, 1], theme::accent()));
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
        if let Some(dir) = crate::crash::folder() {
            app.load_crash_reports(&dir);
        }
        app.restore_tabs(&cc.egui_ctx);
        app
    }

    /// Creates the app with the given settings and no repo open.
    pub fn with_settings(ctx: &egui::Context, settings: Settings) -> Self {
        theme::apply(ctx);
        App {
            settings,
            logo: logo(ctx),
            repo: None,
            opening: Vec::new(),
            error: None,
            scroll: history::Scroll::default(),
            tabs: vec![Tab {
                id: 0,
                path: None,
                parked: None,
            }],
            active: 0,
            next_tab: 1,
            askpass: None,
            prompt: None,
            clone_form: None,
            cloning: None,
            palette: None,
            settings_open: false,
            applied: None,
            updater: crate::updates::Updater::new(),
            restart_to_update: false,
            crash_reports: Vec::new(),
        }
    }

    /// Opens the clone dialog.
    pub fn show_clone(&mut self) {
        let parent = self
            .settings
            .clone_dir
            .clone()
            .or_else(|| {
                std::env::var("USERPROFILE")
                    .or_else(|_| std::env::var("HOME"))
                    .ok()
            })
            .unwrap_or_default();
        self.clone_form = Some(clone::Form {
            parent,
            ..clone::Form::default()
        });
    }

    pub fn is_cloning(&self) -> bool {
        self.cloning.is_some()
    }

    fn start_clone(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.clone_form else {
            return;
        };
        form.error = None;
        let (url, dest) = (form.url.trim().to_string(), form.dest());
        self.settings.clone_dir = Some(form.parent.trim().to_string());
        let progress = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        let (p, c, repaint) = (progress.clone(), cancel.clone(), ctx.clone());
        self.cloning = Some(Cloning {
            progress,
            cancel,
            task: Task::spawn(ctx, move || {
                let mut report = |update| {
                    if let Ok(mut slot) = p.lock() {
                        *slot = Some(update);
                    }
                    repaint.request_repaint();
                };
                create::clone(&url, &dest, &mut report, &c)
            }),
        });
    }

    fn poll_clone(&mut self, ctx: &egui::Context) {
        let Some(result) = self.cloning.as_ref().and_then(|c| c.task.poll()) else {
            return;
        };
        self.cloning = None;
        match result {
            Ok(dest) => {
                self.clone_form = None;
                self.open(ctx, dest.to_string_lossy());
            }
            Err(git_raccoon_core::Error::Cancelled) => {}
            Err(e) => {
                if let Some(form) = &mut self.clone_form {
                    form.error = Some(format!("Could not clone:\n{e}"));
                }
            }
        }
    }

    fn clone_dialog(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.clone_form else {
            return;
        };
        let progress = self
            .cloning
            .as_ref()
            .map(|c| c.progress.lock().ok().and_then(|p| p.clone()));
        match clone::show(ctx, form, progress) {
            Some(clone::Action::Clone) => self.start_clone(ctx),
            Some(clone::Action::Browse) => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Where to clone to")
                    .pick_folder()
                {
                    form.parent = dir.to_string_lossy().into_owned();
                }
            }
            Some(clone::Action::Cancel) => self.clone_form = None,
            Some(clone::Action::Stop) => {
                if let Some(c) = &self.cloning {
                    c.cancel.store(true, Ordering::Relaxed);
                }
            }
            None => {}
        }
    }

    /// Starts a repository in `dir` (or just opens it, if it is one) and opens it.
    pub fn new_repository(&mut self, ctx: &egui::Context, dir: &std::path::Path) {
        match create::init(dir) {
            Ok(()) => self.open(ctx, dir.to_string_lossy()),
            Err(e) => self.error = Some(format!("Could not create the repository:\n{e}")),
        }
    }

    fn pick_new_repository(&mut self, ctx: &egui::Context) {
        if let Some(dir) = rfd::FileDialog::new()
            .set_title("Choose a folder for the new repository")
            .pick_folder()
        {
            self.new_repository(ctx, &dir);
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

    /// Applies the chosen theme (following the system's, if asked) and size,
    /// when they change.
    fn apply_look(&mut self, ctx: &egui::Context) {
        let look = look_for(self.settings.theme, ctx.system_theme());
        let zoom = self.settings.zoom.unwrap_or(1.0);
        if self.applied == Some((look, zoom)) {
            return;
        }
        let theme_changed = self.applied.is_none_or(|(l, _)| l != look);
        self.applied = Some((look, zoom));
        if theme_changed {
            theme::set(ctx, look);
            // Diffs carry their colours: draw them again.
            if let Some(repo) = &mut self.repo {
                repo.diff.clear();
            }
        }
        ctx.set_zoom_factor(zoom);
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
        // (A background tab's fetch can ask too.)
        let git_running = self
            .all_repos()
            .any(|r| r.is_staging() || r.is_committing())
            || self.cloning.is_some();
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
    /// Opens a repository: in this tab if it's empty, otherwise in a new
    /// one; or switches to the tab that already has it.
    pub fn open(&mut self, ctx: &egui::Context, path: impl Into<String>) {
        let path = path.into();
        let key = path.trim_end_matches(['/', '\\']).to_string();
        if let Some(i) = self
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(key.as_str()))
        {
            self.switch_tab(ctx, i);
            return;
        }
        if self.tabs[self.active].path.is_some() {
            self.new_tab(ctx);
        }
        let id = self.tabs[self.active].id;
        self.tabs[self.active].path = Some(key);
        let p = path.clone();
        self.error = None;
        self.opening
            .push((path, Task::spawn(ctx, move || repo_view::load(&p)), id));
    }

    /// Offers the crash reports in `dir` (left by earlier runs).
    pub fn load_crash_reports(&mut self, dir: &std::path::Path) {
        self.crash_reports = crate::crash::pending(dir);
    }

    /// Whether a crash report is waiting for an answer.
    pub fn has_crash_report(&self) -> bool {
        !self.crash_reports.is_empty()
    }

    fn crash_dialog(&mut self, ctx: &egui::Context) {
        let Some(report) = self.crash_reports.first() else {
            return;
        };
        let more = self.crash_reports.len() - 1;
        let Some(action) = crate::ui::crash::show(ctx, report, more) else {
            return;
        };
        match action {
            crate::ui::crash::Action::Send => {
                ctx.copy_text(report.text.clone());
                ctx.open_url(egui::OpenUrl::new_tab(crate::crash::issue_url(report)));
            }
            crate::ui::crash::Action::Copy => ctx.copy_text(report.text.clone()),
            crate::ui::crash::Action::Discard => {}
        }
        let report = self.crash_reports.remove(0);
        crate::crash::forget(&report);
    }

    /// Notes the open tabs in the settings, to restore them next time.
    pub fn remember_tabs(&mut self) {
        self.settings.open_tabs = self.tabs.iter().filter_map(|t| t.path.clone()).collect();
        self.settings.active_tab = self.tabs[..self.active.min(self.tabs.len())]
            .iter()
            .filter(|t| t.path.is_some())
            .count();
    }

    /// Opens the tabs open last time.
    pub fn restore_tabs(&mut self, ctx: &egui::Context) {
        let mut paths = std::mem::take(&mut self.settings.open_tabs);
        if paths.is_empty() {
            paths.extend(self.settings.last_repo.clone());
        }
        let active = self.settings.active_tab;
        for p in paths {
            self.open(ctx, p);
        }
        if active < self.tabs.len() {
            self.switch_tab(ctx, active);
        }
    }

    /// The tabs' names (for the tab bar) and which one is showing.
    pub fn tab_names(&self) -> (Vec<String>, usize) {
        let names = self
            .tabs
            .iter()
            .map(|t| {
                t.path.as_deref().map_or_else(
                    || "New tab".to_string(),
                    |p| settings::repo_name(p).to_string(),
                )
            })
            .collect();
        (names, self.active)
    }

    /// Opens an empty tab (showing the welcome screen) and switches to it.
    pub fn new_tab(&mut self, ctx: &egui::Context) {
        let id = self.next_tab;
        self.next_tab += 1;
        self.tabs.push(Tab {
            id,
            path: None,
            parked: None,
        });
        let last = self.tabs.len() - 1;
        self.switch_tab(ctx, last);
    }

    /// Shows tab `i`.
    pub fn switch_tab(&mut self, ctx: &egui::Context, i: usize) {
        if i >= self.tabs.len() || i == self.active {
            return;
        }
        let current = (self.repo.take(), std::mem::take(&mut self.scroll));
        self.tabs[self.active].parked = Some(current);
        let (repo, scroll) = self.tabs[i].parked.take().unwrap_or_default();
        self.repo = repo;
        self.scroll = scroll;
        self.active = i;
        self.palette = None;
        self.update_title(ctx);
    }

    /// Closes tab `i`. The last tab left is never closed, just emptied.
    pub fn close_tab(&mut self, ctx: &egui::Context, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            self.repo = None;
            self.scroll = history::Scroll::default();
            self.tabs[0].path = None;
            self.opening.clear();
            self.update_title(ctx);
            return;
        }
        let id = self.tabs[i].id;
        self.opening.retain(|o| o.2 != id);
        if i == self.active {
            self.repo = None;
            self.tabs.remove(i);
            self.active = usize::MAX; // nothing is showing for a moment
            let next = i.min(self.tabs.len() - 1);
            let (repo, scroll) = self.tabs[next].parked.take().unwrap_or_default();
            self.repo = repo;
            self.scroll = scroll;
            self.active = next;
        } else {
            self.tabs.remove(i);
            if i < self.active {
                self.active -= 1;
            }
        }
        self.update_title(ctx);
    }

    fn update_title(&self, ctx: &egui::Context) {
        let title = match &self.repo {
            Some(r) => format!(
                "{} — Git Raccoon",
                settings::repo_name(r.path().trim_end_matches(['/', '\\']))
            ),
            None => "Git Raccoon".to_string(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    /// Every open repository, the active one first.
    fn all_repos(&self) -> impl Iterator<Item = &RepoView> {
        self.repo.iter().chain(
            self.tabs
                .iter()
                .filter_map(|t| t.parked.as_ref().and_then(|p| p.0.as_ref())),
        )
    }

    /// Whether the active tab is still opening its repository.
    fn active_opening(&self) -> bool {
        let id = self.tabs[self.active].id;
        self.opening.iter().any(|o| o.2 == id)
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
        !self.opening.is_empty()
            || self.cloning.is_some()
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

    /// Closes the active tab's repository (and the tab, unless it's the last).
    fn close(&mut self, ctx: &egui::Context) {
        self.error = None;
        self.close_tab(ctx, self.active);
    }

    fn poll_opening(&mut self, ctx: &egui::Context) {
        let mut i = 0;
        while i < self.opening.len() {
            let Some(result) = self.opening[i].1.poll() else {
                i += 1;
                continue;
            };
            let (path, _, id) = self.opening.remove(i);
            let Some(tab) = self.tabs.iter().position(|t| t.id == id) else {
                continue; // its tab was closed
            };
            match result {
                Ok(loaded) => {
                    let repo = RepoView::new(ctx, loaded);
                    let root = repo.path().trim_end_matches(['/', '\\']).to_string();
                    self.settings.remember(&root);
                    self.settings.last_repo = Some(root.clone());
                    // Already open in another tab (opened by a subfolder, say): go there.
                    let elsewhere =
                        |t: &Tab| t.id != id && t.path.as_deref() == Some(root.as_str());
                    if self.tabs.iter().any(elsewhere) {
                        self.close_tab(ctx, tab);
                        let found = self
                            .tabs
                            .iter()
                            .position(|t| t.path.as_deref() == Some(root.as_str()));
                        if let Some(other) = found {
                            self.switch_tab(ctx, other);
                        }
                        continue;
                    }
                    self.tabs[tab].path = Some(root);
                    if tab == self.active {
                        self.scroll = history::Scroll::default();
                        self.repo = Some(repo);
                        self.update_title(ctx);
                    } else {
                        self.tabs[tab].parked = Some((Some(repo), history::Scroll::default()));
                    }
                }
                Err(e) => {
                    self.error = Some(format!("Could not open {path}: {e}"));
                    // An empty tab goes back to the welcome screen.
                    let has_repo = if tab == self.active {
                        self.repo.is_some()
                    } else {
                        self.tabs[tab]
                            .parked
                            .as_ref()
                            .is_some_and(|p| p.0.is_some())
                    };
                    if !has_repo {
                        self.tabs[tab].path = None;
                    }
                }
            }
        }
    }

    fn run(&mut self, ctx: &egui::Context, op: Op) {
        if let Some(r) = &mut self.repo {
            r.run(ctx, op);
        }
    }

    /// Whether a command can run right now.
    fn can_run(&self, command: &Command) -> bool {
        let Some(repo) = &self.repo else {
            return matches!(
                command,
                Command::Palette
                    | Command::NewTab
                    | Command::NextTab
                    | Command::PreviousTab
                    | Command::CloseRepo
                    | Command::Settings
                    | Command::OpenRepo
                    | Command::CloneRepo
                    | Command::NewRepo
            );
        };
        let busy = repo.op_running();
        let status = repo.status.as_ref();
        let on_branch = repo.head_branch().is_some();
        let in_progress = status.is_some_and(|s| s.in_progress.is_some());
        match command {
            Command::Palette
            | Command::Settings
            | Command::NewTab
            | Command::NextTab
            | Command::PreviousTab
            | Command::OpenRepo
            | Command::CloneRepo
            | Command::NewRepo
            | Command::CloseRepo
            | Command::Refresh
            | Command::Find
            | Command::ToggleSidebar
            | Command::ToggleSplitDiff
            | Command::ShowWorking
            | Command::WriteCommit
            | Command::AddRemote
            | Command::GoTo { .. } => true,
            Command::FileHistory | Command::Blame => {
                repo.diff_key().is_some() || repo.file_view.is_some()
            }
            Command::Undo => repo.undo.is_some() && !busy,
            Command::Fetch => !busy,
            Command::Pull => !busy && repo.head_upstream().is_some(),
            Command::Push => !busy && on_branch,
            Command::StageAll => !busy && status.is_some_and(|s| !s.unstaged.is_empty()),
            Command::UnstageAll => !busy && status.is_some_and(|s| !s.staged.is_empty()),
            Command::Stash => {
                !busy && status.is_some_and(|s| !s.staged.is_empty() || !s.unstaged.is_empty())
            }
            Command::PopStash => !busy && !repo.refs.stashes.is_empty(),
            Command::NewBranch | Command::NewTag => !busy,
            Command::Continue => {
                !busy && in_progress && status.is_some_and(|s| s.conflicted.is_empty())
            }
            Command::Abort => !busy && in_progress,
            Command::Checkout(_)
            | Command::CheckoutRemote(_)
            | Command::Merge(_)
            | Command::Rebase(_) => !busy,
        }
    }

    /// The palette's lines: every command that applies, then each branch's.
    fn palette_entries(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        let mut add = |command: Command, title: String| {
            let enabled = self.can_run(&command);
            out.push(Entry {
                command,
                title,
                enabled,
            });
        };
        add(Command::Settings, "Settings…".into());
        add(Command::NewTab, "New tab".into());
        if self.tabs.len() > 1 {
            add(Command::NextTab, "Next tab".into());
            add(Command::PreviousTab, "Previous tab".into());
        }
        add(Command::OpenRepo, "Open repository…".into());
        add(Command::CloneRepo, "Clone repository…".into());
        add(Command::NewRepo, "New repository…".into());
        let Some(repo) = &self.repo else { return out };
        let head = repo.head_branch().unwrap_or("HEAD").to_string();
        let state = repo.status.as_ref().and_then(|s| s.in_progress.as_ref());
        if let Some(state) = state {
            let noun = state.what.noun();
            add(Command::Continue, format!("Continue the {noun}"));
            add(Command::Abort, format!("Abort the {noun}…"));
        }
        let undo = repo
            .undo
            .as_ref()
            .map_or_else(|| "Undo".to_string(), |u| u.label.clone());
        add(Command::Undo, undo);
        add(Command::Fetch, "Fetch from all remotes".into());
        add(Command::Pull, format!("Pull into {head}"));
        add(Command::Push, format!("Push {head}"));
        add(Command::ShowWorking, "Show working changes".into());
        add(Command::WriteCommit, "Write a commit message".into());
        add(Command::StageAll, "Stage all changes".into());
        add(Command::UnstageAll, "Unstage all changes".into());
        add(Command::Stash, "Stash changes…".into());
        add(Command::PopStash, "Pop the latest stash".into());
        add(Command::NewBranch, format!("Create branch at {head}…"));
        add(Command::NewTag, format!("Create tag at {head}…"));
        add(Command::AddRemote, "Add remote…".into());
        add(Command::FileHistory, "History of this file".into());
        add(Command::Blame, "Blame this file".into());
        add(Command::Find, "Search commits".into());
        add(Command::Refresh, "Refresh".into());
        add(Command::ToggleSidebar, "Show or hide the sidebar".into());
        add(
            Command::ToggleSplitDiff,
            "Switch diffs between unified and side by side".into(),
        );
        add(Command::CloseRepo, "Close repository".into());
        for b in &repo.refs.local {
            if !b.is_head {
                add(
                    Command::Checkout(b.name.clone()),
                    format!("Check out {}", b.name),
                );
                add(
                    Command::Merge(b.name.clone()),
                    format!("Merge {} into {head}", b.name),
                );
                add(
                    Command::Rebase(b.name.clone()),
                    format!("Rebase {head} onto {}", b.name),
                );
            }
            add(
                Command::GoTo {
                    name: b.name.clone(),
                    id: b.target.clone(),
                },
                format!("Go to {}", b.name),
            );
        }
        for r in &repo.refs.remotes {
            for b in &r.branches {
                let name = b.full_name.clone();
                add(
                    Command::CheckoutRemote(name.clone()),
                    format!("Check out {name}"),
                );
                add(
                    Command::Merge(name.clone()),
                    format!("Merge {name} into {head}"),
                );
                add(
                    Command::GoTo {
                        name: name.clone(),
                        id: b.target.clone(),
                    },
                    format!("Go to {name}"),
                );
            }
        }
        for t in &repo.refs.tags {
            add(
                Command::GoTo {
                    name: t.name.clone(),
                    id: t.target.clone(),
                },
                format!("Go to tag {}", t.name),
            );
        }
        out
    }

    /// Runs a command, if it can run now.
    pub fn run_command(&mut self, ctx: &egui::Context, command: Command) {
        if !self.can_run(&command) {
            return;
        }
        match command {
            Command::Palette => self.palette = Some(palette::State::default()),
            Command::Settings => self.settings_open = true,
            Command::NewTab => self.new_tab(ctx),
            Command::NextTab => {
                let next = (self.active + 1) % self.tabs.len();
                self.switch_tab(ctx, next);
            }
            Command::PreviousTab => {
                let previous = (self.active + self.tabs.len() - 1) % self.tabs.len();
                self.switch_tab(ctx, previous);
            }
            Command::OpenRepo => self.browse(ctx),
            Command::CloneRepo => self.show_clone(),
            Command::NewRepo => self.pick_new_repository(ctx),
            Command::CloseRepo => self.close(ctx),
            Command::ToggleSidebar => self.settings.hide_sidebar = !self.settings.hide_sidebar,
            Command::ToggleSplitDiff => self.settings.split_diff = !self.settings.split_diff,
            command => {
                let Some(repo) = &mut self.repo else { return };
                let head = repo.head_branch().unwrap_or("HEAD").to_string();
                match command {
                    Command::Refresh => repo.refresh(ctx),
                    Command::Find => {
                        repo.active_file = None;
                        repo.planner = None;
                        repo.conflict = None;
                        ctx.memory_mut(|m| m.request_focus(search_bar::input_id()));
                    }
                    Command::Undo => repo.undo(ctx),
                    Command::Fetch => repo.run(ctx, Op::Fetch),
                    Command::Pull => {
                        repo.run(ctx, Op::Pull(git_raccoon_core::sync::PullMode::FastForward))
                    }
                    Command::Push => repo.run(ctx, Op::Push { force: false }),
                    Command::ShowWorking => repo.select_working(),
                    Command::WriteCommit => {
                        repo.select_working();
                        ctx.memory_mut(|m| m.request_focus(working::summary_id()));
                    }
                    Command::StageAll => repo.run(ctx, Op::StageAll),
                    Command::UnstageAll => repo.run(ctx, Op::UnstageAll),
                    Command::Stash => {
                        repo.dialog = Some(Dialog::Stash {
                            message: String::new(),
                            untracked: true,
                        });
                    }
                    Command::PopStash => {
                        if let Some(s) = repo.refs.stashes.first() {
                            let id = s.target.clone();
                            repo.run(ctx, Op::StashPop(id));
                        }
                    }
                    Command::NewBranch => repo.new_branch("HEAD".into(), head),
                    Command::NewTag => repo.new_tag("HEAD".into(), head),
                    Command::AddRemote => repo.edit_remote(None),
                    Command::Continue => repo.run(ctx, Op::Continue),
                    Command::Abort => repo.confirm_abort(),
                    Command::Checkout(name) => repo.checkout(ctx, name),
                    Command::CheckoutRemote(name) => repo.run(ctx, Op::CheckoutRemote(name)),
                    Command::Merge(name) => repo.ask_merge(name, None),
                    Command::Rebase(name) => repo.confirm_rebase(name),
                    Command::FileHistory | Command::Blame => {
                        let mode = if command == Command::Blame {
                            FileMode::Blame
                        } else {
                            FileMode::History
                        };
                        if repo.file_view.is_some() {
                            repo.set_file_mode(ctx, mode);
                        } else if let Some((source, file)) = repo.diff_key() {
                            let at = match source {
                                Source::Commit(id) => Some(id),
                                Source::Working(_) => None,
                            };
                            repo.show_file(ctx, file, at, mode);
                        }
                    }
                    Command::GoTo { id, .. } => {
                        repo.planner = None;
                        repo.select(&id);
                    }
                    _ => {}
                }
            }
        }
    }

    /// The palette, while open.
    fn palette(&mut self, ctx: &egui::Context) {
        if self.palette.is_none() {
            return;
        }
        let entries = self.palette_entries();
        let Some(state) = &mut self.palette else {
            return;
        };
        match palette::show(ctx, state, &entries) {
            Some(palette::Outcome::Run(command)) => {
                self.palette = None;
                self.run_command(ctx, command);
            }
            Some(palette::Outcome::Close) => self.palette = None,
            None => {}
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        // A question, the palette or a dialog has the keyboard.
        let dialog_open = self.repo.as_ref().is_some_and(|r| r.dialog.is_some());
        if self.prompt.is_some()
            || !self.crash_reports.is_empty()
            || self.palette.is_some()
            || self.clone_form.is_some()
            || self.settings_open
            || dialog_open
        {
            return;
        }
        // While typing, only Ctrl+Shift shortcuts (and the palette) apply, so
        // Ctrl+Z, Ctrl+B and the like keep their usual meaning in the text.
        // Shift ones first: Ctrl+Shift+F shouldn't also count as Ctrl+F.
        let typing = ctx.egui_wants_keyboard_input();
        let ordered = commands::SHORTCUTS
            .iter()
            .filter(|(_, s)| s.is_some_and(|s| s.modifiers.shift))
            .chain(
                commands::SHORTCUTS
                    .iter()
                    .filter(|(_, s)| s.is_some_and(|s| !s.modifiers.shift)),
            );
        for (command, shortcut) in ordered {
            let Some(shortcut) = shortcut else { continue };
            let allowed = !typing
                || shortcut.modifiers.shift
                || *command == Command::Palette
                || *command == Command::Find;
            if allowed && ctx.input_mut(|i| i.consume_shortcut(shortcut)) {
                self.run_command(ctx, command.clone());
                return;
            }
        }
        // Ctrl+1 to Ctrl+9: that tab.
        let digits = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
            egui::Key::Num6,
            egui::Key::Num7,
            egui::Key::Num8,
            egui::Key::Num9,
        ];
        for (i, key) in digits.into_iter().enumerate() {
            if ctx.input_mut(|inp| inp.consume_key(egui::Modifiers::COMMAND, key)) {
                self.switch_tab(ctx, i);
                return;
            }
        }
        let Some(repo) = &mut self.repo else { return };
        if typing {
            return;
        }
        let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if escape {
            if repo.file_view.is_some() {
                repo.file_view = None;
            } else if repo.planner.is_some() {
                repo.planner = None;
            } else if repo.conflict.is_some() {
                repo.conflict = None;
            } else if repo.active_file.is_some() {
                repo.active_file = None;
            } else {
                repo.deselect();
            }
            return;
        }
        if repo.active_file.is_some()
            || repo.planner.is_some()
            || repo.conflict.is_some()
            || repo.file_view.is_some()
        {
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

    /// The window's panels: toolbar, sidebar, panes and history.
    fn panels(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let now = settings::now();

        // Tabs.
        let (names, active) = self.tab_names();
        let tab_action = Panel::top("tabs")
            .frame(Frame::new().fill(theme::bg()).inner_margin(Margin {
                left: 6,
                right: 6,
                top: 4,
                bottom: 0,
            }))
            .show(ui, |ui| tabs::show(ui, &names, active))
            .inner;
        match tab_action {
            Some(tabs::Action::Switch(i)) => self.switch_tab(&ctx, i),
            Some(tabs::Action::Close(i)) => self.close_tab(&ctx, i),
            Some(tabs::Action::New) => self.new_tab(&ctx),
            None => {}
        }

        // Toolbar.
        let bar = Panel::top("toolbar")
            .frame(
                Frame::new()
                    .fill(theme::bg_raised())
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
                    lfs: r.extras.lfs,
                });
                toolbar::show(
                    ui,
                    &self.logo,
                    open,
                    &self.settings.recent,
                    self.active_opening(),
                    now,
                    self.updater.ready_version(),
                )
            })
            .inner;
        match bar {
            Some(toolbar::Action::ToggleSidebar) => {
                self.settings.hide_sidebar = !self.settings.hide_sidebar
            }
            Some(toolbar::Action::Browse) => self.browse(&ctx),
            Some(toolbar::Action::Clone) => self.show_clone(),
            Some(toolbar::Action::Init) => self.pick_new_repository(&ctx),
            Some(toolbar::Action::Open(p)) => self.open(&ctx, p),
            Some(toolbar::Action::Forget(p)) => self.settings.forget(&p),
            Some(toolbar::Action::Close) => self.close(&ctx),
            Some(toolbar::Action::Palette) => self.run_command(&ctx, Command::Palette),
            Some(toolbar::Action::Settings) => self.run_command(&ctx, Command::Settings),
            Some(toolbar::Action::RestartToUpdate) => self.restart_to_update = true,
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
                        .fill(theme::danger_bg())
                        .inner_margin(Margin::symmetric(12, 8)),
                )
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(message).color(theme::danger()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::icon_button(ui, widgets::ICON_CLOSE, "Dismiss").clicked() {
                                self.error = None;
                            }
                        });
                    });
                });
        }

        // A merge (rebase, ...) waiting to be finished.
        if let Some(repo) = &mut self.repo
            && let Some(status) = &repo.status
            && let Some(state) = &status.in_progress
        {
            let conflicts = status.conflicted.len();
            let (noun, detail) = (state.what.noun(), state.detail.clone());
            let mut chosen = None;
            Panel::top("in-progress")
                .frame(
                    Frame::new()
                        .fill(theme::bg_raised())
                        .stroke(egui::Stroke::new(1.0, theme::modified()))
                        .inner_margin(Margin::symmetric(12, 8)),
                )
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(egui_phosphor::regular::GIT_MERGE)
                                .color(theme::modified()),
                        );
                        let mut text = format!("A {noun} is in progress");
                        if let Some(d) = &detail {
                            text.push_str(&format!(": {d}"));
                        }
                        ui.label(RichText::new(text).strong());
                        let what_next = match conflicts {
                            0 => "Ready to continue.".to_string(),
                            1 => "1 file has conflicts to resolve.".to_string(),
                            n => format!("{n} files have conflicts to resolve."),
                        };
                        ui.label(RichText::new(what_next).color(theme::muted()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let busy = repo.op_running();
                            if ui
                                .add_enabled(!busy, egui::Button::new(format!("Abort {noun}")))
                                .clicked()
                            {
                                chosen = Some(false);
                            }
                            let ready = !busy && conflicts == 0;
                            let label = RichText::new(format!("Continue {noun}"));
                            let go = if ready {
                                egui::Button::new(label.color(theme::on_accent()).strong())
                                    .fill(theme::accent())
                            } else {
                                egui::Button::new(label.color(theme::muted()))
                                    .fill(theme::bg_hover())
                            };
                            if ui
                                .add_enabled(!busy && conflicts == 0, go)
                                .on_disabled_hover_text("Resolve the conflicts first")
                                .clicked()
                            {
                                chosen = Some(true);
                            }
                        });
                    });
                });
            match chosen {
                Some(true) => repo.run(&ctx, Op::Continue),
                Some(false) => repo.confirm_abort(),
                None => {}
            }
        }

        let Some(repo) = &mut self.repo else {
            egui::CentralPanel::default_margins()
                .frame(
                    Frame::new()
                        .fill(theme::bg())
                        .inner_margin(Margin::same(16)),
                )
                .show(ui, |ui| {
                    if self.active_opening() {
                        return;
                    }
                    match welcome::show(ui, &self.logo, &self.settings.recent, now) {
                        Some(welcome::Action::Browse) => self.browse(&ctx),
                        Some(welcome::Action::Clone) => self.show_clone(),
                        Some(welcome::Action::Init) => self.pick_new_repository(&ctx),
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
                .frame(Frame::new().fill(theme::bg_raised()))
                .show(ui, |ui| {
                    let selected = repo.selected.as_ref().map(|s| s.id.clone());
                    let root = repo.path().to_string();
                    sidebar::show(
                        ui,
                        &repo.refs,
                        &repo.extras,
                        &root,
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
                Some(sidebar::Action::EditRemote(name)) => repo.edit_remote(name),
                Some(sidebar::Action::RemoveRemote(name)) => repo.confirm_remove_remote(name),
                Some(sidebar::Action::Merge { source, target }) => repo.ask_merge(source, target),
                Some(sidebar::Action::Rebase(onto)) => repo.confirm_rebase(onto),
                Some(sidebar::Action::OpenPath(p)) => repo.open_request = Some(p),
                Some(sidebar::Action::UpdateSubmodule(p)) => {
                    repo.run(&ctx, Op::UpdateSubmodule(p));
                }
                Some(sidebar::Action::AddWorktree) => repo.new_worktree(),
                Some(sidebar::Action::RemoveWorktree(p)) => repo.confirm_remove_worktree(p),
                None => {}
            }
        }

        // Details pane: a commit, or the working changes.
        let pane = Frame::new().fill(theme::bg_raised()).inner_margin(Margin {
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
                            active_conflict: repo.conflict.as_ref().map(|c| c.file.as_str()),
                            draft: &mut repo.draft,
                        },
                    )
                })
                .inner;
            match action {
                Some(working::Action::Open(side, f)) => repo.open_working_file(side, f),
                Some(working::Action::OpenConflict(f)) => repo.open_conflict(f),
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
            .frame(Frame::new().fill(theme::bg()))
            .show(ui, |ui| {
                let busy = repo.op_running();
                if let Some(view) = &repo.file_view {
                    let selected = repo.selected.as_ref().map(|s| s.id.clone());
                    let action = file_view::show(ui, view, selected.as_deref(), now);
                    let history = view.mode == FileMode::History;
                    // History: the picked commit's change to the file, beside the list.
                    if history && let Some(key) = repo.diff_key() {
                        let shown = diff::show(
                            ui,
                            &key.1,
                            None,
                            repo.diff.get(&key),
                            repo.diff.is_loading(),
                            None,
                            self.settings.split_diff,
                            None,
                            false,
                        );
                        match shown {
                            Some(diff::Action::SetSplit(s)) => self.settings.split_diff = s,
                            Some(diff::Action::Close) => repo.active_file = None,
                            _ => {}
                        }
                    }
                    match action {
                        Some(file_view::Action::Close) => repo.file_view = None,
                        Some(file_view::Action::Mode(m)) => repo.set_file_mode(&ctx, m),
                        Some(file_view::Action::Pick { id, path }) => {
                            repo.select(&id);
                            repo.active_file = Some(path);
                        }
                        Some(file_view::Action::GoTo(id)) => {
                            repo.file_view = None;
                            repo.select(&id);
                        }
                        None => {}
                    }
                    return;
                }
                if let Some((planner, original)) = &mut repo.planner {
                    match rebase::show(ui, planner, original, busy) {
                        Some(rebase::Action::Start) => repo.start_planned_rebase(&ctx),
                        Some(rebase::Action::Cancel) => repo.planner = None,
                        None => {}
                    }
                    return;
                }
                if let Some(open) = &mut repo.conflict {
                    match conflict::show(ui, open, busy) {
                        Some(conflict::Action::Pick(i, pick)) => repo.pick_conflict(&ctx, i, pick),
                        Some(conflict::Action::TakeSide { ours }) => {
                            let file = open.file.clone();
                            repo.run(&ctx, Op::TakeSide { file, ours });
                        }
                        Some(conflict::Action::MarkResolved) => {
                            let file = open.file.clone();
                            repo.run(&ctx, Op::MarkResolved(file));
                        }
                        Some(conflict::Action::StartEdit) => {
                            open.edit = open.text.as_ref().ok().cloned();
                        }
                        Some(conflict::Action::SaveEdit) => repo.save_conflict_edit(&ctx),
                        Some(conflict::Action::CancelEdit) => open.edit = None,
                        Some(conflict::Action::Close) => repo.conflict = None,
                        None => {}
                    }
                    return;
                }
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
                        true,
                    );
                    match action {
                        Some(diff::Action::Close) => repo.active_file = None,
                        Some(diff::Action::History | diff::Action::Blame) => {
                            let mode = if matches!(action, Some(diff::Action::History)) {
                                FileMode::History
                            } else {
                                FileMode::Blame
                            };
                            let at = match &key.0 {
                                Source::Commit(id) => Some(id.clone()),
                                Source::Working(_) => None,
                            };
                            repo.show_file(&ctx, file.clone(), at, mode);
                        }
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
                    egui::Stroke::new(1.0, theme::border()),
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
                    Some(history::Click::CherryPick { id }) => repo.run(&ctx, Op::CherryPick(id)),
                    Some(history::Click::Revert { id }) => repo.run(&ctx, Op::Revert(id)),
                    Some(history::Click::Reset { id, label }) => repo.ask_reset(id, label),
                    Some(history::Click::InteractiveRebase { id, label }) => {
                        repo.plan_rebase(id, label);
                    }
                    None => {}
                }
            });
        if let Some(op) = dialog::show(&ctx, &mut repo.dialog, &repo.refs) {
            repo.run(&ctx, op);
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_look(ctx);
        self.updater.tick(ctx, !self.settings.no_update_checks);
        self.poll_opening(ctx);
        // A submodule or worktree to open in a tab.
        if let Some(path) = self.repo.as_mut().and_then(|r| r.open_request.take()) {
            self.open(ctx, path);
        }
        // Background tabs keep going: their operations finish, files are watched.
        for tab in &mut self.tabs {
            if let Some((Some(repo), _)) = &mut tab.parked {
                repo.poll(ctx);
            }
        }
        self.poll_clone(ctx);
        if let Some(repo) = &mut self.repo {
            repo.poll(ctx);
            if let Some(e) = repo.refresh_error.take() {
                self.error = Some(e);
            }
        }
        self.shortcuts(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Restarting into an update ends the process at once: save first.
        if std::mem::take(&mut self.restart_to_update) {
            if let Some(storage) = frame.storage_mut() {
                eframe::App::save(self, storage);
                storage.flush();
            }
            if let Err(e) = self.updater.restart() {
                self.error = Some(format!("Could not update: {e}"));
            }
        }
        let ctx = ui.ctx().clone();
        self.panels(ui);
        // Above everything: the clone dialog, then Git's questions.
        self.clone_dialog(&ctx);
        if self.settings_open {
            match crate::ui::settings::show(&ctx, &mut self.settings) {
                Some(crate::ui::settings::Action::Close) => self.settings_open = false,
                Some(crate::ui::settings::Action::ClearRecent) => self.settings.recent.clear(),
                None => {}
            }
        }
        self.palette(&ctx);
        self.crash_dialog(&ctx);
        self.questions(&ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::bg().to_normalized_gamma_f32()
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.remember_tabs();
        eframe::set_value(storage, Settings::KEY, &self.settings);
    }
}

/// The look a theme choice means now (System follows the system's light or dark).
pub(crate) fn look_for(choice: ThemeChoice, system: Option<egui::Theme>) -> theme::Look {
    use theme::Look;
    match choice {
        ThemeChoice::System if system == Some(egui::Theme::Light) => Look::Light,
        ThemeChoice::System | ThemeChoice::Dark => Look::Dark,
        ThemeChoice::Light => Look::Light,
        ThemeChoice::Raccoon => Look::Raccoon,
        ThemeChoice::Pastel => Look::Pastel,
        ThemeChoice::Abyss => Look::Abyss,
        ThemeChoice::Forest => Look::Forest,
        ThemeChoice::Paper => Look::Paper,
        ThemeChoice::HighContrast => Look::HighContrast,
    }
}
