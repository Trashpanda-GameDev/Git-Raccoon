//! Watching a repository for changes made outside the app: branches, tags,
//! HEAD and config (the history), and the index and working tree (the
//! status), so the UI can refresh what changed.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use crate::Result;
use git2::Repository;
use notify::event::{EventKind, ModifyKind};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// Changes closer together than this are reported once.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// What changed in a burst of file events.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changes {
    /// Branches, tags, HEAD or config: the history and refs need reloading.
    pub history: bool,
    /// The index or working tree: the uncommitted changes need reloading.
    pub status: bool,
}

/// Whether a changed path inside a Git directory affects the history or
/// refs. Objects, logs and lock files churn without changing them.
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
/// (including the app's own reads) and metadata changes cannot.
fn is_write(kind: &EventKind) -> bool {
    match kind {
        EventKind::Create(_) | EventKind::Remove(_) => true,
        EventKind::Modify(ModifyKind::Metadata(_)) => false,
        EventKind::Modify(_) => true,
        _ => false,
    }
}

struct Classifier {
    git_dirs: Vec<PathBuf>,
    workdir: Option<PathBuf>,
    repo: Option<Repository>,
}

impl Classifier {
    fn add(&self, path: &Path, changes: &mut Changes) {
        for root in &self.git_dirs {
            if let Ok(rel) = path.strip_prefix(root) {
                if rel == Path::new("index") {
                    changes.status = true;
                } else if is_relevant(rel) {
                    changes.history = true;
                }
                return;
            }
        }
        let Some(workdir) = &self.workdir else { return };
        let Ok(rel) = path.strip_prefix(workdir) else {
            return;
        };
        if rel.as_os_str().is_empty() || rel.starts_with(".git") {
            return;
        }
        // Files the repository ignores (build output) don't change the status.
        let ignored = self
            .repo
            .as_ref()
            .is_some_and(|r| r.status_should_ignore(rel).unwrap_or(false));
        if !ignored {
            changes.status = true;
        }
    }
}

/// Watches a repository until dropped.
pub struct RepoWatcher {
    _watcher: RecommendedWatcher,
    working_tree: bool,
}

impl RepoWatcher {
    /// Calls `on_change` (from a background thread) after relevant changes.
    pub fn new(path: &str, on_change: impl Fn(Changes) + Send + 'static) -> Result<Self> {
        let repo = Repository::discover(path)?;
        // In a linked worktree, HEAD and the index live in the worktree's git
        // dir while refs, packed-refs and config live in the common dir.
        let mut git_dirs = vec![repo.path().to_path_buf(), repo.commondir().to_path_buf()];
        git_dirs.dedup();
        let workdir = repo.workdir().map(Path::to_path_buf);

        let (tx, rx) = mpsc::channel::<PathBuf>();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                let Ok(event) = result else { return };
                if is_write(&event.kind) {
                    for p in event.paths {
                        let _ = tx.send(p);
                    }
                }
            })?;

        // Coalesce bursts (a commit or a build touches many files) into one
        // call. The thread ends when the watcher, and so the sender, drops.
        let classifier = Classifier {
            git_dirs: git_dirs.clone(),
            workdir: workdir.clone(),
            repo: Repository::open(repo.path()).ok(),
        };
        std::thread::spawn(move || {
            while let Ok(first) = rx.recv() {
                let mut changes = Changes::default();
                classifier.add(&first, &mut changes);
                while let Ok(p) = rx.recv_timeout(DEBOUNCE) {
                    classifier.add(&p, &mut changes);
                }
                if changes != Changes::default() {
                    on_change(changes);
                }
            }
        });

        for dir in &git_dirs {
            watcher.watch(dir, RecursiveMode::NonRecursive)?;
            let refs = dir.join("refs");
            if refs.is_dir() {
                watcher.watch(&refs, RecursiveMode::Recursive)?;
            }
        }
        // Huge working trees can exceed the system's watch limit; then only
        // the Git directory is watched and callers poll the status instead.
        let working_tree = match &workdir {
            Some(dir) => match watcher.watch(dir, RecursiveMode::Recursive) {
                Ok(()) => true,
                Err(_) => {
                    let _ = watcher.unwatch(dir);
                    false
                }
            },
            None => false,
        };
        Ok(RepoWatcher {
            _watcher: watcher,
            working_tree,
        })
    }

    /// Whether working tree edits are reported (see [`RepoWatcher::new`]).
    pub fn watches_working_tree(&self) -> bool {
        self.working_tree
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

    fn quiet(rx: &Receiver<Changes>, what: &str) {
        if let Ok(c) = rx.recv_timeout(Duration::from_millis(800)) {
            panic!("{what} was reported: {c:?}");
        }
    }

    fn reported(rx: &Receiver<Changes>, what: &str) -> Changes {
        let changes = rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| panic!("{what} was not reported"));
        // One burst, one report.
        quiet(rx, &format!("{what} (a second time)"));
        changes
    }

    const HISTORY: Changes = Changes {
        history: true,
        status: false,
    };
    const STATUS: Changes = Changes {
        history: false,
        status: true,
    };

    #[test]
    fn reports_history_and_status_changes_but_not_reads() {
        let t = TempRepo::new();
        t.write(".gitignore", "target/\n");
        t.write("a", "1");
        let first = t.commit_all("first");
        let (tx, rx) = mpsc::channel();
        let watcher = RepoWatcher::new(t.path(), move |c| {
            let _ = tx.send(c);
        })
        .unwrap();
        assert!(watcher.watches_working_tree());

        // The app reads refs, history and status on every refresh; that must not loop.
        crate::refs::load_refs(t.path()).unwrap();
        crate::repo::History::load(t.path(), 1).unwrap();
        crate::working::working_status(t.path()).unwrap();
        quiet(&rx, "reading");

        t.write("target/build.o", "binary");
        quiet(&rx, "an ignored file");

        t.write("a", "2");
        assert_eq!(reported(&rx, "a working tree edit"), STATUS);

        let mut index = t.repo.index().unwrap();
        index.add_path(Path::new("a")).unwrap();
        index.write().unwrap();
        assert_eq!(reported(&rx, "staging"), STATUS);

        t.repo
            .branch("feature", &t.repo.find_commit(first).unwrap(), false)
            .unwrap();
        assert_eq!(reported(&rx, "a new branch"), HISTORY);

        let commit = reported_after(&rx, || {
            t.commit_all("second");
        });
        assert!(commit.history, "a commit moves the branch");
    }

    fn reported_after(rx: &Receiver<Changes>, act: impl FnOnce()) -> Changes {
        act();
        reported(rx, "the action")
    }
}
