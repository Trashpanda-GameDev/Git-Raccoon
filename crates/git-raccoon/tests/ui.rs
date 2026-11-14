//! Drives the real app headlessly (software rendering) against a sample
//! repository: opening, the graph, sidebar jumps, details, diffs, image
//! previews, search and auto-refresh.
//!
//! Set `GIT_RACCOON_SCREENSHOTS=<dir>` to save a PNG of each step.

#![expect(
    clippy::unwrap_used,
    reason = "test setup: a failure should simply fail the test"
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use git_raccoon::App;
use git_raccoon::settings::Settings;
use git2::{Oid, Repository, Signature, Time, build::CheckoutBuilder};

const DAY: i64 = 86_400;

/// A sample repository: branches, merges, a tag, a rename, images, a remote
/// branch and a stash.
struct Sample {
    dir: PathBuf,
    repo: Repository,
    now: i64,
}

impl Sample {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "git-raccoon-ui-{}-{}",
            std::process::id(),
            rand_suffix()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        repo.set_head("refs/heads/main").unwrap();
        let now = git_raccoon::settings::now();
        Sample { dir, repo, now }
    }

    fn path(&self) -> &str {
        self.dir.to_str().unwrap()
    }

    fn write(&self, file: &str, contents: impl AsRef<[u8]>) {
        let p = self.dir.join(file);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contents).unwrap();
    }

    fn sig(&self, days_ago: i64) -> Signature<'static> {
        Signature::new(
            "Rocky Raccoon",
            "rocky@example.com",
            &Time::new(self.now - days_ago * DAY, 0),
        )
        .unwrap()
    }

    /// Stages everything and commits on HEAD with extra parents.
    fn commit_with(&self, message: &str, days_ago: i64, extra: &[Oid]) -> Oid {
        let mut index = self.repo.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.update_all(["*"], None).unwrap();
        index.write().unwrap();
        let tree = self.repo.find_tree(index.write_tree().unwrap()).unwrap();
        let mut parents: Vec<git2::Commit<'_>> = self
            .repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .into_iter()
            .collect();
        parents.extend(extra.iter().map(|id| self.repo.find_commit(*id).unwrap()));
        let refs: Vec<&git2::Commit<'_>> = parents.iter().collect();
        let sig = self.sig(days_ago);
        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &refs)
            .unwrap()
    }

    fn commit(&self, message: &str, days_ago: i64) -> Oid {
        self.commit_with(message, days_ago, &[])
    }

    fn checkout(&self, branch: &str) {
        let name = format!("refs/heads/{branch}");
        if self.repo.find_reference(&name).is_err() {
            let head = self.repo.head().unwrap().peel_to_commit().unwrap();
            self.repo.branch(branch, &head, false).unwrap();
        }
        self.repo.set_head(&name).unwrap();
        self.repo
            .checkout_head(Some(CheckoutBuilder::new().force()))
            .unwrap();
    }

    /// Merges `branch` into HEAD (no conflicts expected).
    fn merge(&self, branch: &str, days_ago: i64) -> Oid {
        let theirs = self
            .repo
            .find_branch(branch, git2::BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap();
        let ours = self.repo.head().unwrap().peel_to_commit().unwrap();
        let mut index = self.repo.merge_commits(&ours, &theirs, None).unwrap();
        let tree = self
            .repo
            .find_tree(index.write_tree_to(&self.repo).unwrap())
            .unwrap();
        let sig = self.sig(days_ago);
        let id = self
            .repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("Merge branch '{branch}'"),
                &tree,
                &[&ours, &theirs],
            )
            .unwrap();
        self.repo
            .checkout_head(Some(CheckoutBuilder::new().force()))
            .unwrap();
        id
    }

    fn build() -> Self {
        let s = Sample::new();
        s.write("src/loader.rs", "use std::path::Path;\n\npub fn open(path: &Path) -> Result<Repo, Error> {\n    let repo = Repo::discover(path)?;\n    Ok(repo)\n}\n");
        s.write("README.md", "# Sample\n\nA sample project.\n");
        s.commit("Add repository loader", 20);
        s.checkout("feature/graph");
        s.write(
            "src/graph.rs",
            "pub struct Lane {\n    pub column: usize,\n}\n",
        );
        s.commit("Lay out commit lanes", 18);
        s.checkout("main");
        s.write(
            "src/loader.rs",
            "use std::path::Path;\n\n/// Opens the repository containing `path`.\npub fn open(path: &Path) -> Result<Repo, LoadError> {\n    let repo = Repo::discover(path).map_err(LoadError::NotARepo)?;\n    if repo.is_bare() {\n        return Err(LoadError::Bare);\n    }\n    Ok(repo)\n}\n",
        );
        s.commit(
            "Reject bare repositories when loading\n\nBare repositories have no working tree, so the staging\nviews cannot work with them. Fail early instead.",
            16,
        );
        s.checkout("feature/graph");
        s.write(
            "src/graph.rs",
            "pub struct Lane {\n    pub column: usize,\n    pub colour: u8,\n}\n",
        );
        s.commit("Colour lanes", 14);
        s.checkout("main");
        let merge = s.merge("feature/graph", 12);
        s.repo
            .tag_lightweight("v0.1.0", &s.repo.find_object(merge, None).unwrap(), false)
            .unwrap();
        s.write("assets/icon.png", png(64, 64, 0));
        s.commit("Add icon", 6);
        s.write("assets/icon.png", png(96, 72, 90));
        std::fs::rename(
            s.dir.join("src/loader.rs"),
            s.dir.join("src/repo_loader.rs"),
        )
        .unwrap();
        s.commit("Redraw icon and rename loader", 4);
        s.checkout("fix/ui/toolbar-glitch");
        s.write("src/toolbar.rs", "pub fn draw() {}\n");
        s.commit("Fix toolbar glitch", 2);
        s.checkout("main");
        let head = s.repo.head().unwrap().target().unwrap();
        s.repo
            .reference("refs/remotes/origin/main", head, true, "test")
            .unwrap();
        let mut config = s.repo.config().unwrap();
        config.set_str("branch.main.remote", "origin").unwrap();
        config
            .set_str("branch.main.merge", "refs/heads/main")
            .unwrap();
        s.repo
            .remote("origin", "https://example.com/sample.git")
            .unwrap();
        s.write(
            "README.md",
            "# Sample\n\nA sample project.\nWork in progress.\n",
        );
        let mut repo = Repository::open(&s.dir).unwrap();
        repo.stash_save(&s.sig(0), "half-done README tweak", None)
            .unwrap();
        s
    }
}

impl Drop for Sample {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn rand_suffix() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos()
}

/// A small PNG: a disc on a transparent background.
fn png(w: u32, h: u32, shift: u8) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        let (cx, cy) = (x as f32 - w as f32 / 2.0, y as f32 - h as f32 / 2.0);
        let inside = (cx * cx + cy * cy).sqrt() < w.min(h) as f32 * 0.45;
        image::Rgba([
            (x * 4) as u8 ^ shift,
            (y * 3) as u8,
            160u8.wrapping_add(shift),
            if inside { 255 } else { 0 },
        ])
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

fn harness(settings: Settings) -> Harness<'static, App> {
    // Screenshots meant for people are rendered at 2x.
    let scale = if std::env::var_os("GIT_RACCOON_SCREENSHOTS").is_some() {
        2.0
    } else {
        1.0
    };
    Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_pixels_per_point(scale)
        .wgpu()
        .build_eframe(move |cc| App::with_settings(&cc.egui_ctx, settings))
}

/// Steps frames until background work is done.
fn settle(h: &mut Harness<'_, App>) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        h.step();
        if !h.state().is_busy() {
            for _ in 0..3 {
                h.step();
            }
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("app still busy after 20 s");
}

fn screenshot(h: &mut Harness<'_, App>, name: &str) {
    let Ok(dir) = std::env::var("GIT_RACCOON_SCREENSHOTS") else {
        return;
    };
    let image = h.render().expect("render");
    image
        .save(Path::new(&dir).join(format!("{name}.png")))
        .unwrap();
}

fn open(h: &mut Harness<'_, App>, path: &str) {
    let ctx = h.ctx.clone();
    h.state_mut().open(&ctx, path);
    settle(h);
    assert!(
        h.state().repo().is_some(),
        "repo did not open: {:?}",
        h.state().error()
    );
}

fn selected_summary(h: &Harness<'_, App>) -> Option<String> {
    let repo = h.state().repo()?;
    let row = repo.selected.as_ref()?.row?;
    repo.rows.get(row).map(|c| c.summary.clone())
}

#[test]
fn welcome_lists_recent_repos_and_reports_bad_paths() {
    let sample = Sample::build();
    let mut settings = Settings::default();
    settings.remember("/nowhere/old-site");
    settings.remember(sample.path());
    let mut h = harness(settings);
    h.run_ok();
    h.get_by_label("Open a repository…");
    h.get_by_label("old-site");
    screenshot(&mut h, "1-welcome");

    h.get_by_label("old-site").click();
    settle(&mut h);
    assert!(h.state().repo().is_none());
    assert!(
        h.state().error().unwrap().contains("/nowhere/old-site"),
        "{:?}",
        h.state().error()
    );

    h.get_by_label("Remove old-site from recent").click();
    h.run_ok();
    assert_eq!(h.state().settings().recent.len(), 1);
}

#[test]
fn browses_history_details_diffs_and_images() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());

    let repo = h.state().repo().unwrap();
    assert_eq!(repo.history.total(), 8);
    assert_eq!(repo.history.branch(), Some("main"));
    assert_eq!(
        h.state().settings().recent.len(),
        1,
        "opened repo is remembered"
    );
    screenshot(&mut h, "2-history");

    // Sidebar: a nested branch folder and a jump to the tag.
    h.get_by_label("toolbar-glitch");
    h.get_by_label("v0.1.0").click();
    settle(&mut h);
    assert_eq!(
        selected_summary(&h).as_deref(),
        Some("Merge branch 'feature/graph'")
    );

    // Keyboard: down to the next row, then details for it.
    h.key_press(Key::ArrowDown);
    settle(&mut h);
    assert_eq!(selected_summary(&h).as_deref(), Some("Colour lanes"));

    // Clicking a row opens its details; clicking a file opens its diff.
    h.get_by_label("Reject bare repositories when loading")
        .click();
    settle(&mut h);
    h.get_by_label("Rocky Raccoon");
    screenshot(&mut h, "3-details");
    h.get_by_label("src/loader.rs").click();
    settle(&mut h);
    h.get_by_label("Unified");
    screenshot(&mut h, "4-diff-unified");
    h.get_by_label("Split").click();
    settle(&mut h);
    assert!(h.state().settings().split_diff);
    screenshot(&mut h, "5-diff-split");
    h.key_press(Key::Escape);
    h.run_ok();
    assert!(
        h.state().repo().unwrap().active_file.is_none(),
        "Esc closes the diff"
    );

    // Image previews and a rename.
    h.get_by_label("Redraw icon and rename loader").click();
    settle(&mut h);
    h.get_by_label("src/repo_loader.rs");
    h.get_by_label("assets/icon.png").click();
    settle(&mut h);
    screenshot(&mut h, "6-image");
    h.key_press(Key::Escape);
    h.run_ok();

    // A stash is shown in the details pane without a graph row.
    h.get_by_label_contains("half-done README tweak").click();
    settle(&mut h);
    let sel = h.state().repo().unwrap().selected.as_ref().unwrap();
    assert_eq!(sel.row, None);
    h.get_by_label("README.md");
}

#[test]
fn searches_the_whole_history() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());

    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::F);
    h.run_ok();
    h.get_by_label("Search commits").type_text("lanes");
    settle(&mut h);
    std::thread::sleep(Duration::from_millis(300));
    settle(&mut h);
    let rows = h.state().repo().unwrap().search_rows().to_vec();
    assert_eq!(rows.len(), 2, "two commits mention lanes");
    h.key_press(Key::Enter);
    settle(&mut h);
    assert_eq!(selected_summary(&h).as_deref(), Some("Colour lanes"));
    h.key_press(Key::Enter);
    settle(&mut h);
    assert_eq!(
        selected_summary(&h).as_deref(),
        Some("Lay out commit lanes")
    );
    screenshot(&mut h, "7-search");

    // File-path search.
    h.get_by_label("File").click();
    h.run_ok();
    h.get_by_label("Search commits").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Search commits").type_text("graph.rs");
    settle(&mut h);
    std::thread::sleep(Duration::from_millis(300));
    settle(&mut h);
    assert_eq!(
        h.state().repo().unwrap().search_rows().len(),
        3,
        "graph.rs: two commits and the merge"
    );
}

#[test]
fn refreshes_when_the_repo_changes_on_disk() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    h.get_by_label("Add icon").click();
    settle(&mut h);

    sample.write("NEW.md", "new\n");
    sample.commit("Made outside the app", 0);

    let start = Instant::now();
    while h.state().repo().unwrap().history.total() == 8 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "no refresh after an external commit"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    settle(&mut h);
    assert_eq!(h.state().repo().unwrap().history.total(), 9);
    assert_eq!(
        selected_summary(&h).as_deref(),
        Some("Add icon"),
        "selection survives the refresh"
    );
    h.get_by_label("Made outside the app");
}

/// Opens a large repository and checks that jumping deep into history and
/// scrolling stay fast. Run with `GIT_RACCOON_BIG_REPO=<path> cargo test
/// --release -p git-raccoon --test ui big_repo -- --ignored --nocapture`.
#[test]
#[ignore = "needs a large repository; set GIT_RACCOON_BIG_REPO"]
fn big_repo_stays_responsive() {
    let Ok(path) = std::env::var("GIT_RACCOON_BIG_REPO") else {
        return;
    };
    let mut h = harness(Settings::default());
    let start = Instant::now();
    open(&mut h, &path);
    let total = h.state().repo().unwrap().history.total();
    eprintln!("opened {total} commits in {:?}", start.elapsed());

    let frame = |h: &mut Harness<'_, App>| {
        let t = Instant::now();
        h.step();
        t.elapsed()
    };
    let mut worst = Duration::ZERO;
    for target in [total / 2, total - 1, 10, total / 3] {
        h.state_mut().repo_mut().unwrap().select_index(target);
        let first = frame(&mut h);
        worst = worst.max(first);
        settle(&mut h);
        eprintln!(
            "jump to row {target}: first frame {first:?}, selected {:?}",
            selected_summary(&h)
        );
        assert!(selected_summary(&h).is_some());
    }
    for _ in 0..30 {
        h.key_press(Key::PageDown);
        worst = worst.max(frame(&mut h));
    }
    eprintln!("worst frame {worst:?}");
    assert!(worst < Duration::from_millis(250), "a frame took {worst:?}");
}

/// Waits (stepping frames) until `done` holds.
fn wait_for(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let start = Instant::now();
    while !done(h.state()) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out waiting for {what}"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    settle(h);
}

fn diff_text(h: &Harness<'_, App>) -> String {
    let repo = h.state().repo().unwrap();
    let key = repo.diff_key().expect("a diff is open");
    match repo.diff.get(&key) {
        Some(Ok(d)) => d
            .diff
            .hunks
            .iter()
            .flat_map(|hk| &hk.lines)
            .map(|l| l.content.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        other => panic!("diff not loaded: {:?}", other.map(|r| r.as_ref().err())),
    }
}

#[test]
fn shows_working_changes_and_follows_edits() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    assert!(
        h.query_by_label("Working changes").is_none(),
        "a clean repo has no WIP row"
    );

    sample.write("src/repo_loader.rs", "// edited outside the app\n");
    sample.write("TODO.md", "- stage things\n");
    sample.write("README.md", "# Sample\n\nStaged change.\n");
    let mut index = sample.repo.index().unwrap();
    index.add_path(Path::new("README.md")).unwrap();
    index.write().unwrap();

    wait_for(&mut h, "the WIP row", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| !s.is_clean())
    });
    let status = h.state().repo().unwrap().status.clone().unwrap();
    let names = |files: &[git_raccoon_core::working::WorkingFile]| {
        files.iter().map(|f| f.path.clone()).collect::<Vec<_>>()
    };
    assert_eq!(names(&status.staged), ["README.md"]);
    assert_eq!(names(&status.unstaged), ["TODO.md", "src/repo_loader.rs"]);

    h.get_by_label("Working changes").click();
    settle(&mut h);
    h.get_by_label("TODO.md");
    h.get_by_label("README.md");
    screenshot(&mut h, "8-working-changes");

    h.get_by_label("src/repo_loader.rs").click();
    settle(&mut h);
    h.get_by_label("Unstaged");
    assert!(diff_text(&h).contains("// edited outside the app"));
    screenshot(&mut h, "9-working-diff");

    // Editing the open file again updates its diff.
    sample.write("src/repo_loader.rs", "// edited twice\n");
    wait_for(&mut h, "the diff to update", |app| {
        let repo = app.repo().unwrap();
        repo.diff_key()
            .and_then(|k| {
                repo.diff.get(&k).and_then(|d| d.as_ref().ok()).map(|d| {
                    d.diff
                        .hunks
                        .iter()
                        .flat_map(|hk| &hk.lines)
                        .any(|l| l.content == "// edited twice")
                })
            })
            .unwrap_or(false)
    });

    // Undo everything: the WIP row goes away and the selection with it.
    std::fs::remove_file(sample.dir.join("TODO.md")).unwrap();
    sample
        .repo
        .checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    wait_for(&mut h, "a clean status", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(git_raccoon_core::working::WorkingStatus::is_clean)
    });
    assert!(!h.state().repo().unwrap().working_selected);
    assert!(h.query_by_label("Working changes").is_none());
}
