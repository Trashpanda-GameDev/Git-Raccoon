//! The application: which repo is open, the panels, and keyboard shortcuts.

use eframe::egui::{self, Frame, Margin, Panel, RichText};

use crate::repo_view::{self, Loaded, Op, RepoView, Source};
use crate::settings::{self, Settings, ThemeChoice, ThemeMode};
use crate::task::Task;
use crate::theme;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use git_raccoon_core::create;
use git_raccoon_core::sync::Progress;

use crate::commands::{self, Command, Entry};
use crate::ui::{clone, conflict, new_repo, palette, rebase, tabs};
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
    /// Loading an account's repositories (for that account), and making
    /// a new one there, for the clone dialog.
    clone_repos: Option<(usize, Task<RepoList>)>,
    /// The new-repository dialog, its templates loading, and the job making it.
    new_repo: Option<new_repo::Form>,
    new_repo_templates: Option<(new_repo::TemplatesFrom, Task<TemplateList>)>,
    making: Option<Making>,
    cloning: Option<Cloning>,
    /// The command palette, while open.
    palette: Option<palette::State>,
    /// The settings window is open.
    settings_open: bool,
    /// The look and zoom last applied.
    applied: Option<(theme::Palette, f32)>,
    /// The title bar's colours need setting (the look changed).
    title_bar_stale: bool,
    /// A menu was open when the last frame was drawn (Esc then only closes it).
    menu_was_open: bool,
    updater: crate::updates::Updater,
    /// Restart into the downloaded update once settings are saved.
    restart_to_update: bool,
    /// The licenses window, while open.
    licenses: Option<crate::ui::licenses::Tab>,
    /// The new pull/merge request window, and its request on the way.
    new_request: Option<crate::ui::forge::NewForm>,
    new_request_post: Option<crate::task::Task<Result<crate::forge::Request, String>>>,
    /// Who can review a new request, and its labels, being listed.
    new_request_choices: Option<crate::task::Task<Result<crate::forge::Choices, String>>>,
    /// The notes shown in "What's new", while it's open.
    whats_new: Vec<crate::changelog::Notes>,
    /// The feedback form, while it's open, and its post on the way.
    feedback: Option<crate::ui::feedback::Form>,
    feedback_post: Option<crate::task::Task<Result<u64, String>>>,
    /// Frames waited for the screenshot the feedback form opens with.
    feedback_capture: Option<u8>,
    /// The sign-in window, while it's open, and its work in the background.
    sign_in: Option<crate::ui::sign_in::SignIn>,
    sign_in_work: Option<SignInWork>,
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

/// How wide a side panel (the sidebar, the details) can be dragged: from
/// `min` to as wide as leaves the graph room, so it's up to the person.
fn side_range(ui: &egui::Ui, min: f32) -> std::ops::RangeInclusive<f32> {
    /// Kept for the graph, between the panels.
    const GRAPH: f32 = 320.0;
    min..=(ui.available_width() - GRAPH).max(min)
}

/// An account's repositories, or why they couldn't be listed.
type RepoList = Result<Vec<crate::forge::accounts::Repo>, String>;

/// A forge's templates, or why they couldn't be listed.
type TemplateList = Result<crate::forge::templates::Templates, String>;

/// A new repository being made in the background: what it's doing, and
/// where it opens when it's done (`Err(None)`: stopped).
struct Making {
    doing: Arc<Mutex<String>>,
    cancel: Arc<AtomicBool>,
    task: Task<Result<Made, Option<String>>>,
}

/// What making a new repository came to.
enum Made {
    /// A copy on this computer, to open.
    Here(PathBuf),
    /// Made on a forge only: its path there ("owner/name").
    There(String),
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
            .map(Settings::upgrade)
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
    pub fn with_settings(ctx: &egui::Context, mut settings: Settings) -> Self {
        theme::apply(ctx);
        // Updated since the last run: what's new in the versions since.
        let current = env!("CARGO_PKG_VERSION");
        let whats_new = settings
            .last_version
            .as_deref()
            .filter(|last| *last != current)
            .map(|last| crate::changelog::since(last, current))
            .unwrap_or_default();
        settings.last_version = Some(current.to_string());
        // Zooming is the app's own (see `zoom_input`), so it's remembered.
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
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
            clone_repos: None,
            new_repo: None,
            new_repo_templates: None,
            making: None,
            cloning: None,
            palette: None,
            settings_open: false,
            applied: None,
            title_bar_stale: true,
            menu_was_open: false,
            updater: crate::updates::Updater::new(),
            restart_to_update: false,
            crash_reports: Vec::new(),
            licenses: None,
            new_request: None,
            new_request_post: None,
            new_request_choices: None,
            whats_new,
            feedback: None,
            feedback_post: None,
            feedback_capture: None,
            sign_in: None,
            sign_in_work: None,
        }
    }

    /// Where clones and new repositories go to start with: where the last
    /// one went, or the home folder.
    fn clone_parent(&self) -> String {
        self.settings
            .clone_dir
            .clone()
            .or_else(|| {
                std::env::var("USERPROFILE")
                    .or_else(|_| std::env::var("HOME"))
                    .ok()
            })
            .unwrap_or_default()
    }

    /// Opens the clone dialog.
    pub fn show_clone(&mut self) {
        // Signed in somewhere: start with the first account's repositories.
        self.clone_form = Some(clone::Form::new(self.clone_parent()));
    }

    /// Opens the new-repository dialog: on this computer only, to start with.
    pub fn show_new_repository(&mut self) {
        self.new_repo = Some(new_repo::Form::new(
            self.clone_parent(),
            git_raccoon_core::modules::lfs_installed(),
        ));
    }

    /// The new-repository dialog: its templates, and making it.
    fn new_repo_dialog(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.new_repo else {
            self.new_repo_templates = None;
            return;
        };
        // The templates for where it's going (an account's forge, or
        // GitHub's), once a picker is opened.
        let wanted = form.templates_wanted();
        if form.want_templates && form.templates_from != Some(wanted) {
            form.templates_from = Some(wanted);
            form.templates = None;
            form.gitignore = None;
            form.license = None;
            let account = wanted.and_then(|i| self.settings.accounts.get(i)).cloned();
            self.new_repo_templates = Some((
                wanted,
                Task::spawn(ctx, move || {
                    let source = match &account {
                        Some(a) => crate::forge::accounts::templates_source(a)?,
                        None => crate::forge::templates::Source::github(),
                    };
                    crate::forge::templates::list(&source)
                }),
            ));
        }
        if let Some((from, task)) = &self.new_repo_templates
            && let Some(result) = task.poll()
        {
            if form.templates_from == Some(*from) {
                form.templates = Some(result);
            }
            self.new_repo_templates = None;
        }
        // Being made, or made.
        if let Some(making) = &self.making {
            if let Some(result) = making.task.poll() {
                self.making = None;
                form.progress = None;
                match result {
                    Ok(Made::Here(dir)) => {
                        self.new_repo = None;
                        self.open(ctx, dir.to_string_lossy());
                        return;
                    }
                    Ok(Made::There(path)) => {
                        let place = form
                            .account
                            .and_then(|i| self.settings.accounts.get(i))
                            .map(|a| a.place())
                            .unwrap_or_default();
                        form.done = Some(format!("Made {path} on {place}."));
                    }
                    Err(Some(e)) => form.error = Some(e),
                    Err(None) => {}
                }
            } else if let Ok(doing) = making.doing.lock() {
                form.progress = Some(doing.clone());
            }
        }
        match new_repo::show(ctx, form, &self.settings.accounts) {
            Some(new_repo::Action::Browse) => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Where to put the new repository")
                    .pick_folder()
                {
                    form.parent = dir.to_string_lossy().into_owned();
                }
            }
            Some(new_repo::Action::Create) => {
                form.error = None;
                form.progress = Some("Starting…".into());
                if form.here_too() {
                    self.settings.clone_dir = Some(form.parent.trim().to_string());
                }
                let account = form
                    .account
                    .and_then(|i| self.settings.accounts.get(i))
                    .cloned();
                self.making = Some(start_making(ctx, form, account));
            }
            Some(new_repo::Action::Stop) => {
                if let Some(m) = &self.making {
                    m.cancel.store(true, Ordering::Relaxed);
                }
            }
            // (Not while it's being made: Stop first.)
            Some(new_repo::Action::Cancel) if self.making.is_none() => self.new_repo = None,
            Some(new_repo::Action::Cancel) | None => {}
        }
    }

    pub fn is_cloning(&self) -> bool {
        self.cloning.is_some()
    }

    fn start_clone(&mut self, ctx: &egui::Context) {
        let Some(form) = &mut self.clone_form else {
            return;
        };
        form.error = None;
        let (url, dest, options) = (form.url.trim().to_string(), form.dest(), form.options());
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
                create::clone(&url, &dest, &options, &mut report, &c)
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
            self.clone_repos = None;
            return;
        };
        // The picked account's repositories.
        if let Some((i, task)) = &self.clone_repos
            && let Some(result) = task.poll()
        {
            if form.account == Some(*i) {
                form.repos = Some(result);
            }
            self.clone_repos = None;
        }
        if let Some(i) = form.account
            && form.repos.is_none()
            && self.clone_repos.as_ref().is_none_or(|(j, _)| *j != i)
            && let Some(account) = self.settings.accounts.get(i).cloned()
        {
            self.clone_repos = Some((
                i,
                Task::spawn(ctx, move || crate::forge::accounts::repositories(&account)),
            ));
        }
        let progress = self
            .cloning
            .as_ref()
            .map(|c| c.progress.lock().ok().and_then(|p| p.clone()));
        match clone::show(ctx, form, progress, &self.settings.accounts) {
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
        let look = look_for(
            self.settings.theme,
            self.settings.theme_mode,
            ctx.system_theme(),
        );
        // A custom theme, in the same version (dark or light) as the look.
        let colours = match self.settings.current_theme() {
            Some(custom) => custom.palette(look.is_light()),
            None => *look.palette(),
        };
        let zoom = self.settings.zoom.unwrap_or(1.0);
        if self.applied == Some((colours, zoom)) {
            return;
        }
        let theme_changed = self.applied.is_none_or(|(c, _)| c != colours);
        self.applied = Some((colours, zoom));
        if theme_changed {
            theme::set_palette(ctx, colours);
            let light = (self.settings.theme_mode != ThemeMode::System).then(|| colours.is_light());
            crate::title_bar::follow(ctx, light);
            self.title_bar_stale = true;
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
            // An account signed in here answers for its host, without asking.
            if let Some((account, asked)) =
                crate::forge::accounts::for_git_prompt(&request.prompt, &self.settings.accounts)
            {
                let account = account.clone();
                // Reading the keychain (or renewing a GitLab sign-in) can block.
                std::thread::spawn(move || {
                    request.answer(crate::forge::accounts::answer_git(&account, asked).ok());
                });
                return;
            }
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
        let names = self.tab_list().into_iter().map(|(_, name)| name).collect();
        (names, self.active)
    }

    /// Each tab's id and name.
    fn tab_list(&self) -> Vec<(u64, String)> {
        self.tabs
            .iter()
            .map(|t| {
                let name = t.path.as_deref().map_or_else(
                    || "New tab".to_string(),
                    |p| settings::repo_name(p).to_string(),
                );
                (t.id, name)
            })
            .collect()
    }

    /// Moves a tab (dragged) to another place.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = if self.active == from {
            to
        } else if from < self.active && self.active <= to {
            self.active - 1
        } else if to <= self.active && self.active < from {
            self.active + 1
        } else {
            self.active
        };
        self.remember_tabs();
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

    /// The window is always just "Git Raccoon": the tabs say which repository.
    fn update_title(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title("Git Raccoon".into()));
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

    /// The new pull/merge request window's form, if it's open.
    pub fn new_request_form(&self) -> Option<&crate::ui::forge::NewForm> {
        self.new_request.as_ref()
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
            | Command::ToggleWholeFile
            | Command::ZoomIn
            | Command::ZoomOut
            | Command::ZoomReset
            | Command::Feedback
            | Command::Terminal
            | Command::WhatsNew
            | Command::CheckForUpdates
            | Command::ShowWorking
            | Command::WriteCommit
            | Command::AddRemote
            | Command::GoTo { .. } => true,
            Command::FileHistory | Command::Blame => repo.diff_key().is_some(),
            Command::GoToHead => repo.history.head_row().is_some(),
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
        add(Command::GoToHead, "Go to the checked-out commit".into());
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
        add(Command::ZoomIn, "Zoom in (bigger)".into());
        add(Command::ZoomOut, "Zoom out (smaller)".into());
        add(Command::ZoomReset, "Actual size".into());
        add(Command::Terminal, "Open a terminal here".into());
        add(Command::Feedback, "Send feedback, a bug or an idea…".into());
        add(Command::WhatsNew, "What's new in this version".into());
        if self.updater.is_installed() {
            add(Command::CheckForUpdates, "Check for updates".into());
        }
        add(
            Command::ToggleSplitDiff,
            "Switch diffs between unified and side by side".into(),
        );
        add(
            Command::ToggleWholeFile,
            "Show the whole file in diffs, or just the changes".into(),
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
            Command::NewRepo => self.show_new_repository(),
            Command::CloseRepo => self.close(ctx),
            Command::ToggleSidebar => self.settings.hide_sidebar = !self.settings.hide_sidebar,
            Command::ToggleSplitDiff => self.settings.split_diff = !self.settings.split_diff,
            Command::ToggleWholeFile => self.settings.whole_file = !self.settings.whole_file,
            Command::FileHistory => self.settings.diff_history = !self.settings.diff_history,
            Command::Blame => self.settings.diff_blame = !self.settings.diff_blame,
            Command::ZoomIn => self.settings.zoom_step(true),
            Command::ZoomOut => self.settings.zoom_step(false),
            Command::ZoomReset => self.settings.zoom = None,
            Command::Feedback => self.open_feedback(ctx),
            Command::Terminal => {
                if let Some(path) = self.repo.as_ref().map(|r| r.path().to_string()) {
                    self.open_terminal(&path);
                }
            }
            Command::CheckForUpdates => {
                self.updater.check_now(ctx);
                // Settings › Updates says how it went.
                crate::ui::settings::open_on_updates(ctx);
                self.settings_open = true;
            }
            Command::WhatsNew => {
                self.whats_new = crate::changelog::of(env!("CARGO_PKG_VERSION"))
                    .into_iter()
                    .collect();
            }
            command => {
                let Some(repo) = &mut self.repo else { return };
                let head = repo.head_branch().unwrap_or("HEAD").to_string();
                match command {
                    Command::Refresh => {
                        repo.refresh(ctx);
                        repo.forge_stale = true;
                    }
                    Command::Find => {
                        repo.active_file = None;
                        repo.planner = None;
                        repo.conflict = None;
                        repo.search.open = true;
                        repo.search.focus = true;
                    }
                    Command::Undo => repo.undo(ctx),
                    Command::Fetch => repo.fetch(ctx, None, self.settings.fetch_options()),
                    Command::Pull => repo.pull(ctx, None, self.settings.pull),
                    Command::Push => repo.push(ctx),
                    Command::ShowWorking => repo.select_working(),
                    Command::GoToHead => {
                        if let Some(row) = repo.history.head_row() {
                            repo.select_index(row);
                        }
                    }
                    Command::WriteCommit => {
                        repo.select_working();
                        ctx.memory_mut(|m| m.request_focus(working::summary_id()));
                    }
                    Command::StageAll => repo.run(ctx, Op::StageAll),
                    Command::UnstageAll => repo.run(ctx, Op::UnstageAll),
                    Command::Stash => {
                        repo.dialog = Some(repo.stash_dialog());
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

    /// Finds the open repository's forge when that's due (on opening,
    /// signing in, Refresh, Fetch), loads what's on it, and takes in what's
    /// loaded.
    fn keep_forge(&mut self, ctx: &egui::Context) {
        let Some(repo) = &mut self.repo else { return };
        let remotes: Vec<(String, Option<String>)> = repo
            .refs
            .remotes
            .iter()
            .map(|r| (r.name.clone(), r.url.clone()))
            .collect();
        let managed: Vec<String> = self
            .settings
            .accounts
            .iter()
            .filter(|a| a.kind == crate::forge::Kind::GitLab)
            .map(|a| a.base.clone())
            .collect();
        repo.web = crate::forge::Web::new(&remotes, &managed);
        if std::mem::take(&mut repo.forge_stale) {
            let found = crate::forge_view::ForgeView::find(&remotes, &self.settings.accounts);
            match (&mut repo.forge, found) {
                (Some(now), Some(found)) if now.same_as(&found) => now.load(ctx),
                (forge, Some(mut found)) => {
                    found.load(ctx);
                    *forge = Some(found);
                }
                (forge, None) => *forge = None,
            }
        }
        crate::ui::sidebar::set_request_noun(
            ctx,
            repo.forge
                .as_ref()
                .filter(|f| f.account.is_some())
                .map(|f| f.project.kind.request().to_lowercase()),
        );
        if let Some(f) = &mut repo.forge {
            f.poll();
            if std::mem::take(&mut f.state.reviewed)
                && let Some(crate::forge_view::Opened::Request(n)) = f.open
            {
                self.settings.review_drafts.remove(&f.drafts_key(n));
            }
            if std::mem::take(&mut f.reloading) {
                f.load(ctx);
            }
            let root = repo.path().to_string();
            if let Some(f) = &mut repo.forge {
                f.keep_local(ctx, &root);
            }
        }
    }

    /// The new pull/merge request window, and opening it in the background.
    fn new_request_window(&mut self, ctx: &egui::Context) {
        use crate::ui::forge::NewAction;
        let Some(form) = &mut self.new_request else {
            return;
        };
        let Some(repo) = &mut self.repo else {
            self.new_request = None;
            return;
        };
        // Pushed yet, and what it has for the target (once it's picked).
        compare_form(repo, form);
        // Who can review it, and the labels: listed once, in the background.
        if let Some(result) = self
            .new_request_choices
            .as_ref()
            .and_then(crate::task::Task::poll)
        {
            self.new_request_choices = None;
            form.choices = Some(result);
        }
        if form.choices.is_none()
            && self.new_request_choices.is_none()
            && let Some(f) = &repo.forge
        {
            match f.account.clone() {
                Some(account) => {
                    let project = f.project.clone();
                    self.new_request_choices = Some(crate::task::Task::spawn(ctx, move || {
                        crate::forge::accounts::session(&account, &project)?.choices()
                    }));
                }
                None => form.choices = Some(Err("not signed in".into())),
            }
        }
        let mut create = false;
        if form.pushing && !repo.op_running() {
            form.pushing = false;
            if form.unpushed {
                form.error = Some("pushing it didn't work".into());
            } else {
                create = true;
            }
        }
        if let Some(result) = self
            .new_request_post
            .as_ref()
            .and_then(crate::task::Task::poll)
        {
            self.new_request_post = None;
            form.sending = false;
            match result {
                Ok(made) => {
                    self.new_request = None;
                    repo.forge_stale = true;
                    repo.deselect();
                    if let Some(f) = &mut repo.forge {
                        f.open_request(ctx, made.number);
                    }
                    return;
                }
                Err(e) => form.error = Some(e),
            }
        }
        match crate::ui::forge::new_request(ctx, form) {
            Some(NewAction::PushAndCreate) => {
                form.pushing = true;
                form.error = None;
                repo.push(ctx);
            }
            Some(NewAction::Create) => create = true,
            Some(NewAction::Close) => {
                self.new_request = None;
                self.new_request_post = None;
                self.new_request_choices = None;
                return;
            }
            None => {}
        }
        if create {
            let Some(f) = &repo.forge else { return };
            let Some(account) = f.account.clone() else {
                return;
            };
            let project = f.project.clone();
            let new = crate::forge::NewRequest {
                source: form.source.clone(),
                target: form.target.trim().to_string(),
                title: form.title.trim().to_string(),
                body: form.body.clone(),
                draft: form.draft,
                reviewers: form.reviewers.clone(),
                assignees: form.assignees.clone(),
                labels: form.labels.clone(),
                remove_source: form.remove_source,
                squash: form.squash,
            };
            form.sending = true;
            form.error = None;
            self.new_request_post = Some(crate::task::Task::spawn(ctx, move || {
                crate::forge::accounts::session(&account, &project)?.create(&new)
            }));
        }
    }

    /// Opens a terminal in `dir`; says why if it can't.
    fn open_terminal(&mut self, dir: &str) {
        if let Err(why) = crate::terminal::open(dir, &self.settings.terminal) {
            self.error = Some(format!("Could not open a terminal: {why}."));
        }
    }

    /// Opens the feedback form, once the window's screenshot is taken (so
    /// the form isn't in it).
    fn open_feedback(&mut self, ctx: &egui::Context) {
        if self.feedback.is_some() || self.feedback_capture.is_some() {
            return;
        }
        self.feedback_capture = Some(0);
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(
            "feedback",
        )));
        ctx.request_repaint();
    }

    /// Takes in the screenshot for the feedback form, and opens it; without
    /// one if none comes (not every system can take them).
    fn feedback_screenshot(&mut self, ctx: &egui::Context) {
        let Some(waited) = self.feedback_capture else {
            return;
        };
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot {
                    image, user_data, ..
                } if user_data
                    .data
                    .as_ref()
                    .and_then(|d| d.downcast_ref::<&str>())
                    == Some(&"feedback") =>
                {
                    Some(std::sync::Arc::clone(image))
                }
                _ => None,
            })
        });
        if shot.is_some() || waited >= 10 {
            self.feedback_capture = None;
            self.feedback = Some(crate::ui::feedback::Form::with_screenshot(shot));
        } else {
            self.feedback_capture = Some(waited + 1);
            ctx.request_repaint();
        }
    }

    /// The sign-in window, and its work in the background.
    fn sign_in_window(&mut self, ctx: &egui::Context) {
        use crate::forge::{accounts, oauth};
        use crate::ui::sign_in::{Action, Stage};
        let Some(s) = &mut self.sign_in else { return };
        // Work finished in the background.
        match &self.sign_in_work {
            Some(SignInWork::Code(task)) => {
                if let Some(result) = task.poll() {
                    self.sign_in_work = None;
                    match result {
                        Ok(code) => {
                            ctx.open_url(egui::OpenUrl::new_tab(&code.page));
                            let (kind, base) = s.forge();
                            let client = s.browser().unwrap_or_default();
                            let app = (!client.is_empty()).then(|| client.clone());
                            let cancel = Arc::new(AtomicBool::new(false));
                            let stop = Arc::clone(&cancel);
                            let waiting = code.clone();
                            let task = crate::task::Task::spawn(ctx, move || {
                                let mut interval = waiting.interval;
                                loop {
                                    std::thread::sleep(std::time::Duration::from_secs(interval));
                                    if stop.load(Ordering::Relaxed) {
                                        return Err("cancelled".to_string());
                                    }
                                    match oauth::poll(kind, &base, &client, &waiting) {
                                        oauth::Poll::Waiting => {}
                                        oauth::Poll::SlowDown => interval += 5,
                                        oauth::Poll::Done(secret) => {
                                            return accounts::sign_in(
                                                kind,
                                                &base,
                                                &secret,
                                                app.clone(),
                                            );
                                        }
                                        oauth::Poll::Failed(why) => return Err(why),
                                    }
                                }
                            });
                            self.sign_in_work = Some(SignInWork::Account(task, cancel));
                            s.stage = Stage::Approving(code);
                        }
                        Err(why) => s.stage = Stage::Failed(why),
                    }
                }
            }
            Some(SignInWork::Account(task, _)) => {
                if let Some(result) = task.poll() {
                    self.sign_in_work = None;
                    match result {
                        Ok(account) => {
                            self.settings
                                .accounts
                                .retain(|a| !(a.base == account.base && a.login == account.login));
                            self.settings.accounts.push(account);
                            self.sign_in = None;
                            if let Some(repo) = &mut self.repo {
                                repo.forge_stale = true;
                            }
                            return;
                        }
                        Err(why) => s.stage = Stage::Failed(why),
                    }
                }
            }
            None => {}
        }
        match crate::ui::sign_in::show(ctx, s) {
            Some(Action::Browser) => {
                let (kind, base) = s.forge();
                let client = s.browser().unwrap_or_default();
                s.stage = Stage::Starting;
                self.sign_in_work =
                    Some(SignInWork::Code(crate::task::Task::spawn(ctx, move || {
                        oauth::start(kind, &base, &client)
                    })));
            }
            Some(Action::Token) => {
                let (kind, base) = s.forge();
                let secret = accounts::Secret {
                    token: s.token.trim().to_string(),
                    refresh: None,
                    expires: None,
                };
                s.stage = Stage::Checking;
                let task = crate::task::Task::spawn(ctx, move || {
                    accounts::sign_in(kind, &base, &secret, None)
                });
                self.sign_in_work = Some(SignInWork::Account(task, Arc::default()));
            }
            Some(Action::Open(url)) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            Some(Action::Close) => {
                if let Some(SignInWork::Account(_, cancel)) = &self.sign_in_work {
                    cancel.store(true, Ordering::Relaxed);
                }
                self.sign_in_work = None;
                self.sign_in = None;
            }
            None => {}
        }
    }

    /// Ctrl+scroll, Ctrl+plus, Ctrl+minus and Ctrl+0 size the interface,
    /// anywhere (even in a dialog or while typing), remembered like the
    /// size picked in Settings.
    fn zoom_input(&mut self, ctx: &egui::Context) {
        use commands::Command::{ZoomIn, ZoomOut, ZoomReset};
        let (scroll, keys) = ctx.input_mut(|i| {
            let mut keys = Vec::new();
            for (command, shortcut) in commands::SHORTCUTS {
                if matches!(command, ZoomIn | ZoomOut | ZoomReset)
                    && let Some(s) = shortcut
                    && i.consume_shortcut(s)
                {
                    keys.push(command.clone());
                }
            }
            (i.zoom_delta(), keys)
        });
        if (scroll - 1.0).abs() > f32::EPSILON {
            self.settings.set_zoom(self.settings.zoom() * scroll);
        }
        for command in keys {
            self.run_command(ctx, command);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        // A question, the palette or a dialog has the keyboard.
        let dialog_open = self.repo.as_ref().is_some_and(|r| r.dialog.is_some());
        if self.prompt.is_some()
            || !self.crash_reports.is_empty()
            || self.palette.is_some()
            || self.clone_form.is_some()
            || self.new_repo.is_some()
            || self.settings_open
            || self.licenses.is_some()
            || !self.whats_new.is_empty()
            || self.feedback.is_some()
            || self.sign_in.is_some()
            || self.new_request.is_some()
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
        // Esc with a menu open only closes the menu.
        if escape && self.menu_was_open {
            return;
        }
        if escape {
            // The innermost thing first: an open search bar closes on its
            // own (Esc also takes the focus from it, so it doesn't count
            // as typing by now).
            if repo.search.open {
                repo.search.close();
            } else if let Some(f) = repo.forge.as_mut().filter(|f| f.open.is_some()) {
                let state = &mut f.state;
                if state.finishing.is_some() || state.merging.is_some() {
                    // Its window closes on its own.
                } else if state.composer.is_some() {
                    state.composer = None;
                } else if !state.picked.is_empty() {
                    state.picked.clear();
                } else {
                    f.open = None;
                }
            } else if repo.active_file.is_some() && !repo.picked.is_empty() {
                repo.picked.clear();
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
        if repo.active_file.is_some() || repo.planner.is_some() || repo.conflict.is_some() {
            return;
        }
        // The arrows move a row; with Shift they follow the branch, with
        // Ctrl they go to the next commit with a label.
        let (down, up, mods) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowUp),
                i.modifiers,
            )
        });
        if down || up {
            if mods.shift {
                repo.follow_branch(down);
            } else if mods.command {
                repo.step_label(down);
            } else {
                repo.step(if down { 1 } else { -1 });
            }
            return;
        }
        let jump = ctx.input(|i| {
            [
                (egui::Key::PageDown, 20),
                (egui::Key::PageUp, -20),
                (egui::Key::Home, isize::MIN),
                (egui::Key::End, isize::MAX),
            ]
            .into_iter()
            .find(|(k, _)| i.key_pressed(*k))
            .map(|(_, s)| s)
        });
        if let Some(step) = jump {
            repo.step(step);
        }
    }

    /// The window's panels: toolbar, sidebar, panes and history.
    fn panels(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let now = settings::now();

        // Tabs.
        let (list, active) = (self.tab_list(), self.active);
        let tab_action = Panel::top("tabs")
            .frame(Frame::new().fill(theme::bg()).inner_margin(Margin {
                left: 6,
                right: 6,
                top: 4,
                bottom: 0,
            }))
            .show(ui, |ui| tabs::show(ui, &list, active))
            .inner;
        match tab_action {
            Some(tabs::Action::Switch(i)) => self.switch_tab(&ctx, i),
            Some(tabs::Action::Close(i)) => self.close_tab(&ctx, i),
            Some(tabs::Action::New) => self.new_tab(&ctx),
            Some(tabs::Action::Move { from, to }) => self.move_tab(from, to),
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
                    remotes: r.refs.remotes.iter().map(|r| r.name.as_str()).collect(),
                    fetch: self.settings.fetch_options(),
                    auto_fetch: r.auto_fetch_status(self.settings.auto_fetch_minutes.min(60)),
                    pull: self.settings.pull,
                });
                toolbar::show(
                    ui,
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
            Some(toolbar::Action::Open(p)) => self.open(&ctx, p),
            Some(toolbar::Action::Forget(p)) => self.settings.forget(&p),
            Some(toolbar::Action::NewBranch) => self.run_command(&ctx, Command::NewBranch),
            Some(toolbar::Action::Palette) => self.run_command(&ctx, Command::Palette),
            Some(toolbar::Action::Search) => self.run_command(&ctx, Command::Find),
            Some(toolbar::Action::Settings) => self.run_command(&ctx, Command::Settings),
            Some(toolbar::Action::Feedback) => self.run_command(&ctx, Command::Feedback),
            Some(toolbar::Action::Terminal) => self.run_command(&ctx, Command::Terminal),
            Some(toolbar::Action::RestartToUpdate) => self.restart_to_update = true,
            Some(toolbar::Action::Fetch(from)) => {
                let options = self.settings.fetch_options();
                if let Some(repo) = &mut self.repo {
                    repo.fetch(&ctx, from, options);
                    repo.forge_stale = true;
                }
            }
            Some(toolbar::Action::SetFetchOptions(options)) => {
                self.settings.fetch_prune = options.prune;
                self.settings.fetch_force = options.force;
            }
            Some(toolbar::Action::Pull(from)) => {
                if let Some(repo) = &mut self.repo {
                    repo.pull(&ctx, from, self.settings.pull);
                }
            }
            Some(toolbar::Action::SetPullOptions(options)) => self.settings.pull = options,
            Some(toolbar::Action::Push) => {
                if let Some(r) = &mut self.repo {
                    r.push(&ctx);
                }
            }
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

        // Where to push a branch that isn't on a remote yet.
        if let Some(repo) = &mut self.repo {
            let head = repo.refs.local.iter().find(|b| b.is_head);
            // Another branch checked out since: the question is moot.
            if repo
                .push_to
                .as_ref()
                .is_some_and(|t| head.is_none_or(|h| h.name != t.branch))
            {
                repo.push_to = None;
            }
            if let Some(target) = &mut repo.push_to {
                let mut answer = None;
                Panel::top("push-to")
                    .frame(
                        Frame::new()
                            .fill(theme::bg_raised())
                            .stroke(egui::Stroke::new(1.0, theme::accent()))
                            .inner_margin(Margin::symmetric(12, 8)),
                    )
                    .show(ui, |ui| {
                        answer = crate::ui::push_to::show(ui, target, &repo.refs);
                    });
                match answer {
                    Some(crate::ui::push_to::Answer::Push) => {
                        if let Some(t) = repo.push_to.take() {
                            let to = Some((t.remote, t.name.trim().to_string()));
                            repo.run(&ctx, Op::Push { force: false, to });
                        }
                    }
                    Some(crate::ui::push_to::Answer::Cancel) => repo.push_to = None,
                    None => {}
                }
            }
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
                                    .fill(theme::accent_fill())
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
                        Some(welcome::Action::Init) => self.show_new_repository(),
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
                .size_range(side_range(ui, 140.0))
                .frame(Frame::new().fill(theme::bg_raised()))
                .show(ui, |ui| {
                    let selected = repo.selected.as_ref().map(|s| s.id.clone());
                    let root = repo.path().to_string();
                    let hidden = self.settings.sidebar_hidden.clone();
                    sidebar::show(
                        ui,
                        &repo.refs,
                        &repo.extras,
                        &root,
                        &mut repo.sidebar_filter,
                        &mut repo.collapsed,
                        sidebar::Shown {
                            selected: selected.as_deref(),
                            forge: repo.forge.as_ref(),
                            web: &repo.web,
                            hidden: &hidden,
                        },
                    )
                })
                .inner;
            match jump {
                Some(sidebar::Action::OpenRequest(n)) => {
                    repo.deselect();
                    if let Some(f) = &mut repo.forge {
                        f.open_request(&ctx, n);
                    }
                }
                Some(sidebar::Action::OpenIssue(n)) => {
                    repo.deselect();
                    if let Some(f) = &mut repo.forge {
                        f.open = Some(crate::forge_view::Opened::Issue(n));
                    }
                }
                Some(sidebar::Action::SignInToForge) => {
                    if let Some(f) = &repo.forge {
                        self.sign_in = Some(crate::ui::sign_in::SignIn::for_project(
                            &f.project,
                            &self.settings.accounts,
                        ));
                    }
                }
                Some(sidebar::Action::NewRequest(from)) => {
                    self.new_request = new_form(repo, from.as_deref());
                }
                Some(sidebar::Action::Terminal(dir)) => {
                    if let Err(why) = crate::terminal::open(&dir, &self.settings.terminal) {
                        self.error = Some(format!("Could not open a terminal: {why}."));
                    }
                }
                Some(sidebar::Action::ToggleSection(key)) => {
                    let hidden = &mut self.settings.sidebar_hidden;
                    if let Some(at) = hidden.iter().position(|h| *h == key) {
                        hidden.remove(at);
                    } else {
                        hidden.push(key);
                    }
                }
                Some(sidebar::Action::ShowAllSections) => self.settings.sidebar_hidden.clear(),
                Some(action) => repo_action(repo, &ctx, action),
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
        // A pull/merge request or issue from the forge.
        if let Some(forge) = repo
            .forge
            .as_ref()
            .filter(|f| matches!(f.open, Some(crate::forge_view::Opened::Issue(_))))
        {
            let action = Panel::right("forge-details")
                .resizable(true)
                .default_size(380.0)
                .size_range(side_range(ui, 220.0))
                .frame(pane)
                .show(ui, |ui| crate::ui::forge::show(ui, forge, now))
                .inner;
            match action {
                Some(crate::ui::forge::Action::Open(url)) => {
                    ctx.open_url(egui::OpenUrl::new_tab(url))
                }
                Some(crate::ui::forge::Action::Close) => repo.close_forge_item(),
                None => {}
            }
        } else if let Some(sel) = &repo.selected {
            let id = sel.id.clone();
            let action = Panel::right("details")
                .resizable(true)
                .default_size(380.0)
                .size_range(side_range(ui, 220.0))
                .frame(pane)
                .show(ui, |ui| {
                    let stash = repo.stash(&id).map(|_| repo.is_staging());
                    details::show(
                        ui,
                        &id,
                        repo.details.get(&id),
                        repo.details.is_loading(),
                        details::Files {
                            active: repo.active_file.as_deref(),
                            tree: self.settings.file_tree,
                        },
                        now,
                        stash,
                    )
                })
                .inner;
            match action {
                Some(details::Action::OpenFile(f)) => repo.active_file = Some(f),
                Some(details::Action::Select(p)) => repo.select(&p),
                Some(details::Action::Close) => repo.deselect(),
                Some(details::Action::ToggleTree) => {
                    self.settings.file_tree = !self.settings.file_tree;
                }
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
                .size_range(side_range(ui, 220.0))
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
                            tree: self.settings.file_tree,
                            selection: &mut repo.file_selection,
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
                    repo.dialog = Some(repo.stash_dialog());
                }
                Some(working::Action::Close) => repo.deselect(),
                Some(working::Action::DiscardAll) => repo.confirm_discard_all(),
                Some(working::Action::Bulk(bulk)) => {
                    repo.file_selection = working::FileSelection::default();
                    match bulk {
                        working::Bulk::Stage(files) => {
                            repo.run(
                                &ctx,
                                Op::Then(files.into_iter().map(Op::StageFile).collect()),
                            );
                        }
                        working::Bulk::Unstage(files) => {
                            repo.run(
                                &ctx,
                                Op::Then(files.into_iter().map(Op::UnstageFile).collect()),
                            );
                        }
                        working::Bulk::Discard(
                            git_raccoon_core::working::Side::Unstaged,
                            files,
                        ) => {
                            repo.confirm_discard(files);
                        }
                        working::Bulk::Discard(_, files) => repo.confirm_discard_staged(files),
                        working::Bulk::Ignore(patterns) => repo.run(&ctx, Op::Ignore(patterns)),
                        working::Bulk::Stash(files) => {
                            repo.dialog = Some(repo.stash_files_dialog(files));
                        }
                    }
                }
                Some(working::Action::ToggleTree) => {
                    self.settings.file_tree = !self.settings.file_tree;
                }
                None => {}
            }
        }

        // History (or a diff over it).
        egui::CentralPanel::no_frame()
            .frame(Frame::new().fill(theme::bg()))
            .show(ui, |ui| {
                // A pull/merge request, in place of the graph.
                if let Some(action) = request_view(ui, repo, &self.settings.review_drafts, now) {
                    let pull = self.settings.pull;
                    if let Err(e) =
                        request_action(repo, &mut self.settings.review_drafts, &ctx, action, pull)
                    {
                        self.error = Some(e);
                    }
                }
                if repo
                    .forge
                    .as_ref()
                    .is_some_and(|f| matches!(f.open, Some(crate::forge_view::Opened::Request(_))))
                {
                    return;
                }
                let busy = repo.op_running();
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
                            repo.confirm_take_side(file, ours);
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
                    let lines = diff::Lines {
                        picked: &repo.picked,
                        staging: match key.0 {
                            Source::Working(side) => Some((side, repo.is_staging())),
                            Source::Commit(_) => None,
                        },
                    };
                    let selected = repo.selected.as_ref().map(|s| s.id.clone());
                    let past = diff::Past {
                        history: self.settings.diff_history.then(|| repo.open_history()),
                        blame: self.settings.diff_blame.then(|| repo.open_blame()),
                        history_file: repo.history_file.as_deref().unwrap_or(&file),
                        selected: selected.as_deref().filter(|_| !repo.working_selected),
                        now,
                    };
                    let action = diff::show(
                        ui,
                        &file,
                        side_label,
                        repo.diff.get(&key),
                        repo.diff.is_loading(),
                        images,
                        (self.settings.split_diff, self.settings.whole_file),
                        &lines,
                        Some(&past),
                    );
                    match action {
                        Some(diff::Action::Close) => repo.active_file = None,
                        Some(diff::Action::ToggleHistory) => {
                            self.settings.diff_history = !self.settings.diff_history;
                        }
                        Some(diff::Action::ToggleBlame) => {
                            self.settings.diff_blame = !self.settings.diff_blame;
                        }
                        Some(diff::Action::SetSplit(s)) => self.settings.split_diff = s,
                        Some(diff::Action::SetWhole(w)) => self.settings.whole_file = w,
                        Some(diff::Action::StageHunk(h)) => repo.stage_hunk(&ctx, h),
                        Some(diff::Action::Select { at, how }) => repo.select_line(at, how),
                        Some(diff::Action::SelectAll) => repo.select_all_lines(),
                        Some(diff::Action::CopyPicked) => {
                            if let Some(text) = repo.picked_text() {
                                ctx.copy_text(text);
                            }
                        }
                        Some(diff::Action::Pick { id, path }) => {
                            repo.select(&id);
                            repo.active_file = Some(path);
                        }
                        Some(diff::Action::GoTo(id)) => {
                            repo.active_file = None;
                            repo.select(&id);
                        }
                        Some(diff::Action::StagePicked) => repo.stage_picked(&ctx),
                        None => {}
                    }
                    return;
                }
                // The search box over the history, while searching (Ctrl+F).
                if let Some(back) = highlight_bar(ui, repo) {
                    match back {
                        Some(n) => {
                            repo.highlight = None;
                            repo.deselect();
                            if let Some(f) = &mut repo.forge {
                                f.open_request(&ctx, n);
                                f.state.tab = crate::forge_view::Tab::Commits;
                            }
                            return;
                        }
                        None => repo.highlight = None,
                    }
                }
                if repo.search.open {
                    search_bar::show(&ctx, ui.max_rect(), repo);
                }
                match history::show(
                    ui,
                    repo,
                    &mut self.scroll,
                    now,
                    &mut self.settings.columns,
                    &mut self.settings.column_widths,
                    self.settings.branch_lines,
                ) {
                    Some(history::Click::Row(row)) => {
                        repo.select_index(row);
                        repo.scroll_to = None;
                    }
                    Some(history::Click::Working) => repo.select_working(),
                    Some(history::Click::Branch(sidebar::Action::NewRequest(from))) => {
                        self.new_request = new_form(repo, from.as_deref());
                    }
                    Some(history::Click::Branch(action)) => repo_action(repo, &ctx, action),
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
                    Some(history::Click::Checkout(name)) => repo.checkout(&ctx, name),
                    Some(history::Click::StashApply(id)) => repo.run(&ctx, Op::StashApply(id)),
                    Some(history::Click::StashPop(id)) => repo.run(&ctx, Op::StashPop(id)),
                    Some(history::Click::StashDrop(id)) => repo.confirm_drop(id),
                    Some(history::Click::Merge { source, target }) => {
                        repo.ask_merge(source, Some(target));
                    }
                    Some(history::Click::CheckoutRemote(name)) => {
                        repo.run(&ctx, Op::CheckoutRemote(name));
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
        self.zoom_input(ctx);
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
            repo.whole_file = self.settings.whole_file;
            repo.auto_fetch(
                ctx,
                self.settings.auto_fetch_minutes.min(60),
                self.settings.fetch_prune,
            );
            repo.show_history = self.settings.diff_history;
            repo.show_blame = self.settings.diff_blame;
            repo.poll(ctx);
            if let Some(e) = repo.refresh_error.take() {
                self.error = Some(e);
            }
        }
        self.keep_forge(ctx);
        self.shortcuts(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.draw(ui, frame);
        // Shortcuts (next frame) run before menus are drawn again.
        self.menu_was_open = ui.ctx().any_popup_open();
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::bg().to_normalized_gamma_f32()
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.remember_tabs();
        eframe::set_value(storage, Settings::KEY, &self.settings);
    }
}

impl App {
    fn draw(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // The title bar matches the tab bar beneath it.
        if std::mem::take(&mut self.title_bar_stale) {
            crate::title_bar::colour(frame, theme::bg(), theme::text(), theme::border());
        }
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
        // A branch being dragged (from the sidebar or the graph) follows the pointer.
        sidebar::drag_label(ui.ctx());
        // Above everything: the clone and new-repository dialogs, then Git's
        // questions.
        self.clone_dialog(&ctx);
        self.new_repo_dialog(&ctx);
        if self.settings_open {
            let updates = (
                self.updater.status(),
                self.updater.is_checking(),
                self.updater.is_installed(),
            );
            ctx.data_mut(|d| d.insert_temp(crate::ui::settings::updates_id(), updates));
            match crate::ui::settings::show(&ctx, &mut self.settings) {
                Some(crate::ui::settings::Action::CheckForUpdates) => {
                    self.updater.check_now(&ctx);
                }
                Some(crate::ui::settings::Action::Close) => self.settings_open = false,
                Some(crate::ui::settings::Action::ClearRecent) => self.settings.recent.clear(),
                Some(crate::ui::settings::Action::AddAccount) => {
                    self.settings_open = false;
                    self.sign_in = Some(crate::ui::sign_in::SignIn::default());
                }
                Some(crate::ui::settings::Action::SignOut(i)) => {
                    if i < self.settings.accounts.len() {
                        let account = self.settings.accounts.remove(i);
                        crate::forge::accounts::forget(&account);
                    }
                }
                Some(crate::ui::settings::Action::Licenses) => {
                    self.settings_open = false;
                    self.licenses = Some(crate::ui::licenses::Tab::GitRaccoon);
                }
                Some(crate::ui::settings::Action::ImportTheme) => {
                    if let Some(file) = rfd::FileDialog::new()
                        .set_title("A Git Raccoon theme")
                        .add_filter("Theme", &["json"])
                        .pick_file()
                    {
                        match std::fs::read_to_string(&file)
                            .map_err(|e| e.to_string())
                            .and_then(|text| crate::themes::CustomTheme::from_json(&text))
                        {
                            Ok(mut theme) => {
                                theme.name =
                                    crate::themes::unique_name(&theme.name, &self.settings.themes);
                                self.settings.custom_theme = Some(theme.name.clone());
                                self.settings.themes.push(theme);
                            }
                            Err(e) => self.error = Some(format!("Could not add the theme: {e}.")),
                        }
                    }
                }
                Some(crate::ui::settings::Action::ExportTheme(name)) => {
                    if let Some(theme) = self.settings.themes.iter().find(|t| t.name == name)
                        && let Some(file) = rfd::FileDialog::new()
                            .set_title("Save the theme")
                            .set_file_name(format!("{name}.json"))
                            .add_filter("Theme", &["json"])
                            .save_file()
                        && let Err(e) = std::fs::write(&file, theme.to_json())
                    {
                        self.error = Some(format!("Could not save the theme: {e}."));
                    }
                }
                Some(crate::ui::settings::Action::WhatsNew) => {
                    self.settings_open = false;
                    self.run_command(&ctx, Command::WhatsNew);
                }
                Some(crate::ui::settings::Action::Feedback) => {
                    self.settings_open = false;
                    self.run_command(&ctx, Command::Feedback);
                }
                Some(crate::ui::settings::Action::BrowseCloneDir) => {
                    if let Some(dir) = rfd::FileDialog::new()
                        .set_title("Where clones go")
                        .pick_folder()
                    {
                        self.settings.clone_dir = Some(dir.to_string_lossy().into_owned());
                    }
                }
                None => {}
            }
        }
        self.palette(&ctx);
        if let Some(tab) = &mut self.licenses
            && !crate::ui::licenses::show(&ctx, tab)
        {
            self.licenses = None;
        }
        if !self.whats_new.is_empty() && !crate::ui::whats_new::show(&ctx, &self.whats_new) {
            self.whats_new.clear();
        }
        self.feedback_screenshot(&ctx);
        if let Some(form) = &mut self.feedback {
            use crate::ui::feedback::{Action, Status};
            if let Some(result) = self
                .feedback_post
                .as_ref()
                .and_then(crate::task::Task::poll)
            {
                self.feedback_post = None;
                form.status = match result {
                    Ok(number) => Status::Sent(number),
                    Err(why) => Status::Failed(why),
                };
            }
            let on_github = |form: &crate::ui::feedback::Form| {
                crate::feedback::issue_url(form.kind, &form.title, &form.details)
            };
            // As them if they're signed in to GitHub; otherwise by the bot.
            let route = crate::feedback::route(&self.settings.accounts);
            match crate::ui::feedback::show(&ctx, form, &route) {
                Some(Action::Send) if route.posts() => {
                    form.status = Status::Sending;
                    let (kind, title, details, contact) = (
                        form.kind,
                        form.title.clone(),
                        form.details.clone(),
                        form.contact.clone(),
                    );
                    let shot = form.attached().filter(|_| route.attaches());
                    self.feedback_post = Some(crate::task::Task::spawn(&ctx, move || {
                        let png = shot.map(|s| crate::feedback::png(&s)).transpose()?;
                        crate::feedback::post(
                            &route,
                            (kind, &title, &details),
                            &contact,
                            png.as_deref(),
                        )
                    }));
                }
                Some(Action::Send | Action::OpenOnGitHub) => {
                    // The screenshot goes on the clipboard, for pasting there.
                    if let Some(shot) = form.attached() {
                        ctx.copy_image((*shot).clone());
                    }
                    ctx.open_url(egui::OpenUrl::new_tab(on_github(form)));
                    self.feedback = None;
                }
                Some(Action::Close) => {
                    self.feedback = None;
                    self.feedback_post = None;
                }
                None => {}
            }
        }
        self.sign_in_window(&ctx);
        self.new_request_window(&ctx);
        self.crash_dialog(&ctx);
        self.questions(&ctx);
    }
}

/// The look a theme choice means now (System follows the system's light or dark).
/// The look to show: the chosen family, dark or light as the mode (or
/// the system) says.
/// The window for a new pull/merge request from the checked-out branch.
/// Does what a branch, tag, stash, remote or worktree's menu asked for (in
/// the sidebar, or on a label in the graph), where only the repository is
/// involved; the sidebar's other actions are the app's.
fn repo_action(repo: &mut RepoView, ctx: &egui::Context, action: sidebar::Action) {
    match action {
        sidebar::Action::OpenWeb(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
        sidebar::Action::Jump(id) => repo.select(&id),
        sidebar::Action::StashApply(id) => repo.run(ctx, Op::StashApply(id)),
        sidebar::Action::StashPop(id) => repo.run(ctx, Op::StashPop(id)),
        sidebar::Action::StashDrop(id) => repo.confirm_drop(id),
        sidebar::Action::Checkout(name) => repo.checkout(ctx, name),
        sidebar::Action::CheckoutRemote(name) => repo.run(ctx, Op::CheckoutRemote(name)),
        sidebar::Action::NewBranch { start, label } => repo.new_branch(start, label),
        sidebar::Action::Rename(name) => repo.rename_branch(name),
        sidebar::Action::SetUpstream(name) => repo.set_upstream(name),
        sidebar::Action::DeleteBranch(name) => repo.confirm_delete_branch(name),
        sidebar::Action::DeleteBranches(names) => repo.confirm_delete_branches(names),
        sidebar::Action::DeleteRemoteBranches(names) => {
            repo.confirm_delete_remote_branches(names);
        }
        sidebar::Action::DeleteRemoteBranch(name) => repo.confirm_delete_remote_branch(name),
        sidebar::Action::DeleteTag(name) => repo.confirm_delete_tag(name),
        sidebar::Action::EditRemote(name) => repo.edit_remote(name),
        sidebar::Action::RemoveRemote(name) => repo.confirm_remove_remote(name),
        sidebar::Action::Merge { source, target } => repo.ask_merge(source, target),
        sidebar::Action::Rebase(onto) => repo.confirm_rebase(onto),
        sidebar::Action::OpenPath(p) => repo.open_request = Some(p),
        sidebar::Action::UpdateSubmodule(p) => repo.run(ctx, Op::UpdateSubmodule(p)),
        sidebar::Action::AddWorktree => repo.new_worktree(),
        sidebar::Action::RemoveWorktree(p) => repo.confirm_remove_worktree(p),
        // The app's.
        sidebar::Action::OpenRequest(_)
        | sidebar::Action::OpenIssue(_)
        | sidebar::Action::SignInToForge
        | sidebar::Action::NewRequest(_)
        | sidebar::Action::ToggleSection(_)
        | sidebar::Action::ShowAllSections
        | sidebar::Action::Terminal(_) => {}
    }
}

/// Makes the new repository in `form` in the background, on `account` (else
/// on this computer only): its templates fetched, made on the forge, cloned
/// (or started here), then started with its branch, templates and Git LFS;
/// a first commit on a forge is pushed there.
fn start_making(
    ctx: &egui::Context,
    form: &new_repo::Form,
    account: Option<crate::forge::Account>,
) -> Making {
    use crate::forge::{accounts, templates};
    let doing = Arc::new(Mutex::new(String::new()));
    let cancel = Arc::new(AtomicBool::new(false));
    let (d, c, repaint) = (doing.clone(), cancel.clone(), ctx.clone());
    let say = move |what: &str| {
        if let Ok(mut slot) = d.lock() {
            *slot = what.to_string();
        }
        repaint.request_repaint();
    };
    let (name, description, private) = (
        form.name.trim().to_string(),
        form.description.trim().to_string(),
        form.private,
    );
    let (here_too, dest) = (form.here_too(), form.dest());
    let branch = Some(form.branch.trim().to_string()).filter(|b| !b.is_empty());
    let gitignore = form.gitignore.as_ref().map(|t| t.key.clone());
    let license = form.license.as_ref().map(|t| t.key.clone());
    let lfs = form.lfs;
    let place = account.as_ref().map(|a| a.place()).unwrap_or_default();
    let task = Task::spawn(ctx, move || {
        let failed = |e: String| Some(e);
        let stopped = || c.load(Ordering::Relaxed);
        // The templates' text first, so nothing is made if they can't be had.
        let source = match &account {
            Some(a) => accounts::templates_source(a).map_err(failed)?,
            None => templates::Source::github(),
        };
        let mut files = Vec::new();
        if here_too && let Some(key) = &gitignore {
            say("Getting the .gitignore…");
            files.push((
                ".gitignore".to_string(),
                templates::gitignore(&source, key).map_err(failed)?,
            ));
        }
        if here_too && let Some(key) = &license {
            say("Getting the licence…");
            let holder = git_raccoon_core::create::user_name()
                .or_else(|| account.as_ref().map(|a| a.login.clone()))
                .unwrap_or_default();
            let year = chrono::Datelike::year(&chrono::Local::now());
            files.push((
                "LICENSE".to_string(),
                templates::license(&source, key, &holder, year).map_err(failed)?,
            ));
        }
        if stopped() {
            return Err(None);
        }
        let starter = create::Starter {
            branch: branch.clone(),
            files,
            lfs,
        };
        let Some(account) = account else {
            say("Starting it…");
            create::init(&dest, branch.as_deref()).map_err(|e| failed(e.to_string()))?;
            create::start(&dest, &starter).map_err(|e| failed(e.to_string()))?;
            return Ok(Made::Here(dest));
        };
        say(&format!("Making it on {place}…"));
        let made = accounts::create_repository(&account, &name, &description, private)
            .map_err(|e| failed(format!("Could not make it on {place}: {e}")))?;
        if !here_too {
            return Ok(Made::There(made.path));
        }
        say("Cloning it…");
        let options = create::CloneOptions::default();
        create::clone(&made.clone_url, &dest, &options, &mut |p| say(&p.phase), &c).map_err(
            |e| match e {
                git_raccoon_core::Error::Cancelled => None,
                e => failed(format!("Made {}, but could not clone it: {e}", made.path)),
            },
        )?;
        let first = create::start(&dest, &starter).map_err(|e| failed(e.to_string()))?;
        if let Some(b) = first {
            say("Pushing the first commit…");
            git_raccoon_core::sync::push_to(
                &dest.to_string_lossy(),
                "origin",
                &b,
                false,
                &mut |_| {},
                &c,
            )
            .map_err(|e| {
                failed(format!(
                    "Made and cloned it, but could not push the first commit: {e}"
                ))
            })?;
        }
        Ok(Made::Here(dest))
    });
    Making {
        doing,
        cancel,
        task,
    }
}

fn new_form(repo: &RepoView, branch: Option<&str>) -> Option<crate::ui::forge::NewForm> {
    let forge = repo.forge.as_ref()?;
    let source = match branch {
        Some(b) => b.to_string(),
        None => repo.head_branch()?.to_string(),
    };
    let local = repo.refs.local.iter().find(|b| b.name == source);
    let checked_out = local.is_some_and(|b| b.is_head);
    // Into the remote's default branch: "main" or "master", whichever it has.
    let remote = repo.refs.remotes.iter().find(|r| r.name == forge.remote);
    let has = |b: &str| remote.is_some_and(|r| r.branches.iter().any(|x| x.name == b));
    let target = ["main", "master", "develop"]
        .into_iter()
        .find(|b| has(b) && *b != source)
        .unwrap_or("main")
        .to_string();
    // The remote's branches, the default first, then by name.
    let mut targets: Vec<String> = remote
        .map(|r| {
            r.branches
                .iter()
                .map(|b| b.name.clone())
                .filter(|b| *b != source && b != "HEAD" && *b != target)
                .collect()
        })
        .unwrap_or_default();
    targets.sort();
    if has(&target) {
        targets.insert(0, target.clone());
    }
    let templates =
        crate::ui::forge::templates(std::path::Path::new(repo.path()), forge.project.kind);
    let mut form = crate::ui::forge::NewForm {
        kind: forge.project.kind,
        source,
        target,
        targets,
        title: String::new(),
        body: String::new(),
        draft: false,
        unpushed: true,
        checked_out,
        to_push: 0,
        commits: Vec::new(),
        behind: 0,
        conflicts: false,
        compared: String::new(),
        template: (!templates.is_empty()).then_some(0),
        templates,
        reviewers: Vec::new(),
        assignees: Vec::new(),
        labels: Vec::new(),
        choices: None,
        filter: String::new(),
        remove_source: false,
        squash: false,
        pushing: false,
        sending: false,
        error: None,
    };
    compare_form(repo, &mut form);
    form.fill();
    Some(form)
}

/// What the new request's branch has for its target, and whether it's
/// pushed (again whenever either changes).
fn compare_form(repo: &RepoView, form: &mut crate::ui::forge::NewForm) {
    let Some(forge) = repo.forge.as_ref() else {
        return;
    };
    let local = repo.refs.local.iter().find(|b| b.name == form.source);
    let upstream = local.and_then(|b| b.upstream.as_ref());
    form.unpushed = local.is_some() && upstream.is_none_or(|u| u.ahead > 0 || u.gone);
    if form.compared == form.target {
        if let Some(u) = upstream.filter(|u| !u.gone) {
            form.to_push = u.ahead;
        }
        return;
    }
    form.compared = form.target.clone();
    let target = format!("{}/{}", forge.remote, form.target);
    let source = if local.is_some() {
        form.source.clone()
    } else {
        format!("{}/{}", forge.remote, form.source)
    };
    match git_raccoon_core::branch::compare(repo.path(), &source, &target) {
        Ok(c) => {
            form.commits = c.commits;
            form.behind = c.behind;
            form.conflicts = c.conflicts;
        }
        Err(_) => {
            form.commits.clear();
            form.behind = 0;
            form.conflicts = false;
        }
    }
    form.to_push = match upstream.filter(|u| !u.gone) {
        Some(u) => u.ahead,
        None => form.commits.len(),
    };
}

/// How to check out a pull/merge request: its branch here (brought up to
/// the remote's), or, from a fork, its changes fetched into a branch of
/// its own ("pr/7", "mr/12").
fn check_out_op(forge: &crate::forge_view::ForgeView, number: u64) -> Option<Op> {
    let r = forge.detailed(number).or_else(|| forge.request(number))?;
    let remote = forge.remote.clone();
    if r.from_fork {
        let branch = match forge.project.kind {
            crate::forge::Kind::GitHub => format!("pr/{number}"),
            crate::forge::Kind::GitLab => format!("mr/{number}"),
        };
        let source = match forge.project.kind {
            crate::forge::Kind::GitHub => format!("pull/{number}/head"),
            crate::forge::Kind::GitLab => format!("merge-requests/{number}/head"),
        };
        Some(Op::Then(vec![
            Op::FetchInto {
                remote,
                source,
                branch: branch.clone(),
            },
            Op::Checkout(branch),
        ]))
    } else {
        Some(Op::Then(vec![
            Op::FetchRemote(
                remote.clone(),
                git_raccoon_core::sync::FetchOptions::default(),
            ),
            Op::CheckoutRemote(format!("{remote}/{}", r.source)),
        ]))
    }
}

/// While a request's commits are marked in the graph: how many are here,
/// and going back to it (`Some(Some(n))`) or clearing them (`Some(None)`).
fn highlight_bar(ui: &mut egui::Ui, repo: &RepoView) -> Option<Option<u64>> {
    let (n, ids) = repo.highlight.as_ref()?;
    let kind = repo
        .forge
        .as_ref()
        .map_or(crate::forge::Kind::GitHub, |f| f.project.kind);
    let here = ids
        .iter()
        .filter(|id| repo.history.locate(id).is_some())
        .count();
    let mut chosen = None;
    Frame::new()
        .fill(theme::accent().gamma_multiply(0.12))
        .inner_margin(Margin::symmetric(12, 4))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let s = if ids.len() == 1 { "" } else { "s" };
                let text = if here == ids.len() {
                    format!("{}'s {} commit{s}, marked.", kind.number(*n), ids.len())
                } else {
                    format!(
                        "{}'s commits, marked: {here} of {} are in the graph (fetch, or check it out, for the rest).",
                        kind.number(*n),
                        ids.len()
                    )
                };
                ui.label(RichText::new(text).color(theme::text()));
                if ui.small_button(format!("Back to {}", kind.number(*n))).clicked() {
                    chosen = Some(Some(*n));
                }
                if ui.small_button("Clear").clicked() {
                    chosen = Some(None);
                }
            });
        });
    chosen
}

/// Shows the open pull/merge request's view, if one is open.
fn request_view(
    ui: &mut egui::Ui,
    repo: &mut RepoView,
    drafts: &std::collections::BTreeMap<String, Vec<crate::forge::review::Draft>>,
    now: i64,
) -> Option<crate::ui::request::Action> {
    let branch = repo.head_branch().map(str::to_string);
    let f = repo.forge.as_mut()?;
    let Some(crate::forge_view::Opened::Request(n)) = f.open else {
        return None;
    };
    let mine = drafts.get(&f.drafts_key(n)).map_or(&[][..], Vec::as_slice);
    let here = crate::ui::request::Here {
        branch: branch.as_deref(),
        now,
        drafts: mine,
    };
    crate::ui::request::show(ui, f, &here)
}

/// Does what the request view asked. An error to show, if it went wrong
/// straight away.
fn request_action(
    repo: &mut RepoView,
    drafts: &mut std::collections::BTreeMap<String, Vec<crate::forge::review::Draft>>,
    ctx: &egui::Context,
    action: crate::ui::request::Action,
    pull: crate::settings::PullOptions,
) -> Result<(), String> {
    use crate::forge_view::Sent;
    use crate::ui::request::Action;
    let head = repo.head_branch().map(str::to_string);
    let root = repo.path().to_string();
    let Some(f) = repo.forge.as_mut() else {
        return Ok(());
    };
    let Some(crate::forge_view::Opened::Request(n)) = f.open else {
        return Ok(());
    };
    let key = f.drafts_key(n);
    let viewed = f.viewed().cloned();
    match action {
        Action::Close => f.open = None,
        Action::Open(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
        Action::Reload => f.reloading = true,
        Action::SaveDraft(draft, at) => {
            let list = drafts.entry(key).or_default();
            match at.and_then(|i| list.get_mut(i)) {
                Some(old) => *old = draft,
                None => list.push(draft),
            }
        }
        Action::DeleteDraft(i) => {
            if let Some(list) = drafts.get_mut(&key) {
                if i < list.len() {
                    list.remove(i);
                }
                if list.is_empty() {
                    drafts.remove(&key);
                }
            }
        }
        Action::Submit { summary, finish } => {
            let Some((_, o)) = viewed else { return Ok(()) };
            let mine = drafts.get(&key).cloned().unwrap_or_default();
            f.send(ctx, Sent::Review, move |s| {
                s.submit(n, &o.refs, &mine, &summary, finish)
            });
        }
        Action::Resolve(thread, to) => {
            f.send(ctx, Sent::Resolve, move |s| s.resolve(n, &thread, to));
        }
        Action::Merge(how) => {
            let Some((r, _)) = viewed else { return Ok(()) };
            f.send(ctx, Sent::Merge, move |s| s.merge(&r, &how));
        }
        Action::CheckOut => {
            if let Some(op) = check_out_op(f, n) {
                repo.run(ctx, op);
            }
        }
        Action::CheckOutWorktree => {
            let Some((r, _)) = viewed else { return Ok(()) };
            let kind = f.project.kind;
            let remote = f.remote.clone();
            let (branch, source) = if r.from_fork {
                (
                    crate::ui::request::fork_branch(kind, n),
                    f.session_head_ref(n),
                )
            } else {
                (r.source.clone(), r.source.clone())
            };
            let root = std::path::Path::new(repo.path().trim_end_matches(['/', '\\']));
            let name = root
                .file_name()
                .map_or_else(|| "repo".into(), |n| n.to_string_lossy().into_owned());
            let folder = root
                .parent()
                .unwrap_or(root)
                .join(format!("{name}-{}", branch.replace('/', "-")))
                .to_string_lossy()
                .into_owned();
            let mut steps = vec![Op::FetchInto {
                remote: remote.clone(),
                source,
                branch: branch.clone(),
            }];
            if !r.from_fork {
                steps.push(Op::SetUpstream {
                    branch: branch.clone(),
                    upstream: Some(format!("{remote}/{branch}")),
                });
            }
            steps.push(Op::AddWorktree {
                folder: folder.clone(),
                branch,
                new: false,
            });
            if let Some(f) = repo.forge.as_mut() {
                f.state.sent = Some(Ok(format!(
                    "Checking it out in {folder}: open it from Worktrees in the sidebar."
                )));
            }
            repo.run(ctx, Op::Then(steps));
        }
        Action::PullLatest => repo.pull(ctx, None, pull),
        Action::BackTo(target) => {
            let remote = f.remote.clone();
            let op = if repo.refs.local.iter().any(|b| b.name == target) {
                Op::Checkout(target)
            } else {
                Op::CheckoutRemote(format!("{remote}/{target}"))
            };
            repo.run(ctx, op);
        }
        Action::PullMerged => {
            let Some((r, _)) = viewed else { return Ok(()) };
            if head.as_deref() == Some(r.target.as_str()) {
                repo.pull(ctx, None, pull);
            } else {
                let remote = f.remote.clone();
                let checkout = if repo.refs.local.iter().any(|b| b.name == r.target) {
                    Op::Checkout(r.target.clone())
                } else {
                    Op::CheckoutRemote(format!("{remote}/{}", r.target))
                };
                repo.run(
                    ctx,
                    Op::Then(vec![
                        checkout,
                        Op::Pull {
                            from: None,
                            mode: pull.strategy.mode(),
                            stash: pull.changes == crate::settings::PullChanges::Stash,
                        },
                    ]),
                );
            }
        }
        Action::ApplySuggestions(ids, message) => {
            f.send(ctx, Sent::Apply, move |s| s.apply(&ids, &message));
        }
        Action::ApplyHere(g) => {
            let file = std::path::Path::new(&root).join(&g.path);
            let text = std::fs::read_to_string(&file)
                .map_err(|e| format!("Couldn't read {}: {e}.", g.path))?;
            let Some(changed) =
                crate::forge::review::replace_lines(&text, g.start, g.end, &g.from, &g.to)
            else {
                return Err(format!(
                    "{} has changed there since the suggestion (pull, or apply it by hand).",
                    g.path
                ));
            };
            std::fs::write(&file, changed)
                .map_err(|e| format!("Couldn't write {}: {e}.", g.path))?;
            f.state.sent = Some(Ok(format!(
                "Applied to {}: commit and push it when you're ready.",
                g.path
            )));
            repo.refresh_status(ctx);
        }
        Action::ShowInGraph(pick) => {
            let Some((_, o)) = viewed else { return Ok(()) };
            let ids: std::collections::HashSet<String> =
                o.commits.iter().map(|c| c.sha.clone()).collect();
            f.open = None;
            // The one asked for, or its newest here.
            let here: Vec<&String> = o
                .commits
                .iter()
                .map(|c| &c.sha)
                .filter(|id| repo.history.locate(id).is_some())
                .collect();
            let asked = pick.is_some();
            let select = pick
                .filter(|id| here.contains(&id))
                .or_else(|| here.last().map(|id| (*id).clone()));
            repo.highlight = Some((n, ids));
            if let Some(id) = select {
                repo.select(&id);
            } else if asked {
                return Err(
                    "That commit isn't in the graph yet: fetch, or check the request out.".into(),
                );
            }
        }
        Action::GoToCommit(id) => {
            if repo.history.locate(&id).is_some() {
                repo.select(&id);
            } else {
                return Err(format!(
                    "Commit {} isn't in this repository yet: fetch (or check the request out) first.",
                    crate::format::short_sha(&id)
                ));
            }
        }
    }
    Ok(())
}

/// Signing in, in the background: getting the code to show, then the
/// account (with a flag to stop waiting for approval).
enum SignInWork {
    Code(crate::task::Task<Result<crate::forge::oauth::DeviceCode, String>>),
    Account(
        crate::task::Task<Result<crate::forge::Account, String>>,
        Arc<AtomicBool>,
    ),
}

pub(crate) fn look_for(
    choice: ThemeChoice,
    mode: ThemeMode,
    system: Option<egui::Theme>,
) -> theme::Look {
    let light = match mode {
        ThemeMode::Dark => false,
        ThemeMode::Light => true,
        ThemeMode::System => system == Some(egui::Theme::Light),
    };
    choice.family().look(light)
}
