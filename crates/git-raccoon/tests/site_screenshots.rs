//! The screenshots on the project website (site/screenshots), taken from
//! the real app against a demo project. Not run with the other tests:
//!
//! ```bash
//! cargo test -p git-raccoon --test site_screenshots -- --ignored
//! ```

#![expect(
    clippy::unwrap_used,
    reason = "a screenshot script: a failure should simply stop it"
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui::Key;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use git_raccoon::App;
use git_raccoon::settings::{Settings, ThemeChoice, ThemeMode};
use git2::{Oid, Repository, Signature, Time, build::CheckoutBuilder};

const DAY: i64 = 86_400;

const ROCKY: (&str, &str) = ("Rocky Raccoon", "rocky@example.com");
const ADA: (&str, &str) = ("Ada Bandit", "ada@example.com");
const MILO: (&str, &str) = ("Milo Mask", "milo@example.com");

/// The demo project: "stash-box", a tiny key-value store in Rust.
struct Demo {
    dir: PathBuf,
    repo: Repository,
    now: i64,
}

impl Demo {
    fn write(&self, file: &str, contents: &str) {
        let p = self.dir.join(file);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, contents).unwrap();
    }

    fn commit_with(&self, who: (&str, &str), message: &str, days_ago: f64, extra: &[Oid]) -> Oid {
        let mut index = self.repo.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
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
        let when = Time::new(self.now - (days_ago * DAY as f64) as i64, 0);
        let sig = Signature::new(who.0, who.1, &when).unwrap();
        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &refs)
            .unwrap()
    }

    fn commit(&self, who: (&str, &str), message: &str, days_ago: f64) -> Oid {
        self.commit_with(who, message, days_ago, &[])
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

    fn merge(&self, who: (&str, &str), branch: &str, days_ago: f64) -> Oid {
        let theirs = self
            .repo
            .find_branch(branch, git2::BranchType::Local)
            .unwrap()
            .get()
            .peel_to_commit()
            .unwrap();
        let ours = self.repo.head().unwrap().peel_to_commit().unwrap();
        let mut index = self.repo.merge_commits(&ours, &theirs, None).unwrap();
        index.write_tree_to(&self.repo).unwrap();
        // Check the merged tree out, then commit it with both parents.
        self.repo
            .checkout_index(Some(&mut index), Some(CheckoutBuilder::new().force()))
            .unwrap();
        self.commit_with(
            who,
            &format!("Merge branch '{branch}'"),
            days_ago,
            &[theirs.id()],
        )
    }

    fn tag(&self, name: &str) {
        let head = self.repo.head().unwrap().peel_to_commit().unwrap();
        self.repo
            .tag_lightweight(name, head.as_object(), false)
            .unwrap();
    }

    fn build() -> Self {
        let dir = std::env::temp_dir()
            .join(format!("git-raccoon-site-{}", std::process::id()))
            .join("stash-box");
        let _ = std::fs::remove_dir_all(&dir);
        let repo = Repository::init(&dir).unwrap();
        repo.set_head("refs/heads/main").unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", ROCKY.0).unwrap();
        config.set_str("user.email", ROCKY.1).unwrap();
        config.set_bool("commit.gpgsign", false).unwrap();
        let d = Demo {
            dir,
            repo,
            now: git_raccoon::settings::now(),
        };

        d.write("README.md", README_1);
        d.write("Cargo.toml", CARGO);
        d.write("src/lib.rs", LIB_1);
        d.commit(ROCKY, "Start stash-box", 41.0);
        d.write("src/store.rs", STORE_1);
        d.write("src/lib.rs", LIB_2);
        d.commit(ROCKY, "Add an in-memory store", 39.0);
        d.write("src/main.rs", MAIN_1);
        d.commit(ADA, "Add a command-line interface", 36.0);
        d.tag("v0.1.0");

        d.checkout("feature/persistence");
        d.write("src/disk.rs", DISK_1);
        d.commit(MILO, "Write the store to disk", 34.0);
        d.write("src/disk.rs", DISK_2);
        d.commit(MILO, "Load the store on start", 31.0);

        d.checkout("main");
        d.write("README.md", README_2);
        d.commit(ADA, "Document the API", 32.0);
        d.merge(ROCKY, "feature/persistence", 29.0);
        d.write("src/cache.rs", CACHE_1);
        d.commit(ADA, "Add an LRU cache in front of the store", 27.0);
        d.tag("v0.2.0");

        d.checkout("feature/expiry");
        d.write("src/expiry.rs", EXPIRY_1);
        d.commit(MILO, "Expire keys after a time to live", 20.0);
        d.write("src/expiry.rs", EXPIRY_2);
        d.commit(MILO, "Sweep expired keys in the background", 11.0);

        d.checkout("main");
        d.write("src/store.rs", STORE_2);
        d.commit(
            ROCKY,
            "Find keys through a hash index\n\nLookups were a linear scan of the log, which got slow past a few\nthousand keys. The index maps each key to its latest entry.",
            17.0,
        );
        d.write("src/disk.rs", DISK_3);
        d.commit(ADA, "Handle an empty store file", 14.0);

        d.checkout("fix/flush-on-exit");
        d.write("src/main.rs", MAIN_2);
        d.commit(ADA, "Flush pending writes on exit", 5.0);

        d.checkout("main");
        d.write("src/store.rs", STORE_3);
        d.commit(ROCKY, "Compact the log when it grows", 8.0);
        let pushed = d.repo.head().unwrap().target().unwrap();
        d.write("Cargo.toml", &CARGO.replace("0.2.0", "0.3.0"));
        d.commit(ROCKY, "Release 0.3.0", 2.0);

        d.repo
            .reference("refs/remotes/origin/main", pushed, true, "demo")
            .unwrap();
        d.repo
            .remote("origin", "https://github.com/rocky/stash-box.git")
            .unwrap();
        let mut config = d.repo.config().unwrap();
        config.set_str("branch.main.remote", "origin").unwrap();
        config
            .set_str("branch.main.merge", "refs/heads/main")
            .unwrap();
        d
    }
}

impl Drop for Demo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.dir.parent().unwrap());
    }
}

fn harness(theme: ThemeChoice, theme_mode: ThemeMode) -> Harness<'static, App> {
    harness_with(Settings {
        theme,
        theme_mode,
        split_diff: true,
        no_update_checks: true,
        // Not signed in to the demo's GitHub: no sign-in prompts in the way.
        sidebar_hidden: vec!["requests".into(), "issues".into()],
        ..Settings::default()
    })
}

fn harness_with(settings: Settings) -> Harness<'static, App> {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 800.0))
        .with_pixels_per_point(2.0)
        .wgpu()
        .build_eframe(move |cc| App::with_settings(&cc.egui_ctx, settings))
}

fn settle(h: &mut Harness<'_, App>) {
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

fn out_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../site/screenshots")
}

/// Saves a screenshot, with the pointer moved out of the window first.
fn shot(h: &mut Harness<'_, App>, name: &str) {
    h.event(egui::Event::PointerGone);
    h.run_ok();
    let image = h.render().expect("render");
    std::fs::create_dir_all(out_dir()).unwrap();
    image.save(out_dir().join(format!("{name}.png"))).unwrap();
}

/// A smaller screenshot (half size), for previews.
fn small_shot(h: &mut Harness<'_, App>, name: &str) {
    h.event(egui::Event::PointerGone);
    h.run_ok();
    let image = h.render().expect("render");
    let (w, h) = (image.width() / 2, image.height() / 2);
    let small = image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle);
    std::fs::create_dir_all(out_dir()).unwrap();
    small.save(out_dir().join(format!("{name}.png"))).unwrap();
}

fn open(h: &mut Harness<'_, App>, path: &Path) {
    let ctx = h.ctx.clone();
    h.state_mut().open(&ctx, path.to_str().unwrap());
    settle(h);
    assert!(h.state().repo().is_some(), "{:?}", h.state().error());
}

fn click(h: &mut Harness<'_, App>, label: &str) {
    h.get_by_label(label).click();
    settle(h);
}

fn press(h: &mut Harness<'_, App>, modifiers: egui::Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run_ok();
}

const INDEX_COMMIT: &str = "Find keys through a hash index";

#[test]
#[ignore = "writes the website's screenshots"]
fn website_screenshots() {
    let demo = Demo::build();

    // The graph, with a commit's details.
    let mut h = harness(ThemeChoice::Raccoon, ThemeMode::Dark);
    open(&mut h, &demo.dir);
    click(&mut h, INDEX_COMMIT);
    shot(&mut h, "history");

    // The command palette.
    press(&mut h, egui::Modifiers::COMMAND, Key::K);
    h.get_by_label("Command").type_text("check feat");
    h.run_ok();
    shot(&mut h, "palette");
    press(&mut h, egui::Modifiers::NONE, Key::Escape);

    // Interactive rebase of the commits after the cache.
    h.get_all_by_label("Add an LRU cache in front of the store")
        .next()
        .unwrap()
        .click_secondary();
    h.run_ok();
    h.get_by_label_contains("Interactive rebase from here")
        .click();
    settle(&mut h);
    click(&mut h, "Squash Handle an empty store file");
    click(&mut h, "Reword Release 0.3.0");
    h.get_by_label("Message for Release 0.3.0").click();
    h.run_ok();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Message for Release 0.3.0")
        .type_text("Release stash-box 0.3.0");
    h.run_ok();
    shot(&mut h, "rebase");
    click(&mut h, "Cancel");

    // Its diff, side by side.
    click(&mut h, "src/store.rs");
    shot(&mut h, "diff");

    // The whole file, with its blame and history beside it.
    click(&mut h, "Unified");
    click(&mut h, "Whole file");
    click(&mut h, "Blame");
    click(&mut h, "History");
    let start = Instant::now();
    while h
        .state()
        .repo()
        .is_none_or(|r| r.open_blame().is_none() || r.open_history().is_none())
    {
        assert!(start.elapsed() < Duration::from_secs(20), "no blame");
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    // Room for the code: without the sidebar.
    press(&mut h, egui::Modifiers::COMMAND, Key::B);
    settle(&mut h);
    shot(&mut h, "blame");
    press(&mut h, egui::Modifiers::COMMAND, Key::B);
    h.key_press(Key::Escape);
    settle(&mut h);
    h.key_press(Key::Escape);
    settle(&mut h);

    drop(h);

    // The same view in other themes (before the edits below).
    // Every look, dark and light.
    for (theme, name) in [
        (ThemeChoice::Raccoon, "raccoon"),
        (ThemeChoice::Classic, "classic"),
        (ThemeChoice::Pastel, "pastel"),
        (ThemeChoice::Forest, "forest"),
        (ThemeChoice::Paper, "paper"),
        (ThemeChoice::HighContrast, "high-contrast"),
    ] {
        for (mode, mode_name) in [(ThemeMode::Dark, "dark"), (ThemeMode::Light, "light")] {
            let mut h = harness(theme, mode);
            open(&mut h, &demo.dir);
            click(&mut h, INDEX_COMMIT);
            small_shot(&mut h, &format!("theme-{name}-{mode_name}"));
        }
    }

    // Working changes, back in the Raccoon theme: stage some lines of an edit.
    let mut h = harness(ThemeChoice::Raccoon, ThemeMode::Dark);
    open(&mut h, &demo.dir);
    demo.write("src/cache.rs", CACHE_2);
    demo.write(
        "CHANGELOG.md",
        "# Changelog\n\n## 0.3.0\n\n- Faster lookups.\n",
    );
    let mut index = demo.repo.index().unwrap();
    index.add_path(Path::new("CHANGELOG.md")).unwrap();
    index.write().unwrap();
    let start = Instant::now();
    while h.query_by_label("Working changes").is_none() {
        assert!(start.elapsed() < Duration::from_secs(20), "no WIP row");
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    click(&mut h, "Working changes");
    click(&mut h, "src/cache.rs");
    shot(&mut h, "staging");
    drop(h);

    // Making a theme of your own.
    let mut h = harness(ThemeChoice::Raccoon, ThemeMode::Dark);
    open(&mut h, &demo.dir);
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    click(&mut h, "New theme");
    for (field, hex) in [
        ("Accent colour", "#6fa8ff"),
        ("Background colour", "#141821"),
        ("Panels colour", "#1b2130"),
        ("Borders colour", "#2b3446"),
    ] {
        h.get_by_label(field).click();
        h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
        h.get_by_label(field).type_text(hex);
        h.run_ok();
    }
    h.get_by_label("Theme name").click();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_label("Theme name").type_text("Midnight Raccoon");
    h.run_ok();
    shot(&mut h, "theme-editor");
    drop(h);

    // Merge requests from a GitLab server, beside the graph.
    requests(&demo);

    // SSH: the settings page, and a push it explains.
    #[cfg(unix)]
    ssh(&demo);

    // Resolving a merge's conflicts, as panes (last: it changes the repo).
    conflicts(&demo);
}

/// Settings › SSH, and a push that failed over SSH, explained.
#[cfg(unix)]
fn ssh(demo: &Demo) {
    use egui::accesskit::Role;
    use std::os::unix::fs::PermissionsExt as _;
    let home = demo.dir.parent().unwrap().join("home");
    let dir = home.join(".ssh");
    git_raccoon_core::ssh::set_ssh_dir(Some(dir.clone()));
    git_raccoon_core::ssh::create_key(&dir, "id_ed25519", "rocky@laptop", Some("a passphrase"))
        .unwrap();
    // Trusted hosts, with their published keys.
    std::fs::write(
        dir.join("known_hosts"),
        "github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl\ngitlab.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAfuCHKVTjquxvt6CM6tdG4SLp1Btn/nOeHHE5UOzRdf\n",
    )
    .unwrap();
    let mut h = harness(ThemeChoice::Raccoon, ThemeMode::Dark);
    open(&mut h, &demo.dir);
    press(&mut h, egui::Modifiers::COMMAND, Key::Comma);
    h.get_by_role_and_label(Role::Button, "SSH").click();
    let start = Instant::now();
    while h.query_by_label("Add id_ed25519 to the agent").is_none() {
        assert!(start.elapsed() < Duration::from_secs(20), "no SSH page");
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    settle(&mut h);
    shot(&mut h, "ssh-settings");
    drop(h);

    // A push the host refuses: why, with the fix.
    let fake = home.join("ssh");
    std::fs::write(
        &fake,
        "#!/bin/sh\necho 'debug1: Offering public key: ~/.ssh/id_ed25519 ED25519 SHA256:x agent' >&2\necho 'git@gitlab.com: Permission denied (publickey).' >&2\nexit 255\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut config = demo.repo.config().unwrap();
    config
        .set_str("core.sshCommand", &fake.to_string_lossy())
        .unwrap();
    demo.repo
        .remote_set_url("origin", "git@gitlab.com:rocky/stash-box.git")
        .unwrap();
    let mut h = harness(ThemeChoice::Raccoon, ThemeMode::Dark);
    open(&mut h, &demo.dir);
    click(&mut h, "Push");
    let start = Instant::now();
    while h
        .query_by_label_contains("didn't accept your SSH key")
        .is_none()
    {
        assert!(start.elapsed() < Duration::from_secs(20), "no SSH problem");
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    settle(&mut h);
    shot(&mut h, "ssh-error");
    drop(h);
    config.remove("core.sshCommand").unwrap();
    demo.repo
        .remote_set_url("origin", "https://github.com/rocky/stash-box.git")
        .unwrap();
    git_raccoon_core::ssh::set_ssh_dir(None);
}

/// A merge that conflicts in two places of the store, resolved as panes.
fn conflicts(demo: &Demo) {
    // Back to main as committed, then two branches that disagree.
    demo.checkout("main");
    let limit = |n: &str, doc: &str| {
        STORE_3
            .replace(
                "const COMPACT_AFTER: usize = 1024;",
                &format!("const COMPACT_AFTER: usize = {n};"),
            )
            .replace(
                "    /// How many keys there are.",
                &format!("    /// {doc}"),
            )
    };
    demo.checkout("feature/compact-later");
    demo.write(
        "src/store.rs",
        &limit("4096", "How many keys there are (not entries)."),
    );
    demo.commit(MILO, "Compact less often", 1.5);
    demo.checkout("main");
    demo.write("src/store.rs", &limit("512", "The number of live keys."));
    demo.commit(ADA, "Compact sooner to keep the file small", 1.0);
    let merged = std::process::Command::new("git")
        .args(["merge", "--no-edit", "feature/compact-later"])
        .current_dir(&demo.dir)
        .output()
        .unwrap();
    assert!(!merged.status.success(), "it should conflict");

    let mut h = harness_with(Settings {
        theme: ThemeChoice::Raccoon,
        theme_mode: ThemeMode::Dark,
        no_update_checks: true,
        conflict_panes: true,
        sidebar_hidden: vec!["requests".into(), "issues".into()],
        ..Settings::default()
    });
    open(&mut h, &demo.dir);
    click(&mut h, "Resolve src/store.rs");
    // main's limit; then both docs, the branch's first.
    click(&mut h, "Take main's conflict 1");
    click(&mut h, "Next conflict");
    click(&mut h, "Take line 1 of feature/compact-later in conflict 2");
    click(&mut h, "Take line 1 of main in conflict 2");
    shot(&mut h, "conflicts");
}

/// A merge request open beside the graph, from a stand-in GitLab.
fn requests(demo: &Demo) {
    use git_raccoon::forge::{Account, Kind, accounts};
    let mr = |iid: u64, title: &str, who: &str, source: &str, body: &str, status: &str| {
        format!(
            r#"{{"iid": {iid}, "title": "{title}", "author": {{"username": "{who}"}},
            "source_branch": "{source}", "target_branch": "main", "source_project_id": 1,
            "target_project_id": 1, "draft": false, "web_url": "https://example.com/mr/{iid}",
            "description": "{body}", "created_at": "2026-11-12T09:30:00Z",
            "user_notes_count": 3, "head_pipeline": {{"status": "{status}"}}}}"#
        )
    };
    let expiry = mr(
        7,
        "Expire keys after a time to live",
        "milo",
        "feature/expiry",
        "Keys can be given a time to live. A background sweep removes them once it has passed, so the store doesn't grow forever.",
        "success",
    );
    let flush = mr(
        9,
        "Flush pending writes on exit",
        "ada",
        "fix/flush-on-exit",
        "Ctrl+C lost the last few writes.",
        "running",
    );
    let api = "/api/v4/projects/rocky%2Fstash-box";
    let base = forge_server(vec![
        ("GET /api/v4/user".into(), r#"{"username": "rocky"}"#.into()),
        (format!("GET {api}/merge_requests?"), format!("[{expiry}, {flush}]")),
        (
            format!("GET {api}/issues?"),
            r#"[{"iid": 11, "title": "Keys with spaces can't be read back", "author": {"username": "ada"},
            "labels": ["bug"], "web_url": "u", "description": "", "created_at": "2026-11-10T00:00:00Z",
            "user_notes_count": 2},
            {"iid": 12, "title": "Compression for large values", "author": {"username": "milo"},
            "labels": ["idea"], "web_url": "u", "description": "", "created_at": "2026-11-14T00:00:00Z",
            "user_notes_count": 0}]"#
                .into(),
        ),
        (format!("GET {api}/merge_requests/7"), expiry.clone()),
        (
            format!("GET {api}/merge_requests/7/approvals"),
            r#"{"approved_by": [{"user": {"username": "ada"}}]}"#.into(),
        ),
        (format!("GET {api}/merge_requests/9"), flush.clone()),
        (format!("GET {api}/merge_requests/9/approvals"), r#"{"approved_by": []}"#.into()),
        // Its view: the file it adds, its commits, and the conversation.
        (format!("GET {api}/merge_requests/7/diffs"), {
            let lines: Vec<&str> = EXPIRY_2.lines().collect();
            let mut patch = format!("@@ -0,0 +1,{} @@\n", lines.len());
            for l in &lines {
                patch.push('+');
                patch.push_str(l);
                patch.push('\n');
            }
            serde_json::json!([{"old_path": "src/expiry.rs", "new_path": "src/expiry.rs",
                "new_file": true, "diff": patch}])
            .to_string()
        }),
        (
            format!("GET {api}/merge_requests/7/commits"),
            r#"[{"id": "9f2c41d0", "title": "Sweep expired keys in the background", "author_name": "Milo",
                "created_at": "2026-11-12T09:00:00Z"},
               {"id": "4be7a113", "title": "Expire keys after a time to live", "author_name": "Milo",
                "created_at": "2026-11-11T16:00:00Z"}]"#
                .into(),
        ),
        (
            format!("GET {api}/merge_requests/7/discussions"),
            serde_json::json!([
                {"id": "d1", "individual_note": true, "notes": [{"id": 1, "system": false,
                    "author": {"username": "ada"}, "created_at": "2026-11-12T10:15:00Z",
                    "body": "Nice! Two things before it goes in:\n\n- the sweep should run **off** the write path\n- a test for a key that's set twice\n\nOtherwise good to go."}]},
                {"id": "d2", "individual_note": false, "notes": [
                    {"id": 2, "system": false, "author": {"username": "ada"}, "created_at": "2026-11-12T10:20:00Z",
                     "resolvable": true, "resolved": false,
                     "body": "Could `retain` do this in one pass, without the second `Vec`?",
                     "position": {"old_path": "src/expiry.rs", "new_path": "src/expiry.rs", "old_line": null, "new_line": 22}},
                    {"id": 3, "system": false, "author": {"username": "milo"}, "created_at": "2026-11-12T11:02:00Z",
                     "body": "It needs the removed keys back, so `retain` alone won't do. I'll try `extract_if` once it's stable."}]},
                {"id": "d3", "individual_note": true, "notes": [{"id": 4, "system": true,
                    "author": {"username": "ada"}, "created_at": "2026-11-12T11:30:00Z",
                    "body": "approved this merge request"}]}
            ])
            .to_string(),
        ),
        (format!("GET {api}/merge_requests/9/diffs"), "[]".into()),
        (format!("GET {api}/merge_requests/9/commits"), "[]".into()),
        (format!("GET {api}/merge_requests/9/discussions"), "[]".into()),
        (format!("GET {api}"), r#"{"default_branch": "main", "squash_option": "default_off"}"#.into()),
    ]);
    demo.repo
        .remote_set_url("origin", &format!("{base}/rocky/stash-box.git"))
        .unwrap();
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
            token: "demo".into(),
            refresh: None,
            expires: None,
        },
    )
    .unwrap();
    let mut h = harness_with(Settings {
        theme: ThemeChoice::Raccoon,
        theme_mode: ThemeMode::Dark,
        no_update_checks: true,
        accounts: vec![account],
        ..Settings::default()
    });
    open(&mut h, &demo.dir);
    let start = Instant::now();
    while h
        .state()
        .repo()
        .and_then(|r| r.forge.as_ref())
        .is_none_or(|f| f.requests.is_none() || f.issues.is_none())
    {
        assert!(start.elapsed() < Duration::from_secs(20), "no requests");
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    h.run_ok();
    click(&mut h, "!7 Expire keys after a time to live");
    let start = Instant::now();
    while h
        .state()
        .repo()
        .and_then(|r| r.forge.as_ref())
        .is_none_or(|f| f.detailed(7).is_none())
    {
        assert!(start.elapsed() < Duration::from_secs(20), "no details");
        h.step();
        std::thread::sleep(Duration::from_millis(20));
    }
    settle(&mut h);
    shot(&mut h, "requests");
}

/// A stand-in GitLab: answers each request by its method and path (the
/// longest matching prefix in `routes`).
fn forge_server(routes: Vec<(String, String)>) -> String {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let routes = std::sync::Arc::new(routes);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let routes = std::sync::Arc::clone(&routes);
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
                let reply = routes
                    .iter()
                    .filter(|(prefix, _)| request.starts_with(prefix.as_str()))
                    .max_by_key(|(prefix, _)| prefix.len())
                    .map_or_else(|| (404, "{}".to_string()), |(_, json)| (200, json.clone()));
                let mut stream = stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    reply.0,
                    reply.1.len(),
                    reply.1
                );
            });
        }
    });
    base
}

const CARGO: &str = r#"[package]
name = "stash-box"
version = "0.2.0"
edition = "2024"
description = "A tiny key-value store"
license = "MIT"
"#;

const README_1: &str = "# stash-box\n\nA tiny key-value store.\n";
const README_2: &str = "# stash-box\n\nA tiny key-value store, kept in a single file.\n\n```rust\nlet mut store = Store::open(\"data.box\")?;\nstore.put(\"raccoon\", b\"trash panda\")?;\nassert_eq!(store.get(\"raccoon\"), Some(&b\"trash panda\"[..]));\n```\n";

const LIB_1: &str = "//! A tiny key-value store.\n";
const LIB_2: &str = "//! A tiny key-value store.\n\nmod store;\n\npub use store::Store;\n";

const STORE_1: &str = r#"/// Keys and values, in the order they were written.
pub struct Store {
    log: Vec<(String, Vec<u8>)>,
}

impl Store {
    pub fn new() -> Self {
        Store { log: Vec::new() }
    }

    /// Stores `value` under `key`, replacing what was there.
    pub fn put(&mut self, key: &str, value: &[u8]) {
        self.log.push((key.to_string(), value.to_vec()));
    }

    /// The latest value written under `key`.
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.log
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_slice())
    }
}
"#;

const STORE_2: &str = r#"use std::collections::HashMap;

/// Keys and values, in the order they were written.
pub struct Store {
    log: Vec<(String, Vec<u8>)>,
    /// Where each key's latest value is in the log.
    index: HashMap<String, usize>,
}

impl Store {
    pub fn new() -> Self {
        Store {
            log: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Stores `value` under `key`, replacing what was there.
    pub fn put(&mut self, key: &str, value: &[u8]) {
        self.index.insert(key.to_string(), self.log.len());
        self.log.push((key.to_string(), value.to_vec()));
    }

    /// The latest value written under `key`.
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        let at = *self.index.get(key)?;
        Some(self.log[at].1.as_slice())
    }

    /// How many keys there are.
    pub fn len(&self) -> usize {
        self.index.len()
    }
}
"#;

const STORE_3: &str = r#"use std::collections::HashMap;

/// Past this many stale entries, the log is compacted.
const COMPACT_AFTER: usize = 1024;

/// Keys and values, in the order they were written.
pub struct Store {
    log: Vec<(String, Vec<u8>)>,
    /// Where each key's latest value is in the log.
    index: HashMap<String, usize>,
}

impl Store {
    pub fn new() -> Self {
        Store {
            log: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Stores `value` under `key`, replacing what was there.
    pub fn put(&mut self, key: &str, value: &[u8]) {
        self.index.insert(key.to_string(), self.log.len());
        self.log.push((key.to_string(), value.to_vec()));
        if self.log.len() - self.index.len() > COMPACT_AFTER {
            self.compact();
        }
    }

    /// The latest value written under `key`.
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        let at = *self.index.get(key)?;
        Some(self.log[at].1.as_slice())
    }

    /// How many keys there are.
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Drops the entries that later writes replaced.
    fn compact(&mut self) {
        let mut log = Vec::with_capacity(self.index.len());
        for (key, at) in self.index.iter_mut() {
            log.push((key.clone(), std::mem::take(&mut self.log[*at].1)));
            *at = log.len() - 1;
        }
        self.log = log;
    }
}
"#;

const MAIN_1: &str = r#"use stash_box::Store;

fn main() {
    let mut store = Store::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, key, value] if cmd == "put" => store.put(key, value.as_bytes()),
        [cmd, key] if cmd == "get" => match store.get(key) {
            Some(value) => println!("{}", String::from_utf8_lossy(value)),
            None => eprintln!("no such key: {key}"),
        },
        _ => eprintln!("usage: stash-box put KEY VALUE | get KEY"),
    }
}
"#;

const MAIN_2: &str = r#"use stash_box::Store;

fn main() -> std::io::Result<()> {
    let mut store = Store::open("data.box")?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd, key, value] if cmd == "put" => store.put(key, value.as_bytes()),
        [cmd, key] if cmd == "get" => match store.get(key) {
            Some(value) => println!("{}", String::from_utf8_lossy(value)),
            None => eprintln!("no such key: {key}"),
        },
        _ => eprintln!("usage: stash-box put KEY VALUE | get KEY"),
    }
    // Pending writes reach the disk even when the process is interrupted.
    store.flush()
}
"#;

const DISK_1: &str = r#"use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use crate::Store;

impl Store {
    /// Writes every key and value to `path`.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut file = File::create(path)?;
        for (key, value) in self.entries() {
            writeln!(file, "{key}\t{}", hex(value))?;
        }
        file.sync_all()
    }
}
"#;

const DISK_2: &str = r#"use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use crate::Store;

impl Store {
    /// Writes every key and value to `path`.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut file = File::create(path)?;
        for (key, value) in self.entries() {
            writeln!(file, "{key}\t{}", hex(value))?;
        }
        file.sync_all()
    }

    /// Reads a store written by `save`.
    pub fn load(path: &Path) -> io::Result<Store> {
        let mut store = Store::new();
        for line in fs::read_to_string(path)?.lines() {
            let (key, value) = line.split_once('\t').ok_or(io::ErrorKind::InvalidData)?;
            store.put(key, &unhex(value)?);
        }
        Ok(store)
    }
}
"#;

const DISK_3: &str = r#"use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use crate::Store;

impl Store {
    /// Writes every key and value to `path`.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut file = File::create(path)?;
        for (key, value) in self.entries() {
            writeln!(file, "{key}\t{}", hex(value))?;
        }
        file.sync_all()
    }

    /// Reads a store written by `save`. A missing or empty file is an
    /// empty store.
    pub fn load(path: &Path) -> io::Result<Store> {
        let mut store = Store::new();
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(store),
            Err(e) => return Err(e),
        };
        for line in text.lines().filter(|l| !l.is_empty()) {
            let (key, value) = line.split_once('\t').ok_or(io::ErrorKind::InvalidData)?;
            store.put(key, &unhex(value)?);
        }
        Ok(store)
    }
}
"#;

const CACHE_1: &str = r#"use std::collections::VecDeque;

/// The most recently read values, so hot keys skip the store.
pub struct Cache {
    capacity: usize,
    entries: VecDeque<(String, Vec<u8>)>,
}

impl Cache {
    pub fn new(capacity: usize) -> Self {
        Cache {
            capacity,
            entries: VecDeque::with_capacity(capacity),
        }
    }

    pub fn get(&mut self, key: &str) -> Option<&[u8]> {
        let at = self.entries.iter().position(|(k, _)| k == key)?;
        let entry = self.entries.remove(at)?;
        self.entries.push_front(entry);
        self.entries.front().map(|(_, v)| v.as_slice())
    }

    pub fn insert(&mut self, key: &str, value: &[u8]) {
        if self.entries.len() == self.capacity {
            self.entries.pop_back();
        }
        self.entries.push_front((key.to_string(), value.to_vec()));
    }
}
"#;

const CACHE_2: &str = r#"use std::collections::VecDeque;

/// The most recently read values, so hot keys skip the store.
pub struct Cache {
    capacity: usize,
    entries: VecDeque<(String, Vec<u8>)>,
    hits: u64,
    misses: u64,
}

impl Cache {
    pub fn new(capacity: usize) -> Self {
        Cache {
            capacity,
            entries: VecDeque::with_capacity(capacity),
            hits: 0,
            misses: 0,
        }
    }

    pub fn get(&mut self, key: &str) -> Option<&[u8]> {
        let Some(at) = self.entries.iter().position(|(k, _)| k == key) else {
            self.misses += 1;
            return None;
        };
        self.hits += 1;
        let entry = self.entries.remove(at)?;
        self.entries.push_front(entry);
        self.entries.front().map(|(_, v)| v.as_slice())
    }

    pub fn insert(&mut self, key: &str, value: &[u8]) {
        if self.entries.len() == self.capacity {
            self.entries.pop_back();
        }
        self.entries.push_front((key.to_string(), value.to_vec()));
    }

    /// The share of reads answered from the cache.
    pub fn hit_rate(&self) -> f64 {
        self.hits as f64 / (self.hits + self.misses).max(1) as f64
    }
}
"#;

const EXPIRY_1: &str = r#"use std::time::{Duration, Instant};

/// When each key stops being valid.
pub struct Expiry {
    deadlines: Vec<(String, Instant)>,
}

impl Expiry {
    pub fn set(&mut self, key: &str, ttl: Duration) {
        self.deadlines.push((key.to_string(), Instant::now() + ttl));
    }

    pub fn is_expired(&self, key: &str) -> bool {
        self.deadlines
            .iter()
            .any(|(k, at)| k == key && *at <= Instant::now())
    }
}
"#;

const EXPIRY_2: &str = r#"use std::time::{Duration, Instant};

/// When each key stops being valid.
pub struct Expiry {
    deadlines: Vec<(String, Instant)>,
}

impl Expiry {
    pub fn set(&mut self, key: &str, ttl: Duration) {
        self.deadlines.push((key.to_string(), Instant::now() + ttl));
    }

    pub fn is_expired(&self, key: &str) -> bool {
        self.deadlines
            .iter()
            .any(|(k, at)| k == key && *at <= Instant::now())
    }

    /// Removes the keys whose time is up, and returns them.
    pub fn sweep(&mut self) -> Vec<String> {
        let now = Instant::now();
        let (gone, kept) = self.deadlines.drain(..).partition(|(_, at)| *at <= now);
        self.deadlines = kept;
        gone.into_iter().map(|(k, _): (String, Instant)| k).collect()
    }
}
"#;
