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
use egui_kittest::kittest::{NodeT as _, Queryable as _};
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
        // Commits made through `git` need an identity, whatever the machine has.
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Rocky Raccoon").unwrap();
        config.set_str("user.email", "rocky@example.com").unwrap();
        config.set_bool("commit.gpgsign", false).unwrap();
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
    // Idle for a few frames in a row: some work only starts on the next frame.
    let start = Instant::now();
    let mut idle = 0;
    while start.elapsed() < Duration::from_secs(20) {
        h.step();
        if h.state().is_busy() {
            idle = 0;
        } else {
            idle += 1;
            if idle >= 3 {
                return;
            }
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

    // Hidden until Ctrl+F.
    assert!(h.query_by_label("Search commits").is_none());
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

    // Esc closes it, and the search with it.
    h.get_by_label("Search commits").click();
    h.run_ok();
    h.key_press(Key::Escape);
    h.run_ok();
    assert!(h.query_by_label("Search commits").is_none());
    assert!(h.state().repo().unwrap().search_rows().is_empty());
    // So does its close button.
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::F);
    h.run_ok();
    h.get_by_label("Close search (Esc)").click();
    h.run_ok();
    assert!(h.query_by_label("Search commits").is_none());
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
            start.elapsed() < Duration::from_secs(30),
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
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status
            .lines()
            .filter(|l| l.starts_with("VmHWM") || l.starts_with("VmRSS"))
        {
            eprintln!("{line}");
        }
    }
    assert!(worst < Duration::from_millis(250), "a frame took {worst:?}");
}

/// Waits (stepping frames) until `done` holds.
fn wait_for(h: &mut Harness<'_, App>, what: &str, done: impl Fn(&App) -> bool) {
    let start = Instant::now();
    while !done(h.state()) {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "timed out waiting for {what}; error shown: {:?}",
            h.state()
                .error()
                .map(str::to_string)
                .or_else(|| h.state().repo().and_then(|r| r.refresh_error.clone()))
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

/// The index's version of a file, read fresh from disk.
fn staged(sample: &Sample, file: &str) -> Option<String> {
    let repo = Repository::open(&sample.dir).unwrap();
    let index = repo.index().unwrap();
    let entry = index.get_path(Path::new(file), 0)?;
    Some(String::from_utf8(repo.find_blob(entry.id).unwrap().content().to_vec()).unwrap())
}

fn numbered(edit: impl Fn(usize) -> Option<&'static str>) -> String {
    (1..=20)
        .map(|i| edit(i).map_or_else(|| format!("line {i}\n"), |s| format!("{s}\n")))
        .collect()
}

#[test]
fn stages_files_hunks_and_lines() {
    let sample = Sample::build();
    sample.write("notes.txt", numbered(|_| None));
    sample.commit("Add notes", 1);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());

    let edited = numbered(|i| match i {
        2 => Some("line 2 edited"),
        18 => Some("line 18 edited"),
        _ => None,
    });
    sample.write("notes.txt", &edited);
    sample.write("todo.md", "- one\n");
    wait_for(&mut h, "the WIP row", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.unstaged.len() == 2)
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);

    // Whole files, with the row buttons.
    h.get_by_label("Stage todo.md").click();
    wait_for(&mut h, "todo.md staged", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.staged.len() == 1)
    });
    assert_eq!(staged(&sample, "todo.md").as_deref(), Some("- one\n"));
    h.get_by_label("Unstage todo.md").click();
    wait_for(&mut h, "todo.md unstaged", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.staged.is_empty())
    });
    assert_eq!(
        staged(&sample, "todo.md"),
        None,
        "a new file leaves the index again"
    );

    // Everything at once, and back.
    h.get_by_label_contains("Stage all").click();
    wait_for(&mut h, "all staged", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.unstaged.is_empty())
    });
    h.get_by_label_contains("Unstage all").click();
    wait_for(&mut h, "all unstaged", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.staged.is_empty())
    });

    // One hunk of notes.txt, from its diff.
    h.get_by_label("notes.txt").click();
    settle(&mut h);
    let hunk_buttons = h.get_all_by_label("Stage hunk").count();
    assert_eq!(hunk_buttons, 2);
    h.get_all_by_label("Stage hunk").nth(1).unwrap().click();
    wait_for(&mut h, "the second hunk staged", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.staged.len() == 1)
    });
    // Let the open diff reload before picking lines in it.
    settle(&mut h);
    assert_eq!(
        staged(&sample, "notes.txt").unwrap(),
        numbered(|i| (i == 18).then_some("line 18 edited"))
    );
    assert_eq!(
        std::fs::read_to_string(sample.dir.join("notes.txt")).unwrap(),
        edited,
        "the file itself is untouched"
    );

    // A single line: pick the added "line 2 edited" without removing "line 2".
    h.get_by_label("Pick line: line 2 edited").click();
    h.run_ok();
    assert_eq!(h.state().repo().unwrap().picked.len(), 1);
    screenshot(&mut h, "10-pick-line");
    // Clicked through accessibility rather than the pointer: under heavy
    // load with screenshots on, a pointer click here was sometimes lost.
    h.get_by_label("Stage 1 line").click_accesskit();
    wait_for(&mut h, "the line staged", |app| {
        app.repo().unwrap().picked.is_empty() && !app.is_busy()
    });
    let expected: String = numbered(|i| (i == 18).then_some("line 18 edited")).replacen(
        "line 2\n",
        "line 2\nline 2 edited\n",
        1,
    );
    assert_eq!(staged(&sample, "notes.txt").unwrap(), expected);

    // From the staged side (the pane stays beside the diff): unstage the 18 hunk again.
    let button = egui::accesskit::Role::Button;
    let staged_rows = h.get_all_by_role_and_label(button, "notes.txt").count();
    assert_eq!(staged_rows, 2, "notes.txt is both staged and unstaged");
    h.get_all_by_role_and_label(button, "notes.txt")
        .nth(1)
        .unwrap()
        .click();
    settle(&mut h);
    h.get_by_label("Staged");
    let unstage_buttons: Vec<_> = h.get_all_by_label("Unstage hunk").collect();
    let last = unstage_buttons.len() - 1;
    unstage_buttons.into_iter().nth(last).unwrap().click();
    wait_for(&mut h, "the hunk unstaged", |_| {
        staged(&sample, "notes.txt").is_some_and(|s| !s.contains("line 18 edited"))
    });
    assert_eq!(
        staged(&sample, "notes.txt").unwrap(),
        numbered(|_| None).replacen("line 2\n", "line 2\nline 2 edited\n", 1)
    );
}

/// Clicks into a text field, then types (typing goes to the focused widget).
fn type_into(h: &mut Harness<'_, App>, label: &str, text: &str) {
    h.get_by_label(label).click();
    h.run_ok();
    h.get_by_label(label).type_text(text);
    h.run_ok();
}

fn head_message(sample: &Sample) -> String {
    let repo = Repository::open(&sample.dir).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    head.message().unwrap().to_string()
}

#[test]
fn commits_and_amends() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let before = h.state().repo().unwrap().history.total();

    sample.write("notes.md", "notes\n");
    sample.write("scratch.txt", "not for this commit\n");
    wait_for(&mut h, "the WIP row", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.unstaged.len() == 2)
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);

    // HEAD is already on origin/main, so amending it warns.
    h.get_by_label("Amend last commit").click();
    settle(&mut h);
    h.get_by_label_contains("already pushed");
    assert_eq!(
        h.state().repo().unwrap().draft.summary,
        "Redraw icon and rename loader",
        "amend fills in the last message"
    );
    h.get_by_label("Amend last commit").click();
    settle(&mut h);
    assert!(
        h.state().repo().unwrap().draft.summary.is_empty(),
        "and un-ticking clears it again"
    );

    // Nothing staged yet: the commit button is disabled.
    assert!(
        h.get_by_label("Commit 0 files")
            .accesskit_node()
            .is_disabled()
    );
    h.get_by_label("Stage notes.md").click();
    wait_for(&mut h, "notes.md staged", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.staged.len() == 1)
    });

    type_into(&mut h, "Commit summary", "Add notes");
    type_into(&mut h, "Commit description", "Written from Git Raccoon.");
    h.run_ok();
    screenshot(&mut h, "11-commit-box");
    h.get_by_label("Commit 1 file").click();
    wait_for(&mut h, "the new commit", move |app| {
        app.repo().unwrap().history.total() == before + 1
    });
    assert_eq!(
        head_message(&sample),
        "Add notes\n\nWritten from Git Raccoon.\n"
    );
    let status = h.state().repo().unwrap().status.clone().unwrap();
    assert!(status.staged.is_empty());
    assert_eq!(status.unstaged.len(), 1, "scratch.txt stays uncommitted");
    assert!(
        h.state().repo().unwrap().draft.summary.is_empty(),
        "the box is cleared"
    );
    h.get_by_label("Add notes");

    // Amend the new (unpushed) commit's message.
    h.get_by_label("Working changes").click();
    settle(&mut h);
    h.get_by_label("Amend last commit").click();
    settle(&mut h);
    assert!(h.query_by_label_contains("already pushed").is_none());
    h.state_mut().repo_mut().unwrap().draft.summary = "Add project notes".into();
    h.run_ok();
    h.get_by_label("Amend commit").click();
    wait_for(&mut h, "the amended commit", |app| {
        app.repo().unwrap().draft.summary.is_empty() && !app.is_busy()
    });
    assert_eq!(
        head_message(&sample),
        "Add project notes\n\nWritten from Git Raccoon.\n"
    );
    assert_eq!(
        h.state().repo().unwrap().history.total(),
        before + 1,
        "amending adds no commit"
    );
}

#[cfg(unix)]
#[test]
fn shows_hook_failures_and_keeps_the_message() {
    use std::os::unix::fs::PermissionsExt as _;
    let sample = Sample::build();
    let hook = sample.dir.join(".git/hooks/pre-commit");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'lint: missing licence header' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());

    sample.write("notes.md", "notes\n");
    wait_for(&mut h, "the WIP row", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| !s.is_clean())
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);
    h.get_by_label("Stage notes.md").click();
    wait_for(&mut h, "staged", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.staged.len() == 1)
    });
    type_into(&mut h, "Commit summary", "Blocked");
    h.run_ok();
    h.get_by_label("Commit 1 file").click();
    wait_for(&mut h, "the hook to fail", |app| app.error().is_some());
    assert!(
        h.state()
            .error()
            .unwrap()
            .contains("missing licence header"),
        "{:?}",
        h.state().error()
    );
    assert_eq!(
        h.state().repo().unwrap().draft.summary,
        "Blocked",
        "the message is kept to retry"
    );
    assert_eq!(head_message(&sample), "Redraw icon and rename loader");
    screenshot(&mut h, "12-hook-failure");
}

fn read(sample: &Sample, file: &str) -> Option<String> {
    std::fs::read_to_string(sample.dir.join(file)).ok()
}

#[test]
fn discards_into_the_bin_after_asking() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let bin = sample.dir.with_extension("bin");
    h.state_mut().repo_mut().unwrap().trash_override = Some(bin.clone());

    let original = read(&sample, "src/repo_loader.rs").unwrap();
    sample.write("src/repo_loader.rs", "// my unsaved idea\n");
    sample.write("scratch.txt", "scratch\n");
    wait_for(&mut h, "the WIP row", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.unstaged.len() == 2)
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);

    // Asking first; Cancel changes nothing.
    h.get_by_label("Discard changes to src/repo_loader.rs")
        .click();
    h.run_ok();
    h.get_by_label("Discard changes to src/repo_loader.rs?");
    screenshot(&mut h, "13-discard-confirm");
    h.get_by_label("Cancel").click();
    h.run_ok();
    assert!(h.state().repo().unwrap().dialog.is_none());
    assert_eq!(
        read(&sample, "src/repo_loader.rs").as_deref(),
        Some("// my unsaved idea\n")
    );

    // Confirmed: the file is restored and the discarded version kept.
    h.get_by_label("Discard changes to src/repo_loader.rs")
        .click();
    h.run_ok();
    h.get_by_label("Discard").click();
    wait_for(&mut h, "the discard", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.unstaged.len() == 1)
    });
    assert_eq!(read(&sample, "src/repo_loader.rs").unwrap(), original);
    assert_eq!(
        std::fs::read_to_string(bin.join("repo_loader.rs")).unwrap(),
        "// my unsaved idea\n"
    );

    // Discard all, also asking first.
    h.get_by_label("Discard all unstaged changes").click();
    h.run_ok();
    h.get_by_label("Discard").click();
    wait_for(&mut h, "a clean tree", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.is_clean())
    });
    assert_eq!(read(&sample, "scratch.txt"), None);
    assert_eq!(
        std::fs::read_to_string(bin.join("scratch.txt")).unwrap(),
        "scratch\n"
    );
    let _ = std::fs::remove_dir_all(&bin);
}

#[test]
fn staged_files_stay_in_view_beside_many_unstaged_ones() {
    let sample = Sample::build();
    for i in 0..40 {
        sample.write(&format!("notes/{i:02}.txt"), "new\n");
    }
    sample.write("README.md", "# Sample\n\nStaged.\n");
    git(&sample, &["add", "README.md"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    wait_for(&mut h, "the WIP row", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.unstaged.len() == 40)
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);
    screenshot(&mut h, "45-many-changes");
    // The staged file is shown above the commit box, not scrolled away.
    let staged = h.get_by_label("README.md").rect();
    let commit = h.get_by_label("Commit summary").rect();
    assert!(
        staged.bottom() < commit.top(),
        "staged file at {staged:?}, commit box at {commit:?}"
    );
}

#[test]
fn stashes_pops_and_drops() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let stashes = |app: &App| app.repo().unwrap().refs.stashes.len();
    assert_eq!(stashes(h.state()), 1, "the sample starts with one stash");

    sample.write("src/repo_loader.rs", "// stash me\n");
    sample.write("new.txt", "untracked too\n");
    wait_for(&mut h, "the WIP row", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| !s.is_clean())
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);

    h.get_by_label("Stash changes").click();
    h.run_ok();
    type_into(&mut h, "Stash message", "try another approach");
    screenshot(&mut h, "14-stash-dialog");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Stash")
        .click();
    wait_for(&mut h, "the stash", |app| {
        app.repo().unwrap().refs.stashes.len() == 2
    });
    wait_for(&mut h, "a clean tree", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.is_clean())
    });
    assert_eq!(
        read(&sample, "new.txt"),
        None,
        "untracked files were stashed too"
    );

    // Pop it from the details pane.
    h.get_by_label_contains("try another approach").click();
    settle(&mut h);
    h.get_by_label("Pop").click();
    wait_for(&mut h, "the pop", move |app| stashes(app) == 1);
    wait_for(&mut h, "changes back", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| !s.is_clean())
    });
    assert_eq!(
        read(&sample, "src/repo_loader.rs").as_deref(),
        Some("// stash me\n")
    );
    assert_eq!(read(&sample, "new.txt").as_deref(), Some("untracked too\n"));

    // Stage one change and stash just that: offered, and chosen, first.
    let readme = read(&sample, "README.md").unwrap();
    sample.write("README.md", "# Sample\n\nStaged for later.\n");
    git(&sample, &["add", "README.md"]);
    wait_for(&mut h, "the staged change", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.staged.len() == 1)
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);
    h.get_by_label("Stash changes").click();
    h.run_ok();
    h.get_by_label("Only the staged changes (1 file)");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Stash")
        .click();
    wait_for(&mut h, "the staged stash", move |app| stashes(app) == 2);
    assert_eq!(read(&sample, "README.md"), Some(readme), "stashed");
    assert_eq!(
        read(&sample, "src/repo_loader.rs").as_deref(),
        Some("// stash me\n"),
        "the unstaged change stays"
    );
    assert_eq!(read(&sample, "new.txt").as_deref(), Some("untracked too\n"));
    // Undo brings it back, staged.
    wait_for(&mut h, "the undo", |app| undo_label(app).is_some());
    h.get_by_label("Undo").click();
    wait_for(&mut h, "the stash undone", move |app| {
        stashes(app) == 1
            && app
                .repo()
                .unwrap()
                .status
                .as_ref()
                .is_some_and(|s| s.staged.iter().any(|f| f.path == "README.md"))
    });

    // Drop the sample's own stash, after confirming.
    h.get_by_label_contains("half-done README tweak").click();
    settle(&mut h);
    h.get_by_label("Drop").click();
    h.run_ok();
    h.get_by_label("Drop this stash?");
    h.get_by_label("Drop stash").click();
    wait_for(&mut h, "the drop", move |app| stashes(app) == 0);
}

fn erase(h: &mut Harness<'_, App>, chars: usize) {
    for _ in 0..chars {
        h.key_press(egui::Key::Backspace);
    }
    h.run_ok();
}

fn head_branch(app: &App) -> Option<String> {
    app.repo().unwrap().head_branch().map(str::to_string)
}

fn has_branch(app: &App, name: &str) -> bool {
    app.repo()
        .unwrap()
        .refs
        .local
        .iter()
        .any(|b| b.name == name)
}

fn menu_item(h: &mut Harness<'_, App>, target: &str, item: &str) {
    h.get_all_by_label(target).next().unwrap().click_secondary();
    h.run_ok();
    h.get_by_label_contains(item).click();
    h.run_ok();
}

/// Double-clicks at a point with the pointer.
fn double_click_at(h: &mut Harness<'_, App>, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_ok();
    // The test harness feeds each press and release in a frame of its own,
    // a quarter of a second apart: longer than a double-click allows.
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 2.0);
    for pressed in [true, false, true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    h.run_ok();
    h.ctx
        .options_mut(|o| o.input_options.max_double_click_delay = 0.3);
}

#[test]
fn spreads_out_a_commits_labels_and_checks_them_out() {
    let sample = Sample::build();
    // Where main is: origin/main (shown with main, as one label), and a
    // second branch and a tag: three labels on one commit.
    git(&sample, &["branch", "release"]);
    git(&sample, &["tag", "v0.2.0"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let in_graph = |h: &Harness<'_, App>, name: &str| {
        // The sidebar lists them too: the graph's are further right.
        h.query_all_by_label(name)
            .map(|n| n.rect().center())
            .filter(|p| p.x > 230.0)
            .max_by(|a, b| a.y.total_cmp(&b.y))
    };

    // Hovering over the labels shows each of them.
    let row = h.get_by_label("Redraw icon and rename loader").rect();
    let labels = egui::pos2(row.left() + 30.0, row.center().y);
    assert!(in_graph(&h, "release").is_none(), "only on hover");
    h.hover_at(labels);
    h.run_ok();
    assert!(in_graph(&h, "v0.2.0").is_some());
    assert!(
        h.query_by_label("origin/main").is_none(),
        "origin/main is part of main's label"
    );
    screenshot(&mut h, "44-all-labels");
    // Still there when the pointer moves down onto them.
    let release = in_graph(&h, "release").unwrap();
    h.hover_at(release);
    h.run_ok();
    assert!(in_graph(&h, "v0.2.0").is_some());

    // Double-clicking one checks it out.
    double_click_at(&mut h, release);
    wait_for(&mut h, "the checkout", |app| {
        head_branch(app).as_deref() == Some("release")
    });

    // Moving away puts them away.
    h.hover_at(egui::pos2(row.right() - 10.0, row.bottom() + 200.0));
    h.run_ok();
    assert!(in_graph(&h, "v0.2.0").is_none());

    // A label shown in its row can be double-clicked where it is.
    let row = h.get_by_label("Fix toolbar glitch").rect();
    double_click_at(&mut h, egui::pos2(row.left() + 30.0, row.center().y));
    wait_for(&mut h, "the checkout", |app| {
        head_branch(app).as_deref() == Some("fix/ui/toolbar-glitch")
    });
}

#[test]
fn merges_by_dragging_labels_on_the_graph() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());

    // Drag fix/ui/toolbar-glitch's label onto main's.
    let label = |h: &Harness<'_, App>, summary: &str| {
        let row = h.get_by_label(summary).rect();
        egui::pos2(row.left() + 30.0, row.center().y)
    };
    let (from, to) = (
        label(&h, "Fix toolbar glitch"),
        label(&h, "Redraw icon and rename loader"),
    );
    drag(&mut h, from, to);
    h.get_by_label("Merge fix/ui/toolbar-glitch into main?");
    h.get_by_role_and_label(Role::Button, "Merge").click();
    // The branch is just ahead of main: main fast-forwards to it.
    wait_for(&mut h, "the merge", |app| {
        head_summary(app) == "Fix toolbar glitch"
    });
}

#[test]
fn double_clicking_a_remote_branch_brings_the_local_one_up_to_it() {
    let sample = Sample::build();
    // A local branch behind its remote copy: "sync" at the merge, the
    // remote's two commits further on.
    git(&sample, &["branch", "sync", "v0.1.0"]);
    let theirs = {
        let out = std::process::Command::new("git")
            .args(["rev-parse", "main~1"])
            .current_dir(sample.path())
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    git(
        &sample,
        &["update-ref", "refs/remotes/origin/sync", &theirs],
    );
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());

    // Its label is on "Add icon", the remote's commit.
    let row = h.get_by_label("Add icon").rect();
    double_click_at(&mut h, egui::pos2(row.left() + 30.0, row.center().y));
    wait_for(&mut h, "the checkout", |app| {
        head_branch(app).as_deref() == Some("sync")
    });
    settle(&mut h);
    assert_eq!(head_id(h.state()), theirs, "brought up to origin/sync");
    assert_eq!(
        undo_label(h.state()).as_deref(),
        Some("Undo check out origin/sync")
    );

    // Undo: back on main, and sync where it was.
    h.get_by_label("Undo").click();
    wait_for(&mut h, "the undo", |app| {
        head_branch(app).as_deref() == Some("main") && app.repo().unwrap().undo.is_none()
    });
    let out = std::process::Command::new("git")
        .args(["rev-parse", "sync", "v0.1.0^{commit}"])
        .current_dir(sample.path())
        .output()
        .unwrap();
    let ids: Vec<String> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(ids[0], ids[1], "sync is back at v0.1.0");
}

#[test]
fn creates_checks_out_renames_and_deletes_branches_and_tags() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    // The first commit's loader.rs was renamed later, so main doesn't have it.
    assert_eq!(read(&sample, "src/loader.rs"), None);
    assert_eq!(head_branch(h.state()).as_deref(), Some("main"));

    // A new branch from a commit's context menu, with the name checked as typed.
    menu_item(&mut h, "Add repository loader", "Create branch here");
    type_into(&mut h, "Branch name", "feature/graph");
    h.get_by_label("A branch with this name already exists.");
    erase(&mut h, "graph".len());
    h.get_by_label("Branch name").type_text("loader work");
    h.run_ok();
    h.get_by_label_contains("Not a valid branch name");
    screenshot(&mut h, "15-new-branch");
    erase(&mut h, " work".len());
    h.get_by_label("Branch name").type_text("-work");
    h.run_ok();
    h.get_by_role_and_label(Role::Button, "Create and check out")
        .click();
    wait_for(&mut h, "the new branch", |app| {
        head_branch(app).as_deref() == Some("feature/loader-work")
    });
    assert!(read(&sample, "src/loader.rs").is_some());

    // Local changes in the way of switching back: offer to stash them.
    sample.write("src/loader.rs", "// in the way\n");
    wait_for(&mut h, "the change", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| !s.is_clean())
    });
    menu_item(&mut h, "main", "Check out");
    wait_for(&mut h, "the offer to stash", |app| {
        app.repo().unwrap().dialog.is_some()
    });
    h.get_by_label("Your changes are in the way");
    screenshot(&mut h, "16-stash-and-switch");
    h.get_by_label("Stash and switch").click();
    wait_for(&mut h, "main", |app| {
        head_branch(app).as_deref() == Some("main")
    });
    assert_eq!(read(&sample, "src/loader.rs"), None);
    assert_eq!(h.state().repo().unwrap().refs.stashes.len(), 2);
    assert!(
        h.state().repo().unwrap().refs.stashes[0]
            .message
            .contains("Stashed by Git Raccoon")
    );

    // Rename it.
    menu_item(&mut h, "loader-work", "Rename");
    h.get_by_label("New branch name").click();
    h.run_ok();
    erase(&mut h, "-work".len());
    h.run_ok();
    h.get_by_role_and_label(Role::Button, "Rename").click();
    wait_for(&mut h, "the rename", |app| {
        has_branch(app, "feature/loader")
    });
    assert!(!has_branch(h.state(), "feature/loader-work"));

    // Deleting unmerged work asks twice.
    menu_item(&mut h, "toolbar-glitch", "Delete");
    h.get_by_label("Delete branch").click();
    wait_for(&mut h, "the second question", |app| {
        app.repo().unwrap().dialog.is_some()
    });
    h.get_by_label("fix/ui/toolbar-glitch has unmerged work");
    screenshot(&mut h, "17-delete-unmerged");
    h.get_by_label("Delete anyway").click();
    wait_for(&mut h, "the delete", |app| {
        !has_branch(app, "fix/ui/toolbar-glitch")
    });
    // A merged branch goes after one question.
    menu_item(&mut h, "loader", "Delete");
    h.get_by_label("Delete branch").click();
    wait_for(&mut h, "the delete", |app| {
        !has_branch(app, "feature/loader")
    });

    // Tags: an annotated one from the history, then delete the sample's.
    menu_item(&mut h, "Lay out commit lanes", "Create tag here");
    type_into(&mut h, "Tag name", "v0.0.1");
    type_into(&mut h, "Tag message", "First lanes");
    screenshot(&mut h, "18-new-tag");
    h.get_by_label("Create annotated tag").click();
    wait_for(&mut h, "the tag", |app| {
        app.repo()
            .unwrap()
            .refs
            .tags
            .iter()
            .any(|t| t.name == "v0.0.1" && t.annotated)
    });
    menu_item(&mut h, "v0.1.0", "Delete tag");
    h.get_by_role_and_label(Role::Button, "Delete tag").click();
    wait_for(&mut h, "the tag delete", |app| {
        app.repo()
            .unwrap()
            .refs
            .tags
            .iter()
            .all(|t| t.name != "v0.1.0")
    });

    // Check out a commit without a branch.
    menu_item(&mut h, "Add repository loader", "Check out this commit");
    h.get_by_label("Check out commit").click();
    wait_for(&mut h, "detached HEAD", |app| head_branch(app).is_none());
    assert!(read(&sample, "src/loader.rs").is_some());
    screenshot(&mut h, "19-detached");
}

fn undo_label(app: &App) -> Option<String> {
    app.repo().unwrap().undo.as_ref().map(|u| u.label.clone())
}

fn git(sample: &Sample, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(sample.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn undoes_the_last_action() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let head = |app: &App| {
        app.repo().unwrap().history.ids()[app.repo().unwrap().history.head_row().unwrap()]
    };
    assert_eq!(undo_label(h.state()), None);
    let first_head = head(h.state());

    // Undo a commit: the change is staged again and the message is back.
    sample.write("notes.txt", "remember this\n");
    git(&sample, &["add", "notes.txt"]);
    wait_for(&mut h, "the staged file", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.staged.len() == 1)
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);
    type_into(&mut h, "Commit summary", "Write notes");
    h.get_by_role_and_label(Role::Button, "Commit 1 file")
        .click();
    wait_for(&mut h, "the undo", |app| undo_label(app).is_some());
    assert_eq!(
        undo_label(h.state()).unwrap(),
        "Undo commit \u{201c}Write notes\u{201d}"
    );
    settle(&mut h);
    assert_ne!(head(h.state()), first_head);
    screenshot(&mut h, "20-undo-commit");
    h.get_by_label("Undo").click();
    wait_for(&mut h, "the commit undone", move |app| {
        app.repo().unwrap().undo.is_none() && !app.is_busy()
    });
    settle(&mut h);
    assert_eq!(head(h.state()), first_head);
    let repo = h.state().repo().unwrap();
    assert_eq!(repo.draft.summary, "Write notes");
    assert_eq!(repo.status.as_ref().unwrap().staged[0].path, "notes.txt");

    // Undo deleting a branch.
    menu_item(&mut h, "graph", "Delete");
    h.get_by_label("Delete branch").click();
    wait_for(&mut h, "the delete", |app| {
        !has_branch(app, "feature/graph")
    });
    wait_for(&mut h, "the undo", |app| undo_label(app).is_some());
    h.get_by_label("Undo").click();
    wait_for(&mut h, "the branch back", |app| {
        has_branch(app, "feature/graph")
    });

    // Undo a checkout. (The staged notes travel along.)
    menu_item(&mut h, "toolbar-glitch", "Check out");
    wait_for(&mut h, "the checkout", |app| {
        head_branch(app).as_deref() == Some("fix/ui/toolbar-glitch")
    });
    wait_for(&mut h, "the undo", |app| undo_label(app).is_some());
    h.get_by_label("Undo").click();
    wait_for(&mut h, "main again", |app| {
        head_branch(app).as_deref() == Some("main")
    });

    // Ctrl+Z undoes creating a tag.
    menu_item(&mut h, "Add icon", "Create tag here");
    type_into(&mut h, "Tag name", "oops");
    h.get_by_role_and_label(Role::Button, "Create tag").click();
    wait_for(&mut h, "the tag", |app| {
        app.repo()
            .unwrap()
            .refs
            .tags
            .iter()
            .any(|t| t.name == "oops")
    });
    wait_for(&mut h, "the undo", |app| undo_label(app).is_some());
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    wait_for(&mut h, "the tag gone", |app| {
        app.repo()
            .unwrap()
            .refs
            .tags
            .iter()
            .all(|t| t.name != "oops")
    });

    // Moving HEAD outside the app drops the undo.
    menu_item(&mut h, "graph", "Rename");
    h.get_by_label("New branch name").click();
    h.get_by_label("New branch name").type_text("-2");
    h.run_ok();
    h.get_by_role_and_label(Role::Button, "Rename").click();
    wait_for(&mut h, "the undo", |app| undo_label(app).is_some());
    git(&sample, &["commit", "-q", "-m", "Committed in a terminal"]);
    wait_for(&mut h, "the undo dropped", |app| undo_label(app).is_none());
}

fn upstream_counts(app: &App) -> Option<(usize, usize)> {
    app.repo()
        .unwrap()
        .head_upstream()
        .map(|u| (u.ahead, u.behind))
}

#[test]
fn fetches_pulls_and_pushes() {
    let sample = Sample::build();
    let base = std::env::temp_dir().join(format!("git-raccoon-remote-{}", rand_suffix()));
    let (bare, other) = (base.join("remote.git"), base.join("other"));
    std::fs::create_dir_all(&base).unwrap();
    let run = |dir: &Path, args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(&base, &["init", "-q", "--bare", "remote.git"]);
    let url = bare.to_str().unwrap().replace('\\', "/");
    git(&sample, &["remote", "set-url", "origin", &url]);
    git(&sample, &["push", "-q", "origin", "main"]);
    run(&base, &["clone", "-q", "-b", "main", &url, "other"]);
    for (k, v) in [
        ("user.name", "Other Raccoon"),
        ("user.email", "other@example.com"),
        ("commit.gpgsign", "false"),
    ] {
        run(&other, &["config", k, v]);
    }
    let someone_else_commits = |file: &str| {
        run(&other, &["pull", "-q", "--rebase"]);
        std::fs::write(other.join(file), "theirs\n").unwrap();
        run(&other, &["add", file]);
        run(&other, &["commit", "-q", "-m", &format!("Add {file}")]);
        run(&other, &["push", "-q", "origin", "HEAD"]);
    };

    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    assert_eq!(upstream_counts(h.state()), Some((0, 0)));

    // Push a local commit.
    git(
        &sample,
        &["commit", "-q", "--allow-empty", "-m", "Local work"],
    );
    wait_for(&mut h, "ahead by one", |app| {
        upstream_counts(app) == Some((1, 0))
    });
    h.get_by_label("Push").click();
    wait_for(&mut h, "the push", |app| {
        upstream_counts(app) == Some((0, 0)) && !app.is_busy()
    });

    // Someone else pushes: fetch shows it, pull brings it in.
    someone_else_commits("theirs.txt");
    h.get_by_label("Fetch").click();
    wait_for(&mut h, "behind by one", |app| {
        upstream_counts(app) == Some((0, 1))
    });
    screenshot(&mut h, "21-behind");
    h.get_by_label("Pull").click();
    wait_for(&mut h, "the pull", |app| {
        upstream_counts(app) == Some((0, 0)) && !app.is_busy()
    });
    assert_eq!(read(&sample, "theirs.txt").as_deref(), Some("theirs\n"));

    // Both sides move on: the push is refused, and the app offers a way on.
    someone_else_commits("more.txt");
    git(
        &sample,
        &["commit", "-q", "--allow-empty", "-m", "More local work"],
    );
    wait_for(&mut h, "ahead", |app| upstream_counts(app) == Some((1, 0)));
    h.get_by_label("Push").click();
    wait_for(&mut h, "the question", |app| {
        app.repo().unwrap().dialog.is_some()
    });
    h.get_by_label("The remote has commits you don't have");
    screenshot(&mut h, "22-push-rejected");
    h.get_by_label("Pull first").click();
    wait_for(&mut h, "the next question", |app| {
        app.repo().unwrap().dialog.is_some() && !app.is_busy()
    });
    h.get_by_label_contains("have both moved on");
    h.get_by_label("Rebase").click();
    wait_for(&mut h, "the rebase", |app| {
        upstream_counts(app) == Some((1, 0)) && !app.is_busy()
    });
    assert_eq!(read(&sample, "more.txt").as_deref(), Some("theirs\n"));
    h.get_by_label("Push").click();
    wait_for(&mut h, "the push", |app| {
        upstream_counts(app) == Some((0, 0)) && !app.is_busy()
    });
    assert_eq!(h.state().repo().unwrap().refresh_error, None);

    // A remote that never answers can be cancelled.
    git(
        &sample,
        &[
            "config",
            "remote.origin.uploadpack",
            "sleep 60; git-upload-pack",
        ],
    );
    h.get_by_label("Fetch").click();
    // (Not wait_for: that also waits for the app to be idle.)
    let start = Instant::now();
    while h.state().repo().unwrap().sync_progress().is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "the fetch didn't start"
        );
        h.step();
    }
    for _ in 0..5 {
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    screenshot(&mut h, "23-fetching");
    h.get_by_label("Cancel fetch").click();
    wait_for(&mut h, "the cancel", |app| {
        app.repo().unwrap().sync_progress().is_none()
    });
    assert_eq!(
        h.state().repo().unwrap().refresh_error,
        None,
        "cancelling isn't an error"
    );
    let _ = std::fs::remove_dir_all(&base);
}

fn remote_names(app: &App) -> Vec<(String, usize)> {
    app.repo()
        .unwrap()
        .refs
        .remotes
        .iter()
        .map(|r| (r.name.clone(), r.branches.len()))
        .collect()
}

#[test]
fn adds_edits_and_removes_remotes() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    let bare = std::env::temp_dir().join(format!("git-raccoon-fork-{}.git", rand_suffix()));
    let url = bare.to_str().unwrap().replace('\\', "/");
    git(&sample, &["init", "-q", "--bare", &url]);
    git(&sample, &["push", "-q", &url, "main", "feature/graph"]);

    let mut h = harness(Settings::default());
    open(&mut h, sample.path());

    // Add one: the name is checked as typed, and it's fetched once added.
    h.get_by_label("Add remote").click();
    h.run_ok();
    type_into(&mut h, "Remote name", "origin");
    h.get_by_label("A remote with this name already exists.");
    erase(&mut h, "origin".len());
    h.get_by_label("Remote name").type_text("fork");
    type_into(&mut h, "Remote URL", &url);
    screenshot(&mut h, "25-add-remote");
    h.get_by_label("Add and fetch").click();
    wait_for(&mut h, "the fetched remote", |app| {
        remote_names(app).contains(&("fork".into(), 2))
    });
    assert_eq!(h.state().error(), None);

    // Rename it.
    menu_item(&mut h, "fork", "Edit");
    h.get_by_label("Remote name").click();
    h.run_ok();
    erase(&mut h, "fork".len());
    h.get_by_label("Remote name").type_text("mirror");
    h.run_ok();
    h.get_by_role_and_label(Role::Button, "Save").click();
    wait_for(&mut h, "the rename", |app| {
        remote_names(app).contains(&("mirror".into(), 2))
    });

    // Remove it, after confirming.
    menu_item(&mut h, "mirror", "Remove");
    h.get_by_label("Remove remote mirror?");
    h.get_by_label("Remove remote").click();
    wait_for(&mut h, "the removal", |app| {
        remote_names(app).iter().all(|(n, _)| n != "mirror")
    });
    assert_eq!(remote_names(h.state()), [("origin".to_string(), 1)]);
    let _ = std::fs::remove_dir_all(&bare);
}

#[test]
fn clones_and_starts_repositories() {
    let sample = Sample::build();
    let base = std::env::temp_dir().join(format!("git-raccoon-clones-{}", rand_suffix()));
    std::fs::create_dir_all(&base).unwrap();
    let parent = base.to_str().unwrap().replace('\\', "/");
    let mut h = harness(Settings::default());
    h.run_ok();

    // Clone the sample from the welcome screen.
    h.get_by_label("Clone a repository…").click();
    h.run_ok();
    type_into(&mut h, "Repository URL", sample.path());
    h.get_by_label("Clone folder").click();
    h.run_ok();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Clone folder").type_text(&parent);
    h.run_ok();
    let name = Path::new(sample.path())
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        h.get_by_label("Clone name").value().as_deref(),
        Some(name.as_str()),
        "named after the URL"
    );
    screenshot(&mut h, "26-clone");
    h.get_by_label("Clone").click();
    wait_for(&mut h, "the clone to open", |app| app.repo().is_some());
    let repo = h.state().repo().unwrap();
    assert_eq!(
        Path::new(repo.path()).canonicalize().unwrap(),
        base.join(&name).canonicalize().unwrap()
    );
    assert!(repo.history.total() > 5);
    assert_eq!(
        h.state().settings().clone_dir.as_deref(),
        Some(parent.as_str()),
        "remembered"
    );

    // Cloning into a folder that's in use isn't offered.
    // From a new tab.
    press(&mut h, egui::Modifiers::COMMAND, Key::T);
    h.get_by_label("Clone a repository…").click();
    h.run_ok();
    type_into(&mut h, "Repository URL", sample.path());
    h.get_by_label_contains("already exists and isn't empty");
    h.get_by_label("Cancel").click();
    h.run_ok();

    // A new, empty repository opens too.
    let fresh = base.join("fresh");
    let ctx = h.ctx.clone();
    h.state_mut().new_repository(&ctx, &fresh);
    wait_for(&mut h, "the new repository", |app| {
        app.repo().is_some_and(|r| r.path().contains("fresh"))
    });
    assert_eq!(h.state().error(), None);
    assert_eq!(h.state().repo().unwrap().history.total(), 0);
    screenshot(&mut h, "27-new-repository");
    let _ = std::fs::remove_dir_all(&base);
}

fn head_summary(app: &App) -> String {
    let repo = app.repo().unwrap();
    let row = repo.history.head_row().unwrap();
    repo.history.page(row, 1).unwrap().commits[0]
        .summary
        .clone()
}

fn in_progress(app: &App) -> bool {
    app.repo()
        .unwrap()
        .status
        .as_ref()
        .is_some_and(|s| s.in_progress.is_some())
}

fn conflicted(app: &App) -> usize {
    app.repo()
        .unwrap()
        .status
        .as_ref()
        .map_or(0, |s| s.conflicted.len())
}

/// Drags one sidebar row onto another with the pointer.
fn drag(h: &mut Harness<'_, App>, from: egui::Pos2, to: egui::Pos2) {
    h.event(egui::Event::PointerMoved(from));
    h.run_ok();
    h.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_ok();
    for i in 1..=10 {
        h.event(egui::Event::PointerMoved(
            from + (to - from) * (i as f32 / 10.0),
        ));
        h.step();
    }
    h.run_ok();
    h.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_ok();
}

#[test]
fn merges_resolves_conflicts_continues_and_aborts() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    // A branch that changes README.md one way while main changes it another.
    git(&sample, &["switch", "-q", "-c", "clash"]);
    sample.write("README.md", "# Sample\n\nclash version\n");
    git(&sample, &["commit", "-q", "-am", "README from clash"]);
    git(&sample, &["switch", "-q", "main"]);
    sample.write("README.md", "# Sample\n\nmain version\n");
    git(&sample, &["commit", "-q", "-am", "README from main"]);

    let mut h = harness(Settings::default());
    open(&mut h, sample.path());

    // A clean merge from the menu, with a merge commit, then undone.
    let before = head_summary(h.state());
    menu_item(&mut h, "toolbar-glitch", "Merge into main");
    h.get_by_label("Merge fix/ui/toolbar-glitch into main?");
    h.get_by_label("Always create a merge commit").click();
    wait_for(&mut h, "the merge", |app| {
        head_summary(app) == "Merge branch 'fix/ui/toolbar-glitch'"
    });
    assert_eq!(
        read(&sample, "src/toolbar.rs").as_deref(),
        Some("pub fn draw() {}\n")
    );
    wait_for(&mut h, "the undo", |app| {
        undo_label(app).as_deref() == Some("Undo merge of fix/ui/toolbar-glitch")
    });
    h.get_by_label("Undo").click();
    wait_for(&mut h, "the merge undone", move |app| {
        head_summary(app) == before
    });
    assert_eq!(read(&sample, "src/toolbar.rs"), None);

    // The same merge by dragging the branch onto main.
    let from = h.get_by_label("toolbar-glitch").rect().center();
    let to = h.get_all_by_label("main").next().unwrap().rect().center();
    drag(&mut h, from, to);
    h.get_by_label("Merge fix/ui/toolbar-glitch into main?");
    h.get_by_role_and_label(Role::Button, "Merge").click();
    wait_for(&mut h, "the dragged merge", |app| {
        head_summary(app) == "Merge branch 'fix/ui/toolbar-glitch'"
    });

    // A merge with a conflict: it opens straight away, with a banner.
    menu_item(&mut h, "clash", "Merge into main");
    h.get_by_role_and_label(Role::Button, "Merge").click();
    wait_for(&mut h, "the conflict", |app| {
        app.repo().unwrap().conflict.is_some() && conflicted(app) == 1
    });
    h.get_by_label_contains("A merge is in progress");
    screenshot(&mut h, "28-conflict");
    h.get_by_label("Use both for conflict 1").click();
    wait_for(&mut h, "the pick", |app| {
        app.repo()
            .unwrap()
            .conflict
            .as_ref()
            .is_some_and(|c| c.text.as_ref().is_ok_and(|t| !t.contains("<<<<<<<")))
    });
    assert_eq!(
        read(&sample, "README.md").as_deref(),
        Some("# Sample\n\nmain version\nclash version\n")
    );
    h.get_by_label("Mark resolved").click();
    wait_for(&mut h, "resolved", |app| conflicted(app) == 0);
    screenshot(&mut h, "29-ready-to-continue");
    h.get_by_label("Continue merge").click();
    wait_for(&mut h, "the merge commit", |app| !in_progress(app));
    assert_eq!(head_summary(h.state()), "Merge branch 'clash'");

    // Again, edited by hand, then aborted.
    h.get_by_label("Undo").click();
    wait_for(&mut h, "undone", |app| {
        head_summary(app) != "Merge branch 'clash'"
    });
    menu_item(&mut h, "clash", "Merge into main");
    h.get_by_role_and_label(Role::Button, "Merge").click();
    wait_for(&mut h, "the conflict", |app| {
        app.repo().unwrap().conflict.is_some()
    });
    h.get_by_label_contains("Edit").click();
    h.run_ok();
    h.get_by_label("Resolved text").click();
    h.run_ok();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Resolved text")
        .type_text("# Sample\n\nwritten by hand\n");
    h.run_ok();
    h.get_by_label("Save").click();
    wait_for(&mut h, "the save", |app| {
        app.repo().unwrap().conflict.as_ref().is_some_and(|c| {
            c.edit.is_none() && c.text.as_ref().is_ok_and(|t| t.contains("by hand"))
        })
    });
    h.get_by_label("Abort merge").click();
    h.run_ok();
    h.get_by_label("Yes, abort").click();
    wait_for(&mut h, "the abort", |app| !in_progress(app));
    assert_eq!(
        read(&sample, "README.md").as_deref(),
        Some("# Sample\n\nmain version\n")
    );
    assert_eq!(h.state().error(), None);
}

fn head_id(app: &App) -> String {
    let repo = app.repo().unwrap();
    repo.history.ids()[repo.history.head_row().unwrap()].to_string()
}

fn status_counts(app: &App) -> (usize, usize) {
    app.repo()
        .unwrap()
        .status
        .as_ref()
        .map_or((0, 0), |s| (s.staged.len(), s.unstaged.len()))
}

#[test]
fn cherry_picks_reverts_resets_and_rebases() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let start = head_id(h.state());
    let undo = |h: &mut Harness<'_, App>, label: &str, back_to: &str| {
        let (label, back_to) = (label.to_string(), back_to.to_string());
        wait_for(h, "the undo", move |app| {
            undo_label(app).is_some_and(|l| l.starts_with(&label))
        });
        h.get_by_label("Undo").click();
        wait_for(h, "undone", move |app| head_id(app) == back_to);
    };

    // Cherry-pick a commit from another branch.
    menu_item(&mut h, "Fix toolbar glitch", "Cherry-pick onto main");
    wait_for(&mut h, "the cherry-pick", |app| {
        head_summary(app) == "Fix toolbar glitch"
    });
    assert!(read(&sample, "src/toolbar.rs").is_some());
    undo(&mut h, "Undo cherry-pick of", &start);
    assert!(read(&sample, "src/toolbar.rs").is_none());

    // Revert one.
    menu_item(
        &mut h,
        "Redraw icon and rename loader",
        "Revert this commit",
    );
    wait_for(&mut h, "the revert", |app| {
        head_summary(app) == "Revert \"Redraw icon and rename loader\""
    });
    undo(&mut h, "Undo revert of", &start);

    // Reset: mixed keeps the changes in the files; undoing leaves them alone.
    menu_item(&mut h, "Add icon", "Reset main to here");
    h.get_by_label_contains("Reset main to");
    screenshot(&mut h, "30-reset");
    h.get_by_label("Mixed").click();
    wait_for(&mut h, "the reset", |app| head_summary(app) == "Add icon");
    wait_for(&mut h, "changes kept", |app| status_counts(app).1 > 0);
    undo(&mut h, "Undo reset", &start);
    // Hard: the files go back too, and come forward again with undo.
    menu_item(&mut h, "Add icon", "Reset main to here");
    h.get_by_label("Hard").click();
    wait_for(&mut h, "the hard reset", |app| {
        head_summary(app) == "Add icon"
    });
    wait_for(&mut h, "clean", |app| status_counts(app) == (0, 0));
    undo(&mut h, "Undo reset", &start);
    wait_for(&mut h, "clean again", |app| status_counts(app) == (0, 0));

    // Rebase a branch made from an older commit onto main.
    git(&sample, &["switch", "-q", "-c", "topic", "HEAD~2"]);
    sample.write("topic.txt", "topic\n");
    git(&sample, &["add", "topic.txt"]);
    git(&sample, &["commit", "-q", "-m", "Topic work"]);
    wait_for(&mut h, "topic checked out", |app| {
        head_branch(app).as_deref() == Some("topic")
    });
    let topic_before = head_id(h.state());
    menu_item(&mut h, "main", "Rebase topic onto main");
    h.get_by_label("Rebase topic onto main?");
    h.get_by_label("Rebase").click();
    wait_for(&mut h, "the rebase", move |app| {
        head_summary(app) == "Topic work" && head_id(app) != topic_before
    });
    let repo = Repository::open(sample.path()).unwrap();
    let parent = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent_id(0)
        .unwrap();
    assert_eq!(parent.to_string(), start, "now on top of main");
    let topic_before = repo.revparse_single("topic@{1}").unwrap().id().to_string();
    undo(&mut h, "Undo rebase onto main", &topic_before);
    assert_eq!(h.state().error(), None);
}

fn log_summaries(sample: &Sample) -> Vec<String> {
    let out = std::process::Command::new("git")
        .args(["log", "--format=%s", "-5"])
        .current_dir(sample.path())
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn plans_and_runs_an_interactive_rebase() {
    let sample = Sample::build();
    for n in ["one", "two"] {
        sample.write(&format!("notes/{n}.txt"), format!("{n}\n"));
        git(&sample, &["add", "notes"]);
        git(&sample, &["commit", "-q", "-m", &format!("Note {n}")]);
    }
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let start = head_id(h.state());

    menu_item(
        &mut h,
        "Merge branch 'feature/graph'",
        "Interactive rebase from here",
    );
    h.get_by_label("Interactive rebase");
    assert_eq!(
        h.state()
            .repo()
            .unwrap()
            .planner
            .as_ref()
            .unwrap()
            .0
            .rows
            .len(),
        4
    );

    // Drag "Note two" to the bottom (oldest).
    let from = h.get_by_label("Drag Note two").rect().center();
    let to = h.get_by_label("Drag Add icon").rect().center();
    drag(&mut h, from, to);
    let order: Vec<String> = h
        .state()
        .repo()
        .unwrap()
        .planner
        .as_ref()
        .unwrap()
        .0
        .rows
        .iter()
        .map(|r| r.summary.clone())
        .collect();
    assert_eq!(
        order,
        [
            "Note one",
            "Redraw icon and rename loader",
            "Add icon",
            "Note two"
        ]
    );

    h.get_by_label("Drop Note one").click();
    h.run_ok();
    h.get_by_label("Fixup Redraw icon and rename loader")
        .click();
    h.run_ok();
    h.get_by_label("Reword Add icon").click();
    h.run_ok();
    h.get_by_label("Message for Add icon").click();
    h.run_ok();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Message for Add icon")
        .type_text("Add the icon");
    h.run_ok();
    screenshot(&mut h, "31-interactive-rebase");
    h.get_by_label("Start rebase").click();
    wait_for(&mut h, "the rebase", |app| {
        head_summary(app) == "Add the icon"
    });
    assert_eq!(
        log_summaries(&sample)[..3],
        ["Add the icon", "Note two", "Merge branch 'feature/graph'"]
    );
    assert!(read(&sample, "notes/two.txt").is_some() && read(&sample, "notes/one.txt").is_none());
    assert!(
        read(&sample, "src/repo_loader.rs").is_some(),
        "the fixed-up commit's changes stay"
    );

    // And undo the lot.
    wait_for(&mut h, "the undo", |app| {
        undo_label(app).as_deref() == Some("Undo interactive rebase")
    });
    h.get_by_label("Undo").click();
    wait_for(&mut h, "undone", move |app| head_id(app) == start);
    assert_eq!(h.state().error(), None);
}

fn press(h: &mut Harness<'_, App>, modifiers: egui::Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run_ok();
}

const CTRL_SHIFT: egui::Modifiers = egui::Modifiers {
    shift: true,
    ..egui::Modifiers::COMMAND
};

#[test]
fn runs_commands_from_the_palette_and_shortcuts() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());

    // Ctrl+K, a few letters, Enter.
    press(&mut h, egui::Modifiers::COMMAND, Key::K);
    h.get_by_label("Command").type_text("co toolbar");
    h.run_ok();
    screenshot(&mut h, "32-palette");
    press(&mut h, egui::Modifiers::NONE, Key::Enter);
    wait_for(&mut h, "the checkout", |app| {
        head_branch(app).as_deref() == Some("fix/ui/toolbar-glitch")
    });

    // Ctrl+Shift+P works too; this time a dialog.
    press(&mut h, CTRL_SHIFT, Key::P);
    h.get_by_label("Command").type_text("create branch");
    h.run_ok();
    press(&mut h, egui::Modifiers::NONE, Key::Enter);
    h.get_by_label("Branch name");
    press(&mut h, egui::Modifiers::NONE, Key::Escape);
    assert!(h.state().repo().unwrap().dialog.is_none());

    // Arrows choose: the second "Go to" line.
    press(&mut h, egui::Modifiers::COMMAND, Key::K);
    h.get_by_label("Command").type_text("go to tag");
    h.run_ok();
    press(&mut h, egui::Modifiers::NONE, Key::ArrowDown);
    press(&mut h, egui::Modifiers::NONE, Key::ArrowUp);
    press(&mut h, egui::Modifiers::NONE, Key::Enter);
    wait_for(&mut h, "the tag selected", |app| {
        selected_summary_of(app).as_deref() == Some("Merge branch 'feature/graph'")
    });

    // Ctrl+B toggles the sidebar, but not while typing.
    press(&mut h, egui::Modifiers::COMMAND, Key::B);
    assert!(h.state().settings().hide_sidebar);
    press(&mut h, egui::Modifiers::COMMAND, Key::B);
    assert!(!h.state().settings().hide_sidebar);

    // Ctrl+Shift+S stages everything, even from the commit message box.
    sample.write("src/toolbar.rs", "pub fn draw() { /* changed */ }\n");
    wait_for(&mut h, "the change", |app| status_counts(app).1 == 1);
    press(&mut h, CTRL_SHIFT, Key::M);
    h.get_by_label("Commit summary").type_text("Tweak");
    h.run_ok();
    press(&mut h, egui::Modifiers::COMMAND, Key::B);
    assert!(
        !h.state().settings().hide_sidebar,
        "Ctrl+B while typing is the text's"
    );
    press(&mut h, CTRL_SHIFT, Key::S);
    wait_for(&mut h, "staged", |app| status_counts(app) == (1, 0));
    assert_eq!(h.state().repo().unwrap().draft.summary, "Tweak");
}

fn selected_summary_of(app: &App) -> Option<String> {
    let repo = app.repo()?;
    let row = repo.selected.as_ref()?.row?;
    repo.history
        .page(row, 1)
        .ok()?
        .commits
        .first()
        .map(|c| c.summary.clone())
}

#[test]
fn shows_a_files_history_and_blame() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());

    // From a commit's diff of the renamed loader.
    h.get_by_label("Redraw icon and rename loader").click();
    settle(&mut h);
    h.get_by_label_contains("src/repo_loader.rs").click();
    settle(&mut h);
    h.get_by_label("History").click();
    wait_for(&mut h, "the history", |app| {
        app.repo()
            .unwrap()
            .file_view
            .as_ref()
            .is_some_and(|v| v.history.is_some())
    });
    let history: Vec<String> = h
        .state()
        .repo()
        .unwrap()
        .file_view
        .as_ref()
        .unwrap()
        .history
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| c.summary.clone())
        .collect();
    assert_eq!(
        history,
        [
            "Redraw icon and rename loader",
            "Reject bare repositories when loading",
            "Add repository loader"
        ],
        "follows the rename"
    );

    // An older entry shows its change to the file, by its old name.
    h.get_by_label("Add repository loader").click();
    settle(&mut h);
    assert_eq!(
        selected_summary(&h).as_deref(),
        Some("Add repository loader")
    );
    assert_eq!(
        h.state().repo().unwrap().active_file.as_deref(),
        Some("src/loader.rs")
    );
    screenshot(&mut h, "33-file-history");

    // Blame: who last changed each line.
    h.get_by_label("Blame").click();
    wait_for(&mut h, "the blame", |app| {
        app.repo()
            .unwrap()
            .file_view
            .as_ref()
            .is_some_and(|v| v.blame.is_some())
    });
    let blame = h
        .state()
        .repo()
        .unwrap()
        .file_view
        .as_ref()
        .unwrap()
        .blame
        .clone()
        .unwrap()
        .unwrap();
    let authors: std::collections::HashSet<&str> =
        blame.commits.iter().map(|c| c.summary.as_str()).collect();
    assert!(
        authors.contains("Reject bare repositories when loading"),
        "{authors:?}"
    );
    screenshot(&mut h, "34-blame");
    h.get_all_by_label_contains("Reject bare repositories when loading")
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("Commit "))
        })
        .unwrap()
        .click();
    settle(&mut h);
    assert!(h.state().repo().unwrap().file_view.is_none());
    assert_eq!(
        selected_summary(&h).as_deref(),
        Some("Reject bare repositories when loading")
    );
}

#[test]
fn switches_theme_and_size_in_settings() {
    use git_raccoon::settings::ThemeChoice;
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    assert_eq!(
        h.state().settings().theme,
        ThemeChoice::Raccoon,
        "Raccoon is the default look"
    );
    assert!(h.ctx.theme() == egui::Theme::Dark);

    // A diff open, to see it in both themes.
    h.get_by_label("Reject bare repositories when loading")
        .click();
    settle(&mut h);
    h.get_by_label("src/loader.rs").click();
    settle(&mut h);

    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    h.get_by_label("Light").click();
    h.run_ok();
    assert_eq!(h.state().settings().theme, ThemeChoice::Light);
    assert!(h.ctx.theme() == egui::Theme::Light);
    h.get_by_label("Done").click();
    settle(&mut h);
    screenshot(&mut h, "35-light-diff");
    press(&mut h, egui::Modifiers::NONE, Key::Escape);
    settle(&mut h);
    screenshot(&mut h, "36-light-graph");

    // Bigger, then back to normal and dark.
    h.get_by_label("Settings").click();
    h.run_ok();
    h.get_by_label("125%").click();
    h.run_ok();
    assert!((h.ctx.zoom_factor() - 1.25).abs() < 0.001);
    assert_eq!(h.state().settings().zoom, Some(1.25));
    screenshot(&mut h, "37-settings");
    h.get_by_label("100%").click();
    h.run_ok();
    h.get_by_label("Dark").click();
    h.run_ok();
    assert_eq!(h.state().settings().zoom, None);
    assert!(h.ctx.theme() == egui::Theme::Dark);
}

#[test]
fn renders_every_theme() {
    use git_raccoon::settings::ThemeChoice;
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    h.get_by_label("Reject bare repositories when loading")
        .click();
    settle(&mut h);
    h.get_by_label("src/loader.rs").click();
    settle(&mut h);
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    for (choice, name) in [
        (ThemeChoice::Raccoon, "Raccoon"),
        (ThemeChoice::Pastel, "Pastel"),
        (ThemeChoice::Abyss, "Abyss"),
        (ThemeChoice::Forest, "Forest"),
        (ThemeChoice::Paper, "Paper"),
        (ThemeChoice::HighContrast, "High contrast"),
        (ThemeChoice::Light, "Light"),
        (ThemeChoice::Dark, "Dark"),
    ] {
        h.get_by_label(name).click();
        h.run_ok();
        assert_eq!(h.state().settings().theme, choice);
        if name == "Raccoon" {
            screenshot(&mut h, "38-theme-picker");
        }
        h.get_by_label("Done").click();
        settle(&mut h);
        screenshot(
            &mut h,
            &format!("39-theme-{}", name.replace(' ', "-").to_lowercase()),
        );
        press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    }
}

fn tab_names(h: &Harness<'_, App>) -> (Vec<String>, usize) {
    h.state().tab_names()
}

#[test]
fn keeps_repositories_in_tabs() {
    let a = Sample::build();
    let b = Sample::build();
    let (name_a, name_b) = (
        Path::new(a.path())
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
        Path::new(b.path())
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string(),
    );
    let mut h = harness(Settings::default());
    open(&mut h, a.path());
    assert_eq!(tab_names(&h), (vec![name_a.clone()], 0));

    // A new tab shows the welcome screen; opening there fills it.
    press(&mut h, egui::Modifiers::COMMAND, Key::T);
    assert_eq!(tab_names(&h), (vec![name_a.clone(), "New tab".into()], 1));
    assert!(h.state().repo().is_none());
    h.get_by_label("Open a repository…");
    open(&mut h, b.path());
    assert_eq!(tab_names(&h), (vec![name_a.clone(), name_b.clone()], 1));
    screenshot(&mut h, "40-tabs");

    // Opening one that's already open just switches to it.
    open(&mut h, a.path());
    assert_eq!(tab_names(&h).0.len(), 2);
    assert_eq!(tab_names(&h).1, 0);
    assert!(h.state().repo().unwrap().path().contains(&name_a));

    // Dragging a tab past its neighbour reorders them; it stays the one shown.
    let tab = |h: &Harness<'_, App>, name: &str| {
        h.get_all_by_label(name)
            .map(|n| n.rect())
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap()
    };
    let from = tab(&h, &name_a);
    drag(
        &mut h,
        from.center(),
        from.center() + egui::vec2(from.width() * 1.6, 0.0),
    );
    assert_eq!(tab_names(&h), (vec![name_b.clone(), name_a.clone()], 1));
    assert!(h.state().repo().unwrap().path().contains(&name_a));
    assert_eq!(h.state().settings().open_tabs.len(), 2);
    assert!(
        h.state().settings().open_tabs[0].contains(&name_b),
        "remembered"
    );
    // And back.
    let from = tab(&h, &name_a);
    drag(
        &mut h,
        from.center(),
        from.center() - egui::vec2(from.width() * 1.6, 0.0),
    );
    assert_eq!(tab_names(&h), (vec![name_a.clone(), name_b.clone()], 0));

    // Keys and clicks switch; each tab keeps its own selection.
    h.get_by_label("Add icon").click();
    settle(&mut h);
    press(&mut h, egui::Modifiers::COMMAND, Key::Tab);
    assert_eq!(tab_names(&h).1, 1);
    assert!(h.state().repo().unwrap().selected.is_none());
    press(&mut h, egui::Modifiers::COMMAND, Key::Num1);
    assert_eq!(tab_names(&h).1, 0);
    assert_eq!(selected_summary(&h).as_deref(), Some("Add icon"));
    h.get_by_label(&name_b).click();
    h.run_ok();
    assert_eq!(tab_names(&h).1, 1);

    // An operation started in one tab finishes while another is showing.
    b.write("README.md", "# Changed in the background\n");
    wait_for(&mut h, "the change", |app| status_counts(app).1 == 1);
    press(&mut h, CTRL_SHIFT, Key::S);
    press(&mut h, egui::Modifiers::COMMAND, Key::Num1);
    let staged_in_b = || {
        std::process::Command::new("git")
            .args(["diff", "--cached", "--name-only"])
            .current_dir(b.path())
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let start = Instant::now();
    while staged_in_b() != "README.md" {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "staged in the background"
        );
        h.step();
    }

    // The tabs come back next time.
    h.state_mut().remember_tabs();
    let saved = h.state().settings().clone();
    assert_eq!(saved.open_tabs.len(), 2);
    let mut again = harness(saved);
    let ctx = again.ctx.clone();
    again.state_mut().restore_tabs(&ctx);
    settle(&mut again);
    assert_eq!(tab_names(&again), (vec![name_a.clone(), name_b.clone()], 0));

    // Closing a tab; the last one is just emptied.
    h.get_by_label(&format!("Close tab {name_a}")).click();
    h.run_ok();
    assert_eq!(tab_names(&h), (vec![name_b.clone()], 0));
    assert!(h.state().repo().unwrap().path().contains(&name_b));
    press(&mut h, egui::Modifiers::COMMAND, Key::W);
    assert_eq!(tab_names(&h), (vec!["New tab".to_string()], 0));
    assert!(h.state().repo().is_none());
}

const POINTER: &str = "version https://git-lfs.github.com/spec/v1\noid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393\nsize 2400000\n";

fn extras_state(app: &App) -> (usize, Option<git_raccoon_core::modules::SubmoduleState>) {
    let e = &app.repo().unwrap().extras;
    (e.worktrees.len(), e.submodules.first().map(|s| s.state))
}

#[test]
fn shows_submodules_worktrees_and_lfs() {
    use git_raccoon_core::modules::SubmoduleState;
    let lib = Sample::build();
    let sample = Sample::build();
    git(
        &sample,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            lib.path(),
            "vendor/lib",
        ],
    );
    sample.write("art/cover.psd", POINTER);
    sample.write(
        ".gitattributes",
        "*.psd filter=lfs diff=lfs merge=lfs -text\n",
    );
    git(
        &sample,
        &[
            "-c",
            "filter.lfs.clean=cat",
            "-c",
            "filter.lfs.required=false",
            "add",
            ".",
        ],
    );
    git(
        &sample,
        &["commit", "-q", "-m", "Add the library and the cover art"],
    );
    // Where git-lfs is set up (as on CI machines), checking out the cover
    // art would try to download it from a server that doesn't exist.
    git(&sample, &["config", "filter.lfs.smudge", "cat"]);
    git(&sample, &["config", "filter.lfs.process", ""]);
    git(&sample, &["config", "filter.lfs.required", "false"]);
    // The submodule on another commit than recorded.
    git(&sample, &["-C", "vendor/lib", "checkout", "-q", "HEAD~1"]);

    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    assert_eq!(
        extras_state(h.state()),
        (1, Some(SubmoduleState::OtherCommit))
    );
    h.get_by_label_contains("LFS"); // the toolbar badge
    screenshot(&mut h, "41-submodules-worktrees");

    // Update the submodule.
    menu_item(&mut h, "vendor/lib", "Update");
    wait_for(&mut h, "the update", |app| {
        extras_state(app).1 == Some(SubmoduleState::UpToDate)
    });

    // The LFS file's diff says what it is.
    h.get_by_label("Add the library and the cover art").click();
    settle(&mut h);
    h.get_by_label_contains("art/cover.psd").click();
    settle(&mut h);
    h.get_by_label_contains("Stored with Git LFS");
    h.get_by_label_contains("2.3 MB");
    press(&mut h, egui::Modifiers::NONE, Key::Escape);

    // A worktree: add, then remove.
    let folder = std::env::temp_dir().join(format!("git-raccoon-wt-ui-{}", rand_suffix()));
    h.get_by_label("Add worktree").click();
    h.run_ok();
    h.get_by_label("Worktree folder").click();
    h.run_ok();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Worktree folder")
        .type_text(&folder.to_string_lossy());
    type_into(&mut h, "Worktree branch", "side-work");
    // What was typed is what's asked for (and the button is enabled).
    match h.state().repo().unwrap().dialog.as_ref() {
        Some(git_raccoon::repo_view::Dialog::Worktree {
            folder: f,
            branch,
            new,
        }) => {
            assert_eq!(
                (f.as_str(), branch.as_str(), *new),
                (&*folder.to_string_lossy(), "side-work", true)
            );
        }
        _ => panic!("the worktree dialog isn't open"),
    }
    // Through accessibility: a pointer click here was lost on a slow CI machine.
    h.get_by_label("Create worktree").click_accesskit();
    wait_for(&mut h, "the worktree", |app| extras_state(app).0 == 2);
    assert!(folder.join("README.md").exists());
    let name = folder.file_name().unwrap().to_string_lossy().to_string();
    menu_item(&mut h, &name, "Remove");
    h.get_by_label("Remove worktree").click();
    wait_for(&mut h, "the removal", |app| extras_state(app).0 == 1);
    assert!(!folder.exists());

    // The submodule opens in its own tab.
    menu_item(&mut h, "vendor/lib", "Open in a tab");
    wait_for(&mut h, "the submodule's tab", |app| {
        app.repo().is_some_and(|r| r.path().contains("vendor"))
    });
    assert_eq!(h.state().tab_names().0.len(), 2);
}

#[test]
fn offers_crash_reports_without_sending_them() {
    let dir = std::env::temp_dir().join(format!("git-raccoon-crashes-{}", rand_suffix()));
    std::fs::create_dir_all(&dir).unwrap();
    for (n, what) in [(1, "index out of bounds"), (2, "attempt to divide by zero")] {
        std::fs::write(
            dir.join(format!("crash-{n}.txt")),
            format!("Git Raccoon 0.5.0\nSystem: linux x86_64\nWhat: {what}\n"),
        )
        .unwrap();
    }
    let mut h = harness(Settings::default());
    h.state_mut().load_crash_reports(&dir);
    h.run_ok();
    h.get_by_label("Git Raccoon ran into a problem");
    h.get_by_label_contains("index out of bounds");
    h.get_by_label_contains("1 more after this one");
    screenshot(&mut h, "42-crash-report");

    // Copying keeps nothing; declining throws the next one away. Nothing is sent.
    h.get_by_label("Copy report").click();
    h.run_ok();
    h.get_by_label_contains("divide by zero");
    h.get_by_label("Don't send").click();
    h.run_ok();
    assert!(!h.state().has_crash_report());
    assert_eq!(
        std::fs::read_dir(&dir).unwrap().count(),
        0,
        "reports are removed once answered"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shows_the_licenses() {
    let mut h = harness(Settings::default());
    h.run_ok();
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    h.get_by_label("Licenses…").click();
    h.run_ok();
    h.get_by_label_contains("is open source, licensed under either of the MIT license");
    h.get_by_label("Libraries").click();
    h.run_ok();
    h.get_by_label("THIRD-PARTY NOTICES");
    h.get_by_label_contains("libgit2 (Git in C)");
    screenshot(&mut h, "43-licenses");
    h.get_by_label("Syntax themes").click();
    h.run_ok();
    h.get_by_label_contains("two-face");
    h.get_by_label("Close licenses").click();
    h.run_ok();
    assert!(h.query_by_label("Syntax themes").is_none(), "closed");
}
