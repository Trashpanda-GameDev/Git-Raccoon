//! Watching a repository's Git directory for changes to branches, tags,
//! HEAD and config, so the UI can refresh when Git is used elsewhere.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use git2::Repository;
use notify::event::{EventKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// Changes closer together than this are reported once.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// Whether a changed path inside a Git directory affects what the UI shows.
/// Objects, the index, logs and lock files churn without changing history
/// or refs on their own.
pub fn is_relevant(relative: &Path) -> bool {
    if relative.extension().is_some_and(|e| e == "lock") {
        return false;
    }
    let mut parts = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy());
    match parts.next().as_deref() {
        Some("HEAD" | "packed-refs" | "config") => parts.next().is_none(),
        Some("refs") => true,
        _ => false,
    }
}

/// Whether an event kind can change a file's contents or existence. Reads
/// (including the app's own reads of refs) and metadata changes cannot.
fn is_write(kind: &EventKind) -> bool {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) => true,
        EventKind::Modify(ModifyKind::Metadata(_)) => false,
        EventKind::Modify(_) => true,
        _ => false,
    }
}

/// Watches a repository until dropped.
pub struct RepoWatcher {
    _watcher: RecommendedWatcher,
}

impl RepoWatcher {
    /// Calls `on_change` (from a background thread) after relevant changes.
    pub fn new(path: &str, on_change: impl Fn() + Send + 'static) -> Result<Self, String> {
        let repo = Repository::discover(path).map_err(|e| e.message().to_string())?;
        // In a linked worktree, HEAD lives in the worktree's git dir while
        // refs, packed-refs and config live in the common dir.
        let mut git_dirs = vec![repo.path().to_path_buf(), repo.commondir().to_path_buf()];
        git_dirs.dedup();

        let (tx, rx) = mpsc::channel::<()>();
        let roots: Vec<PathBuf> = git_dirs.clone();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                let Ok(event) = result else { return };
                let relevant = is_write(&event.kind)
                    && event.paths.iter().any(|p| {
                        roots
                            .iter()
                            .filter_map(|root| p.strip_prefix(root).ok())
                            .any(is_relevant)
                    });
                if relevant {
                    let _ = tx.send(());
                }
            })
            .map_err(|e| e.to_string())?;

        // Coalesce bursts (a commit or fetch touches several files) into one
        // call. The thread ends when the watcher, and so the sender, drops.
        std::thread::spawn(move || {
            while rx.recv().is_ok() {
                while rx.recv_timeout(DEBOUNCE).is_ok() {}
                on_change();
            }
        });

        for dir in &git_dirs {
            watcher
                .watch(dir, RecursiveMode::NonRecursive)
                .map_err(|e| e.to_string())?;
            let refs = dir.join("refs");
            if refs.is_dir() {
                watcher
                    .watch(&refs, RecursiveMode::Recursive)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(RepoWatcher { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;
    use std::sync::mpsc::Receiver;

    #[test]
    fn filters_paths_that_do_not_affect_history_or_refs() {
        for path in [
            "HEAD",
            "packed-refs",
            "config",
            "refs/heads/main",
            "refs/remotes/origin/x/y",
            "refs/stash",
        ] {
            assert!(is_relevant(Path::new(path)), "{path}");
        }
        for path in [
            "index",
            "objects/ab/cdef",
            "logs/HEAD",
            "HEAD.lock",
            "refs/heads/main.lock",
            "FETCH_HEAD",
            "hooks/pre-commit",
        ] {
            assert!(!is_relevant(Path::new(path)), "{path}");
        }
    }

    fn quiet(rx: &Receiver<()>, what: &str) {
        assert!(
            rx.recv_timeout(Duration::from_millis(800)).is_err(),
            "{what} was reported"
        );
    }

    fn reported(rx: &Receiver<()>, what: &str) {
        rx.recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("{what} was not reported"));
        // One burst, one report.
        quiet(rx, &format!("{what} (a second time)"));
    }

    #[test]
    fn reports_ref_changes_but_not_reads_or_working_tree_edits() {
        let t = TempRepo::new();
        t.write("a", "1");
        let first = t.commit_all("first");
        let (tx, rx) = mpsc::channel();
        let _watcher = RepoWatcher::new(t.path(), move || {
            let _ = tx.send(());
        })
        .unwrap();

        // The app reads refs on every refresh; that must not loop.
        crate::refs::load_refs(t.path()).unwrap();
        crate::repo::History::load(t.path(), 1).unwrap();
        quiet(&rx, "reading refs");

        t.write("a", "2");
        quiet(&rx, "a working tree edit");

        t.repo
            .branch("feature", &t.repo.find_commit(first).unwrap(), false)
            .unwrap();
        reported(&rx, "a new branch");

        t.commit_all("second");
        reported(&rx, "a commit");

        t.repo.set_head("refs/heads/feature").unwrap();
        reported(&rx, "a checkout");
    }
}
