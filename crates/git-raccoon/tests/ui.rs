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
use git_raccoon::settings::{PullChanges, PullStrategy, Settings};
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

/// A different number for each temporary folder: tests run side by side,
/// and the clock alone can give two of them the same one.
fn rand_suffix() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let count = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    (count << 32) | u64::from(nanos)
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
    // Seven commits and the stash.
    assert_eq!(repo.history.total(), 8 + 1);
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

    // A stash is in the graph too, just above the commit it was made on
    // (main's), and shows in the details pane with its own buttons.
    h.get_all_by_label_contains("half-done README tweak")
        .next()
        .unwrap()
        .click();
    settle(&mut h);
    let sel = h.state().repo().unwrap().selected.as_ref().unwrap();
    let row = sel.row.expect("a graph row");
    assert_eq!(h.state().repo().unwrap().history.total(), 9);
    h.get_by_label("README.md");
    h.get_by_label("Pop");
    let below = h
        .state()
        .repo()
        .unwrap()
        .rows
        .get(row + 1)
        .map(|c| c.summary.clone());
    assert_eq!(below.as_deref(), Some("Redraw icon and rename loader"));
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
    h.get_by_label("2 matches");
    // Enter steps through them, and the box says which one is showing.
    h.key_press(Key::Enter);
    settle(&mut h);
    assert_eq!(selected_summary(&h).as_deref(), Some("Colour lanes"));
    h.get_by_label("1 of 2");
    h.key_press(Key::Enter);
    settle(&mut h);
    assert_eq!(
        selected_summary(&h).as_deref(),
        Some("Lay out commit lanes")
    );
    h.get_by_label("2 of 2");
    screenshot(&mut h, "7-search");

    // File-path search.
    h.get_by_label("Search file paths").click();
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

    // Esc closes it, and the search with it, but nothing else (#4): the
    // commit picked stays open beside it.
    assert!(h.state().repo().unwrap().selected.is_some());
    h.get_by_label("Search commits").click();
    h.run_ok();
    h.key_press(Key::Escape);
    h.run_ok();
    assert!(h.query_by_label("Search commits").is_none());
    assert!(h.state().repo().unwrap().search_rows().is_empty());
    assert!(
        h.state().repo().unwrap().selected.is_some(),
        "the commit's details stay open"
    );
    // So does its close button.
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::F);
    h.run_ok();
    h.get_by_label("Close search (Esc)").click();
    h.run_ok();
    assert!(h.query_by_label("Search commits").is_none());

    // The toolbar's Search button opens it as Ctrl+F does, ready to type in.
    h.get_by_label("Search").click();
    h.run_ok();
    h.get_by_label("Search commits");
    h.event(egui::Event::Text("lanes".into()));
    h.run_ok();
    assert_eq!(h.state().repo().unwrap().search.query, "lanes");
}

#[test]
fn a_long_diff_fills_the_view_to_the_bottom() {
    let sample = Sample::build();
    // 300 new lines: green from top to bottom.
    let lines: String = (0..300).map(|i| format!("line {i}\n")).collect();
    sample.write("long.txt", lines);
    git(&sample, &["add", "long.txt"]);
    git(&sample, &["commit", "-q", "-m", "Add a long file"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    h.get_by_label("Add a long file").click();
    settle(&mut h);
    h.get_by_label("long.txt").click();
    settle(&mut h);

    for view in ["Unified", "Split"] {
        h.get_by_label(view).click();
        settle(&mut h);
        // The lines go on down to the bottom of the window, with no gap
        // left to scroll into (#38).
        let image = h.render().expect("render");
        let scale = h.ctx.pixels_per_point();
        let heading = h.get_by_label("Unified").rect();
        // Just left of the view buttons: on the added side when split.
        let x = ((heading.left() - 20.0) * scale) as u32;
        let bottom = image.get_pixel(x, ((h.ctx.content_rect().bottom() - 6.0) * scale) as u32);
        // An added line's background is green; the empty view's isn't.
        let [r, g, b, _] = bottom.0;
        assert!(
            g > r && g > b,
            "{view}: green down to the bottom, not {bottom:?}"
        );
    }
}

#[test]
fn side_by_side_diffs_scroll_sideways() {
    let sample = Sample::build();
    // A changed line whose only text, before and after, is far to the right.
    let wide = |text: &str| {
        format!(
            "{}{}
",
            " ".repeat(300),
            text.repeat(20)
        )
    };
    sample.write("wide.txt", wide("OLD"));
    git(&sample, &["add", "wide.txt"]);
    git(&sample, &["commit", "-q", "-m", "Add a wide file"]);
    sample.write("wide.txt", wide("NEW"));
    git(&sample, &["commit", "-q", "-am", "Change the wide file"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    h.get_by_label("Change the wide file").click();
    settle(&mut h);
    h.get_by_label("wide.txt").click();
    settle(&mut h);
    h.get_by_label("Split").click();
    settle(&mut h);
    // How much text shows in each half's code, under the hunk's heading:
    // (old, new).
    let text_shown = |h: &mut Harness<'_, App>| {
        let image = h.render().expect("render");
        let scale = h.ctx.pixels_per_point();
        let buttons = h.get_by_label("Unified").rect();
        let count = |from: f32| {
            let mut bright = 0;
            for x in from as u32..(from + 200.0) as u32 {
                for y in (buttons.bottom() + 40.0) as u32..(buttons.bottom() + 90.0) as u32 {
                    let [r, g, b, _] = image
                        .get_pixel((x as f32 * scale) as u32, (y as f32 * scale) as u32)
                        .0;
                    if r > 150 && g > 150 && b > 150 {
                        bright += 1;
                    }
                }
            }
            bright
        };
        (count(buttons.left() - 300.0), count(buttons.left() + 40.0))
    };
    assert_eq!(text_shown(&mut h), (0, 0), "the text is off to the right");

    // Each half has a scrollbar along the bottom, and they move together:
    // dragging the new half's to the end brings both halves' text in.
    let bottom = h.ctx.content_rect().bottom() - 4.0;
    let buttons = h.get_by_label("Unified").rect();
    drag(
        &mut h,
        egui::pos2(buttons.left() + 30.0, bottom),
        egui::pos2(buttons.left() + 2000.0, bottom),
    );
    settle(&mut h);
    // (The bars show while the pointer is over the view.)
    h.hover_at(buttons.left_bottom() + egui::vec2(100.0, 300.0));
    settle(&mut h);
    screenshot(&mut h, "38-split-sideways");
    let (old, new) = text_shown(&mut h);
    assert!(
        old > 50 && new > 50,
        "both scrolled to the text: {old}, {new}"
    );

    // The horizontal wheel over the old half moves both too: back to the start.
    let over = buttons.left_bottom() + egui::vec2(-200.0, 120.0);
    h.event(egui::Event::PointerMoved(over));
    h.run_ok();
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(100_000.0, 0.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    settle(&mut h);
    assert_eq!(text_shown(&mut h), (0, 0), "both back at the start");
}

#[test]
fn moves_through_the_graph_with_modifier_keys() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let press = |h: &mut Harness<'_, App>, mods: egui::Modifiers, key: Key| {
        h.key_press_modifiers(mods, key);
        settle(h);
    };
    let at = |h: &Harness<'_, App>| selected_summary(h).unwrap_or_default();

    // Shift follows the branch: from the merge, down its own line (main),
    // not into the branch it took in, which is the next row.
    h.get_by_label("Merge branch 'feature/graph'").click();
    settle(&mut h);
    press(&mut h, egui::Modifiers::NONE, Key::ArrowDown);
    assert_eq!(at(&h), "Colour lanes");
    press(&mut h, egui::Modifiers::NONE, Key::ArrowUp);
    press(&mut h, egui::Modifiers::SHIFT, Key::ArrowDown);
    assert_eq!(at(&h), "Reject bare repositories when loading");
    // And back up the same line.
    press(&mut h, egui::Modifiers::SHIFT, Key::ArrowUp);
    assert_eq!(at(&h), "Merge branch 'feature/graph'");
    // Along feature/graph, up to where it ends: the merge isn't its own.
    h.get_by_label("Lay out commit lanes").click();
    settle(&mut h);
    press(&mut h, egui::Modifiers::SHIFT, Key::ArrowUp);
    assert_eq!(at(&h), "Colour lanes");

    // Ctrl goes from label to label.
    h.get_by_label("Add icon").click();
    settle(&mut h);
    press(&mut h, egui::Modifiers::COMMAND, Key::ArrowDown);
    assert_eq!(at(&h), "Merge branch 'feature/graph'", "tagged v0.1.0");
    press(&mut h, egui::Modifiers::COMMAND, Key::ArrowDown);
    assert_eq!(at(&h), "Colour lanes", "feature/graph");
    press(&mut h, egui::Modifiers::COMMAND, Key::ArrowUp);
    assert_eq!(at(&h), "Merge branch 'feature/graph'");

    // Home and End: the newest and oldest; Ctrl+H: the checked-out commit.
    press(&mut h, egui::Modifiers::NONE, Key::End);
    assert_eq!(at(&h), "Add repository loader");
    press(&mut h, egui::Modifiers::NONE, Key::Home);
    assert_eq!(at(&h), "Fix toolbar glitch");
    press(&mut h, egui::Modifiers::COMMAND, Key::H);
    assert_eq!(at(&h), "Redraw icon and rename loader");
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
    while h.state().repo().unwrap().history.total() == 8 + 1 {
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "no refresh after an external commit"
        );
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    settle(&mut h);
    assert_eq!(h.state().repo().unwrap().history.total(), 9 + 1);
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
fn unstaged_and_staged_share_the_pane_like_the_sidebar() {
    let sample = Sample::build();
    sample.write("notes.txt", "one\n");
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    wait_for(&mut h, "the WIP row", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| !s.unstaged.is_empty())
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);
    // With a file or two, the line between the lists is still halfway down,
    // not right under the last unstaged file.
    let line = |h: &Harness<'_, App>| h.get_by_label("Resize Unstaged files").rect().center().y;
    let bottom = h.get_by_label("Commit summary").rect().top();
    let top = h.get_by_label("Working changes").rect().bottom();
    let middle = (top + bottom) / 2.0;
    assert!((line(&h) - middle).abs() < 60.0, "{} vs {middle}", line(&h));
    screenshot(&mut h, "83-working-lists");
    // Dragging it moves it; a double-click puts it back halfway.
    let at = egui::pos2(
        h.get_by_label("Resize Unstaged files").rect().center().x,
        line(&h),
    );
    drag(&mut h, at, at - egui::vec2(0.0, 80.0));
    settle(&mut h);
    let moved = line(&h);
    assert!((moved - (at.y - 80.0)).abs() < 8.0, "{moved}");
    double_click_at(&mut h, egui::pos2(at.x, moved));
    settle(&mut h);
    assert!((line(&h) - at.y).abs() < 2.0, "back halfway");
    // Fold the staged list: its heading goes down to the commit box, and
    // the unstaged list takes the room.
    h.get_by_label("Hide staged files").click();
    settle(&mut h);
    assert!(
        h.query_by_label("Resize Unstaged files").is_none(),
        "no line to drag"
    );
    // (Below it: the commit box's own heading, then its summary field.)
    let staged = h.get_by_label("Show staged files").rect();
    assert!(bottom - staged.bottom() < 80.0, "{staged:?} vs {bottom}");
    // And the unstaged list: the staged one comes up under its heading.
    h.get_by_label("Show staged files").click();
    h.get_by_label("Hide unstaged files").click();
    settle(&mut h);
    let unstaged = h.get_by_label("Show unstaged files").rect();
    let staged = h.get_by_label("Hide staged files").rect();
    assert!(
        staged.top() - unstaged.bottom() < 30.0,
        "{unstaged:?} {staged:?}"
    );
}

#[test]
fn a_long_commit_description_scrolls_and_the_box_resizes() {
    let sample = Sample::build();
    sample.write("notes.txt", "one\n");
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    wait_for(&mut h, "the WIP row", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| !s.unstaged.is_empty())
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);
    let summary = |h: &Harness<'_, App>| h.get_by_label("Commit summary").rect();
    // Steady from the start (it once crept down a little every frame).
    let first = summary(&h);
    for _ in 0..5 {
        h.step();
    }
    assert_eq!(summary(&h), first, "the box keeps its size");
    let before = summary(&h);
    let staged = h.get_by_label("Hide staged files").rect();
    // Sixty lines: the box stays as it is, and the description scrolls.
    let long: String = (1..=60).map(|i| format!("line {i}\n")).collect();
    type_into(&mut h, "Commit description", &long);
    settle(&mut h);
    assert_eq!(summary(&h), before, "the box didn't grow");
    assert_eq!(
        h.get_by_label("Hide staged files").rect(),
        staged,
        "the lists are where they were"
    );
    let field = h.get_by_label("Commit description").rect();
    let button = h.get_by_label_contains("Commit 0 files").rect();
    assert!(
        button.top() > field.top(),
        "the button is still below the description"
    );
    screenshot(&mut h, "84-long-description");
    // Its top edge drags: find it by the pointer it shows.
    let x = before.center().x;
    let edge = (0..80)
        .map(|d| before.top() - d as f32)
        .find(|&y| {
            h.hover_at(egui::pos2(x, y));
            h.run_ok();
            h.output().platform_output.cursor_icon == egui::CursorIcon::ResizeVertical
        })
        .expect("a resize edge above the commit box");
    drag(&mut h, egui::pos2(x, edge), egui::pos2(x, edge - 100.0));
    settle(&mut h);
    assert!(
        summary(&h).top() < before.top() - 80.0,
        "taller: {:?}",
        summary(&h)
    );
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
    h.get_by_label("Select line: line 2 edited").click();
    h.run_ok();
    assert_eq!(h.state().repo().unwrap().picked.len(), 1);
    screenshot(&mut h, "10-pick-line");
    // From its right-click menu.
    h.get_by_label("Select line: line 2 edited")
        .click_secondary();
    h.run_ok();
    screenshot(&mut h, "10b-line-menu");
    h.get_by_label("Stage selection").click_accesskit();
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

    // The staged list folds away, and back.
    h.get_by_label("Hide staged files").click();
    h.run_ok();
    assert!(h.query_by_label("README.md").is_none());
    h.get_by_label("Show staged files").click();
    h.run_ok();
    h.get_by_label("README.md");

    // As folders: notes/ holds the forty files, and folds them away.
    h.get_by_label("Show folders").click();
    h.run_ok();
    assert!(h.state().settings().file_tree, "remembered");
    h.get_by_label("notes/");
    h.get_by_label("notes/00.txt");
    screenshot(&mut h, "46-folders");
    h.get_by_label("notes/").click();
    h.run_ok();
    assert!(h.query_by_label("notes/00.txt").is_none());
    h.get_by_label("Show paths").click();
    h.run_ok();
    assert!(!h.state().settings().file_tree);

    // Discard all, after asking: staged, unstaged and new files.
    let bin = sample.dir.with_extension("bin");
    h.state_mut().repo_mut().unwrap().trash_override = Some(bin.clone());
    h.get_by_label("Discard all changes").click();
    h.run_ok();
    h.get_by_label("Discard all changes?");
    h.get_by_label("Discard all").click();
    wait_for(&mut h, "a clean tree", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.is_clean())
    });
    assert!(bin.join("README.md").exists(), "kept in the bin");
    assert!(bin.join("00.txt").exists());
    let _ = std::fs::remove_dir_all(&bin);
}

#[test]
fn picks_several_files_and_acts_on_them_together() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    for i in 0..6 {
        sample.write(&format!("notes/{i:02}.txt"), "new\n");
    }
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let bin = sample.dir.with_extension("bin");
    h.state_mut().repo_mut().unwrap().trash_override = Some(bin.clone());
    let status = |app: &App| app.repo().unwrap().status.clone().unwrap_or_default();
    let names = |files: &[git_raccoon_core::working::WorkingFile]| {
        files.iter().map(|f| f.path.clone()).collect::<Vec<_>>()
    };
    wait_for(&mut h, "the new files", |app| {
        status(app).unstaged.len() == 6
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);

    // One file picked is just the one open: the buttons stay as they are.
    h.get_by_label("notes/00.txt").click();
    h.run_ok();
    h.get_by_label_contains("Stage all");
    h.get_by_label("Discard all changes");
    // Ctrl+click picks a second file; right-click stages both.
    h.get_by_label("notes/02.txt")
        .click_modifiers(egui::Modifiers::COMMAND);
    h.run_ok();
    h.get_by_label("notes/02.txt").click_secondary();
    h.run_ok();
    screenshot(&mut h, "47-file-menu");
    // The menu offers it, and so does the list's button, now about the two.
    assert_eq!(h.get_all_by_label_contains("Stage 2 files").count(), 2);
    h.key_press(Key::Escape);
    h.run_ok();
    h.get_by_label("Discard changes to the 2 picked files");
    h.get_by_label_contains("Stage 2 files").click();
    wait_for(&mut h, "two staged", |app| status(app).staged.len() == 2);
    assert_eq!(
        names(&status(h.state()).staged),
        ["notes/00.txt", "notes/02.txt"]
    );

    // Shift+click picks a range; they're ignored.
    h.get_by_label("notes/01.txt").click();
    h.run_ok();
    h.get_by_label("notes/04.txt")
        .click_modifiers(egui::Modifiers::SHIFT);
    h.run_ok();
    h.get_by_label("notes/04.txt").click_secondary();
    h.run_ok();
    h.get_by_label_contains("Ignore 3 new files").click();
    wait_for(&mut h, "ignored", |app| {
        names(&status(app).unstaged) == [".gitignore", "notes/05.txt"]
    });
    assert_eq!(
        read(&sample, ".gitignore").as_deref(),
        Some("/notes/01.txt\n/notes/03.txt\n/notes/04.txt\n")
    );

    // A folder's menu, in the folder view: stash what's in notes/.
    h.get_by_label("Show folders").click();
    h.run_ok();
    h.get_all_by_label("notes/")
        .next()
        .unwrap()
        .click_secondary();
    h.run_ok();
    h.get_by_label_contains("Stash notes/").click();
    h.run_ok();
    h.get_by_role_and_label(Role::Button, "Stash").click();
    wait_for(&mut h, "stashed", |app| {
        names(&status(app).unstaged) == [".gitignore"]
    });
    assert_eq!(read(&sample, "notes/05.txt"), None);

    // Discarding staged files asks first, then they're gone (to the bin).
    let staged = h.get_all_by_label("notes/").last().unwrap();
    staged.click_secondary();
    h.run_ok();
    h.get_by_label_contains("Discard changes to notes/").click();
    h.run_ok();
    h.get_by_role_and_label(Role::Button, "Discard").click();
    wait_for(&mut h, "discarded", |app| status(app).staged.is_empty());
    assert_eq!(read(&sample, "notes/00.txt"), None);
    assert!(bin.join("00.txt").exists());
    let _ = std::fs::remove_dir_all(&bin);
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
    // The summary written in the WIP row is the commit's, and titles the stash.
    // Its field starts small, and grows with what's written.
    let empty = h.get_by_label("WIP summary").rect().width();
    type_into(&mut h, "WIP summary", "try another approach");
    settle(&mut h);
    let written = h.get_by_label("WIP summary").rect().width();
    assert!(written > empty, "{empty} grew to {written}");
    h.get_by_label("WIP summary")
        .type_text(&" and then another, and another".repeat(10));
    settle(&mut h);
    let long = h.get_by_label("WIP summary").rect().width();
    assert!(long > written && long <= 520.5, "capped: {long}");
    h.get_by_label("WIP summary").click();
    h.run_ok();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("WIP summary")
        .type_text("try another approach");
    settle(&mut h);
    assert!(
        h.state().repo().unwrap().working_selected,
        "writing there shows the changes"
    );
    assert_eq!(
        h.state().repo().unwrap().draft.summary,
        "try another approach"
    );

    h.get_by_label("Stash changes").click();
    h.run_ok();
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

    // Pop it from the details pane (it's in the sidebar, and in the graph).
    h.get_all_by_label_contains("try another approach")
        .next()
        .unwrap()
        .click();
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
    h.get_all_by_label_contains("half-done README tweak")
        .next()
        .unwrap()
        .click();
    settle(&mut h);
    h.get_by_label("Delete").click();
    h.run_ok();
    h.get_by_label("Delete this stash?");
    h.get_by_label("Delete stash").click();
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
fn scrolls_on_over_labels_without_spreading_them_out() {
    let sample = Sample::build();
    // A long history with two labels on every fifth commit: rows whose
    // labels would spread out pass under the pointer as the list scrolls.
    for i in 0..60 {
        git(
            &sample,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                &format!("Step {i:02}"),
            ],
        );
        if i % 5 == 0 {
            git(&sample, &["branch", &format!("pair-a-{i:02}")]);
            git(&sample, &["branch", &format!("pair-b-{i:02}")]);
        }
    }
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    // Spread out, a row's labels are listed over the graph (the sidebar
    // lists the branches too, further left).
    let spread = |h: &Harness<'_, App>, i: usize| {
        h.query_all_by_label(&format!("pair-b-{i:02}"))
            .any(|n| n.rect().center().x > 230.0)
    };

    // Over the labels of a row that has two: they spread out.
    let row = h.get_by_label("Step 50").rect();
    let labels = egui::pos2(row.left() + 30.0, row.center().y);
    h.hover_at(labels);
    h.run_ok();
    assert!(spread(&h, 50), "spread out");

    // The wheel there scrolls the list, and keeps scrolling it.
    let mut last = h.get_by_label("Step 50").rect().top();
    for _ in 0..4 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        // (A step is a quarter of a second.)
        h.step();
        // Labels passing under the pointer stay in their rows.
        assert!(
            !(0..60).step_by(5).any(|i| spread(&h, i)),
            "nothing spread out while scrolling"
        );
        h.step();
        let top = h
            .query_by_label("Step 50")
            .map_or(f32::NEG_INFINITY, |n| n.rect().top());
        assert!(top < last - 1.0, "scrolled on: {last} -> {top}");
        last = top;
        // Settled: labels the pointer rests on may spread out again, and
        // the next turn of the wheel starts over them.
        for _ in 0..4 {
            h.step();
        }
    }
}

fn right_click_at(h: &mut Harness<'_, App>, pos: egui::Pos2) {
    h.hover_at(pos);
    h.run_ok();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_ok();
    }
}

#[test]
fn right_clicking_a_label_on_the_graph_shows_its_menu() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let label = |h: &Harness<'_, App>, summary: &str| {
        let row = h.get_by_label(summary).rect();
        egui::pos2(row.left() + 30.0, row.center().y)
    };

    // A branch's label: what its menu in the sidebar has, then the
    // commit's (#27).
    let at = label(&h, "Colour lanes");
    right_click_at(&mut h, at);
    h.get_by_label("Merge into main");
    h.get_by_label("Rename…");
    h.get_by_label_contains("Create branch here…");
    h.get_by_label_contains("Interactive rebase from here…");
    screenshot(&mut h, "45-label-menu");
    h.get_by_label("Check out feature/graph").click();
    wait_for(&mut h, "the checkout", |app| {
        head_branch(app).as_deref() == Some("feature/graph")
    });

    // A tag's too.
    let at = label(&h, "Merge branch 'feature/graph'");
    right_click_at(&mut h, at);
    h.get_by_label("Delete…");
    h.get_by_label_contains("Create tag here…").click();
    settle(&mut h);
    assert!(matches!(
        h.state().repo().unwrap().dialog,
        Some(git_raccoon::repo_view::Dialog::NewTag { .. })
    ));

    // And each of several labels, spread out.
    h.key_press(Key::Escape);
    settle(&mut h);
    git(&sample, &["branch", "release", "HEAD"]);
    h.key_press(Key::F5);
    settle(&mut h);
    let row = h.get_by_label("Colour lanes").rect();
    h.hover_at(egui::pos2(row.left() + 30.0, row.center().y));
    h.run_ok();
    let release = h
        .query_all_by_label("release")
        .map(|n| n.rect().center())
        .find(|p| p.x > 230.0)
        .unwrap();
    right_click_at(&mut h, release);
    h.get_by_label_contains("Create branch here…");
    h.get_by_label("Delete…").click();
    settle(&mut h);
    assert!(
        matches!(
            h.state().repo().unwrap().dialog,
            Some(git_raccoon::repo_view::Dialog::Delete { .. })
        ),
        "asks before deleting release"
    );

    // Labels under a window or a menu stay as they are: spread out, they'd
    // cover it.
    let spread = |h: &Harness<'_, App>| {
        h.query_all_by_label("release")
            .any(|n| n.rect().center().x > 230.0)
    };
    let colour_lanes = label(&h, "Colour lanes");
    h.hover_at(colour_lanes);
    h.run_ok();
    assert!(!spread(&h), "not under the question");
    h.key_press(Key::Escape);
    settle(&mut h);
    let at = label(&h, "Redraw icon and rename loader");
    right_click_at(&mut h, at);
    h.get_by_label("Rename…");
    h.hover_at(colour_lanes);
    h.run_ok();
    assert!(!spread(&h), "not under the menu");
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
fn sidebar_sections_scroll_on_their_own_and_resize() {
    let sample = Sample::build();
    for i in 0..80 {
        git(&sample, &["branch", &format!("topic-{i:02}"), "HEAD~1"]);
    }
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    let screen = h.ctx.content_rect().bottom();
    // Every section's heading stays in view, with 80 branches above them.
    for heading in ["Worktrees", "Stashes", "Tags", "Remote"] {
        let r = h.get_by_label(heading).rect();
        assert!(r.bottom() <= screen, "{heading} is in view: {r:?}");
    }
    screenshot(&mut h, "57-sidebar-sections");
    // The sidebar as a whole doesn't scroll: wheeling over a heading
    // leaves everything where it is.
    let tags = h.get_by_label("Tags").rect();
    h.event(egui::Event::PointerMoved(tags.center()));
    h.run_ok();
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -400.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    settle(&mut h);
    assert_eq!(h.get_by_label("Tags").rect(), tags, "nothing moved");
    // Local scrolls inside itself: its last branch isn't drawn yet.
    assert!(
        h.query_by_label("topic-79")
            .is_none_or(|n| n.rect().top() > screen
                || n.rect().bottom() < h.get_by_label("Remote").rect().top())
    );
    // Dragging the handle under it makes it shorter.
    let handle = h.get_by_label("Resize Local").rect().center();
    let remote_before = h.get_by_label("Remote").rect().top();
    drag(&mut h, handle, handle - egui::vec2(0.0, 150.0));
    settle(&mut h);
    let remote_after = h.get_by_label("Remote").rect().top();
    assert!(
        remote_after < remote_before - 100.0,
        "{remote_before} -> {remote_after}"
    );
}

#[test]
fn sidebar_sections_share_the_height_and_collapse_in_place() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    let bottom = h.ctx.content_rect().bottom();
    let top = |h: &Harness<'_, App>, s: &str| h.get_by_label(s).rect().top();
    // A handle moves just the line between its two sections: the ones
    // below stay where they are, in every frame of the drag.
    let (remote, tags, stashes) = (top(&h, "Remote"), top(&h, "Tags"), top(&h, "Stashes"));
    let from = h.get_by_label("Resize Local").rect().center();
    h.event(egui::Event::PointerMoved(from));
    h.run_ok();
    h.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_ok();
    for i in 1..=12 {
        #[expect(clippy::cast_precision_loss, reason = "a few steps")]
        h.event(egui::Event::PointerMoved(
            from - egui::vec2(0.0, 5.0 * i as f32),
        ));
        h.step();
        assert_eq!(
            top(&h, "Tags"),
            tags,
            "Tags stays put while dragging (step {i})"
        );
        assert_eq!(top(&h, "Stashes"), stashes, "Stashes too (step {i})");
    }
    h.event(egui::Event::PointerButton {
        pos: from - egui::vec2(0.0, 60.0),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    settle(&mut h);
    assert!(top(&h, "Remote") < remote - 40.0, "Remote moved up");
    assert_eq!(top(&h, "Tags"), tags, "Tags stayed");
    assert_eq!(top(&h, "Stashes"), stashes, "Stashes stayed");
    // A collapsed section stays in its place, its heading right on top of
    // the next section's; the open ones fill the space it leaves.
    h.get_by_label("Tags").click();
    settle(&mut h);
    let (remote, tags, stashes) = (
        h.get_by_label("Remote").rect(),
        h.get_by_label("Tags").rect(),
        h.get_by_label("Stashes").rect(),
    );
    assert!(remote.top() < tags.top(), "Tags is still below Remote");
    screenshot(&mut h, "71a-sidebar-tags-collapsed");
    assert!(
        (stashes.top() - tags.bottom()).abs() < 1.0,
        "Tags sits on Stashes: {tags:?} {stashes:?}"
    );
    let last = h.get_by_label("Worktrees").rect();
    assert!(
        (bottom - last.bottom()).abs() > 40.0,
        "Worktrees is open, with room below its heading"
    );
    // With them all collapsed, the headings stack up at the top, in order.
    let local = top(&h, "Local");
    for s in ["Local", "Remote", "Stashes", "Worktrees"] {
        h.get_by_label(s).click();
        settle(&mut h);
    }
    assert_eq!(top(&h, "Local"), local, "Local stays at the top");
    let heads: Vec<f32> = ["Local", "Remote", "Tags", "Stashes", "Worktrees"]
        .iter()
        .map(|s| top(&h, s))
        .collect();
    assert!(
        heads.windows(2).all(|w| (w[1] - w[0] - 28.0).abs() < 1.0),
        "{heads:?}"
    );
    screenshot(&mut h, "71-sidebar-collapsed");
}

#[test]
fn branches_can_be_pinned_to_the_top() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let top = |h: &Harness<'_, App>, label: &str| h.get_by_label(label).rect().top();
    // Pin two: they move to the top of Local, by name, and out of their
    // folders.
    for name in ["toolbar-glitch", "graph"] {
        h.get_by_label(name).click_secondary();
        h.run_ok();
        h.get_by_label("Pin to top").click();
        h.run_ok();
    }
    let pinned = h
        .state()
        .settings()
        .pinned_branches
        .get(sample.path())
        .cloned();
    assert_eq!(
        pinned,
        Some(vec![
            "fix/ui/toolbar-glitch".to_string(),
            "feature/graph".to_string()
        ])
    );
    assert!(
        top(&h, "feature/graph") < top(&h, "fix/ui/toolbar-glitch"),
        "by name"
    );
    let main = h
        .get_all_by_label("main")
        .map(|n| n.rect().top())
        .fold(f32::MAX, f32::min);
    assert!(top(&h, "fix/ui/toolbar-glitch") < main);
    assert!(
        h.query_by_label("toolbar-glitch").is_none(),
        "not in its folder too"
    );
    screenshot(&mut h, "85-pinned-branches");
    // Unpinned: back in its folder.
    h.get_by_label("feature/graph").click_secondary();
    h.run_ok();
    h.get_by_label("Unpin").click();
    h.run_ok();
    h.get_by_label("graph");
    assert_eq!(
        h.state()
            .settings()
            .pinned_branches
            .get(sample.path())
            .cloned(),
        Some(vec!["fix/ui/toolbar-glitch".to_string()])
    );
}

#[test]
fn sidebar_sections_can_be_hidden() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    // Right-click a heading, and untick Tags.
    h.get_by_label("Stashes").click_secondary();
    h.run_ok();
    screenshot(&mut h, "73-sidebar-menu");
    h.get_by_role_and_label(egui::accesskit::Role::CheckBox, "Tags")
        .click();
    settle(&mut h);
    assert!(h.query_by_label("Tags").is_none(), "Tags is hidden");
    assert_eq!(
        h.state().settings().sidebar_hidden,
        vec!["tags".to_string()]
    );
    // The same switches in the settings.
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    settle(&mut h);
    type_into(&mut h, "Search settings", "sidebar");
    settle(&mut h);
    screenshot(&mut h, "74-settings-sidebar");
    h.key_press(Key::Escape);
    settle(&mut h);
    // And back.
    menu_item(&mut h, "Local", "Show all sections");
    settle(&mut h);
    assert!(h.query_by_label("Tags").is_some(), "Tags is back");
    assert!(h.state().settings().sidebar_hidden.is_empty());
}

#[test]
fn hunks_stage_from_the_whole_file() {
    let sample = Sample::build();
    sample.write("notes.txt", numbered(|_| None));
    sample.commit("Add notes", 1);
    let mut h = harness(Settings {
        whole_file: true,
        ..Settings::default()
    });
    open(&mut h, sample.path());
    sample.write(
        "notes.txt",
        numbered(|i| match i {
            2 => Some("line 2 edited"),
            18 => Some("line 18 edited"),
            _ => None,
        }),
    );
    wait_for(&mut h, "the change", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.unstaged.len() == 1)
    });
    h.get_by_label("Working changes").click();
    settle(&mut h);
    h.get_by_label("notes.txt").click();
    settle(&mut h);
    assert_eq!(h.get_all_by_label("Stage hunk").count(), 2);
    screenshot(&mut h, "76-whole-file-staging");
    h.get_all_by_label("Stage hunk").nth(1).unwrap().click();
    wait_for(&mut h, "the second hunk staged", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.staged.len() == 1)
    });
    assert_eq!(
        staged(&sample, "notes.txt").unwrap(),
        numbered(|i| (i == 18).then_some("line 18 edited"))
    );
}

#[test]
fn fetches_in_the_background_when_its_time() {
    let sample = Sample::build();
    // A remote with a commit the sample doesn't have yet.
    let bare = sample.dir.with_extension("auto.git");
    git(&sample, &["init", "-q", "--bare", bare.to_str().unwrap()]);
    git(&sample, &["push", "-q", bare.to_str().unwrap(), "main"]);
    let other = sample.dir.with_extension("other");
    git(
        &sample,
        &[
            "clone",
            "-q",
            "-b",
            "main",
            bare.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    std::fs::write(other.join("later.txt"), "later\n").unwrap();
    for args in [
        &["add", "later.txt"][..],
        &[
            "-c",
            "user.name=A",
            "-c",
            "user.email=a@example.com",
            "commit",
            "-qm",
            "Later on",
        ],
        &["push", "-q"],
    ] {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(&other)
            .status()
            .unwrap();
        assert!(ok.success());
    }
    git(
        &sample,
        &["remote", "set-url", "origin", bare.to_str().unwrap()],
    );
    let mut h = harness(Settings {
        auto_fetch_minutes: 5,
        ..Settings::default()
    });
    open(&mut h, sample.path());
    settle(&mut h);
    let behind = |app: &App| {
        app.repo()
            .unwrap()
            .refs
            .local
            .iter()
            .find(|b| b.name == "main")
            .and_then(|b| b.upstream.as_ref())
            .map_or(0, |u| u.behind)
    };
    assert_eq!(behind(h.state()), 0, "not fetched yet: not due");
    h.state_mut().repo_mut().unwrap().fetch_due_now();
    wait_for(&mut h, "the background fetch", |app| behind(app) == 1);
    let _ = std::fs::remove_dir_all(&other);
    let _ = std::fs::remove_dir_all(&bare);
}

#[test]
fn checks_for_updates_from_settings() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    h.get_by_role_and_label(Role::Button, "Updates and privacy")
        .click();
    h.run_ok();
    // A copy run from the build (not installed) can't update itself: the
    // button is there, but off, and says why.
    let button = h.get_by_role_and_label(Role::Button, "Check for updates now");
    assert!(button.accesskit_node().is_disabled(), "not installed");
    h.get_by_label_contains("wasn't installed");
    screenshot(&mut h, "86-check-for-updates");
}

#[test]
fn the_fetch_interval_is_a_setting() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    assert_eq!(
        h.state().settings().auto_fetch_minutes,
        10,
        "every 10 minutes to start with"
    );
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    type_into(&mut h, "Search settings", "fetch");
    h.get_by_label("Fetch interval in minutes");
    screenshot(&mut h, "79-fetch-interval");
}

#[test]
fn a_diff_can_show_the_whole_file() {
    let sample = Sample::build();
    let file = Path::new(sample.path()).join("long.txt");
    let old: String = (1..=200).map(|n| format!("line {n}\n")).collect();
    std::fs::write(&file, &old).unwrap();
    git(&sample, &["add", "long.txt"]);
    git(&sample, &["commit", "-qm", "Add a long file"]);
    std::fs::write(&file, old.replace("line 150\n", "line 150, changed\n")).unwrap();
    git(&sample, &["commit", "-qam", "Change line 150"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    h.get_by_label("Change line 150").click();
    settle(&mut h);
    h.get_by_label("long.txt").click();
    settle(&mut h);
    let rows = |h: &Harness<'_, App>| {
        let repo = h.state().repo().unwrap();
        let key = repo.diff_key().unwrap();
        match repo.diff.get(&key) {
            Some(Ok(d)) => d.unified.len(),
            _ => 0,
        }
    };
    // Just the change: a header, three lines either side, and the change.
    assert_eq!(rows(&h), 9);
    h.get_by_label("Whole file").click();
    settle(&mut h);
    assert!(h.state().settings().whole_file, "remembered");
    // Every line, with the changed one twice (before and after), no header.
    assert_eq!(rows(&h), 201);
    screenshot(&mut h, "72-whole-file");
    h.get_by_label("Split").click();
    settle(&mut h);
    screenshot(&mut h, "75-whole-file-split");
    h.get_by_label("Unified").click();
    settle(&mut h);
    h.get_by_label("Changes").click();
    settle(&mut h);
    assert_eq!(rows(&h), 9);
}

#[test]
fn the_side_panels_are_as_wide_as_you_drag_them() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    let screen = h.ctx.content_rect();
    // The sidebar, well past the old 420 limit.
    let local = h.get_by_label("Local").rect();
    let edge = egui::pos2(local.right() + 1.0, 600.0);
    drag(&mut h, edge, edge + egui::vec2(400.0, 0.0));
    settle(&mut h);
    let wide = h.get_by_label("Local").rect().width();
    assert!(wide > 600.0, "sidebar {wide}");
    // Back narrower, for room for the details.
    let edge = egui::pos2(h.get_by_label("Local").rect().right() + 1.0, 600.0);
    drag(&mut h, edge, edge - egui::vec2(450.0, 0.0));
    settle(&mut h);
    let narrow = h.get_by_label("Local").rect().width();
    assert!(narrow < 200.0, "sidebar {narrow}");
    // The details pane, past its old 640 limit.
    h.get_by_label("Add icon").click();
    settle(&mut h);
    let details_left = |h: &Harness<'_, App>| {
        h.get_all_by_label("Add icon")
            .map(|n| n.rect().left())
            .fold(f32::MIN, f32::max)
    };
    let left = details_left(&h);
    let edge = egui::pos2(left - 10.0, 700.0);
    drag(&mut h, edge, edge - egui::vec2(400.0, 0.0));
    settle(&mut h);
    let width = screen.right() - details_left(&h);
    assert!(width > 660.0, "details {width}");
    screenshot(&mut h, "60-wide-panels");
    // The graph always keeps some room.
    let edge = egui::pos2(details_left(&h) - 10.0, 700.0);
    drag(&mut h, edge, egui::pos2(0.0, 700.0));
    settle(&mut h);
    let graph = details_left(&h) - h.get_by_label("Local").rect().right();
    assert!(graph >= 300.0, "graph {graph}");
}

#[test]
fn deletes_a_tag_here_on_the_remote_or_both() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    let bare = sample.dir.with_extension("tags.git");
    git(&sample, &["init", "-q", "--bare", bare.to_str().unwrap()]);
    git(
        &sample,
        &["remote", "set-url", "origin", bare.to_str().unwrap()],
    );
    git(&sample, &["push", "-q", "origin", "main", "v0.1.0"]);
    let there = || {
        let out = std::process::Command::new("git")
            .args(["ls-remote", "--tags", bare.to_str().unwrap(), "v0.1.0"])
            .output()
            .unwrap();
        !String::from_utf8_lossy(&out.stdout).trim().is_empty()
    };
    let here = |app: &App| {
        app.repo()
            .unwrap()
            .refs
            .tags
            .iter()
            .any(|t| t.name == "v0.1.0")
    };
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    // Delete…, then tick where: Local starts ticked, the remotes don't.
    let delete = |h: &mut Harness<'_, App>, toggle: &[&str]| {
        menu_item(h, "v0.1.0", "Delete…");
        for place in toggle {
            h.get_by_role_and_label(Role::CheckBox, place).click();
            h.run_ok();
        }
        h.get_by_role_and_label(Role::Button, "Delete").click();
        settle(h);
    };

    // Only on the remote: kept here.
    menu_item(&mut h, "v0.1.0", "Delete…");
    h.get_by_role_and_label(Role::CheckBox, "Local").click();
    h.get_by_role_and_label(Role::CheckBox, "origin").click();
    h.run_ok();
    screenshot(&mut h, "81-delete-tag");
    h.get_by_role_and_label(Role::Button, "Delete").click();
    settle(&mut h);
    assert!(!there(), "gone from origin");
    assert!(here(h.state()), "still here");
    // Undo puts it back there.
    h.get_by_label("Undo").click();
    settle(&mut h);
    assert!(there(), "back on origin");

    // Here and on the remote; undo brings both back.
    delete(&mut h, &["origin"]);
    assert!(!there() && !here(h.state()), "gone from both");
    h.get_by_label("Undo").click();
    settle(&mut h);
    assert!(there() && here(h.state()), "back in both");

    // Only here, as before.
    delete(&mut h, &[]);
    assert!(there() && !here(h.state()));
    let _ = std::fs::remove_dir_all(&bare);
}

#[test]
fn a_branch_label_shared_with_its_remote_copy_deletes_either_or_both() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    let bare = sample.dir.with_extension("shared.git");
    git(&sample, &["init", "-q", "--bare", bare.to_str().unwrap()]);
    git(
        &sample,
        &["remote", "set-url", "origin", bare.to_str().unwrap()],
    );
    // feature/graph is merged into main; "solo" has a commit nowhere else.
    git(&sample, &["push", "-q", "origin", "main", "feature/graph"]);
    git(&sample, &["checkout", "-q", "-b", "solo"]);
    std::fs::write(sample.dir.join("solo.txt"), "solo\n").unwrap();
    git(&sample, &["add", "solo.txt"]);
    git(&sample, &["commit", "-qm", "Solo work"]);
    git(&sample, &["push", "-q", "origin", "solo"]);
    git(&sample, &["checkout", "-q", "main"]);
    let there = |b: &str| {
        let out = std::process::Command::new("git")
            .args(["ls-remote", "--heads", bare.to_str().unwrap(), b])
            .output()
            .unwrap();
        !String::from_utf8_lossy(&out.stdout).trim().is_empty()
    };
    let here = |app: &App, b: &str| app.repo().unwrap().refs.local.iter().any(|l| l.name == b);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    // On the graph, feature/graph and origin/feature/graph are one label.
    // A commit's label on the graph, by its row.
    let open_menu = |h: &mut Harness<'_, App>, summary: &str| {
        let row = h.get_by_label(summary).rect();
        right_click_at(h, egui::pos2(row.left() + 30.0, row.center().y));
    };
    // feature/graph and origin/feature/graph: one label, on "Colour lanes".
    open_menu(&mut h, "Colour lanes");
    h.get_by_label("Delete…").click();
    h.run_ok();
    h.get_by_role_and_label(Role::CheckBox, "origin").click();
    h.run_ok();
    screenshot(&mut h, "82-delete-branch");
    h.get_by_role_and_label(Role::Button, "Delete").click();
    settle(&mut h);
    assert!(
        !here(h.state(), "feature/graph") && !there("feature/graph"),
        "gone from both"
    );
    h.get_by_label("Undo").click();
    settle(&mut h);
    assert!(
        here(h.state(), "feature/graph") && there("feature/graph"),
        "back in both"
    );

    // Work that's on no other branch: asked again, nothing deleted yet.
    open_menu(&mut h, "Solo work");
    h.get_by_label("Delete…").click();
    h.run_ok();
    h.get_by_role_and_label(Role::CheckBox, "origin").click();
    h.run_ok();
    h.get_by_role_and_label(Role::Button, "Delete").click();
    settle(&mut h);
    h.get_by_label("Delete anyway");
    assert!(
        here(h.state(), "solo") && there("solo"),
        "nothing deleted yet"
    );
    h.get_by_role_and_label(Role::Button, "Delete anyway")
        .click();
    settle(&mut h);
    assert!(!here(h.state(), "solo") && !there("solo"));
    let _ = std::fs::remove_dir_all(&bare);
}

#[test]
fn a_dragged_branch_is_held_in_a_hand() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    let cursor = |h: &Harness<'_, App>| {
        let out = &h.output().platform_output;
        (out.cursor_icon, out.cursor_image.is_some())
    };
    // A branch in the sidebar, picked up and carried towards main.
    let from = h.get_by_label("toolbar-glitch").rect().center();
    let to = h.get_all_by_label("main").next().unwrap().rect().center();
    h.event(egui::Event::PointerMoved(from));
    h.run_ok();
    h.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_ok();
    for i in 1..=6 {
        #[expect(clippy::cast_precision_loss, reason = "a few steps")]
        h.event(egui::Event::PointerMoved(
            from + (to - from) * (i as f32 / 10.0),
        ));
        h.step();
    }
    h.step();
    // A closed hand (an image: the system's grab cursor is four arrows on
    // Windows), not the arrows.
    assert_eq!(cursor(&h), (egui::CursorIcon::Grabbing, true));
    screenshot(&mut h, "80-dragging-branch");
    h.event(egui::Event::PointerButton {
        pos: from,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_ok();
}

#[test]
fn moves_columns_by_dragging_their_headings() {
    use git_raccoon::settings::Column;
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    let x = |h: &Harness<'_, App>, label: &str| h.get_by_label(label).rect().left();
    assert!(x(&h, "Author") > x(&h, "Commit message"));
    // Drag Author to the left of Commit message. While it's held, it follows
    // the pointer and the columns show where it would land; nothing is
    // kept until it's dropped.
    let message_was = x(&h, "Commit message");
    let from = h.get_by_label("Author").rect().center();
    let to = egui::pos2(message_was + 10.0, from.y);
    // The pointer is an open hand over a heading, a closed one holding it
    // (images of hands: the system's show four arrows on Windows).
    let cursor = |h: &Harness<'_, App>| {
        let out = &h.output().platform_output;
        (out.cursor_icon, out.cursor_image.is_some())
    };
    h.event(egui::Event::PointerMoved(from));
    h.run_ok();
    assert_eq!(cursor(&h), (egui::CursorIcon::Grab, true));
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
    h.step();
    assert_eq!(cursor(&h), (egui::CursorIcon::Grabbing, true));
    assert!((h.get_by_label("Author").rect().center().x - to.x).abs() < 1.0);
    assert!(
        x(&h, "Commit message") > message_was + 50.0,
        "the message column made room"
    );
    assert_eq!(h.state().settings().columns, Column::ALL, "not yet");
    screenshot(&mut h, "60-moving-column");
    h.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    settle(&mut h);
    assert_eq!(
        h.state().settings().columns,
        [
            Column::Refs,
            Column::Graph,
            Column::Author,
            Column::Message,
            Column::Date,
            Column::Sha
        ]
    );
    assert!(
        x(&h, "Author") < x(&h, "Commit message"),
        "the heading moved"
    );
    // The graph can go first too.
    let from = h.get_by_label("Graph").rect().center();
    drag(&mut h, from, egui::pos2(2.0, from.y));
    settle(&mut h);
    assert_eq!(h.state().settings().columns[0], Column::Graph);
    screenshot(&mut h, "61-moved-columns");
    // Right-click a heading for the usual order.
    h.get_by_label("Date").click_secondary();
    h.run_ok();
    h.get_by_label("Put the columns back in order").click();
    settle(&mut h);
    assert_eq!(h.state().settings().columns, Column::ALL);

    // A wide column lands where the pointer is, not where its middle is:
    // taken by its left end and moved a little right, the message column
    // goes just past Author (its middle is far beyond Sha's).
    let message = h.get_by_label("Commit message").rect();
    let from = egui::pos2(message.left() + 20.0, message.center().y);
    drag(&mut h, from, from + egui::vec2(100.0, 0.0));
    settle(&mut h);
    assert_eq!(
        h.state().settings().columns,
        [
            Column::Refs,
            Column::Graph,
            Column::Author,
            Column::Message,
            Column::Date,
            Column::Sha
        ]
    );
}

#[test]
fn resizes_columns_by_their_edges() {
    use git_raccoon::settings::Column;
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    let heading = |h: &Harness<'_, App>, label: &str| h.get_by_label(label).rect();
    let given = |h: &Harness<'_, App>, column: Column| {
        h.state()
            .settings()
            .column_widths
            .iter()
            .find(|(c, _)| *c == column)
            .map(|(_, w)| *w)
    };

    // Dragging the edge after Branch / Tag makes it wider; the message
    // column gives up the room.
    let refs = heading(&h, "Branch / Tag").width();
    let message = heading(&h, "Commit message").width();
    let edge = heading(&h, "Resize Branch / Tag").center();
    drag(&mut h, edge, edge + egui::vec2(80.0, 0.0));
    settle(&mut h);
    assert!((heading(&h, "Branch / Tag").width() - (refs + 80.0)).abs() < 1.0);
    assert!((heading(&h, "Commit message").width() - (message - 80.0)).abs() < 1.0);
    screenshot(&mut h, "62-resized-column");

    // Never narrower than its heading...
    let edge = heading(&h, "Resize Branch / Tag").center();
    drag(&mut h, edge, egui::pos2(0.0, edge.y));
    settle(&mut h);
    let narrowest = heading(&h, "Branch / Tag").width();
    assert!(narrowest > 60.0 && narrowest < 140.0, "{narrowest}");
    // ...and the message column never narrower than its own.
    let edge = heading(&h, "Resize Branch / Tag").center();
    drag(&mut h, edge, egui::pos2(edge.x + 2000.0, edge.y));
    settle(&mut h);
    let squeezed = heading(&h, "Commit message").width();
    assert!(squeezed > 100.0 && squeezed < 160.0, "{squeezed}");

    // The edge right of the message column resizes the column after it.
    h.get_by_label("Branch / Tag").click_secondary();
    h.run_ok();
    h.get_by_label("Put the column widths back").click();
    settle(&mut h);
    assert!(h.state().settings().column_widths.is_empty());
    let author = heading(&h, "Author").width();
    let edge = heading(&h, "Resize Author from its left").center();
    drag(&mut h, edge, edge - egui::vec2(40.0, 0.0));
    settle(&mut h);
    assert!(
        (heading(&h, "Author").width() - (author + 40.0)).abs() < 1.0,
        "{author} -> {}",
        heading(&h, "Author").width()
    );

    // Double-clicking an edge fits the column to what it shows: the author
    // column to "Rocky Raccoon".
    let edge = heading(&h, "Resize Author").center();
    double_click_at(&mut h, edge);
    settle(&mut h);
    let fitted = given(&h, Column::Author).unwrap();
    assert!(fitted < author && fitted > 80.0, "{fitted}");
    // Kept in the settings.
    assert!(given(&h, Column::Refs).is_none());
}

#[test]
fn a_wide_graph_scrolls_sideways() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    assert!(
        h.query_by_label("Scroll the graph sideways").is_none(),
        "a graph that fits doesn't scroll"
    );
    // Thirty branches side by side: far more lanes than the column shows.
    let out = |args: &[&str]| {
        let o = std::process::Command::new("git")
            .args(args)
            .current_dir(sample.path())
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8(o.stdout).unwrap().trim().to_string()
    };
    for i in 0..30 {
        let id = out(&[
            "commit-tree",
            "HEAD^{tree}",
            "-p",
            "HEAD~3",
            "-m",
            &format!("Side {i}"),
        ]);
        out(&["branch", &format!("side-{i:02}"), &id]);
    }
    h.key_press(Key::F5);
    wait_for(&mut h, "the branches", |app| {
        app.repo().unwrap().history.width() > 25
    });
    settle(&mut h);
    let path = h.state().repo().unwrap().path().to_string();
    let offset = |h: &Harness<'_, App>| {
        h.ctx
            .data(|d| d.get_temp::<f32>(egui::Id::new(("graph-x", path.as_str()))))
            .unwrap_or(0.0)
    };
    assert_eq!(offset(&h), 0.0);
    let bar = h.get_by_label("Scroll the graph sideways").rect();
    // Shift and the wheel over the graph move it sideways, not the list.
    let over_graph = egui::pos2(bar.center().x, bar.bottom() + 200.0);
    h.event(egui::Event::PointerMoved(over_graph));
    h.run_ok();
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(-150.0, 0.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    settle(&mut h);
    let moved = offset(&h);
    assert!(moved > 50.0, "scrolled sideways: {moved}");
    screenshot(&mut h, "62-wide-graph");
    // Dragging the bar too.
    drag(&mut h, bar.center(), bar.center() - egui::vec2(400.0, 0.0));
    settle(&mut h);
    assert_eq!(offset(&h), 0.0, "back to the start");
    drag(&mut h, bar.center(), bar.center() + egui::vec2(400.0, 0.0));
    settle(&mut h);
    assert!(offset(&h) > moved, "all the way along: {}", offset(&h));
}

#[test]
fn opens_a_terminal_in_the_repository() {
    let sample = Sample::build();
    // A stand-in terminal that leaves a mark where it's started.
    let mut h = harness(Settings {
        terminal: "touch opened-here".into(),
        ..Settings::default()
    });
    open(&mut h, sample.path());
    let mark = Path::new(sample.path()).join("opened-here");
    // Ctrl+` (or the toolbar's button, or the palette).
    press(&mut h, egui::Modifiers::COMMAND, Key::Backtick);
    for _ in 0..100 {
        if mark.exists() {
            break;
        }
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(mark.exists(), "opened in the repository's folder");
    std::fs::remove_file(&mark).unwrap();
    h.get_by_label("Open a terminal here (Ctrl+`)").click();
    for _ in 0..100 {
        if mark.exists() {
            break;
        }
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(mark.exists(), "from the toolbar too");
    // One that doesn't start says so.
    let mut h = harness(Settings {
        terminal: "no-such-terminal-anywhere".into(),
        ..Settings::default()
    });
    open(&mut h, sample.path());
    press(&mut h, egui::Modifiers::COMMAND, Key::Backtick);
    h.run_ok();
    assert!(
        h.state()
            .error()
            .is_some_and(|e| e.contains("Could not open a terminal")),
        "{:?}",
        h.state().error()
    );
}

#[test]
fn a_label_cut_short_shows_its_whole_name_on_hover() {
    let sample = Sample::build();
    let long = "feature/a-branch-name-much-too-long-to-fit-in-the-column";
    git(&sample, &["branch", long, "HEAD~1"]);
    git(&sample, &["branch", "short", ":/Reject bare"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let hover = |h: &mut Harness<'_, App>, summary: &str| {
        let row = h.get_by_label(summary).rect();
        h.event(egui::Event::PointerMoved(egui::pos2(
            row.left() + 30.0,
            row.center().y,
        )));
        for _ in 0..30 {
            h.step();
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let column = h.get_by_label("Add icon").rect().left();
    hover(&mut h, "Add icon");
    // The label grows to its whole name, over the graph (no tooltip).
    let label = h.get_by_label(long).rect();
    assert!(
        label.right() > column,
        "wider than its column: {label:?}, messages start at {column}"
    );
    assert!(h.query_by_value(long).is_none(), "not a tooltip");
    screenshot(&mut h, "56-long-label");
    // A name that fits stays as it is (only the sidebar's row is named).
    let before = h.query_all_by_label("short").count();
    hover(&mut h, "Reject bare repositories when loading");
    assert_eq!(h.query_all_by_label("short").count(), before);
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
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Delete")
        .click();
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
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Delete")
        .click();
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
    menu_item(&mut h, "v0.1.0", "Delete…");
    h.get_by_role_and_label(Role::Button, "Delete").click();
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
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Delete")
        .click();
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
fn asks_where_to_push_a_new_branch() {
    let sample = Sample::build();
    let base = std::env::temp_dir().join(format!("git-raccoon-push-to-{}", rand_suffix()));
    std::fs::create_dir_all(&base).unwrap();
    let bare = |name: &str| {
        let out = std::process::Command::new("git")
            .args(["init", "-q", "--bare", name])
            .current_dir(&base)
            .output()
            .unwrap();
        assert!(out.status.success());
        base.join(name).to_str().unwrap().replace('\\', "/")
    };
    git(
        &sample,
        &["remote", "set-url", "origin", &bare("origin.git")],
    );
    git(&sample, &["remote", "add", "fork", &bare("fork.git")]);
    git(&sample, &["checkout", "-q", "-b", "fix/hazel-moves"]);

    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    assert_eq!(upstream_counts(h.state()), None);

    // No upstream yet: Push asks, in a bar, with the same name on origin.
    h.get_by_label("Push").click();
    wait_for(&mut h, "the question", |app| {
        app.repo().unwrap().push_to.is_some()
    });
    let target = |app: &App| app.repo().unwrap().push_to.clone().unwrap();
    let asked = target(h.state());
    assert_eq!(
        (
            asked.branch.as_str(),
            asked.remote.as_str(),
            asked.name.as_str()
        ),
        ("fix/hazel-moves", "origin", "fix/hazel-moves")
    );
    h.get_by_label_contains("Where should fix/hazel-moves push to");
    h.get_by_label("Push to origin/fix/hazel-moves");
    screenshot(&mut h, "25-push-to");

    // With two remotes, the other can be chosen; and another name there.
    h.get_by_label("Push to remote").click();
    h.run_ok();
    h.get_all_by_label("fork").last().unwrap().click();
    h.run_ok();
    assert_eq!(target(h.state()).remote, "fork");
    h.get_by_label("Remote branch name").click();
    h.run_ok();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Remote branch name")
        .type_text("fix/NM3-1026");
    h.run_ok();
    h.get_by_label("Push to fork/fix/NM3-1026").click();
    wait_for(&mut h, "the push", |app| {
        upstream_counts(app) == Some((0, 0)) && !app.is_busy()
    });
    let upstream = h
        .state()
        .repo()
        .unwrap()
        .head_upstream()
        .unwrap()
        .name
        .clone();
    assert_eq!(upstream, "fork/fix/NM3-1026");
    assert_eq!(h.state().repo().unwrap().refresh_error, None);
    assert!(
        h.state().repo().unwrap().push_to.is_none(),
        "the bar is gone"
    );

    // From then on, Push just pushes.
    git(&sample, &["commit", "-q", "--allow-empty", "-m", "More"]);
    wait_for(&mut h, "ahead by one", |app| {
        upstream_counts(app) == Some((1, 0))
    });
    h.get_by_label("Push").click();
    wait_for(&mut h, "the second push", |app| {
        upstream_counts(app) == Some((0, 0)) && !app.is_busy()
    });
    assert!(h.state().repo().unwrap().push_to.is_none());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn deletes_a_branch_on_the_remote_and_undoes_it() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    let base = std::env::temp_dir().join(format!("git-raccoon-remote-{}", rand_suffix()));
    std::fs::create_dir_all(&base).unwrap();
    let ok = std::process::Command::new("git")
        .args(["init", "-q", "--bare", "remote.git"])
        .current_dir(&base)
        .status()
        .unwrap();
    assert!(ok.success());
    let url = base.join("remote.git").to_str().unwrap().replace('\\', "/");
    git(&sample, &["remote", "set-url", "origin", &url]);
    git(&sample, &["push", "-q", "origin", "main", "main:topic"]);
    git(&sample, &["fetch", "-q", "origin"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let remote_branches = |app: &App| -> Vec<String> {
        app.repo().unwrap().refs.remotes[0]
            .branches
            .iter()
            .map(|b| b.name.clone())
            .collect()
    };
    assert!(remote_branches(h.state()).contains(&"topic".to_string()));

    // Right-click origin's topic in the sidebar; delete it there, after asking.
    menu_item(&mut h, "topic", "Delete…");
    h.get_by_label("Delete branch topic");
    h.get_by_role_and_label(Role::Button, "Delete").click();
    wait_for(&mut h, "the delete", |app| {
        !remote_branches(app).contains(&"topic".to_string())
    });
    // Undo puts it back.
    wait_for(&mut h, "the undo", |app| {
        undo_label(app).as_deref() == Some("Undo delete origin/topic")
    });
    h.get_by_label("Undo").click();
    wait_for(&mut h, "it's back", |app| {
        remote_branches(app).contains(&"topic".to_string())
    });
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn picks_several_branches_and_deletes_them_together() {
    use egui::accesskit::Role;
    let sample = Sample::build();
    let base = std::env::temp_dir().join(format!("git-raccoon-remote-{}", rand_suffix()));
    std::fs::create_dir_all(&base).unwrap();
    let ok = std::process::Command::new("git")
        .args(["init", "-q", "--bare", "remote.git"])
        .current_dir(&base)
        .status()
        .unwrap();
    assert!(ok.success());
    let url = base.join("remote.git").to_str().unwrap().replace('\\', "/");
    git(&sample, &["remote", "set-url", "origin", &url]);
    git(
        &sample,
        &["push", "-q", "origin", "main", "main:one", "main:two"],
    );
    git(&sample, &["fetch", "-q", "origin"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let local = |app: &App| -> Vec<String> {
        app.repo()
            .unwrap()
            .refs
            .local
            .iter()
            .map(|b| b.name.clone())
            .collect()
    };
    let remote = |app: &App| -> Vec<String> {
        app.repo().unwrap().refs.remotes[0]
            .branches
            .iter()
            .map(|b| b.name.clone())
            .collect()
    };

    // Shift+click picks everything shown between the last branch clicked
    // and this one (folders aside).
    h.get_by_label("graph").click();
    h.run_ok();
    h.get_by_label("toolbar-glitch")
        .click_modifiers(egui::Modifiers::SHIFT);
    h.run_ok();
    h.get_by_label("graph").click_secondary();
    h.run_ok();
    h.get_by_label("Delete 2 branches…");
    h.key_press(Key::Escape);
    h.run_ok();
    h.key_press(Key::Escape);
    h.run_ok();

    // Ctrl+click two local branches; right-click one of them.
    h.get_by_label("graph")
        .click_modifiers(egui::Modifiers::COMMAND);
    h.run_ok();
    h.get_by_label("toolbar-glitch")
        .click_modifiers(egui::Modifiers::COMMAND);
    h.run_ok();
    h.get_by_label("graph").click_secondary();
    h.run_ok();
    screenshot(&mut h, "55-picked-branches");
    h.get_by_label("Delete 2 branches…").click();
    h.run_ok();
    h.get_by_label("Delete 2 branches?");
    h.get_by_role_and_label(Role::Button, "Delete 2 branches")
        .click();
    // The fix isn't merged anywhere: nothing goes until that's agreed to.
    wait_for(&mut h, "the question", |app| {
        app.repo().is_some_and(|r| r.dialog.is_some())
    });
    h.run_ok();
    h.get_by_label("Some have unmerged work");
    assert!(
        local(h.state()).contains(&"feature/graph".to_string()),
        "none deleted yet"
    );
    h.get_by_role_and_label(Role::Button, "Delete all 2 anyway")
        .click();
    wait_for(&mut h, "the delete", |app| {
        !local(app)
            .iter()
            .any(|b| b == "feature/graph" || b == "fix/ui/toolbar-glitch")
    });
    wait_for(&mut h, "the undo", |app| {
        undo_label(app).as_deref() == Some("Undo delete 2 branches")
    });
    h.get_by_label("Undo").click();
    wait_for(&mut h, "they're back", |app| {
        let l = local(app);
        l.contains(&"feature/graph".to_string()) && l.contains(&"fix/ui/toolbar-glitch".to_string())
    });

    // Two on the remote, the other way: click one, Shift+click the last
    // of the range.
    h.get_by_label("one").click();
    h.run_ok();
    h.get_by_label("two")
        .click_modifiers(egui::Modifiers::SHIFT);
    h.run_ok();
    h.get_by_label("two").click_secondary();
    h.run_ok();
    h.get_by_label("Delete 2 on the remote…").click();
    h.run_ok();
    h.get_by_label("Delete 2 branches on the remote?");
    h.get_by_role_and_label(Role::Button, "Delete on the remote")
        .click();
    wait_for(&mut h, "the remote delete", |app| {
        !remote(app).iter().any(|b| b == "one" || b == "two")
    });
    wait_for(&mut h, "its undo", |app| {
        undo_label(app).as_deref() == Some("Undo delete 2 remote branches")
    });
    h.get_by_label("Undo").click();
    wait_for(&mut h, "they're back there", |app| {
        let r = remote(app);
        r.contains(&"one".to_string()) && r.contains(&"two".to_string())
    });
    let _ = std::fs::remove_dir_all(&base);
}

/// What a stand-in forge was sent: the method and path, and the body.
type Seen = std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>;

/// A stand-in forge: answers each request by its method and path (the
/// longest matching prefix in `routes`), and keeps what it was sent. It
/// also serves `repo` (a bare repository) at /team/app.git, read-only,
/// over git's dumb HTTP.
fn forge_server(routes: Vec<(&'static str, String)>, repo: std::path::PathBuf) -> (String, Seen) {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let log = std::sync::Arc::clone(&seen);
    let routes = std::sync::Arc::new(routes);
    let here = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (routes, log) = (std::sync::Arc::clone(&routes), std::sync::Arc::clone(&log));
            let repo = repo.clone();
            let here = here.clone();
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                let _ = reader.read_line(&mut first);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(n) = line.to_lowercase().strip_prefix("content-length:") {
                        length = n.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let request = first.split(' ').take(2).collect::<Vec<_>>().join(" ");
                let mut stream = stream;
                if let Some(file) = request.strip_prefix("GET /team/app.git/") {
                    let file = file.split('?').next().unwrap_or_default();
                    let reply = std::fs::read(repo.join(file));
                    let (status, bytes) = match reply {
                        Ok(b) => (200, b),
                        Err(_) => (404, Vec::new()),
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status} X\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    );
                    let _ = stream.write_all(&bytes);
                    return;
                }
                log.lock()
                    .unwrap()
                    .push((request.clone(), String::from_utf8_lossy(&body).into_owned()));
                let reply = routes
                    .iter()
                    .filter(|(prefix, _)| request.starts_with(prefix))
                    .max_by_key(|(prefix, _)| prefix.len())
                    .map_or_else(
                        || (404, "{}".to_string()),
                        |(_, json)| (200, json.replace("{base}", &here)),
                    );
                // "@link <…>; rel=\"next\"" on its first line: a Link header.
                let (link, reply) = match reply.1.strip_prefix("@link ") {
                    Some(rest) => {
                        let (link, body) = rest.split_once('\n').unwrap_or((rest, ""));
                        (format!("Link: {link}\r\n"), (reply.0, body.to_string()))
                    }
                    None => (String::new(), reply),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {} X\r\nContent-Type: application/json\r\n{link}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.0,
                    reply.1.len(),
                    reply.1
                );
            });
        }
    });
    (base, seen)
}

#[test]
fn shows_merge_requests_and_issues_from_a_self_managed_gitlab() {
    use egui::accesskit::Role;
    use git_raccoon::forge::{Account, Kind, accounts};
    let sample = Sample::build();
    let mr = |iid: u64, title: &str, source: &str| {
        format!(
            r#"{{"iid": {iid}, "title": "{title}", "author": {{"username": "ada"}},
            "source_branch": "{source}", "target_branch": "main", "source_project_id": 1,
            "target_project_id": 1, "draft": false, "web_url": "https://example.com/mr/{iid}",
            "description": "Caches the pipeline.", "created_at": "2026-11-01T00:00:00Z",
            "user_notes_count": 2, "head_pipeline": {{"status": "running"}}}}"#
        )
    };
    let api = "/api/v4/projects/team%2Fapp";
    // Its head is main's commit (here already, so nothing to fetch).
    let main = Repository::open(&sample.dir)
        .unwrap()
        .revparse_single("main")
        .unwrap()
        .id()
        .to_string();
    // The repository on the stand-in forge.
    let bare = sample.dir.with_extension("remote.git");
    git(&sample, &["init", "-q", "--bare", bare.to_str().unwrap()]);
    git(
        &sample,
        &["push", "-q", bare.to_str().unwrap(), "main", "main:cache"],
    );
    let served = std::process::Command::new("git")
        .args(["update-server-info"])
        .current_dir(&bare)
        .status()
        .unwrap();
    assert!(served.success());
    let (base, seen) = forge_server(vec![
        ("GET /api/v4/user", r#"{"username": "milo"}"#.into()),
        // Listing.
        ("GET /api/v4/projects/team%2Fapp/merge_requests?", format!(
            "@link <{{base}}/api/v4/projects/team%2Fapp/merge_requests?state=opened&per_page=50&order_by=created_at&sort=desc&page=2>; rel=\"next\"\n[{}]",
            mr(12, "Pipeline cache", "cache")
        )),
        ("GET /api/v4/projects/team%2Fapp/merge_requests?state=opened&per_page=50&order_by=created_at&sort=desc&page=2",
            format!("[{}]", mr(11, "Older one", "older"))),
        ("GET /api/v4/projects/team%2Fapp/issues?", r#"[{"iid": 4, "title": "Slow start", "author": {"username": "milo"},
            "labels": ["perf"], "web_url": "u", "description": "It takes ages.", "created_at": "2026-11-01T00:00:00Z",
            "user_notes_count": 1}]"#.into()),
        // Details.
        ("GET /api/v4/projects/team%2Fapp/merge_requests/12", mr(12, "Pipeline cache", "cache").replace(
            r#""head_pipeline""#,
            &format!(r#""diff_refs": {{"base_sha": "{main}", "start_sha": "{main}", "head_sha": "{main}"}}, "head_pipeline""#),
        )),
        ("GET /api/v4/projects/team%2Fapp/merge_requests/12/approvals", r#"{"approved_by": [{"user": {"username": "rocky"}}]}"#.into()),
        ("GET /api/v4/projects/team%2Fapp/merge_requests/12/diffs", r#"[{"old_path": "README.md", "new_path": "README.md",
            "diff": "@@ -1,2 +1,3 @@\n # Sample\n-Old line\n+New line\n+Another\n"}]"#.into()),
        ("GET /api/v4/projects/team%2Fapp/merge_requests/12/commits", format!(r#"[{{"id": "{main}", "title": "Cache the pipeline",
            "author_name": "Ada", "created_at": "2026-11-01T00:00:00Z"}}]"#)),
        ("GET /api/v4/projects/team%2Fapp/merge_requests/12/discussions", r#"[
            {"id": "d1", "individual_note": true, "notes": [{"id": 1, "body": "Looks **good**", "author": {"username": "milo"},
                "created_at": "2026-11-02T00:00:00Z", "system": false}]},
            {"id": "d4", "individual_note": true, "notes": [{"id": 7, "body": "Typo in the title.", "author": {"username": "rocky"},
                "created_at": "2026-11-02T12:00:00Z", "system": false}]},
            {"id": "d2", "individual_note": false, "notes": [{"id": 2, "body": "Why another?\n```suggestion:-0+0\nOne more\n```", "author": {"username": "milo"},
                "created_at": "2026-11-03T00:00:00Z", "system": false, "resolvable": true, "resolved": false,
                "suggestions": [{"id": 5, "from_line": 3, "to_line": 3, "from_content": "Another\n",
                    "to_content": "One more\n", "applied": false, "appliable": true}],
                "position": {"old_path": "README.md", "new_path": "README.md", "old_line": null, "new_line": 3}}]}]"#.into()),
        ("GET /api/v4/projects/team%2Fapp/merge_requests/13", mr(13, "Faster start", "cache")),
        ("GET /api/v4/projects/team%2Fapp/merge_requests/13/approvals", r#"{"approved_by": []}"#.into()),
        ("GET /api/v4/projects/team%2Fapp/merge_requests/13/diffs", "[]".into()),
        ("GET /api/v4/projects/team%2Fapp/merge_requests/13/commits", "[]".into()),
        ("GET /api/v4/projects/team%2Fapp/merge_requests/13/discussions", "[]".into()),
        ("GET /api/v4/projects/team%2Fapp", r#"{"default_branch": "main", "squash_option": "default_off"}"#.into()),
        // Reviewing and merging.
        ("POST /api/v4/projects/team%2Fapp/merge_requests/12/draft_notes", r#"{"id": 91}"#.into()),
        ("POST /api/v4/projects/team%2Fapp/merge_requests/12/draft_notes/bulk_publish", "{}".into()),
        ("GET /api/v4/projects/team%2Fapp/merge_requests/12/draft_notes", "[]".into()),
        ("PUT /api/v4/projects/team%2Fapp/merge_requests/12", "{}".into()),
        ("PUT /api/v4/suggestions/batch_apply", "[]".into()),
        ("DELETE /api/v4/projects/team%2Fapp/merge_requests/12/notes", "{}".into()),
        // Who can review a new one, and the labels.
        ("GET /api/v4/projects/team%2Fapp/members/all", r#"[{"username": "rocky", "state": "active"},
            {"username": "milo", "state": "active"}]"#.into()),
        ("GET /api/v4/projects/team%2Fapp/labels", r##"[{"name": "perf", "color": "#d73a4a"},
            {"name": "ux", "color": "#0e8a16"}]"##.into()),
        ("GET /api/v4/users?username=milo", r#"[{"id": 31}]"#.into()),
        // Opening one.
        ("POST /api/v4/projects/team%2Fapp/merge_requests", mr(13, "Faster start", "cache")),
        // Signing in with the browser.
        ("POST /oauth/authorize_device", r#"{"device_code": "dev", "user_code": "WXYZ-0001",
            "verification_uri": "https://example.com/device", "interval": 1}"#.into()),
        ("POST /oauth/token", r#"{"access_token": "glo", "refresh_token": "r", "expires_in": 7200}"#.into()),
    ], bare.clone());
    let url = format!("{base}/team/app.git");
    git(&sample, &["remote", "set-url", "origin", &url]);
    git(&sample, &["fetch", "-q", "origin"]);
    accounts::use_memory_store();
    let account = Account {
        kind: Kind::GitLab,
        base: base.clone(),
        login: "rocky".into(),
        client: None,
    };
    accounts::save(
        &account,
        &accounts::Secret {
            token: "glpat".into(),
            refresh: None,
            expires: None,
        },
    )
    .unwrap();
    let mut h = harness(Settings {
        accounts: vec![account],
        ..Settings::default()
    });
    open(&mut h, sample.path());

    // The lists, in the sidebar.
    wait_for(&mut h, "the merge requests", |app| {
        app.repo()
            .unwrap()
            .forge
            .as_ref()
            .is_some_and(|f| f.requests.is_some() && f.issues.is_some())
    });
    h.run_ok();
    // More than a page of them: the next on request.
    h.get_by_label("Load more requests").click();
    wait_for(&mut h, "the next page", |app| {
        let f = app.repo().unwrap().forge.as_ref().unwrap();
        !f.more_requests && f.request(11).is_some()
    });
    h.run_ok();
    h.get_by_label("!11 Older one");
    assert!(h.query_by_label("Load more requests").is_none());
    h.get_by_label("!12 Pipeline cache").click();
    wait_for(&mut h, "its details", |app| {
        app.repo()
            .unwrap()
            .forge
            .as_ref()
            .is_some_and(|f| f.detailed(12).is_some())
    });
    h.run_ok();
    // Its view, in place of the graph.
    h.get_by_label_contains("Approved by rocky");
    h.get_by_label_contains("1 check running");
    h.get_by_label_contains("1 unresolved thread");
    h.get_by_label_contains("Caches the pipeline.");
    // The conversation: comments in Markdown, and the thread with its code.
    h.get_by_label("good");
    h.get_by_label("Why another?");
    h.get_by_label_contains("README.md, line 3");
    screenshot(&mut h, "50-merge-request");

    // Its commits, marked in the graph; and back.
    h.get_by_label("Commits tab").click();
    h.run_ok();
    h.get_by_label_contains("Show them in the graph").click();
    h.run_ok();
    {
        let repo = h.state().repo().unwrap();
        assert!(
            repo.highlight
                .as_ref()
                .is_some_and(|(n, ids)| *n == 12 && ids.contains(&main))
        );
        assert_eq!(
            repo.selected.as_ref().map(|s| s.id.clone()),
            Some(main.clone())
        );
    }
    h.get_by_label_contains("'s 1 commit, marked.");
    screenshot(&mut h, "58b-request-commits-marked");
    h.get_by_label("Back to !12").click();
    h.run_ok();
    assert!(h.state().repo().unwrap().highlight.is_none());
    h.get_by_label("Conversation tab").click();
    h.run_ok();

    // Your own comments can be edited and deleted (others' can't).
    assert_eq!(h.get_all_by_label("Edit comment").count(), 1);
    h.get_by_label("Edit comment").click();
    h.run_ok();
    h.get_by_label("Edit the comment").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Edit the comment")
        .type_text("Typo in the title: cahce.");
    h.run_ok();
    h.get_by_label("Save").click();
    let saved = |app: &App| {
        let s = &app.repo().unwrap().forge.as_ref().unwrap().state;
        s.sending.is_none() && s.sent.is_some() && s.editing_comment.is_none()
    };
    wait_for(&mut h, "the edit", saved);
    assert!(seen.lock().unwrap().iter().any(|(r, b)| {
        r == &format!("PUT {api}/merge_requests/12/notes/7") && b.contains("cahce")
    }));
    h.get_by_label("Delete comment").click();
    h.run_ok();
    h.get_by_label("Delete this comment, for everyone?");
    h.get_by_label("Delete").click();
    wait_for(&mut h, "the delete", |app| {
        let s = &app.repo().unwrap().forge.as_ref().unwrap().state;
        s.sending.is_none() && s.deleting_comment.is_none()
    });
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|(r, _)| r == &format!("DELETE {api}/merge_requests/12/notes/7"))
    );

    // A suggestion in the thread: what it changes, and applying it.
    h.get_by_label_contains("Suggested change");
    h.get_by_label("+ One more");
    h.get_by_label("− Another");
    screenshot(&mut h, "58-suggestion");
    h.get_by_label("Apply suggestion").click();
    let applied = |app: &App| {
        app.repo()
            .unwrap()
            .forge
            .as_ref()
            .is_some_and(|f| f.state.sending.is_none() && f.state.sent.is_some())
    };
    wait_for(&mut h, "applying it", applied);
    {
        let seen = seen.lock().unwrap();
        let apply = seen
            .iter()
            .find(|(r, _)| r == "PUT /api/v4/suggestions/batch_apply")
            .expect("applied on GitLab");
        assert_eq!(apply.1, r#"{"ids":[5]}"#);
    }
    // Several, in one commit.
    h.get_by_label("Apply with others").click();
    h.run_ok();
    type_into(&mut h, "Suggestions commit message", "Tidy the README");
    h.get_by_label("Apply 1 in one commit").click();
    wait_for(&mut h, "applying them", |app| {
        applied(app)
            && app
                .repo()
                .unwrap()
                .forge
                .as_ref()
                .unwrap()
                .state
                .batch
                .is_empty()
    });
    assert!(seen.lock().unwrap().iter().any(|(r, b)| {
        r == "PUT /api/v4/suggestions/batch_apply" && b.contains("Tidy the README")
    }));

    // Changes: the diff, with the thread under its line.
    h.get_by_label("Changes tab").click();
    h.run_ok();
    h.get_by_label("Select line: Another");
    h.get_by_label("Why another?");
    // The whole file, side by side, with blame: from its commits, here.
    wait_for(&mut h, "its commits", |app| {
        app.repo()
            .unwrap()
            .forge
            .as_ref()
            .unwrap()
            .state
            .local
            .is_some()
    });
    fn state(app: &App) -> &git_raccoon::forge_view::RequestState {
        &app.repo().unwrap().forge.as_ref().unwrap().state
    }
    assert_eq!(state(h.state()).local, Some(Ok(())));
    for switch in ["Whole file", "Split", "Blame"] {
        h.get_by_label(switch).click();
        h.run_ok();
    }
    assert!(state(h.state()).split);
    wait_for(&mut h, "the whole file and its blame", |app| {
        let s = state(app);
        matches!(s.wholes.get("README.md"), Some(Ok(_)))
            && matches!(s.blames.get("README.md"), Some(Ok(_)))
    });
    h.run_ok();
    screenshot(&mut h, "59-request-whole-split-blame");
    // The thread is still under its line.
    h.get_by_label("Why another?");
    for switch in ["Changes", "Unified", "Blame"] {
        h.get_by_label(switch).click();
        h.run_ok();
    }
    // Resolve it.
    h.get_by_label("Resolve").click();
    let sent = |app: &App| {
        app.repo()
            .unwrap()
            .forge
            .as_ref()
            .is_some_and(|f| f.state.sending.is_none() && f.state.sent.is_some())
    };
    wait_for(&mut h, "resolving", sent);
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|(r, _)| r == &format!("PUT {api}/merge_requests/12/discussions/d2?resolved=true"))
    );
    // Suggesting a change: the picked line, ready to edit.
    h.get_by_label("Select line: New line").click_secondary();
    h.run_ok();
    h.get_by_label("Suggest a change").click();
    h.run_ok();
    let body = h
        .state()
        .repo()
        .unwrap()
        .forge
        .as_ref()
        .unwrap()
        .state
        .composer
        .clone()
        .unwrap()
        .body;
    assert_eq!(body, "```suggestion:-0+0\nNew line\n```");
    h.get_by_label("Cancel").click();
    h.run_ok();
    // A comment on a line: a draft, until the review is finished.
    h.get_by_label("Select line: Old line").click_secondary();
    h.run_ok();
    // The menu's (the line's + is labelled the same).
    h.get_all_by_label("Comment on this line")
        .last()
        .unwrap()
        .click();
    h.run_ok();
    h.get_by_label("Comment").type_text("Why drop it?");
    h.run_ok();
    screenshot(&mut h, "55-draft-comment");
    h.get_by_label("Add to review").click();
    h.run_ok();
    let drafts = |app: &App| {
        app.settings()
            .review_drafts
            .values()
            .map(Vec::len)
            .sum::<usize>()
    };
    assert_eq!(drafts(h.state()), 1);
    // Kept on GitLab straight away, as a draft note.
    wait_for(&mut h, "the draft note", |app| {
        app.settings()
            .review_drafts
            .values()
            .flatten()
            .any(|d| d.remote.as_deref() == Some("91"))
    });
    h.get_by_label("Why drop it?");
    h.get_by_label("Finish review (1)").click();
    h.run_ok();
    h.get_by_label("Approve").click();
    h.run_ok();
    screenshot(&mut h, "56-finish-review");
    h.get_by_label("Submit review and send 1").click();
    wait_for(&mut h, "the review", |app| sent(app) && drafts(app) == 0);
    {
        let seen = seen.lock().unwrap();
        let note = seen
            .iter()
            .find(|(r, _)| r == &format!("POST {api}/merge_requests/12/draft_notes"))
            .expect("a draft note");
        assert!(
            note.1.contains("Why drop it?") && note.1.contains(r#""old_line":2"#),
            "{}",
            note.1
        );
        let posted = seen
            .iter()
            .filter(|(r, _)| r == &format!("POST {api}/merge_requests/12/draft_notes"))
            .count();
        assert_eq!(posted, 1, "sent once, when it was written");
        for end in ["draft_notes/bulk_publish", "approve"] {
            assert!(
                seen.iter()
                    .any(|(r, _)| r == &format!("POST {api}/merge_requests/12/{end}")),
                "{end}"
            );
        }
    }
    // Merging.
    h.get_by_label("Merge…").click();
    h.run_ok();
    h.get_by_label("Delete cache after merging");
    screenshot(&mut h, "57-merge");
    h.get_all_by_label("Merge commit").last().unwrap().click();
    wait_for(&mut h, "the merge", sent);
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|(r, b)| r == &format!("PUT {api}/merge_requests/12/merge")
                && b.contains(r#""should_remove_source_branch":true"#))
    );
    h.get_by_label("Conversation tab").click();
    h.run_ok();

    // Right-click to open things on the forge.
    let opened = |h: &mut Harness<'_, App>, row: &str| -> String {
        h.get_all_by_label(row).next().unwrap().click_secondary();
        h.run_ok();
        if row == "Add icon" {
            screenshot(h, "54-open-on-forge");
        }
        // The menu's (the details pane has a button too).
        h.get_all_by_label_contains("Open on GitLab")
            .last()
            .unwrap()
            .click();
        h.step();
        let url = h
            .output()
            .platform_output
            .commands
            .iter()
            .find_map(|c| match c {
                egui::OutputCommand::OpenUrl(u) => Some(u.url.clone()),
                _ => None,
            })
            .expect("a page opened");
        h.run_ok();
        url
    };
    assert_eq!(
        opened(&mut h, "!12 Pipeline cache"),
        "https://example.com/mr/12"
    );
    assert_eq!(opened(&mut h, "#4 Slow start"), "u");
    assert_eq!(opened(&mut h, "origin"), format!("{base}/team/app"));
    assert_eq!(
        opened(&mut h, "cache"),
        format!("{base}/team/app/-/tree/cache")
    );
    // The graph is back once the request's view is closed (the first Esc
    // lets go of the line still picked).
    h.key_press(Key::Escape);
    h.run_ok();
    assert!(h.query_by_label("Conversation tab").is_some());
    h.key_press(Key::Escape);
    h.run_ok();
    assert!(
        h.query_by_label("Conversation tab").is_none(),
        "Esc closes it"
    );
    let commit = opened(&mut h, "Add icon");
    assert!(
        commit.starts_with(&format!("{base}/team/app/-/commit/")),
        "{commit}"
    );
    let seen_now = seen.lock().unwrap().clone();
    assert!(
        seen_now
            .iter()
            .any(|(r, _)| r == &format!("GET {api}/merge_requests/12/approvals"))
    );

    // Check it out: its branch, from the remote.
    // With uncommitted changes, it asks first.
    let readme = sample.dir.join("README.md");
    let before = std::fs::read_to_string(&readme).unwrap();
    std::fs::write(&readme, format!("{before}Work in progress.\n")).unwrap();
    h.key_press(Key::F5);
    wait_for(&mut h, "the change", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| !s.unstaged.is_empty())
    });
    h.get_by_label("!12 Pipeline cache").click();
    h.run_ok();
    h.get_by_label_contains("Check out").click();
    h.run_ok();
    h.get_by_label("Check out cache").click();
    h.run_ok();
    h.get_by_label("You have uncommitted changes");
    screenshot(&mut h, "60-request-changes-in-the-way");
    h.get_by_label("Put them aside and check out").click();
    wait_for(&mut h, "the checkout", |app| {
        head_branch(app).as_deref() == Some("cache")
    });
    assert_eq!(
        std::fs::read_to_string(&readme).unwrap(),
        before,
        "put aside"
    );
    // Back where it was, with them.
    h.get_by_label("Back to main").click();
    wait_for(&mut h, "going back", |app| {
        head_branch(app).as_deref() == Some("main") && app.repo().unwrap().refs.stashes.len() == 1
    });
    assert!(
        std::fs::read_to_string(&readme)
            .unwrap()
            .ends_with("Work in progress.\n")
    );
    // Or, with them, a worktree of its own instead: it opens in a tab.
    h.get_by_label_contains("Check out").click();
    h.run_ok();
    h.get_by_label("Check out cache").click();
    h.run_ok();
    h.get_by_label("Use a worktree instead").click();
    wait_for(&mut h, "the worktree's tab", |app| {
        app.repo()
            .is_some_and(|r| r.path().trim_end_matches('/').ends_with("-cache"))
    });
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::W);
    wait_for(&mut h, "back in the first tab", |app| {
        app.repo()
            .is_some_and(|r| !r.path().trim_end_matches('/').ends_with("-cache"))
    });
    assert!(
        std::fs::read_to_string(&readme)
            .unwrap()
            .ends_with("Work in progress.\n"),
        "left here"
    );
    // Checking it out again goes to that worktree.
    h.get_by_label_contains("Check out").click();
    h.run_ok();
    h.get_by_label("Check out cache").click();
    wait_for(&mut h, "the worktree's tab again", |app| {
        app.repo()
            .is_some_and(|r| r.path().trim_end_matches('/').ends_with("-cache"))
    });
    let worktree = h.state().repo().unwrap().path().to_string();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::W);
    wait_for(&mut h, "back in the first tab", |app| {
        app.repo()
            .is_some_and(|r| !r.path().trim_end_matches('/').ends_with("-cache"))
    });
    git(&sample, &["worktree", "remove", "--force", &worktree]);
    // Now without changes in the way (put them aside by hand).
    std::fs::write(&readme, &before).unwrap();
    h.key_press(Key::F5);
    wait_for(&mut h, "no changes", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| s.unstaged.is_empty() && s.staged.is_empty())
    });
    h.get_by_label_contains("Check out").click();
    h.run_ok();
    h.get_by_label("Check out cache").click();
    wait_for(&mut h, "the checkout", |app| {
        head_branch(app).as_deref() == Some("cache")
    });

    // An issue.
    h.get_by_label("#4 Slow start").click();
    h.run_ok();
    h.get_by_value("ISSUE #4");
    h.get_by_value("perf");
    h.key_press(Key::Escape);
    h.run_ok();
    assert!(h.query_by_value("ISSUE #4").is_none(), "Esc closes it");

    // A new merge request from the checked-out branch (it's pushed).
    h.get_by_label("New merge request").click();
    h.run_ok();
    // The branch it goes into is picked from the remote's, the default first.
    // (Here the remote has only main besides the branch itself.)
    let shown = |h: &Harness<'_, App>| h.get_all_by_role_and_label(Role::Button, "main").count();
    let before = shown(&h);
    h.get_by_label("Target branch").click();
    h.run_ok();
    assert_eq!(shown(&h), before + 1, "main is offered");
    screenshot(&mut h, "78-request-target");
    h.get_all_by_role_and_label(Role::Button, "main")
        .last()
        .unwrap()
        .click();
    h.run_ok();
    // Nothing new on it: its title is its name, and it says so.
    assert_eq!(h.state().new_request_form().unwrap().title, "Cache");
    h.get_by_label_contains("has nothing main doesn't have");
    h.get_by_label("Request title").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Request title").type_text("Faster start");
    h.run_ok();
    // Reviewers and labels: picked from the forge's, not typed.
    wait_for(&mut h, "who can review it", |app| {
        app.new_request_form().is_some_and(|f| f.choices.is_some())
    });
    h.run_ok();
    h.get_by_label("Request reviewers").click();
    h.run_ok();
    h.get_by_label("milo").click();
    h.run_ok();
    h.key_press(Key::Escape);
    h.run_ok();
    h.get_by_label("Request labels").click();
    h.run_ok();
    h.get_by_label("perf").click();
    h.get_by_label("ux").click();
    h.run_ok();
    screenshot(&mut h, "79b-request-labels");
    h.key_press(Key::Escape);
    h.run_ok();
    let form = h.state().new_request_form().unwrap();
    assert_eq!(
        (form.reviewers.clone(), form.labels.clone()),
        (
            vec!["milo".to_string()],
            vec!["perf".to_string(), "ux".to_string()]
        )
    );
    h.get_by_label("Squash its commits when it's merged")
        .click();
    h.run_ok();
    h.get_by_role_and_label(Role::Button, "Open it").click();
    wait_for(&mut h, "the new one", |app| {
        app.repo()
            .unwrap()
            .forge
            .as_ref()
            .is_some_and(|f| f.open == Some(git_raccoon::forge_view::Opened::Request(13)))
    });
    let posted = seen
        .lock()
        .unwrap()
        .iter()
        .find(|(r, _)| r == &format!("POST {api}/merge_requests"))
        .cloned()
        .unwrap();
    let sent: serde_json::Value = serde_json::from_str(&posted.1).unwrap();
    assert_eq!(
        (
            sent["source_branch"].as_str(),
            sent["target_branch"].as_str()
        ),
        (Some("cache"), Some("main"))
    );
    assert_eq!(sent["title"].as_str(), Some("Faster start"));
    assert_eq!(sent["labels"].as_str(), Some("perf,ux"));
    assert_eq!(sent["squash"].as_bool(), Some(true));
    assert_eq!(sent["reviewer_ids"], serde_json::json!([31]));

    // From any branch: its right-click menu.
    h.get_by_label("graph").click_secondary();
    h.run_ok();
    h.get_by_label("Open a merge request…").click();
    h.run_ok();
    let form = h.state().new_request_form().unwrap();
    assert_eq!(form.source, "feature/graph");
    // (Merged into main already, so it's named after the branch.)
    assert_eq!((form.title.as_str(), form.commits.len()), ("Graph", 0));
    screenshot(&mut h, "79-request-from-branch");
    h.get_by_label("Cancel").click();
    h.run_ok();
    // One with something new: its commits and the files it changes.
    h.get_by_label("toolbar-glitch").click_secondary();
    h.run_ok();
    h.get_by_label("Open a merge request…").click();
    h.run_ok();
    let form = h.state().new_request_form().unwrap();
    assert_eq!(form.commits.len(), 1);
    assert_eq!(
        form.title, "Fix toolbar glitch",
        "named after its one commit"
    );
    let file = form
        .files
        .first()
        .expect("a file it changes")
        .diff
        .path
        .clone();
    h.get_by_label("Changes tab").click();
    h.run_ok();
    h.get_all_by_label(&file).last().unwrap().click();
    h.run_ok();
    assert_eq!(h.state().new_request_form().unwrap().open_file, Some(0));
    // Not pushed, and not checked out: it's pushed by name.
    assert!(
        !h.get_by_label("Push 1 commit and open")
            .accesskit_node()
            .is_disabled(),
        "pushing doesn't need it checked out"
    );
    screenshot(&mut h, "79c-request-files");
    h.get_by_label("Cancel").click();
    h.run_ok();

    // Pushing a new branch offers to open a request for it. (Pushes go
    // straight to the remote's folder; the stand-in forge can't take them.)
    git(
        &sample,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            bare.to_str().unwrap(),
        ],
    );
    git(&sample, &["checkout", "-q", "-b", "fresh"]);
    h.key_press(Key::Escape);
    h.key_press(Key::F5);
    wait_for(&mut h, "the new branch", |app| {
        head_branch(app).as_deref() == Some("fresh")
    });
    h.get_by_label("Push").click();
    h.run_ok();
    h.get_by_label("Push to origin/fresh").click();
    wait_for(&mut h, "the offer", |app| {
        app.repo().unwrap().offer_request == Some(("fresh".to_string(), true))
    });
    h.run_ok();
    h.get_by_label("fresh is on the remote now.");
    screenshot(&mut h, "79d-offer-request");
    h.get_by_label("Open a merge request…").click();
    h.run_ok();
    assert_eq!(h.state().new_request_form().unwrap().source, "fresh");
    assert!(h.state().repo().unwrap().offer_request.is_none());
    h.get_by_label("Cancel").click();
    h.run_ok();

    // Signing in with a token, through the window.
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    h.get_by_role_and_label(Role::Button, "Accounts").click();
    h.run_ok();
    h.get_by_label("Add account…").click();
    h.run_ok();
    h.get_by_label("Self-managed GitLab").click();
    h.run_ok();
    h.get_by_label("GitLab server address").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("GitLab server address").type_text(&base);
    type_into(&mut h, "Access token", "glpat-milo");
    screenshot(&mut h, "51-sign-in");
    h.get_by_role_and_label(Role::Button, "Sign in").click();
    wait_for(&mut h, "the account", |app| {
        app.settings().accounts.iter().any(|a| a.login == "milo")
    });

    // And with the browser, through the server's own app.
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    h.get_by_role_and_label(Role::Button, "Accounts").click();
    h.run_ok();
    h.get_by_label("Add account…").click();
    h.run_ok();
    h.get_by_label("Self-managed GitLab").click();
    h.run_ok();
    h.get_by_label("GitLab server address").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("GitLab server address").type_text(&base);
    h.get_by_label("Use the browser instead").click();
    h.run_ok();
    type_into(&mut h, "Application ID", "server-app");
    screenshot(&mut h, "52-sign-in-app");
    h.get_by_label("Sign in with the browser").click();
    let mut shown = false;
    for _ in 0..100 {
        h.run_ok();
        if h.query_by_value("WXYZ-0001").is_some() {
            shown = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(shown, "the code to enter");
    screenshot(&mut h, "53-sign-in-code");
    wait_for(&mut h, "the browser sign-in", |app| {
        app.settings()
            .accounts
            .iter()
            .any(|a| a.client.as_deref() == Some("server-app"))
    });
    let asked = seen.lock().unwrap().clone();
    assert!(
        asked
            .iter()
            .any(|(r, body)| r == "POST /oauth/authorize_device"
                && body.contains("client_id=server-app"))
    );
    let _ = std::fs::remove_dir_all(&bare);
}

#[test]
fn fetches_from_one_remote_with_options() {
    let sample = Sample::build();
    let base = std::env::temp_dir().join(format!("git-raccoon-fetch-{}", rand_suffix()));
    std::fs::create_dir_all(&base).unwrap();
    let out = std::process::Command::new("git")
        .args(["init", "-q", "--bare", "remote.git"])
        .current_dir(&base)
        .output()
        .unwrap();
    assert!(out.status.success());
    let url = base.join("remote.git").to_str().unwrap().replace('\\', "/");
    git(&sample, &["remote", "set-url", "origin", &url]);
    git(
        &sample,
        &["push", "-q", "origin", "main", "main:refs/heads/gone"],
    );
    git(&sample, &["fetch", "-q", "origin"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    assert!(h.state().settings().fetch_prune && !h.state().settings().fetch_force);

    // With only origin, there's no choosing where from.
    h.get_by_label("Fetch options").click();
    h.run_ok();
    h.get_by_label_contains("Prune");
    assert!(h.query_by_label("All remotes").is_none());
    h.get_by_label("Fetch options").click();
    h.run_ok();
    git(&sample, &["remote", "add", "fork", &url]);
    h.key_press(Key::F5);
    settle(&mut h);

    // Beside Fetch: where from, and how (remembered).
    h.get_by_label("Fetch options").click();
    h.run_ok();
    h.get_by_label("All remotes");
    h.get_by_label_contains("Prune").click();
    h.run_ok();
    h.get_by_label_contains("Force").click();
    h.run_ok();
    assert!(!h.state().settings().fetch_prune && h.state().settings().fetch_force);
    screenshot(&mut h, "21-fetch-options");

    // From origin only, without pruning: a branch deleted there stays, and
    // one new there arrives.
    let on_remote = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("--git-dir")
            .arg(&url)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success());
    };
    on_remote(&["update-ref", "-d", "refs/heads/gone"]);
    on_remote(&["branch", "fresh", "main"]);
    // (The sidebar lists origin too; the menu's is drawn last.)
    h.get_all_by_label("origin").last().unwrap().click();
    // Force can overwrite local refs, so it asks first.
    h.run_ok();
    h.get_by_label("Fetch with Force?");
    h.get_by_label("Fetch with Force").click();
    wait_for(&mut h, "the fetch", |app| !app.is_busy());
    assert_eq!(h.state().repo().unwrap().refresh_error, None);
    let has = |h: &Harness<'_, App>, name: &str| {
        h.state()
            .repo()
            .unwrap()
            .refs
            .remotes
            .iter()
            .flat_map(|r| &r.branches)
            .any(|b| b.name == name)
    };
    assert!(has(&h, "fresh"), "fetched");
    assert!(has(&h, "gone"), "not pruned");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn pulls_from_one_remote_with_options() {
    let sample = Sample::build();
    let base = std::env::temp_dir().join(format!("git-raccoon-pull-{}", rand_suffix()));
    std::fs::create_dir_all(&base).unwrap();
    let out = std::process::Command::new("git")
        .args(["init", "-q", "--bare", "remote.git"])
        .current_dir(&base)
        .output()
        .unwrap();
    assert!(out.status.success());
    let url = base.join("remote.git").to_str().unwrap().replace('\\', "/");
    git(&sample, &["remote", "set-url", "origin", &url]);
    // The remote is a commit ahead, and main a commit of its own.
    git(&sample, &["commit", "-q", "--allow-empty", "-m", "Theirs"]);
    git(&sample, &["push", "-q", "-u", "origin", "main"]);
    git(&sample, &["reset", "-q", "--hard", "HEAD~1"]);
    git(&sample, &["commit", "-q", "--allow-empty", "-m", "Ours"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let bin = sample.dir.with_extension("bin");
    h.state_mut().repo_mut().unwrap().trash_override = Some(bin.clone());
    let readme = read(&sample, "README.md").unwrap();
    sample.write("README.md", "an unsaved idea\n");
    wait_for(&mut h, "the change", |app| {
        app.repo()
            .unwrap()
            .status
            .as_ref()
            .is_some_and(|s| !s.is_clean())
    });

    // With only origin, there's no choosing where from.
    h.get_by_label("Pull options").click();
    h.run_ok();
    h.get_by_label("Rebase");
    assert!(h.query_by_label("Pull from").is_none());
    h.get_by_label("Pull options").click();
    h.run_ok();
    git(&sample, &["remote", "add", "fork", &url]);
    h.key_press(Key::F5);
    settle(&mut h);

    // Beside Pull: where from, and how (remembered).
    h.get_by_label("Pull options").click();
    h.run_ok();
    h.get_by_label("Pull from");
    h.get_by_label("Upstream (origin/main)");
    h.get_by_label("Fast-forward if possible, else merge")
        .click();
    h.run_ok();
    h.get_by_label("Discard them (asks first)").click();
    h.run_ok();
    let pull = h.state().settings().pull;
    assert_eq!(pull.strategy, PullStrategy::FastForwardOrMerge);
    assert_eq!(pull.changes, PullChanges::Discard);
    screenshot(&mut h, "21-pull-options");

    // From origin: discarding asks first, then a merge joins both sides.
    // (The sidebar lists origin too; the menu's is drawn last.)
    h.get_all_by_label("origin").last().unwrap().click();
    h.run_ok();
    h.get_by_label("Discard your changes and pull?");
    h.get_by_label("Discard and pull").click();
    wait_for(&mut h, "the pull", |app| !app.is_busy());
    assert_eq!(h.state().repo().unwrap().refresh_error, None);
    let summary = head_summary(h.state());
    assert!(summary.starts_with("Merge branch 'main' of "), "{summary}");
    // (Restored as Git checks files out here: maybe with CRLF endings.)
    assert_eq!(
        read(&sample, "README.md").map(|t| t.replace("\r\n", "\n")),
        Some(readme.replace("\r\n", "\n"))
    );
    assert!(bin.join("README.md").exists(), "kept in the bin");
    let _ = std::fs::remove_dir_all(&base);
    let _ = std::fs::remove_dir_all(&bin);
}

#[test]
fn sets_and_removes_a_branch_upstream() {
    let sample = Sample::build();
    let remote = "refs/remotes/origin/fix/ui/toolbar-glitch";
    git(&sample, &["update-ref", remote, "fix/ui/toolbar-glitch"]);
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let upstream = |app: &App, branch: &str| {
        app.repo()
            .unwrap()
            .refs
            .local
            .iter()
            .find(|b| b.name == branch)
            .and_then(|b| b.upstream.as_ref())
            .map(|u| u.name.clone())
    };
    assert_eq!(upstream(h.state(), "fix/ui/toolbar-glitch"), None);

    // The same name on the remote is offered first.
    menu_item(&mut h, "toolbar-glitch", "Set upstream…");
    h.get_by_label("Set upstream for fix/ui/toolbar-glitch");
    h.get_by_label("Set upstream").click();
    wait_for(&mut h, "the upstream", |app| {
        upstream(app, "fix/ui/toolbar-glitch").as_deref() == Some("origin/fix/ui/toolbar-glitch")
    });

    // None removes it.
    menu_item(&mut h, "toolbar-glitch", "Set upstream…");
    h.get_by_label("Upstream branch").click();
    h.run_ok();
    h.get_by_label("None").click();
    h.run_ok();
    h.get_by_label("Remove upstream").click();
    wait_for(&mut h, "no upstream", |app| {
        upstream(app, "fix/ui/toolbar-glitch").is_none()
    });
    assert_eq!(h.state().error(), None);
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
fn clones_from_an_account_and_makes_new_repositories_there() {
    use git_raccoon::forge::{Account, Kind, accounts};
    let sample = Sample::build();
    // The forge serves the sample's commits as team/app.git.
    let bare = sample.dir.with_extension("forge.git");
    git(&sample, &["init", "-q", "--bare", bare.to_str().unwrap()]);
    git(&sample, &["push", "-q", bare.to_str().unwrap(), "main"]);
    let ok = std::process::Command::new("git")
        .args(["symbolic-ref", "HEAD", "refs/heads/main"])
        .current_dir(&bare)
        .status()
        .unwrap();
    assert!(ok.success());
    let served = std::process::Command::new("git")
        .args(["update-server-info"])
        .current_dir(&bare)
        .status()
        .unwrap();
    assert!(served.success());
    let (base, seen) = forge_server(
        vec![
            (
                "GET /api/v4/projects?",
                r#"[{"path_with_namespace": "team/app", "description": "The app",
                     "visibility": "private", "http_url_to_repo": "{base}/team/app.git"},
                    {"path_with_namespace": "rocky/notes", "description": "",
                     "visibility": "public", "http_url_to_repo": "{base}/rocky/notes.git"}]"#
                    .into(),
            ),
            (
                "POST /api/v4/projects",
                r#"{"path_with_namespace": "rocky/fresh", "visibility": "private",
                    "http_url_to_repo": "{base}/team/app.git"}"#
                    .into(),
            ),
            (
                "GET /api/v4/templates/gitignores",
                r#"[{"key": "Rust", "name": "Rust"}]"#.into(),
            ),
            (
                "GET /api/v4/templates/licenses",
                r#"[{"key": "mit", "name": "MIT License"}]"#.into(),
            ),
        ],
        bare.clone(),
    );
    accounts::use_memory_store();
    let account = Account {
        kind: Kind::GitLab,
        base: base.clone(),
        login: "rocky".into(),
        client: None,
    };
    accounts::save(
        &account,
        &accounts::Secret {
            token: "glpat".into(),
            refresh: None,
            expires: None,
        },
    )
    .unwrap();
    let parent = std::env::temp_dir().join(format!("git-raccoon-account-clones-{}", rand_suffix()));
    std::fs::create_dir_all(&parent).unwrap();
    let mut h = harness(Settings {
        accounts: vec![account],
        clone_dir: Some(parent.to_str().unwrap().replace('\\', "/")),
        ..Settings::default()
    });
    h.run_ok();

    // Just a URL to start with; choosing the account adds its
    // repositories, to pick from in a dropdown.
    h.get_by_label("Clone a repository…").click();
    h.run_ok();
    h.get_by_label("Repository URL");
    assert!(h.query_by_label("Repository to clone").is_none());
    h.get_by_label("Account to clone from").click();
    h.run_ok();
    h.get_by_label_contains("rocky (").click();
    h.run_ok();
    h.get_by_label("Repository to clone").click();
    let mut listed = false;
    for _ in 0..200 {
        h.run_ok();
        if h.query_by_label_contains("team/app").is_some() {
            listed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(listed, "the account's repositories are listed");
    type_into(&mut h, "Find a repository", "app");
    assert!(
        h.query_by_label_contains("rocky/notes").is_none(),
        "narrowed"
    );
    h.get_by_label_contains("team/app").click();
    h.run_ok();
    // Picked: named after it.
    assert_eq!(h.get_by_label("Clone name").value().as_deref(), Some("app"));
    screenshot(&mut h, "58-clone-from-account");
    h.get_by_label("Clone").click();
    wait_for(&mut h, "the clone to open", |app| app.repo().is_some());
    assert!(h.state().repo().unwrap().history.total() > 5);

    // A new one, made on the account and cloned: New repository asks where
    // (#23), and the clone dialog doesn't make them any more.
    press(&mut h, egui::Modifiers::COMMAND, Key::T);
    h.run_ok();
    h.get_by_label("Clone a repository…").click();
    h.run_ok();
    assert!(h.query_by_label_contains("New repository on").is_none());
    h.get_by_label("Cancel").click();
    h.run_ok();
    h.get_by_label("New repository…").click();
    h.run_ok();
    // Only on this computer to start with; choosing the account makes it
    // on its forge, with what can be set there.
    assert!(h.query_by_label("New repository description").is_none());
    h.get_by_label("Account to make it on").click();
    h.run_ok();
    h.get_by_label_contains("rocky (").click();
    h.run_ok();
    type_into(&mut h, "New repository name", "fresh");
    type_into(&mut h, "New repository description", "Brand new");
    // The forge's templates, fetched once a picker is opened.
    h.get_by_label("Choose a .gitignore template").click();
    let mut offered = false;
    for _ in 0..200 {
        h.run_ok();
        if h.query_by_label("Rust").is_some() {
            offered = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(offered, "the forge's .gitignore templates");
    screenshot(&mut h, "59-new-on-account");
    h.key_press(Key::Escape);
    h.run_ok();
    h.get_by_label("Create repository and clone").click();
    wait_for(&mut h, "the new one to open", |app| {
        app.repo().is_some_and(|r| {
            r.path()
                .replace('\\', "/")
                .trim_end_matches('/')
                .ends_with("/fresh")
        })
    });
    settle(&mut h);
    screenshot(&mut h, "60-made-on-account");
    let made = seen
        .lock()
        .unwrap()
        .iter()
        .find(|(r, _)| r == "POST /api/v4/projects")
        .cloned()
        .expect("asked for a new repository");
    let sent: serde_json::Value = serde_json::from_str(&made.1).unwrap();
    assert_eq!(
        (
            sent["name"].as_str(),
            sent["description"].as_str(),
            sent["visibility"].as_str()
        ),
        (Some("fresh"), Some("Brand new"), Some("private"))
    );
    let _ = std::fs::remove_dir_all(&parent);
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
    // Only some folders: src, not assets.
    h.get_by_label("Sparse checkout").click();
    h.run_ok();
    type_into(&mut h, "Folders to include", "src");
    screenshot(&mut h, "26-clone");
    h.get_by_label("Clone").click();
    wait_for(&mut h, "the clone to open", |app| app.repo().is_some());
    let repo = h.state().repo().unwrap();
    assert_eq!(
        Path::new(repo.path()).canonicalize().unwrap(),
        base.join(&name).canonicalize().unwrap()
    );
    assert!(repo.history.total() > 5);
    assert!(base.join(&name).join("src").exists());
    assert!(!base.join(&name).join("assets").exists(), "sparse");
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

    // A new, empty repository, on this computer only, opens too.
    press(&mut h, egui::Modifiers::COMMAND, Key::T);
    h.get_by_label("New repository…").click();
    h.run_ok();
    type_into(&mut h, "New repository name", "fresh");
    h.get_by_label("First branch name").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("First branch name").type_text("trunk");
    h.run_ok();
    h.get_by_label("Create repository").click();
    wait_for(&mut h, "the new repository", |app| {
        app.repo().is_some_and(|r| r.path().contains("fresh"))
    });
    assert_eq!(h.state().error(), None);
    assert_eq!(h.state().repo().unwrap().history.total(), 0);
    assert!(
        h.state()
            .repo()
            .unwrap()
            .history
            .branch()
            .is_some_and(|b| b.ends_with("trunk")),
        "its first branch"
    );
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
    // Taking a whole side would replace that edit, so it asks first.
    h.get_by_label("Use theirs for the file").click();
    h.run_ok();
    h.get_by_label("Use theirs for README.md?");
    h.get_by_label("Cancel").click();
    h.run_ok();
    assert_eq!(
        read(&sample, "README.md").as_deref(),
        Some("# Sample\n\nwritten by hand\n")
    );
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
fn the_palette_scrolls_freely_and_follows_the_arrows() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    press(&mut h, egui::Modifiers::COMMAND, Key::K);
    let first = |h: &Harness<'_, App>| h.get_by_label("Settings…").rect().top();
    let top = first(&h);

    // The wheel scrolls the list away from the chosen (first) line, and it
    // stays there rather than jumping back (#24).
    h.event(egui::Event::PointerMoved(
        h.get_by_label("New tab").rect().center(),
    ));
    h.run_ok();
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -200.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    });
    for _ in 0..8 {
        h.step();
    }
    let scrolled = first(&h);
    assert!(scrolled < top - 50.0, "scrolled: {top} -> {scrolled}");
    for _ in 0..8 {
        h.step();
    }
    assert_eq!(first(&h), scrolled, "stays scrolled");

    // The arrows still bring the line they choose into view.
    press(&mut h, egui::Modifiers::NONE, Key::ArrowDown);
    press(&mut h, egui::Modifiers::NONE, Key::ArrowUp);
    assert!(first(&h) > scrolled + 50.0, "back to the chosen line");
}

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
fn shows_a_files_history_and_blame_beside_its_diff() {
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
        app.repo().unwrap().open_history().is_some()
    });
    let history = |h: &Harness<'_, App>| -> Vec<String> {
        h.state()
            .repo()
            .unwrap()
            .open_history()
            .unwrap()
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| c.summary.clone())
            .collect()
    };
    assert_eq!(
        history(&h),
        [
            "Redraw icon and rename loader",
            "Reject bare repositories when loading",
            "Add repository loader"
        ],
        "follows the rename"
    );
    assert!(h.state().settings().diff_history, "remembered");

    // Blame as well, in the whole file: who last changed each line.
    h.get_by_label("Whole file").click();
    h.get_by_label("Blame").click();
    wait_for(&mut h, "the blame", |app| {
        app.repo().unwrap().open_blame().is_some()
    });
    let blame = h
        .state()
        .repo()
        .unwrap()
        .open_blame()
        .unwrap()
        .clone()
        .unwrap();
    let commits: std::collections::HashSet<&str> =
        blame.commits.iter().map(|c| c.summary.as_str()).collect();
    assert!(
        commits.contains("Reject bare repositories when loading"),
        "{commits:?}"
    );
    settle(&mut h);
    screenshot(&mut h, "34-blame");

    // An older entry shows its change to the file, by its old name; the
    // list stays, and the blame follows.
    h.get_all_by_role_and_label(egui::accesskit::Role::Button, "Add repository loader")
        .next()
        .unwrap()
        .click();
    settle(&mut h);
    assert_eq!(
        selected_summary(&h).as_deref(),
        Some("Add repository loader")
    );
    assert_eq!(
        h.state().repo().unwrap().active_file.as_deref(),
        Some("src/loader.rs")
    );
    assert_eq!(history(&h).len(), 3);
    wait_for(&mut h, "the older blame", |app| {
        app.repo().unwrap().open_blame().is_some()
    });
    settle(&mut h);
    screenshot(&mut h, "33-file-history");
    // A commit in the blame goes to it.
    h.get_all_by_label_contains("Add repository loader")
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("Commit "))
        })
        .unwrap()
        .click();
    settle(&mut h);
    assert!(h.state().repo().unwrap().active_file.is_none());
    assert_eq!(
        selected_summary(&h).as_deref(),
        Some("Add repository loader")
    );
}

/// Issue #29: picking another file of a commit picked in the history
/// shows that file's history, with the commit still picked in it.
#[test]
fn the_history_follows_another_file_picked_in_the_commit() {
    let sample = Sample::build();
    let mut h = harness(Settings {
        diff_history: true,
        ..Settings::default()
    });
    open(&mut h, sample.path());
    h.get_by_label("Redraw icon and rename loader").click();
    settle(&mut h);
    h.get_by_label_contains("src/repo_loader.rs").click();
    wait_for(&mut h, "the history", |app| {
        app.repo().unwrap().open_history().is_some()
    });
    // Back to the loader's first commit, from the list.
    h.get_all_by_role_and_label(egui::accesskit::Role::Button, "Add repository loader")
        .next()
        .unwrap()
        .click();
    settle(&mut h);
    // That commit's other file: the list is README.md's now.
    h.get_by_label("README.md").click();
    wait_for(&mut h, "README's history", |app| {
        let r = app.repo().unwrap();
        r.history_file.as_deref() == Some("README.md") && r.open_history().is_some()
    });
    let list: Vec<String> = h
        .state()
        .repo()
        .unwrap()
        .open_history()
        .unwrap()
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| c.summary.clone())
        .collect();
    assert!(
        list.contains(&"Add repository loader".to_string()),
        "{list:?}"
    );
    assert!(
        !list.contains(&"Redraw icon and rename loader".to_string()),
        "{list:?}"
    );
    assert_eq!(
        selected_summary(&h).as_deref(),
        Some("Add repository loader")
    );
}

#[test]
fn lines_of_a_diff_can_be_picked_and_copied() {
    let sample = Sample::build();
    let mut h = harness(Settings {
        whole_file: true,
        ..Settings::default()
    });
    open(&mut h, sample.path());
    h.get_by_label("Reject bare repositories when loading")
        .click();
    settle(&mut h);
    h.get_by_label("src/loader.rs").click();
    settle(&mut h);
    let lines = |h: &Harness<'_, App>| h.state().repo().unwrap().picked.len();
    // A click picks a line, unchanged or not; another click, just that one.
    let all: Vec<String> = h
        .get_all_by_label_contains("Select line: ")
        .map(|n| n.accesskit_node().label().unwrap().to_string())
        .collect();
    assert!(all.len() >= 4, "{all:?}");
    h.get_by_label(&all[0]).click();
    h.run_ok();
    h.get_by_label(&all[1]).click();
    h.run_ok();
    assert_eq!(lines(&h), 1);
    // Shift+click picks the range.
    h.get_by_label(&all[3])
        .click_modifiers(egui::Modifiers::SHIFT);
    h.run_ok();
    assert_eq!(lines(&h), 3);
    // Right-click one of them for what to do with them.
    h.get_by_label(&all[2]).click_secondary();
    h.run_ok();
    assert!(h.query_by_label("Copy").is_some());
    h.key_press(Key::Escape);
    h.run_ok();
    // Ctrl+click takes one out.
    h.get_by_label(&all[2])
        .click_modifiers(egui::Modifiers::COMMAND);
    h.run_ok();
    assert_eq!(lines(&h), 2);
    let text = h.state().repo().unwrap().picked_text().unwrap();
    let want = |l: &str| l.trim_start_matches("Select line: ").to_string();
    assert!(text.contains(&want(&all[1])) && text.contains(&want(&all[3])));
    // Dragging over lines picks them all.
    let from = h.get_by_label(&all[0]).rect().center();
    let to = h.get_by_label(&all[3]).rect().center();
    drag(&mut h, from, to);
    h.run_ok();
    assert_eq!(lines(&h), 4);
    screenshot(&mut h, "77-picked-lines");
    // Esc lets go of them first, then closes the diff.
    h.key_press(Key::Escape);
    h.run_ok();
    assert!(h.state().repo().unwrap().picked.is_empty());
    assert!(h.state().repo().unwrap().active_file.is_some());
}

#[test]
fn switches_theme_and_size_in_settings() {
    use git_raccoon::settings::{ThemeChoice, ThemeMode};
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
    // The same look, light.
    h.get_by_label("Light").click();
    h.run_ok();
    assert_eq!(h.state().settings().theme, ThemeChoice::Raccoon);
    assert_eq!(h.state().settings().theme_mode, ThemeMode::Light);
    assert!(h.ctx.theme() == egui::Theme::Light);
    h.get_by_label("Close settings").click();
    settle(&mut h);
    screenshot(&mut h, "35-light-diff");
    press(&mut h, egui::Modifiers::NONE, Key::Escape);
    settle(&mut h);
    screenshot(&mut h, "36-light-graph");

    // Bigger, then back to normal and dark.
    h.get_by_label("Settings").click();
    h.run_ok();
    scroll_and_click(&mut h, "125%");
    assert!((h.ctx.zoom_factor() - 1.25).abs() < 0.001);
    assert_eq!(h.state().settings().zoom, Some(1.25));
    screenshot(&mut h, "37-settings");
    scroll_and_click(&mut h, "100%");
    scroll_and_click(&mut h, "Dark");
    assert_eq!(h.state().settings().zoom, None);
    assert!(h.ctx.theme() == egui::Theme::Dark);
}

#[test]
fn zooms_with_ctrl_and_the_keys_or_the_wheel() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    let zoom = |h: &Harness<'_, App>| h.state().settings().zoom;

    press(&mut h, egui::Modifiers::COMMAND, Key::Equals);
    h.run_ok();
    assert_eq!(zoom(&h), Some(1.1));
    assert!((h.ctx.zoom_factor() - 1.1).abs() < 0.001, "applied");
    press(&mut h, egui::Modifiers::COMMAND, Key::Minus);
    press(&mut h, egui::Modifiers::COMMAND, Key::Minus);
    assert_eq!(zoom(&h), Some(0.9));
    // Ctrl+scroll (egui hands it over as a zoom).
    h.event(egui::Event::Zoom(1.5));
    h.run_ok();
    assert_eq!(zoom(&h), Some(1.35));
    // In a dialog too; Ctrl+0 for normal size.
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    h.get_by_label("135%");
    press(&mut h, egui::Modifiers::COMMAND, Key::Num0);
    h.run_ok();
    assert_eq!(zoom(&h), None);
    assert!((h.ctx.zoom_factor() - 1.0).abs() < 0.001);
}

#[test]
fn shows_whats_new_after_an_update_and_takes_feedback() {
    let current = env!("CARGO_PKG_VERSION");
    // A fresh install: nothing to show, but the version is noted.
    let h = harness(Settings::default());
    assert!(h.query_by_label("Close what's new").is_none());
    assert_eq!(h.state().settings().last_version.as_deref(), Some(current));
    drop(h);

    // Updated from an older version: what's new, once.
    let mut h = harness(Settings {
        last_version: Some("0.4.0".into()),
        ..Settings::default()
    });
    h.run_ok();
    h.get_by_label(&format!("Version {current}"));
    screenshot(&mut h, "48-whats-new");
    h.get_by_label("Nice").click();
    h.run_ok();
    assert!(h.query_by_label("Close what's new").is_none());
    assert_eq!(h.state().settings().last_version.as_deref(), Some(current));

    // Feedback, from the toolbar: kind, title, details, then on to GitHub.
    let sample = Sample::build();
    open(&mut h, sample.path());
    h.get_by_label("Send feedback: a bug, a thought or an idea")
        .click();
    h.run_ok();
    h.get_by_label("Idea").click();
    h.run_ok();
    type_into(&mut h, "Feedback title", "Raccoon-shaped cursor");
    type_into(&mut h, "Feedback details", "For fun.");
    // A screenshot of the window as it was (this build can't upload it, so
    // it's copied, for pasting into the issue).
    h.get_by_label("Copy a screenshot of the window, to paste into the issue")
        .click();
    h.run_ok();
    h.get_by_label_contains("It's public with the issue");
    screenshot(&mut h, "49-feedback");
    h.get_by_label("Continue on GitHub").click();
    h.step();
    let copied = h
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| match c {
            egui::OutputCommand::CopyImage(image) => Some(image.size),
            _ => None,
        });
    let [w, _] = copied.expect("the screenshot was copied");
    assert!(w > 100, "the whole window");
    h.run_ok();
    assert!(h.query_by_label("Feedback title").is_none(), "closed");
}

#[test]
fn feedback_is_sent_as_the_person_when_signed_in_to_github() {
    use git_raccoon::forge::{Account, Kind};
    let account = Account {
        kind: Kind::GitHub,
        base: "https://github.com".into(),
        login: "rocky".into(),
        client: None,
    };
    let mut h = harness(Settings {
        accounts: vec![account],
        ..Settings::default()
    });
    let sample = Sample::build();
    open(&mut h, sample.path());
    h.get_by_label("Send feedback: a bug, a thought or an idea")
        .click();
    h.run_ok();
    // Posted from here, with their own account: no "continue on GitHub",
    // and no "how to reach you" (replies reach them there).
    h.get_by_label_contains("as you, @rocky");
    assert!(h.query_by_label("Continue on GitHub").is_none());
    assert!(h.query_by_label("How to reach you").is_none());
    h.get_by_label("Send");
    screenshot(&mut h, "85-feedback-as-you");
}

#[test]
fn renders_every_theme() {
    use git_raccoon::settings::{ThemeChoice, ThemeMode};
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    h.get_by_label("Reject bare repositories when loading")
        .click();
    settle(&mut h);
    h.get_by_label("src/loader.rs").click();
    settle(&mut h);
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    // Every look, dark and light.
    for (mode, mode_name) in [(ThemeMode::Light, "Light"), (ThemeMode::Dark, "Dark")] {
        h.get_by_label(mode_name).click();
        h.run_ok();
        assert_eq!(h.state().settings().theme_mode, mode);
        for (choice, name) in [
            (ThemeChoice::Raccoon, "Raccoon"),
            (ThemeChoice::Classic, "Classic"),
            (ThemeChoice::Pastel, "Pastel"),
            (ThemeChoice::Forest, "Forest"),
            (ThemeChoice::Paper, "Paper"),
            (ThemeChoice::HighContrast, "High contrast"),
        ] {
            h.get_by_label(name).click();
            h.run_ok();
            assert_eq!(h.state().settings().theme, choice);
            assert_eq!(
                h.ctx.theme() == egui::Theme::Light,
                mode == ThemeMode::Light
            );
            if name == "Raccoon" {
                screenshot(
                    &mut h,
                    &format!("38-theme-picker-{}", mode_name.to_lowercase()),
                );
            }
            h.get_by_label("Close settings").click();
            settle(&mut h);
            screenshot(
                &mut h,
                &format!(
                    "39-theme-{}-{}",
                    name.replace(' ', "-").to_lowercase(),
                    mode_name.to_lowercase()
                ),
            );
            press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
        }
    }
    // Following the system.
    h.get_by_label("Match system").click();
    h.run_ok();
    assert_eq!(h.state().settings().theme_mode, ThemeMode::System);
    screenshot(&mut h, "38-theme-picker-system");
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
    let other = tab(&h, &name_b);
    let to = from.center() + egui::vec2(from.width() * 1.6, 0.0);
    h.event(egui::Event::PointerMoved(from.center()));
    h.run_ok();
    h.event(egui::Event::PointerButton {
        pos: from.center(),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_ok();
    for i in 1..=10 {
        h.event(egui::Event::PointerMoved(
            from.center() + (to - from.center()) * (i as f32 / 10.0),
        ));
        h.step();
    }
    h.step();
    // While it's held, it follows the pointer and the other slides over to
    // make room; nothing has moved yet.
    assert_eq!(tab_names(&h).0, vec![name_a.clone(), name_b.clone()]);
    let held = tab(&h, &name_a);
    assert!((held.center().x - to.x).abs() < 1.0, "under the pointer");
    let made_room = tab(&h, &name_b);
    assert!(
        (made_room.left() - from.left()).abs() < 1.0,
        "slid from {} to {}",
        other.left(),
        made_room.left()
    );
    screenshot(&mut h, "41-tab-drag");
    h.event(egui::Event::PointerButton {
        pos: to,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_ok();
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
fn settings_are_in_sections_and_can_be_searched() {
    use egui::accesskit::Role;
    use git_raccoon::settings::{PullStrategy, ThemeMode};
    let mut h = harness(Settings::default());
    h.run_ok();
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    // It opens on Appearance.
    h.get_by_label("Light or dark");
    assert!(
        h.query_by_label("Terminal command").is_none(),
        "one section at a time"
    );
    // Git: choose rebasing, then put the section back as it was.
    h.get_by_role_and_label(Role::Button, "Git").click();
    h.run_ok();
    h.get_by_label("Rebase my commits on top").click();
    h.run_ok();
    assert_eq!(h.state().settings().pull.strategy, PullStrategy::Rebase);
    h.get_by_label("Side by side").click();
    h.run_ok();
    assert!(h.state().settings().split_diff);
    screenshot(&mut h, "63-settings-git");
    h.get_by_label("Reset Git to defaults").click();
    h.run_ok();
    assert_eq!(
        h.state().settings().pull.strategy,
        PullStrategy::FastForwardOnly
    );
    assert!(!h.state().settings().split_diff);
    // Searching finds a setting in any section.
    type_into(&mut h, "Search settings", "terminal");
    h.get_by_label("Terminal command");
    assert!(h.query_by_label("Light or dark").is_none());
    screenshot(&mut h, "64-settings-search");
    h.get_by_label("Search settings").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Search settings")
        .type_text("no such setting");
    h.run_ok();
    h.get_by_label_contains("No settings match");
    // Clearing it goes back to the section it was on (Git).
    h.get_by_label("Search settings").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.key_press(Key::Backspace);
    h.run_ok();
    h.get_by_label("When both sides have new commits");
    h.get_by_role_and_label(Role::Button, "Appearance").click();
    h.run_ok();
    h.get_by_label("Match system").click();
    h.run_ok();
    assert_eq!(h.state().settings().theme_mode, ThemeMode::System);
    h.get_by_label("Close settings").click();
    h.run_ok();
    assert!(h.query_by_label("Search settings").is_none(), "closed");
}

/// Scrolls an item into view (as a person would), then clicks it.
fn scroll_and_click(h: &mut Harness<'_, App>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    h.run_ok();
    h.get_by_label(label).click();
    h.run_ok();
}

#[test]
fn makes_edits_and_deletes_a_custom_theme() {
    use egui::accesskit::Role;
    use git_raccoon::settings::ThemeChoice;
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    // A new theme starts from the look in use, and is used straight away.
    h.get_by_label("New theme").click();
    h.run_ok();
    assert_eq!(h.state().settings().themes.len(), 1);
    assert_eq!(
        h.state().settings().custom_theme.as_deref(),
        Some("My theme")
    );
    h.get_by_label("Theme name");
    // Its accent, typed: the app takes it as it's typed.
    let set = |h: &mut Harness<'_, App>, field: &str, hex: &str| {
        h.get_by_label(field).click();
        h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
        h.get_by_label(field).type_text(hex);
        h.run_ok();
    };
    set(&mut h, "Accent colour", "#6fa8ff");
    let theme = &h.state().settings().themes[0];
    assert_eq!(theme.dark.accent.text(), "#6fa8ff");
    assert_eq!(
        theme.light.accent, theme.light.accent,
        "the light version is its own"
    );
    set(&mut h, "Background colour", "#141821");
    set(&mut h, "Panels colour", "#1b2130");
    // Hard-to-read text is pointed out, and fixed in one click.
    set(&mut h, "Secondary text colour", "#3a4050");
    h.get_by_label_contains("Secondary text on panels is");
    screenshot(&mut h, "65-theme-editor");
    h.get_by_label("Make secondary text on panels readable")
        .click();
    h.run_ok();
    let dark = &h.state().settings().themes[0].dark;
    assert!(
        dark.problems()
            .iter()
            .all(|p| p.what != "Secondary text on panels")
    );
    // Renamed; the name in use follows.
    h.get_by_label("Theme name").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Theme name").type_text("Midnight Raccoon");
    h.run_ok();
    assert_eq!(
        h.state().settings().custom_theme.as_deref(),
        Some("Midnight Raccoon")
    );
    // Back: a built-in look, then the theme again, from its swatch.
    scroll_and_click(&mut h, "Done");
    h.run_ok();
    h.get_by_role_and_label(Role::RadioButton, "Forest").click();
    h.run_ok();
    assert_eq!(h.state().settings().theme, ThemeChoice::Forest);
    assert_eq!(h.state().settings().custom_theme, None);
    h.get_by_role_and_label(Role::RadioButton, "Midnight Raccoon")
        .scroll_to_me();
    h.run_ok();
    h.get_by_role_and_label(Role::RadioButton, "Midnight Raccoon")
        .click();
    h.run_ok();
    assert_eq!(
        h.state().settings().custom_theme.as_deref(),
        Some("Midnight Raccoon")
    );
    screenshot(&mut h, "66-your-themes");
    // Deleting asks once more.
    scroll_and_click(&mut h, "Edit Midnight Raccoon");
    h.run_ok();
    scroll_and_click(&mut h, "Delete theme…");
    h.run_ok();
    assert_eq!(h.state().settings().themes.len(), 1, "not yet");
    scroll_and_click(&mut h, "Delete Midnight Raccoon");
    h.run_ok();
    assert!(h.state().settings().themes.is_empty());
    assert_eq!(h.state().settings().custom_theme, None);
    h.get_by_label("Light or dark");
}

#[test]
fn draws_branch_lines_the_chosen_way() {
    use egui::accesskit::Role;
    use git_raccoon::settings::{BranchLines, LineStyle};
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    assert_eq!(h.state().settings().branch_lines, BranchLines::default());
    assert_eq!(h.state().settings().branch_lines.starts, LineStyle::Curved);
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    let pick = |h: &mut Harness<'_, App>, name: &str| {
        h.get_by_role_and_label(Role::RadioButton, name)
            .scroll_to_me();
        h.run_ok();
        h.get_by_role_and_label(Role::RadioButton, name).click();
        h.run_ok();
    };
    // Rounded merges, curved branch starts: each its own way.
    pick(&mut h, "Rounded merges");
    assert_eq!(
        h.state().settings().branch_lines,
        BranchLines {
            starts: LineStyle::Curved,
            merges: LineStyle::Rounded,
            corner: 7.0,
        }
    );
    screenshot(&mut h, "67-branch-lines-setting");
    h.get_by_label("Close settings").click();
    settle(&mut h);
    screenshot(&mut h, "68-branch-lines-rounded-merges");
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    pick(&mut h, "Rounded branch starts");
    assert_eq!(h.state().settings().branch_lines.starts, LineStyle::Rounded);
    h.get_by_label("Close settings").click();
    settle(&mut h);
    screenshot(&mut h, "68-branch-lines-rounded-both");
    // Square corners: the roundness set to 0.
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    h.get_by_role_and_label(Role::Slider, "Corner roundness")
        .scroll_to_me();
    h.run_ok();
    h.get_by_role_and_label(Role::Slider, "Corner roundness")
        .focus();
    h.run_ok();
    // Step by step with the arrow key, past the bottom.
    for _ in 0..10 {
        h.key_press(Key::ArrowLeft);
        h.run_ok();
    }
    assert_eq!(h.state().settings().branch_lines.corner, 0.0);
    screenshot(&mut h, "69-square-corners-setting");
    h.get_by_label("Close settings").click();
    settle(&mut h);
    screenshot(&mut h, "69-square-corners");
    // Curved at 0: straight across.
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    h.get_by_role_and_label(Role::RadioButton, "Curved merges")
        .scroll_to_me();
    h.run_ok();
    h.get_by_role_and_label(Role::RadioButton, "Curved merges")
        .click();
    h.get_by_role_and_label(Role::RadioButton, "Curved branch starts")
        .click();
    h.run_ok();
    assert_eq!(h.state().settings().branch_lines.merges, LineStyle::Curved);
    h.get_by_label("Close settings").click();
    settle(&mut h);
    screenshot(&mut h, "69-curved-flat");
    // Found by what people call them, too.
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    type_into(&mut h, "Search settings", "curves");
    h.get_by_label("Where a branch starts");
    h.get_by_label("Where a branch is merged");
}

#[test]
fn copies_branch_and_tag_names() {
    let sample = Sample::build();
    let mut h = harness(Settings::default());
    open(&mut h, sample.path());
    settle(&mut h);
    // Clicks "Copy name" in the open menu, and says what was copied.
    let copy = |h: &mut Harness<'_, App>| -> String {
        h.get_by_label("Copy name").click();
        h.step();
        let copied = h
            .output()
            .platform_output
            .commands
            .iter()
            .find_map(|c| match c {
                egui::OutputCommand::CopyText(text) => Some(text.clone()),
                _ => None,
            })
            .expect("something was copied");
        h.run_ok();
        copied
    };
    // A branch in the sidebar (shown in its folder, copied whole).
    h.get_by_label("toolbar-glitch").click_secondary();
    h.run_ok();
    assert_eq!(copy(&mut h), "fix/ui/toolbar-glitch");
    // A tag in the sidebar.
    h.get_all_by_label("v0.1.0")
        .last()
        .unwrap()
        .click_secondary();
    h.run_ok();
    assert_eq!(copy(&mut h), "v0.1.0");
    // A branch's label on the graph.
    let row = h.get_by_label("Colour lanes").rect();
    let label = egui::pos2(row.left() + 30.0, row.center().y);
    h.event(egui::Event::PointerMoved(label));
    h.run_ok();
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton {
            pos: label,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_ok();
    }
    screenshot(&mut h, "70-copy-name");
    assert_eq!(copy(&mut h), "feature/graph");
}

#[test]
fn shows_the_licenses() {
    let mut h = harness(Settings::default());
    h.run_ok();
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "About")
        .click();
    h.run_ok();
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
