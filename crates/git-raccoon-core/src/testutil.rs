//! Throwaway Git repositories for tests.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use git2::{IndexAddOption, Oid, Repository, Signature};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

pub struct TempRepo {
    pub dir: PathBuf,
    pub repo: Repository,
}

impl TempRepo {
    pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("git-raccoon-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        // Files exactly as written, whatever the machine's settings (Git for
        // Windows turns on line-ending conversion by default). Tests of that
        // conversion turn it on themselves.
        repo.config()
            .unwrap()
            .set_bool("core.autocrlf", false)
            .unwrap();
        TempRepo { dir, repo }
    }

    pub fn path(&self) -> &str {
        self.dir.to_str().unwrap()
    }

    pub fn write(&self, file: &str, contents: impl AsRef<[u8]>) {
        let path = self.dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    pub fn remove(&self, file: &str) {
        std::fs::remove_file(self.dir.join(file)).unwrap();
    }

    /// Stages everything in the working tree and commits it on HEAD.
    pub fn commit_all(&self, message: &str) -> Oid {
        let mut index = self.repo.index().unwrap();
        index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
        index.update_all(["*"], None).unwrap();
        index.write().unwrap();
        let parents: Vec<Oid> = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.target())
            .into_iter()
            .collect();
        self.commit(Some("HEAD"), message, &parents)
    }

    /// Commits the current index with explicit parents, updating `update_ref`.
    pub fn commit(&self, update_ref: Option<&str>, message: &str, parents: &[Oid]) -> Oid {
        let sig = Signature::now("Raccoon", "raccoon@example.com").unwrap();
        let tree = self
            .repo
            .find_tree(self.repo.index().unwrap().write_tree().unwrap())
            .unwrap();
        let parents: Vec<_> = parents
            .iter()
            .map(|p| self.repo.find_commit(*p).unwrap())
            .collect();
        let parents: Vec<_> = parents.iter().collect();
        self.repo
            .commit(update_ref, &sig, &sig, message, &tree, &parents)
            .unwrap()
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
