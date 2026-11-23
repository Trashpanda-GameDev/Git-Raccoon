//! Updates, through Velopack: an installed app checks the project's GitHub
//! releases now and then, downloads a newer version in the background,
//! and offers to restart into it. Builds that weren't installed (`cargo
//! run`, say) don't check.

use std::time::{Duration, Instant};

use eframe::egui;
use velopack::sources::GithubSource;
use velopack::{UpdateCheck, UpdateInfo, UpdateManager};

use crate::task::Task;

/// Where releases are published: this project's GitHub releases.
pub const RELEASES: &str = "https://github.com/Trashpanda-GameDev/Git-Raccoon";

/// How often to look again while the app stays open.
const EVERY: Duration = Duration::from_secs(30 * 60);

/// How long until the next check is due (zero if it's due now).
fn wait(last_check: Option<Instant>, now: Instant) -> Duration {
    last_check.map_or(Duration::ZERO, |t| EVERY.saturating_sub(now - t))
}

pub struct Updater {
    /// `None` when this copy wasn't installed (so it can't update itself).
    manager: Option<UpdateManager>,
    task: Option<Task<Result<Option<UpdateInfo>, String>>>,
    /// Downloaded and ready to apply on restart.
    ready: Option<UpdateInfo>,
    last_check: Option<Instant>,
    /// How the last check went, for "Check for updates" to show.
    outcome: Option<Outcome>,
    /// A version to say is downloaded (tests, which can't download one).
    pretend_ready: Option<String>,
}

/// How the last check went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    UpToDate,
    /// A newer version, downloaded and waiting for a restart.
    Ready(String),
    Failed(String),
}

impl Updater {
    pub fn new() -> Self {
        let source = GithubSource::new(RELEASES, None, false);
        Updater {
            manager: UpdateManager::new(source, None, None).ok(),
            task: None,
            ready: None,
            last_check: None,
            outcome: None,
            pretend_ready: None,
        }
    }

    /// Checks now, whatever the schedule (and even with automatic checks
    /// off): "Check for updates".
    pub fn check_now(&mut self, ctx: &egui::Context) {
        if self.task.is_none() {
            self.last_check = None;
            self.outcome = None;
            self.start(ctx);
        }
    }

    /// A check (or its download) is running.
    pub fn is_checking(&self) -> bool {
        self.task.is_some()
    }

    /// What to say about updates: what a check found, or that one is
    /// running, or why this copy can't.
    pub fn status(&self) -> String {
        if self.pretend_ready.is_none() && !self.is_installed() {
            return "This copy wasn't installed, so it can't update itself.".into();
        }
        if self.is_checking() {
            return "Checking…".into();
        }
        match &self.outcome {
            _ if self.ready_version().is_some() => format!(
                "Version {} is downloaded: restart to use it. Your tabs and settings are kept.",
                self.ready_version().unwrap_or_default()
            ),
            Some(Outcome::UpToDate) => {
                format!("You're up to date ({}).", env!("CARGO_PKG_VERSION"))
            }
            Some(Outcome::Failed(why)) => format!("Couldn't check: {why}."),
            Some(Outcome::Ready(_)) | None => {
                format!("This is version {}.", env!("CARGO_PKG_VERSION"))
            }
        }
    }

    /// Whether this copy can update itself (it was installed).
    pub fn is_installed(&self) -> bool {
        self.manager.is_some()
    }

    /// Checks (and downloads) in the background if it's time to, and
    /// wakes the app up for the next check: an idle window doesn't redraw,
    /// so without that a check would wait for the mouse to move over it.
    pub fn tick(&mut self, ctx: &egui::Context, enabled: bool) {
        if let Some(result) = self.task.as_ref().and_then(Task::poll) {
            self.task = None;
            // A failed check (offline, say) just waits for the next one.
            self.outcome = Some(match result {
                Ok(Some(info)) => {
                    let version = info.TargetFullRelease.Version.clone();
                    self.ready = Some(info);
                    Outcome::Ready(version)
                }
                Ok(None) => Outcome::UpToDate,
                Err(why) => Outcome::Failed(why),
            });
        }
        let Some(manager) = self.manager.clone().filter(|_| enabled) else {
            return;
        };
        if self.task.is_some() {
            return;
        }
        let wait = wait(self.last_check, Instant::now());
        if !wait.is_zero() {
            ctx.request_repaint_after(wait);
            return;
        }
        let _ = manager;
        self.start(ctx);
    }

    /// Starts a check (and download), in the background.
    fn start(&mut self, ctx: &egui::Context) {
        let Some(manager) = self.manager.clone() else {
            return;
        };
        self.last_check = Some(Instant::now());
        // A newer release than the one already downloaded replaces it.
        let have = self
            .ready
            .as_ref()
            .map(|i| i.TargetFullRelease.Version.clone());
        self.task = Some(Task::spawn(ctx, move || {
            match manager.check_for_updates().map_err(|e| e.to_string())? {
                UpdateCheck::UpdateAvailable(info)
                    if have.as_deref() != Some(info.TargetFullRelease.Version.as_str()) =>
                {
                    manager
                        .download_updates(&info, None)
                        .map_err(|e| e.to_string())?;
                    Ok(Some(*info))
                }
                // Already downloaded; or nothing new.
                UpdateCheck::UpdateAvailable(_)
                | UpdateCheck::NoUpdateAvailable
                | UpdateCheck::RemoteIsEmpty => Ok(None),
            }
        }));
    }

    /// The version downloaded and waiting for a restart.
    pub fn ready_version(&self) -> Option<&str> {
        self.ready
            .as_ref()
            .map(|i| i.TargetFullRelease.Version.as_str())
            .or(self.pretend_ready.as_deref())
    }

    /// Says `version` is downloaded, though it isn't (for tests).
    #[doc(hidden)]
    pub fn pretend_ready(&mut self, version: &str) {
        self.pretend_ready = Some(version.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checks_again_every_half_hour() {
        let start = Instant::now();
        assert_eq!(wait(None, start), Duration::ZERO, "first thing");
        assert_eq!(wait(Some(start), start), EVERY);
        let later = start + Duration::from_secs(20 * 60);
        assert_eq!(wait(Some(start), later), Duration::from_secs(10 * 60));
        let overdue = start + Duration::from_secs(3 * 60 * 60);
        assert_eq!(wait(Some(start), overdue), Duration::ZERO);
        assert!(EVERY <= Duration::from_secs(60 * 60), "not hours");
    }
}
