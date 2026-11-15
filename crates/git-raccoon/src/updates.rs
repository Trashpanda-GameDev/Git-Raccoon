//! Updates, through Velopack: an installed app checks the public releases
//! repository now and then, downloads a newer version in the background,
//! and offers to restart into it. Builds that weren't installed (`cargo
//! run`, say) don't check.

use std::time::{Duration, Instant};

use eframe::egui;
use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager};

use crate::task::Task;

/// Where releases are published (a public repository holding only them).
pub const RELEASES: &str = "https://github.com/Trashpanda-GameDev/Git-Raccoon-Releases";

/// How often to look again while the app stays open.
const EVERY: Duration = Duration::from_secs(6 * 60 * 60);

pub struct Updater {
    /// `None` when this copy wasn't installed (so it can't update itself).
    manager: Option<UpdateManager>,
    task: Option<Task<Result<Option<UpdateInfo>, String>>>,
    /// Downloaded and ready to apply on restart.
    ready: Option<UpdateInfo>,
    last_check: Option<Instant>,
}

impl Updater {
    pub fn new() -> Self {
        let source = GithubSource::new(RELEASES, None, false);
        Updater {
            manager: UpdateManager::new(source, None, None).ok(),
            task: None,
            ready: None,
            last_check: None,
        }
    }

    /// Whether this copy can update itself (it was installed).
    pub fn is_installed(&self) -> bool {
        self.manager.is_some()
    }

    /// Checks (and downloads) in the background if it's time to.
    pub fn tick(&mut self, ctx: &egui::Context, enabled: bool) {
        if let Some(result) = self.task.as_ref().and_then(Task::poll) {
            self.task = None;
            // A failed check (offline, say) just waits for the next one.
            if let Ok(Some(info)) = result {
                self.ready = Some(info);
            }
        }
        let due = self.last_check.is_none_or(|t| t.elapsed() >= EVERY);
        if !enabled || !due || self.task.is_some() || self.ready.is_some() {
            return;
        }
        let Some(manager) = self.manager.clone() else {
            return;
        };
        self.last_check = Some(Instant::now());
        self.task = Some(Task::spawn(ctx, move || {
            match manager.check_for_updates().map_err(|e| e.to_string())? {
                UpdateCheck::UpdateAvailable(info) => {
                    manager
                        .download_updates(&info, None)
                        .map_err(|e| e.to_string())?;
                    Ok(Some(*info))
                }
                UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty => Ok(None),
            }
        }));
    }

    /// The version downloaded and waiting for a restart.
    pub fn ready_version(&self) -> Option<&str> {
        self.ready
            .as_ref()
            .map(|i| i.TargetFullRelease.Version.as_str())
    }

    /// Restarts into the downloaded version. (Exits the app: save first.)
    pub fn restart(&self) -> Result<(), String> {
        match (&self.manager, &self.ready) {
            (Some(m), Some(info)) => m
                .apply_updates_and_restart(&info.TargetFullRelease)
                .map_err(|e| e.to_string()),
            _ => Err("there's no update to apply".into()),
        }
    }
}

impl Default for Updater {
    fn default() -> Self {
        Self::new()
    }
}
